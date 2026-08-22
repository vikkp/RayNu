//! CLI, REST API, Web UI, VM lifecycle.
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! M5.0: durable create / start / stop / destroy surface over a guest table.
//! M5.1: CLI + REST dispatch (`api`) over that table.
//! M5.2: embedded Web UI SPA (`webui`, PE `.aswebui`) drives list/start/stop.
//! M6.4: REST auth with bring-up mock token (`m6_auth_gate`).
//! M6.6: mock HA primary↔standby failover + harden checklist (`ha`, `m6_ha_gate`).
//! M6.7: fault injection suite (`fault`, `m6_fault_gate`).
//! M6.8: 72-hr soak thresholds (`soak`, `m6_soak_gate`).
//! M6.9: external audit + spec review (`ext`, `m6_ext_gate`).
//! Bring-up in `src/main.rs` remains the live VMLAUNCH path; this module is the
//! management-plane state machine those ops drive.

use crate::audit::AuditEvent;
use crate::audit_log;

/// Host / CI marker when the M5.0 lifecycle gate passes.
pub const M5_LIFE_OK_MARKER: &str = "RAYNU-V-M5-LIFE-OK";

/// Host / CI marker when the M5.1 API gate passes (re-export).
pub use api::M5_API_OK_MARKER;

/// Host / CI marker when the M5.2 Web UI gate passes (re-export).
pub use webui::M5_WEBUI_OK_MARKER;

/// Max guests tracked by the management-plane table (M5.5 migrate needs ≥10).
pub const MGMT_GUEST_CAP: usize = 16;

/// VM lifecycle state for the management plane (not Proven Core vCPU state).
///
/// Transitions (M5.0):
///   Defined → Running → Stopped → Destroyed
///   Defined → Destroyed (cancel before start)
///   Stopped → Running (restart)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmLifecycle {
    Defined,
    Running,
    Stopped,
    Destroyed,
}

/// Error from a lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    InvalidGuest,
    Full,
    NotFound,
    BadState,
}

/// One guest slot in the management-plane registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmRecord {
    pub guest_id: u64,
    pub state: VmLifecycle,
    /// vCPU count (M7.4 create-VM fields).
    pub cpu: u8,
    /// RAM in MiB.
    pub ram_mib: u32,
    /// Install / disk size in MiB.
    pub disk_mib: u32,
    /// Attached ISO image id (0 = none); from M7.2 library.
    pub iso_id: u64,
    /// ADR-014 image type. `None` = E4 SHELL / no product ISO.
    pub image_type: Option<guest_image::GuestImageType>,
}

/// Create-VM resource / media specification (M7.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmSpec {
    pub cpu: u8,
    pub ram_mib: u32,
    pub disk_mib: u32,
    pub iso_id: u64,
    /// Product ISO type when `iso_id != 0`. Lab `linux_bzimage` is rejected.
    pub image_type: Option<guest_image::GuestImageType>,
}

impl VmSpec {
    pub const fn defaults() -> Self {
        Self {
            cpu: 1,
            ram_mib: 512,
            disk_mib: 1024,
            iso_id: 0,
            image_type: None,
        }
    }

    pub fn validate(self) -> Result<(), LifecycleError> {
        if self.cpu == 0 || self.cpu > 64 {
            return Err(LifecycleError::InvalidGuest);
        }
        if self.ram_mib == 0 || self.disk_mib == 0 {
            return Err(LifecycleError::InvalidGuest);
        }
        match (self.iso_id, self.image_type) {
            (0, Some(_)) => return Err(LifecycleError::InvalidGuest),
            (0, None) => {}
            (_, Some(t)) if t.is_lab_only() => return Err(LifecycleError::InvalidGuest),
            (_, Some(_)) => {}
            (_, None) => {}
        }
        Ok(())
    }

    /// Fill `linux_iso` when an ISO is attached and the caller omitted the type.
    pub fn with_product_iso_default(mut self) -> Self {
        if self.iso_id != 0 && self.image_type.is_none() {
            self.image_type = Some(guest_image::GuestImageType::LinuxIso);
        }
        self
    }
}

/// Fixed-capacity guest lifecycle table.
pub struct VmTable {
    slots: [Option<VmRecord>; MGMT_GUEST_CAP],
    len: usize,
}

