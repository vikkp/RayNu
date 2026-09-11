//! M8.0 persist host gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves the backend choice is written down, the iron marker is minted, and
//! the host GPT+ESP+ext4 round-trip exists. Does **not** print
//! `RAYNU-V-M7-ISO-INSTALL-OK` or the iron persist marker. Nested QEMU is
//! not this gate. Iron COM2 is not this gate.

use crate::mgmt::disk_persist::{
    esp_copy_is_rejected, host_never_prints_iso_install_ok, kind_survives_hv_reboot,
    select_persist_kind, PersistKind, ESP_COPY_REJECT_NOTE, M8_DISK_PERSIST_HOST_OK_MARKER,
    M8_DISK_PERSIST_OK_MARKER, PERC_UBUNTU_UNTOUCHED_NOTE, PERSIST_EXCLUSIVE_OWNERSHIP_NOTE,
    UDISK_TOO_SMALL_NOTE,
};

/// Host / CI marker when the M8.0 persist package passes.
pub const M8_DISK_PERSIST_GATE_MARKER: &str = M8_DISK_PERSIST_HOST_OK_MARKER;

/// Honesty: host round-trip ≠ nested Alpine kill/restart ≠ iron persist.
pub const M8_DISK_PERSIST_RESIDUAL_NOTE: &str =
    "residual: persist-first attach_disk_keep + host File round-trip is not nested Alpine kill/restart and not iron RAYNU-V-M8-DISK-PERSIST-OK; leftover DRAM remains the fallback; M8_PERSIST_IMG NVDIMM default off; tools/m8-persist-nested.sh is the nested two-boot harness; do not print ISO-INSTALL-OK";

/// True when plan, markers, leftover fallback, persist-first attach, and exclusive-ownership notes exist.
pub fn disk_persist_surface_present() -> bool {
    let persist = include_str!("disk_persist.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let leftover = include_str!("iso_install.rs");
    let attach = include_str!("../vmx/guest_uefi.rs");
    let virtio = include_str!("../devices/guest_virtio_blk.rs");
    let handoff = include_str!("../boot/handoff.rs");
    let qemu = include_str!("../tools/run-qemu.sh");
    let nested = include_str!("../tools/m8-persist-nested.sh");
    persist.contains("enum PersistKind")
        && persist.contains("File")
        && persist.contains("DurableLun")
        && persist.contains("LeftoverDram")
        && persist.contains("fn select_persist_kind(")
        && persist.contains("fn restored_disk_is_installed(")
        && persist.contains("fn persist_media_looks_installed(")
        && persist.contains("fn choose_install_disk_attach(")
        && persist.contains("fn take_persist_install_disk(")
        && persist.contains("fn reserve_persist_install_disk(")
        && persist.contains("fn host_never_prints_iso_install_ok(")
        && persist.contains(M8_DISK_PERSIST_OK_MARKER)
        && persist.contains(M8_DISK_PERSIST_HOST_OK_MARKER)
        && persist.contains("RAYNU-V-M8-DISK-PERSIST-NESTED-OK")
        && persist.contains("ADR-004")
        && persist.contains("reset_keep_disk")
        && persist.contains("InstallDiskChoice::PersistKeep")
        && !persist.contains("println!(\"RAYNU-V-M7-ISO-INSTALL-OK\")")
        && !persist.contains("println!(\"RAYNU-V-M8-DISK-PERSIST-OK\")")
        && plan.contains("RAYNU-V-M8-DISK-PERSIST-OK")
        && plan.contains("RAYNU-V-M8-DISK-PERSIST-HOST-OK")
        && plan.contains("file-backed nested")
        && plan.contains("durable LUN")
        && plan.contains("leftover DRAM")
        && plan.contains("Force Off")
        && plan.contains("attach_disk_keep")
        && leftover.contains("fn carve_leftover_install_disk(")
        && leftover.contains("fn take_leftover_install_disk(")
        && attach.contains("fn try_alloc_product_iso_install_disk(")
        && attach.contains("take_leftover_install_disk")
        && attach.contains("take_persist_install_disk")
        && attach.contains("attach_disk_keep")
        && virtio.contains("fn attach_disk_keep(")
        && virtio.contains("fn attach_disk(")
        && handoff.contains("PERSISTENT_MEMORY")
        && handoff.contains("leftover install disk skip persist")
        && qemu.contains("M8_PERSIST_IMG")
        && qemu.contains("nvdimm=on")
        && qemu.contains("memory-backend-file")
        && !qemu.contains("-mem-path")
        && nested.contains("MODE=smoke")
        && nested.contains("MODE=full")
        && nested.contains("M8_PERSIST_IMG")
        && nested.contains("Installation is complete")
        && nested.contains("keep=1")
        && nested.contains("RAYNU-V-M8-DISK-PERSIST-NESTED-OK")
        && nested.contains("kill HV")
        && !nested.contains("println!(\"RAYNU-V-M7-ISO-INSTALL-OK\")")
        && !nested.contains("echo \"RAYNU-V-M8-DISK-PERSIST-OK\"")
}

/// Host package: backend choice + leftover fallback + never ISO-INSTALL-OK.
pub fn run_m8_disk_persist_host_gate() -> bool {
    M8_DISK_PERSIST_GATE_MARKER == "RAYNU-V-M8-DISK-PERSIST-HOST-OK"
        && M8_DISK_PERSIST_OK_MARKER == "RAYNU-V-M8-DISK-PERSIST-OK"
        && host_never_prints_iso_install_ok()
        && select_persist_kind(true, true) == PersistKind::File
        && select_persist_kind(false, true) == PersistKind::DurableLun
        && select_persist_kind(true, false) == PersistKind::LeftoverDram
        && kind_survives_hv_reboot(PersistKind::File)
        && kind_survives_hv_reboot(PersistKind::DurableLun)
        && !kind_survives_hv_reboot(PersistKind::LeftoverDram)
        && esp_copy_is_rejected()
        && ESP_COPY_REJECT_NOTE.contains("installdisk.bin")
        && UDISK_TOO_SMALL_NOTE.contains("994 MiB")
        && PERC_UBUNTU_UNTOUCHED_NOTE.contains("PERC")
        && PERSIST_EXCLUSIVE_OWNERSHIP_NOTE.contains("ADR-004")
        && M8_DISK_PERSIST_RESIDUAL_NOTE.contains("ISO-INSTALL-OK")
        && disk_persist_surface_present()
}

#[cfg(test)]
#[path = "m8_disk_persist_gate_test.rs"]
mod m8_disk_persist_gate_test;
