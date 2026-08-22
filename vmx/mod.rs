//! VT-x / VMX setup, VMCS management, entry/exit.
//!
//! Pillar: [V]
//! Proven Core: **inside** (ADR-002)
//! VERIFICATION: L0/L1 — see `*_spec.rs` placeholders

pub mod fields;
pub mod guest_pt;
pub mod hardware;
pub mod launch;
pub mod lifecycle;
pub mod mmio_decode;
pub mod noirq_gate;
pub mod ops;
pub mod vmcs;

pub use crate::memory::{M2_EPT_OK_MARKER, M2_GUEST_OK_MARKER, M2_OWN_OK_MARKER};
pub use hardware::{M1_VMXON_OK_MARKER, M1_VMXON_SKIP_MARKER};
pub use launch::{
    alias_ept_covers_reset, arm_guest_uefi_firmware_alias, arm_guest_uefi_live_issue,
    arm_guest_uefi_private_vmcs, arm_guest_uefi_real_launch, arm_guest_uefi_reset_vector,
    arm_live_esp_ovmf_mapping, firmware_alias_gpa, guest_uefi_alias_ept_is_installed,
    guest_uefi_alias_ept_is_programmed, guest_uefi_firmware_alias_is_armed,
    guest_uefi_live_bytes_is_probed, guest_uefi_live_esp_is_required,
    guest_uefi_live_esp_is_admitted, guest_uefi_live_esp_is_applied, guest_uefi_live_esp_is_committed, guest_uefi_live_esp_is_copied, guest_uefi_live_esp_is_latched, guest_uefi_live_esp_is_placed, guest_uefi_live_esp_is_presented, guest_uefi_live_esp_is_read, guest_uefi_live_fd_is_required,
    guest_uefi_live_issue_is_armed, admit_guest_uefi_live_esp, apply_guest_uefi_live_esp, commit_guest_uefi_live_esp, copy_guest_uefi_live_esp, latch_guest_uefi_live_esp, place_guest_uefi_live_esp, present_guest_uefi_live_esp, read_guest_uefi_live_esp,
    probe_guest_uefi_live_bytes, require_guest_uefi_live_fd,
    guest_uefi_private_vmcs_is_armed, guest_uefi_real_esp_is_qualified,
    guest_uefi_real_launch_is_armed, guest_uefi_reset_vector_is_armed,
    install_guest_uefi_alias_ept, live_esp_ovmf_is_mapped, program_guest_uefi_alias_ept,
    qualify_guest_uefi_real_esp, require_guest_uefi_live_esp, reset_guest_uefi_reset_vector,
    reset_live_esp_ovmf_mapping, try_vmlaunch_guest_uefi_ovmf, GuestUefiAliasEpt,
    GuestUefiLaunchError, GuestUefiResetVmcs, LaunchError, LaunchFrames, GUEST_UEFI_ALIAS_EPT,
    GUEST_UEFI_FIRMWARE_TOP_GPA, GUEST_UEFI_OVMF_ESP_PATH, GUEST_UEFI_PRIVATE_VMCS_ID,
    GUEST_UEFI_RESET_VMCS, GUEST_UEFI_UNRESTRICTED_GUEST, GUEST_UEFI_VMLAUNCH_OPCODE,
    M1_VMEXIT_OK_MARKER, MIN_FIRMWARE_ALIAS_BYTES, MIN_LIVE_ESP_OVMF_BYTES,
};
pub use lifecycle::{VmxError, VmxLifecycle, VmxState};
pub use noirq_gate::{run_noirq_gate, M3_NOIRQ_OK_MARKER};
pub use vmcs::{VmcsHandle, VmcsRegion};