impl VmTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; MGMT_GUEST_CAP],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Look up a non-destroyed guest by id.
    pub fn get(&self, guest_id: u64) -> Option<&VmRecord> {
        self.slots
            .iter()
            .flatten()
            .find(|r| r.guest_id == guest_id && r.state != VmLifecycle::Destroyed)
    }

    fn get_mut(&mut self, guest_id: u64) -> Option<&mut VmRecord> {
        for slot in self.slots.iter_mut() {
            if let Some(r) = slot {
                if r.guest_id == guest_id && r.state != VmLifecycle::Destroyed {
                    return Some(r);
                }
            }
        }
        None
    }

    /// Create a guest in `Defined` state with default resources. Emits `VmCreated`.
    pub fn create(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        self.create_with_spec(guest_id, VmSpec::defaults())
    }

    /// Create a guest with CPU/RAM/disk/ISO fields (M7.4).
    pub fn create_with_spec(&mut self, guest_id: u64, spec: VmSpec) -> Result<(), LifecycleError> {
        if guest_id == 0 {
            return Err(LifecycleError::InvalidGuest);
        }
        let spec = spec.with_product_iso_default();
        spec.validate()?;
        if self.get(guest_id).is_some() {
            return Err(LifecycleError::BadState);
        }
        let rec = VmRecord {
            guest_id,
            state: VmLifecycle::Defined,
            cpu: spec.cpu,
            ram_mib: spec.ram_mib,
            disk_mib: spec.disk_mib,
            iso_id: spec.iso_id,
            image_type: spec.image_type,
        };
        // Reuse a Destroyed slot if present; else take a free slot.
        for slot in self.slots.iter_mut() {
            match slot {
                None => {
                    *slot = Some(rec);
                    self.len += 1;
                    audit_log!(AuditEvent::VmCreated { guest_id });
                    return Ok(());
                }
                Some(r) if r.state == VmLifecycle::Destroyed => {
                    *r = rec;
                    self.len += 1;
                    audit_log!(AuditEvent::VmCreated { guest_id });
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(LifecycleError::Full)
    }

    /// Defined | Stopped → Running. Emits `VmStarted`.
    pub fn start(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Defined | VmLifecycle::Stopped => {
                rec.state = VmLifecycle::Running;
                audit_log!(AuditEvent::VmStarted { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }

    /// Running → Stopped. Emits `VmStopped`.
    pub fn stop(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Running => {
                rec.state = VmLifecycle::Stopped;
                audit_log!(AuditEvent::VmStopped { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }

    /// Defined | Stopped → Destroyed. Emits `VmDestroyed`.
    ///
    /// Running guests must be stopped first (BadState).
    pub fn destroy(&mut self, guest_id: u64) -> Result<(), LifecycleError> {
        let rec = self.get_mut(guest_id).ok_or(LifecycleError::NotFound)?;
        match rec.state {
            VmLifecycle::Defined | VmLifecycle::Stopped => {
                rec.state = VmLifecycle::Destroyed;
                // len counts active (non-destroyed) slots.
                self.len = self.len.saturating_sub(1);
                audit_log!(AuditEvent::VmDestroyed { guest_id });
                Ok(())
            }
            _ => Err(LifecycleError::BadState),
        }
    }

    /// Copy active (non-destroyed) records into `out`; returns count written.
    pub fn list(&self, out: &mut [Option<VmRecord>]) -> usize {
        let mut n = 0;
        for slot in self.slots.iter() {
            if let Some(r) = slot {
                if r.state != VmLifecycle::Destroyed {
                    if n < out.len() {
                        out[n] = Some(*r);
                        n += 1;
                    }
                }
            }
        }
        n
    }
}

impl VmRecord {
    /// ADR-014 product boot spec when an ISO type is attached. `None` for E4 SHELL.
    pub fn boot_spec(self) -> Option<guest_image::GuestBootSpec> {
        let t = self.image_type?;
        guest_image::GuestBootSpec::product_iso(t, self.iso_id, self.disk_mib)
    }
}

impl Default for VmTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Initial lifecycle for a freshly created guest.
pub fn initial_lifecycle() -> VmLifecycle {
    VmLifecycle::Defined
}

/// Host-testable lifecycle round-trip (Defined→Running→Stopped→Destroyed).
pub fn prop_lifecycle_roundtrip() -> bool {
    let mut t = VmTable::new();
    let gid = 1u64;
    if t.create(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Defined) {
        return false;
    }
    if t.start(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Running) {
        return false;
    }
    if t.stop(gid).is_err() {
        return false;
    }
    if t.get(gid).map(|r| r.state) != Some(VmLifecycle::Stopped) {
        return false;
    }
    if t.destroy(gid).is_err() {
        return false;
    }
    t.get(gid).is_none() && t.len() == 0
}

pub mod api;
#[cfg(feature = "uefi-bin")]
pub mod bcm5720;
pub mod bcm5720_mmio;
pub mod datastore;
#[cfg(feature = "uefi-bin")]
pub mod e1000;
pub mod e1000_mmio;
pub mod el_torito;
pub mod ext;
pub mod fault;
pub mod guest_fw;
pub mod guest_image;
pub mod ha;
pub mod host_nic;
pub mod host_nic_coexist;
#[cfg(feature = "uefi-bin")]
pub mod host_nic_listen;
pub mod host_nic_poll;
pub mod http;
pub mod http_listen;
pub mod iso;
pub mod iso_install;
pub mod m5_api_gate;
pub mod m5_life_gate;
pub mod m5_webui_gate;
pub mod m6_auth_gate;
pub mod m6_ext_gate;
pub mod m6_fault_gate;
pub mod m6_ha_gate;
pub mod m6_soak_gate;
pub mod m7_e4_spa_gate;
pub mod m7_e5_alias_ept_gate;
pub mod m7_e5_boot_spec_gate;
pub mod m7_e5_cdrom_attach_gate;
pub mod m7_e5_cdrom_firmware_gate;
pub mod m7_e5_esp_launch_gate;
pub mod m7_e5_esp_map_gate;
pub mod m7_e5_ept_install_gate;
pub mod m7_e5_fw_alias_gate;
pub mod m7_e5_real_esp_gate;
pub mod m7_e5_real_launch_gate;
pub mod m7_e5_live_exec_gate;
pub mod m7_e5_priv_vmcs_gate;
pub mod m7_e5_live_issue_gate;
pub mod m7_e5_live_bytes_gate;
pub mod m7_e5_live_fd_gate;
pub mod m7_e5_live_present_gate;
pub mod m7_e5_live_admit_gate;
pub mod m7_e5_live_read_gate;
pub mod m7_e5_live_copy_gate;
pub mod m7_e5_live_place_gate;
pub mod m7_e5_fw_bind_gate;
pub mod m7_e5_fw_edk2_gate;
pub mod m7_e5_fw_floor_gate;
pub mod m7_e5_fw_prep_gate;
pub mod m7_e5_guest_fw_gate;
pub mod m7_e5_guest_fw_load_gate;
pub mod m7_e5_ovmf_esp_gate;
pub mod m7_e5_ovmf_probe_gate;
pub mod m7_e5_ovmf_slot_gate;
pub mod m7_e5_reset_vec_gate;
pub mod m7_host_nic_gate;
pub mod m7_http_gate;
pub mod m7_iso_gate;
pub mod m7_iso_install_gate;
pub mod m7_post_ebs_http_gate;
pub mod m7_r640_gate;
pub mod m7_ship_gate;
pub mod m7_store_gate;
pub mod m7_uefi_http_gate;
pub mod m7_ui_gate;
pub mod mgmt_arena;
pub mod mgmt_lease;
#[cfg(feature = "uefi-bin")]
pub mod net_probe_uefi;
pub mod pci_census;
pub mod pre_ebs_mgmt;
pub mod ship;
#[cfg(feature = "uefi-bin")]
pub mod snp_listen_uefi;
#[cfg(feature = "uefi-bin")]
pub mod snp_uefi;
pub mod soak;
pub mod spa_launch;
#[cfg(feature = "uefi-bin")]
pub mod tcp4_uefi;
pub mod webui;

pub use api::{
    clear_operator_token, dispatch_cli, dispatch_rest, parse_cli, parse_rest_method,
    prop_auth_deny_allow, prop_cli_rest_roundtrip, set_operator_token, ApiReply, CliCommand,
    RestMethod, RestRequest, RestResponse, AUTH_GAP_NOTE, BRINGUP_AUTH_TOKEN, M6_AUTH_OK_MARKER,
};
pub use datastore::{
    dispatch_store_rest, prop_datastore_package, ImageKind, ImageTable, M7_STORE_OK_MARKER,
    STORE_GAP_NOTE,
};
pub use el_torito::{parse_el_torito, ElToritoError, ElToritoImage};
pub use ext::{
    prop_external_audit_package, prop_findings_no_open_critical, prop_spec_review_filed,
    EXT_GAP_NOTE, M6_EXT_OK_MARKER,
};
pub use fault::{
    prop_corrupt_page_fail_closed, prop_drop_irq_fail_closed, prop_fault_suite,
    prop_kill_vcpu_recover, prop_net_partition_recover, FAULT_GAP_NOTE, M6_FAULT_OK_MARKER,
};
pub use guest_fw::{
    arm_ovmf_esp_launch, arm_ovmf_firmware_alias, arm_ovmf_firmware_slot, arm_ovmf_reset_vector,
    bind_ovmf_firmware_guest, box_guest_firmware, dispatch_guest_fw_rest, load_guest_firmware,
    load_ovmf_from_esp, map_live_esp_ovmf, prepare_ovmf_firmware_launch, probe_ovmf_firmware,
    arm_ovmf_real_launch, install_ovmf_alias_ept, program_ovmf_alias_ept, qualify_real_esp_ovmf,
    arm_ovmf_live_issue, arm_ovmf_private_vmcs, admit_ovmf_live_esp, present_ovmf_live_esp, probe_ovmf_live_bytes, read_ovmf_live_esp, require_ovmf_live_esp, require_ovmf_live_fd, stage_edk2_ovmf_firmware,
    stage_ovmf_firmware_floor, try_vmlaunch_ovmf_firmware, GuestFwBlob, GuestFwError, GuestFwKind,
    OvmfAlias, OvmfAliasEpt, OvmfAliasEptInstall, OvmfBind, OvmfEdk2, OvmfEspLaunch, OvmfFloor,
    OvmfFv, OvmfLaunchPrep, OvmfLiveAdmit, OvmfLiveBytes, OvmfLiveExec, OvmfLiveFd, OvmfLiveIssue, OvmfLiveMap, OvmfLivePresent, OvmfLiveRead, OvmfPrivateVmcs, OvmfRealEsp,
    OvmfRealLaunch, OvmfResetVec, OvmfSlot,
};
pub use guest_image::{GuestBootSpec, GuestFirmware, GuestImageType};
pub use ha::{
    dispatch_ha_rest, prop_ha_failover_restart, prop_security_harden_checklist, HaPair, HaRole,
    HA_GAP_NOTE, M6_HA_OK_MARKER,
};
pub use host_nic::{
    probe_host_nic_lab_flag, M7_HOST_NIC_HTTP_OK_MARKER, M7_HOST_NIC_QEMU_MARKER,
    M7_HOST_NIC_SCAFFOLD_MARKER,
};
pub use host_nic_coexist::{prop_coexist_wired, tick_native_coexist, try_arm_native_coexist};
pub use http::{prop_http_mgmt_package, HTTP_GAP_NOTE, HTTP_LAB_NOTE, M7_HTTP_OK_MARKER};
pub use http_listen::{
    run_post_ebs_http_idle, run_post_ebs_http_snp_warn_only, run_post_ebs_mgmt_listen,
    run_pre_ebs_mgmt_listen, M7_POST_EBS_HTTP_OK_MARKER, M7_POST_EBS_HTTP_SCAFFOLD_MARKER,
    M7_UEFI_HTTP_OK_MARKER, M7_UEFI_HTTP_SCAFFOLD_MARKER, UEFI_HTTP_GAP_NOTE,
};
pub use iso::{
    attach_cdrom_firmware, attach_cdrom_host, dispatch_iso_attach_rest, dispatch_iso_firmware_rest,
    dispatch_iso_rest, prop_iso_deploy_package, CdromAttach, CdromAttachState, CdromTable,
    FirmwareBootImage, IsoDeployPlan, ISO_GAP_NOTE, M7_ISO_OK_MARKER,
};
pub use iso_install::{
    disk_bytes_for_virtio_launch, dispatch_iso_install_rest, install_disk_armed_for_launch,
    install_disk_preload_bytes, lab_reboot_armed, probe_iso_install_lab_flag,
    probe_iso_persist_reboot, probe_iso_reboot_lab_flag, prop_iso_install_lab_package,
    prop_iso_install_package, prop_iso_reboot_lab_package, InstallToDiskPlan, ISO_INSTALL_GAP_NOTE,
    M7_ISO_INSTALL_OK_MARKER, M7_ISO_INSTALL_SCAFFOLD_MARKER,
};
pub use m5_api_gate::run_m5_api_gate;
pub use m5_life_gate::run_m5_life_gate;
pub use m5_webui_gate::run_m5_webui_gate;
pub use m6_auth_gate::{run_m6_auth_gate, M6_AUTH_GATE_MARKER};
pub use m6_ext_gate::{run_m6_ext_gate, M6_EXT_GATE_MARKER};
pub use m6_fault_gate::{run_m6_fault_gate, M6_FAULT_GATE_MARKER};
pub use m6_ha_gate::{run_m6_ha_gate, M6_HA_GATE_MARKER};
pub use m6_soak_gate::{run_m6_soak_gate, M6_SOAK_GATE_MARKER};
pub use m7_e4_spa_gate::run_m7_e4_spa_gate;
pub use m7_e5_alias_ept_gate::{run_m7_e5_alias_ept_gate, M7_E5_ALIAS_EPT_OK_MARKER};
pub use m7_e5_boot_spec_gate::{run_m7_e5_boot_spec_gate, M7_E5_BOOT_SPEC_OK_MARKER};
pub use m7_e5_cdrom_attach_gate::{run_m7_e5_cdrom_attach_gate, M7_E5_CDROM_ATTACH_OK_MARKER};
pub use m7_e5_cdrom_firmware_gate::{
    run_m7_e5_cdrom_firmware_gate, M7_E5_CDROM_FIRMWARE_OK_MARKER,
};
pub use m7_e5_esp_launch_gate::{run_m7_e5_esp_launch_gate, M7_E5_ESP_LAUNCH_OK_MARKER};
pub use m7_e5_esp_map_gate::{run_m7_e5_esp_map_gate, M7_E5_ESP_MAP_OK_MARKER};
pub use m7_e5_ept_install_gate::{run_m7_e5_ept_install_gate, M7_E5_EPT_INSTALL_OK_MARKER};
pub use m7_e5_fw_alias_gate::{run_m7_e5_fw_alias_gate, M7_E5_FW_ALIAS_OK_MARKER};
pub use m7_e5_real_esp_gate::{run_m7_e5_real_esp_gate, M7_E5_REAL_ESP_OK_MARKER};
pub use m7_e5_real_launch_gate::{run_m7_e5_real_launch_gate, M7_E5_REAL_LAUNCH_OK_MARKER};
pub use m7_e5_live_exec_gate::{run_m7_e5_live_exec_gate, M7_E5_LIVE_EXEC_OK_MARKER};
pub use m7_e5_priv_vmcs_gate::{run_m7_e5_priv_vmcs_gate, M7_E5_PRIV_VMCS_OK_MARKER};
pub use m7_e5_live_issue_gate::{run_m7_e5_live_issue_gate, M7_E5_LIVE_ISSUE_OK_MARKER};
pub use m7_e5_live_bytes_gate::{run_m7_e5_live_bytes_gate, M7_E5_LIVE_BYTES_OK_MARKER};
pub use m7_e5_live_fd_gate::{run_m7_e5_live_fd_gate, M7_E5_LIVE_FD_OK_MARKER};
pub use m7_e5_live_present_gate::{run_m7_e5_live_present_gate, M7_E5_LIVE_PRESENT_OK_MARKER};
pub use m7_e5_live_admit_gate::{run_m7_e5_live_admit_gate, M7_E5_LIVE_ADMIT_OK_MARKER};
pub use m7_e5_live_read_gate::{run_m7_e5_live_read_gate, M7_E5_LIVE_READ_OK_MARKER};
pub use m7_e5_live_copy_gate::{run_m7_e5_live_copy_gate, M7_E5_LIVE_COPY_OK_MARKER};
pub use m7_e5_live_place_gate::{run_m7_e5_live_place_gate, M7_E5_LIVE_PLACE_OK_MARKER};
pub use m7_e5_fw_bind_gate::{run_m7_e5_fw_bind_gate, M7_E5_FW_BIND_OK_MARKER};
pub use m7_e5_fw_edk2_gate::{run_m7_e5_fw_edk2_gate, M7_E5_FW_EDK2_OK_MARKER};
pub use m7_e5_fw_floor_gate::{run_m7_e5_fw_floor_gate, M7_E5_FW_FLOOR_OK_MARKER};
pub use m7_e5_fw_prep_gate::{run_m7_e5_fw_prep_gate, M7_E5_FW_PREP_OK_MARKER};
pub use m7_e5_guest_fw_gate::{run_m7_e5_guest_fw_gate, M7_E5_GUEST_FW_OK_MARKER};
pub use m7_e5_guest_fw_load_gate::{run_m7_e5_guest_fw_load_gate, M7_E5_GUEST_FW_LOAD_OK_MARKER};
pub use m7_e5_ovmf_esp_gate::{run_m7_e5_ovmf_esp_gate, M7_E5_OVMF_ESP_OK_MARKER};
pub use m7_e5_ovmf_probe_gate::{run_m7_e5_ovmf_probe_gate, M7_E5_OVMF_PROBE_OK_MARKER};
pub use m7_e5_ovmf_slot_gate::{run_m7_e5_ovmf_slot_gate, M7_E5_OVMF_SLOT_OK_MARKER};
pub use m7_e5_reset_vec_gate::{run_m7_e5_reset_vec_gate, M7_E5_RESET_VEC_OK_MARKER};
pub use m7_host_nic_gate::{run_m7_host_nic_scaffold_gate, M7_HOST_NIC_GATE_MARKER};
pub use m7_http_gate::{run_m7_http_gate, M7_HTTP_GATE_MARKER};
pub use m7_iso_gate::{run_m7_iso_gate, M7_ISO_GATE_MARKER};
pub use m7_iso_install_gate::{run_m7_iso_install_scaffold_gate, M7_ISO_INSTALL_GATE_MARKER};
pub use m7_post_ebs_http_gate::{run_m7_post_ebs_http_scaffold_gate, M7_POST_EBS_HTTP_GATE_MARKER};
pub use m7_r640_gate::{
    run_m7_r640_scaffold_gate, M7_R640_OK_MARKER, M7_R640_SCAFFOLD_MARKER, R640_GAP_NOTE,
};
pub use m7_ship_gate::{run_m7_ship_gate, M7_SHIP_GATE_MARKER};
pub use m7_store_gate::{run_m7_store_gate, M7_STORE_GATE_MARKER};
pub use m7_uefi_http_gate::{run_m7_uefi_http_scaffold_gate, M7_UEFI_HTTP_GATE_MARKER};
pub use m7_ui_gate::{run_m7_ui_gate, M7_UI_OK_MARKER, UI_GAP_NOTE};
pub use mgmt_arena::{
    inject_mgmt_fatals, prop_arena_reset_rewinds, MgmtArena, MgmtFatal, MGMT_ARENA_BYTES,
    MGMT_FATAL_INJECT_N,
};
pub use pci_census::{census_pick, iron_marker_allowed, run_pre_ebs_pci_census, CENSUS_NOTE};
pub use pre_ebs_mgmt::{prop_pre_ebs_mgmt_durable, reset_pre_ebs_mgmt};
pub use ship::{prop_release_kit_package, M7_SHIP_OK_MARKER, SHIP_GAP_NOTE};
pub use soak::{
    prop_soak_72h_thresholds, run_soak_simulation, thresholds_met, SoakMetrics, M6_SOAK_OK_MARKER,
    SOAK_GAP_NOTE, SOAK_TARGET_HOURS,
};
pub use spa_launch::{note_spa_start, note_spa_stop, take_spa_start, M7_E4_SPA_LAUNCH_OK_MARKER};
pub use webui::{dispatch_webui_action, load_webui, prop_webui_list_start_stop, WebUiAction};

#[cfg(test)]
#[path = "mgmt_test.rs"]
mod mgmt_test;
