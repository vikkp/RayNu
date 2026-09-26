//! M8.7 host gate (outside Proven Core).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves the frame pack, the spare size fence, the read-only fwstate gate,
//! and that host/CI never print `RAYNU-V-M8-PERC-LUN-OK`. No doorbell.
//! `pick_durable_lun` still skips PERC.

use crate::mgmt::megaraid::{
    host_never_prints_iron_perc_ok, prop_perc_host_package, M8_PERC_HOST_OK_MARKER,
    M8_PERC_LUN_OK_MARKER, PERC_HOST_RESIDUAL_NOTE,
};

/// Host / CI marker when the M8.7 pack passes.
pub const M8_PERC_GATE_MARKER: &str = M8_PERC_HOST_OK_MARKER;

/// True when the plan, the fence, and the still-skipped PERC census agree.
pub fn perc_surface_present() -> bool {
    let mega = include_str!("megaraid.rs");
    let lun = include_str!("durable_lun.rs");
    let plan = include_str!("../docs/m8_plan.md");
    let handoff = include_str!("../boot/handoff.rs");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-PERC-LUN-OK\")");
    mega.contains("fn pack_ld_get_list(")
        && mega.contains("fn pack_ld_read16(")
        && mega.contains("fn pick_spare(")
        && mega.contains("fn fw_state_allows_mailbox(")
        && mega.contains("fn fwstate_may_load(")
        && mega.contains("fn pci_cmd_for_fwstate_load(")
        && mega.contains("fn memory_bar64(")
        && mega.contains("fn perc_fwstate_probe(")
        && mega.contains("fn dcmd_is_allowed(")
        && mega.contains("fn adapter_reset_is_allowed(")
        && mega.contains("boot: perc fwstate")
        && mega.contains("not PERC-LUN-OK")
        && mega.contains(M8_PERC_LUN_OK_MARKER)
        && mega.contains(M8_PERC_HOST_OK_MARKER)
        && !mega.contains(forbidden)
        && !mega.contains("pci_write32(bus, dev, func, 0x00")
        && !mega.contains("pci_write32(bus, dev, func, 0x10")
        && !mega.contains("pci_write32(bus, dev, func, 0x20")
        && !lun.contains("fn pack_ld_get_list(")
        && !lun.contains("perc_fwstate_probe")
        && !lun.contains(forbidden)
        && lun.contains("skip PERC")
        && handoff.contains("perc_fwstate_probe()")
        && plan.contains("M8.7")
        && plan.contains(M8_PERC_HOST_OK_MARKER)
        && plan.contains("skip PERC")
        && PERC_HOST_RESIDUAL_NOTE.contains("no doorbell")
        && PERC_HOST_RESIDUAL_NOTE.contains("not iron")
        && PERC_HOST_RESIDUAL_NOTE.contains("skip PERC")
}

/// Full M8.7 host artifact gate. Not an iron read.
pub fn run_m8_perc_host_gate() -> bool {
    perc_surface_present()
        && prop_perc_host_package()
        && host_never_prints_iron_perc_ok()
        && M8_PERC_LUN_OK_MARKER == "RAYNU-V-M8-PERC-LUN-OK"
        && M8_PERC_GATE_MARKER == "RAYNU-V-M8-PERC-HOST-OK"
}

#[cfg(test)]
#[path = "m8_perc_gate_test.rs"]
mod m8_perc_gate_test;
