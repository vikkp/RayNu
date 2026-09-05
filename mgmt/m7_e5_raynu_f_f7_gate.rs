//! E5 RayNu-F F7 — guest reset → disk ESP boot (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-016 / ADR-017)
//! VERIFICATION: N/A (host `include_str!` + unit tests)
//!
//! Host/CI: F7 surfaces exist. Nested `fe4785a` on `raynuvsrv1` reached
//! reboot-to-disk (second `Linux version`, `root=UUID=`, `DISK-BOOT-OK`).
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK`. Iron E5 stays open.

/// Host / CI marker when the F7 surface gate passes.
pub const M7_E5_RAYNU_F_F7_OK_MARKER: &str = "RAYNU-V-M7-E5-RAYNU-F-F7-OK";

/// Honest residual: nested reboot-to-disk ≠ iron E5.
pub const E5_RAYNU_F_F7_RESIDUAL_NOTE: &str =
    "residual: nested fe4785a reboot-to-disk (DISK-BOOT-OK + second Linux root=UUID=) is proven on raynuvsrv1; iron ISO-INSTALL-OK is not claimed";

/// True when F7 function names, markers, and honesty lines exist.
pub fn raynu_f_f7_surface_present() -> bool {
    let gpt = include_str!("../raynu_f/gpt.rs");
    let proto = include_str!("../raynu_f/protocol.rs");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let plat = include_str!("../devices/guest_platform.rs");
    let acpi = include_str!("../devices/guest_acpi.rs");
    let virtio = include_str!("../devices/guest_virtio_blk.rs");
    let serial = include_str!("../devices/guest_serial_answer.rs");
    let harness = include_str!("../tools/e5-product-iso-qemu-serial.sh");
    let rf = include_str!("../raynu_f/mod.rs");
    gpt.contains("fn find_esp")
        && gpt.contains("fn raynu_f_boot_source(")
        && gpt.contains("ESP_TYPE_GUID")
        && proto.contains("fn encode_hd_device_path(")
        && proto.contains("fn encode_whole_disk_device_path(")
        && proto.contains("fn device_path_is_grub_partition_child(")
        && plat.contains("fn reset_request_from_io(")
        && plat.contains("enum ResetSrc")
        && acpi.contains("RESET_REG_SUP")
        && acpi.contains("RESET_VALUE")
        && virtio.contains("fn reset_keep_disk(")
        && virtio.contains("DISK_HPA")
        && serial.contains("fn begin_second_boot(")
        && serial.contains("KERNELOPTS=")
        && serial.contains("PHASE_INSTALLED")
        && guest.contains("fn raynu_f_reset_relaunch")
        && guest.contains("fn raynu_f_on_guest_reset")
        && guest.contains("fn raynu_f_reset_vmcs_guest_state")
        && guest.contains("fn raynu_f_stage_disk_bootloader")
        && guest.contains("encode_whole_disk_device_path")
        && !guest.contains("encode_hd_device_path")
        && guest.contains("RAYNU_F_RESET_MAX")
        && guest.contains("boot: RayNu-F guest reset requested src=")
        && guest.contains("boot: RayNu-F relaunch after reset (F7; not ISO-INSTALL-OK)")
        && guest.contains("boot: RayNu-F GPT ESP lba=")
        && guest.contains("boot: RayNu-F disk whole-disk path (F7; not ISO-INSTALL-OK)")
        && guest.contains("(F7 disk; not ISO-INSTALL-OK)")
        && guest.contains("reset-cap")
        && !guest.contains("RAYNU-V-M7-ISO-INSTALL-OK")
        && rf.contains("RAYNU-V-RAYNU-F-DISK-BOOT-OK")
        && harness.contains("RAYNU-V-RAYNU-F-DISK-BOOT-OK")
        && harness.contains("nested reboot-to-disk reached a second Linux boot")
        && harness.contains("Nested fe4785a (raynuvsrv1)")
        && harness.contains("TIMEOUT_SECS:-1800")
        && harness.contains("nested/host printed iron ISO-INSTALL-OK")
}

/// Full F7 host package. Nested reboot-to-disk proven on `raynuvsrv1`
/// `fe4785a`. Not iron `ISO-INSTALL-OK`.
pub fn run_m7_e5_raynu_f_f7_gate() -> bool {
    E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("not claimed")
        && M7_E5_RAYNU_F_F7_OK_MARKER == "RAYNU-V-M7-E5-RAYNU-F-F7-OK"
        && crate::raynu_f::RAYNU_F_DISK_BOOT_OK_MARKER == "RAYNU-V-RAYNU-F-DISK-BOOT-OK"
        && crate::vmx::guest_uefi::RAYNU_F_RESET_MAX == 1
        && crate::devices::guest_platform::reset_request_from_io(0xCF9, false, 1, 0x06)
            == Some(crate::devices::guest_platform::ResetSrc::Cf9)
        && crate::devices::guest_platform::reset_request_from_io(0x64, false, 1, 0xFE)
            == Some(crate::devices::guest_platform::ResetSrc::Kbc)
        && crate::raynu_f::raynu_f_boot_source(false) == crate::raynu_f::BootSource::Iso
        && crate::raynu_f::raynu_f_boot_source(true) == crate::raynu_f::BootSource::Disk
        && raynu_f_f7_surface_present()
}

#[cfg(test)]
#[path = "m7_e5_raynu_f_f7_gate_test.rs"]
mod m7_e5_raynu_f_f7_gate_test;
