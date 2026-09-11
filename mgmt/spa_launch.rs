//! E4 — SPA create/start queues a real VMLAUNCH (private EPT, slab VMCS).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002). The HTTP handler only sets a flag;
//! `schedule_preempt` performs VMLAUNCH in VMX root.
//!
//! G1–G3 M4 stubs keep VMCS in the G0 identity pool (Linux can scribble).
//! SPA start relocates G1 into its 2 MiB slab (already punched out of G0 EPT)
//! and builds a **single 2 MiB** private EPT. G0's VMCS is cloned with
//! VMREAD/VMWRITE into a host-only punched slab (memcpy of a `VMCLEAR`'d
//! region is not VMPTRLD-safe). Distro installer closed on iron via SPA (`f72b4276`). TLS remains later.
//!
//! Phase B (P0-63): a product-ISO start (`linux_iso` / `windows_iso` /
//! `generic_uefi`, iso ≠ 0) queues **RayNu-F**, not the SHELL CPUID stub.
//! `iso=0` stays E4 SHELL. Host/CI never prints `ISO-INSTALL-OK`.

use super::guest_image::{GuestBootSpec, GuestImageType};

/// Iron marker when SPA start VMLAUNCHes the private-EPT SHELL guest.
pub const M7_E4_SPA_LAUNCH_OK_MARKER: &str = "RAYNU-V-M7-E4-SPA-LAUNCH-OK";

/// Host/CI marker when SPA/REST product-ISO start queues RayNu-F (P0-63).
/// Iron close is COM2 `f72b4276` / `34552377351`. Never `ISO-INSTALL-OK`.
pub const M7_PHASE_B_SPA_WIRE_OK_MARKER: &str = "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK";

/// Firmware serial when the coexist scheduler consumes a product-ISO start.
pub const M7_PHASE_B_SPA_RAYNU_F_NOTE: &str =
    "boot: E4 SPA start — RayNu-F product ISO (Phase B; not SHELL; not ISO-INSTALL-OK)";

/// What `POST /vms/{id}/start` asks the coexist scheduler to launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpaStartKind {
    /// E4 SHELL CPUID stub (`iso=0` / no product boot spec).
    Shell,
    /// ADR-014 product ISO → RayNu-F ISO / installed-disk path (ADR-016).
    RayNuF,
}

static mut PENDING_START: Option<u64> = None;
static mut PENDING_KIND: SpaStartKind = SpaStartKind::Shell;
static mut PENDING_STOP: bool = false;

/// Queue a SPA `POST /vms/{id}/start` as E4 SHELL (lab / `iso=0`).
pub fn note_spa_start(guest_id: u64) {
    note_spa_start_kind(guest_id, SpaStartKind::Shell);
}

/// Queue a SPA start with an explicit launch kind (Phase B).
pub fn note_spa_start_kind(guest_id: u64, kind: SpaStartKind) {
    // SAFETY: BSP-only HTTP tick / host tests.
    unsafe {
        PENDING_START = Some(guest_id);
        PENDING_KIND = kind;
    }
}

/// Classify a VM record: product ISO → RayNu-F; otherwise SHELL.
pub fn kind_for_record(
    iso_id: u64,
    image_type: Option<GuestImageType>,
    disk_mib: u32,
) -> SpaStartKind {
    let Some(t) = image_type else {
        return SpaStartKind::Shell;
    };
    match GuestBootSpec::product_iso(t, iso_id, disk_mib) {
        Some(s) if s.is_product_path() => SpaStartKind::RayNuF,
        _ => SpaStartKind::Shell,
    }
}

/// Queue a SPA stop (park the E4 slot; G0 keeps running).
pub fn note_spa_stop(_guest_id: u64) {
    unsafe {
        PENDING_STOP = true;
    }
}

/// Take a pending start (scheduler). `None` if idle. Kind is discarded;
/// prefer [`take_spa_start_kind`].
pub fn take_spa_start() -> Option<u64> {
    take_spa_start_kind().map(|(id, _)| id)
}

/// Take a pending start and its launch kind.
pub fn take_spa_start_kind() -> Option<(u64, SpaStartKind)> {
    unsafe {
        let id = PENDING_START.take()?;
        let kind = PENDING_KIND;
        PENDING_KIND = SpaStartKind::Shell;
        Some((id, kind))
    }
}

/// Take a pending stop.
pub fn take_spa_stop() -> bool {
    unsafe {
        let v = PENDING_STOP;
        PENDING_STOP = false;
        v
    }
}

/// Host: start REST queues a launch token (SHELL default + product-ISO kind).
pub fn prop_spa_start_queues() -> bool {
    let _ = take_spa_start_kind();
    let _ = take_spa_stop();
    note_spa_start(9);
    let a = take_spa_start_kind() == Some((9, SpaStartKind::Shell));
    let b = take_spa_start_kind().is_none();
    note_spa_start_kind(4, SpaStartKind::RayNuF);
    let c = take_spa_start_kind() == Some((4, SpaStartKind::RayNuF));
    note_spa_stop(9);
    let d = take_spa_stop() && !take_spa_stop();
    a && b && c && d
}

/// Host: record fields pick SHELL vs RayNu-F without VMLAUNCH.
pub fn prop_spa_kind_for_record() -> bool {
    let shell = kind_for_record(0, None, 1024) == SpaStartKind::Shell;
    let linux = kind_for_record(1, Some(GuestImageType::LinuxIso), 1024)
        == SpaStartKind::RayNuF;
    let win = kind_for_record(2, Some(GuestImageType::WindowsIso), 64)
        == SpaStartKind::RayNuF;
    let generic = kind_for_record(3, Some(GuestImageType::GenericUefi), 32)
        == SpaStartKind::RayNuF;
    let bz = kind_for_record(1, Some(GuestImageType::LinuxBzImage), 1024)
        == SpaStartKind::Shell;
    let no_iso = kind_for_record(0, Some(GuestImageType::LinuxIso), 1024)
        == SpaStartKind::Shell;
    shell && linux && win && generic && bz && no_iso
}

#[cfg(test)]
#[path = "spa_launch_test.rs"]
mod spa_launch_test;
