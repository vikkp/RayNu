//! E5 RayNu-F F7 — guest reset → disk ESP boot (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-016 / ADR-017)
//! VERIFICATION: N/A (host `include_str!` + unit tests)
//!
//! Host/CI: F7 surfaces exist. Nested `fe4785a` on `raynuvsrv1` reached
//! reboot-to-disk (second `Linux version`, `root=UUID=`, `DISK-BOOT-OK`).
//! **Iron `56a3ffd` (run `34480107961`, 2026-09-10) closed it on the real
//! R640:** install → `reboot` → F7 relaunch → installed GRUB countdown →
//! `RAYNU-V-RAYNU-F-DISK-BOOT-OK` → second Linux `root=UUID=` → `login:`.
//! Evidence: `docs/evidence/r640/2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md`.
//! Nested `088ab25` showed the reset lines: Alpine `reboot` pulsed the i8042
//! (`src=kbc`), not `0xCF9` / FADT — the kernel runs `efi=noruntime`, so the
//! CF9 and triple-fault classifiers stay host-tested only.
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK` from host/CI.
//!
//! Iron `59ac070` / run `34425781629` (R640, 2026-09-10): Alpine `setup-disk`
//! finished on virtio-blk `vda` (GPT, grub-install, initramfs, `Installation
//! finished. No error reported.`) and the iron-only install marker printed
//! (`take_iso_install_ok`). `reboot` → `src=kbc` → F7 relaunch **failed**:
//! `launch failed: VMCLEAR/VMPTRLD`. Release asm: `raynu_f_reset_relaunch`
//! had an 81,024-byte frame (`RAYNU_F_STATE = FirmwareState::new()` built
//! the 79,704-byte `PagePool` on the stack) on a 16 KiB host stack whose
//! next-lower frame was the private VMCS — `VMPTRLD` saw a foreign revision
//! dword. Fix: `.rdata` template `memcpy`, 32-page host stack + guard page,
//! split VMCLEAR/VMPTRLD diagnostics. Iron reboot-to-disk stays open.
//!
//! Iron `975f8fc` / run `34474850361` (R640, 2026-09-10): the relaunch
//! worked — `guest reset requested src=kbc n=1`, `GPT ESP lba=2048
//! sectors=98304 part=1`, `disk whole-disk path`, GRUB 2.12 from the
//! installed ESP (`image=DISK-BOOTX64`, `CONOUT-OK`, menu "*Alpine Linux
//! v3.21 … executed automatically in 2s"). Then `stop exit-cap
//! exits=1048577 svc=492778` before the 2 s timeout: GRUB's `run_menu`
//! polls `ReadKeyStroke` + the serial terminal LSR with no idle, ~2 exits
//! per µs on the R640, so the 1 M exit count fired inside GRUB's own
//! timeout (nested KVM exits are 10–50× slower and never reached it).
//! Fix: RayNu-F wall cap — time bounds the loader phase, the exit count
//! only guards a u32 wrap. `56a3ffd` then closed reboot-to-disk on iron.

/// Host / CI marker when the F7 surface gate passes.
pub const M7_E5_RAYNU_F_F7_OK_MARKER: &str = "RAYNU-V-M7-E5-RAYNU-F-F7-OK";

/// Honest residual: nested reboot-to-disk ≠ iron E5.
pub const E5_RAYNU_F_F7_RESIDUAL_NOTE: &str =
    "residual: nested fe4785a reboot-to-disk (DISK-BOOT-OK + second Linux root=UUID=) is proven on raynuvsrv1; iron 59ac070 installed to vda and printed the install marker but F7 relaunch failed VMCLEAR/VMPTRLD (host-stack overflow into the VMCS; fixed by RayNu-F F7 template reset + guest-UEFI host stack guard); iron 975f8fc relaunched and reached the installed GRUB menu, then the 1 M exit-cap fired inside GRUB's 2 s menu timeout (fixed by the RayNu-F wall cap: time, not exits, bounds the loader phase); iron 56a3ffd (run 34480107961) closed reboot-to-disk on the real R640: countdown 2s/1s/0s, DISK-BOOT-OK, EBS-OK, second Linux root=UUID= from vda2, login; E5 Phase A is closed on iron; Phase B (SPA/REST start launches the RayNu-F ISO/installed-disk path instead of the SHELL stub) is not claimed and the ISO-INSTALL-OK marker is never printed from host/CI";

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
        && guest.contains("RayNu-F F7 template reset")
        && guest.contains("static RAYNU_F_STATE_TEMPLATE")
        && !guest.contains("RAYNU_F_STATE = crate::raynu_f::FirmwareState::new()")
        && guest.contains("guest-UEFI host stack guard")
        && guest.contains("fn guest_uefi_host_stack_guard_ok")
        && guest.contains("fn raynu_f_launch_vmcs_fail")
        && guest.contains("serial::flush_guest_tx()")
        && guest.contains("RayNu-F wall cap")
        && guest.contains("fn raynu_f_wall_cap_hit")
        && guest.contains("raynu_f_stop(\"wall-cap\")")
        && guest.contains("RAYNU_F_LAUNCH_TSC.store(cpu::rdtsc().max(1)")
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
        && E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("iron 56a3ffd")
        && E5_RAYNU_F_F7_RESIDUAL_NOTE.contains("Phase B")
        && M7_E5_RAYNU_F_F7_OK_MARKER == "RAYNU-V-M7-E5-RAYNU-F-F7-OK"
        && crate::vmx::guest_uefi::GUEST_UEFI_HOST_STACK_PAGES >= 32
        && crate::vmx::guest_uefi::GUEST_UEFI_HOST_STACK_GUARD_PAGES >= 1
        && crate::vmx::guest_uefi::guest_uefi_host_stack_headroom(4, 81_024) < 0
        && crate::vmx::guest_uefi::guest_uefi_host_stack_headroom(
            crate::vmx::guest_uefi::GUEST_UEFI_HOST_STACK_PAGES,
            81_024,
        ) > 0
        // RayNu-F wall cap (iron 975f8fc): the count cap must not fire inside
        // GRUB's 2 s menu timeout at iron exit rates; the wall cap is the bound.
        && crate::vmx::guest_uefi::RAYNU_F_EXIT_CAP >= 1 << 28
        && (60..=600).contains(&crate::vmx::guest_uefi::RAYNU_F_WALL_CAP_S)
        && !crate::vmx::guest_uefi::raynu_f_wall_cap_hit(1, 1 + 2 * 2_100_000_000, 2_100_000_000, crate::vmx::guest_uefi::RAYNU_F_WALL_CAP_S)
        && crate::vmx::guest_uefi::raynu_f_wall_cap_hit(
            1,
            1 + (crate::vmx::guest_uefi::RAYNU_F_WALL_CAP_S + 1) * 2_100_000_000,
            2_100_000_000,
            crate::vmx::guest_uefi::RAYNU_F_WALL_CAP_S,
        )
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
