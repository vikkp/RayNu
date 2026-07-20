//! M5.8 host verification gate (NUMA in ghost *spec*).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + `memory/numa` (not a boot path).
//!
//! Checks that `ept_model` carries `GhostNumaTopology` / `numa_map_enabled` /
//! `mock_bringup_numa`, that `ept_spec` / `ept_proof` close the NUMA *spec* GAP,
//! that the iDRAC SRAT/SLIT mock feeds a well-formed host view, and that the
//! smoke script is present. Full NUMA affinity L3 remains GAP → M6.
//! Runtime `cargo verus verify -p ept_model` is exercised by
//! `tools/verus-numa-smoke.sh`.

use crate::memory::numa::{prop_mock_numa_runtime, M5_NUMA_OK_MARKER, NUMA_L3_GAP_NOTE};

/// Host / CI marker when the M5.8 NUMA-spec gate passes.
pub const M5_NUMA_GATE_MARKER: &str = M5_NUMA_OK_MARKER;

/// True when a non-comment source line is an `admit(` statement.
fn source_has_admit_call(s: &str) -> bool {
    for line in s.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("admit(") || t.starts_with("admit (") {
            return true;
        }
    }
    false
}

/// True when `ept_model` opts into Verus verification.
pub fn ept_model_opts_into_verus() -> bool {
    let cargo = include_str!("../ept_model/Cargo.toml");
    cargo.contains("[package.metadata.verus]")
        && cargo.contains("verify = true")
        && cargo.contains("vstd")
}

/// True when NUMA ghost *spec* artifacts are present (no admit; marker).
pub fn ept_model_has_numa_spec() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("struct GhostNumaTopology")
        && s.contains("numa_well_formed")
        && s.contains("slit_symmetric")
        && s.contains("numa_map_enabled")
        && s.contains("guest_frames_on_node")
        && s.contains("mock_bringup_numa")
        && s.contains("lemma_mock_bringup_numa_facts")
        && s.contains("lemma_numa_map_ok_exclusive")
        && s.contains("SLIT_LOCAL")
        && s.contains(M5_NUMA_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close NUMA TODO/GAP on the *spec* side.
pub fn ept_spec_closes_numa_todo() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M5.8 CLOSED): NUMA in ghost spec")
        && spec.contains("GhostNumaTopology")
        && proof.contains("GAP(CLOSED M5.8): NUMA in ghost spec")
        && proof.contains(NUMA_L3_GAP_NOTE)
        && proof.contains("mock_bringup_numa")
}

/// True when the NUMA smoke script is present.
pub fn numa_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-numa-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M5_NUMA_OK_MARKER)
        && smoke.contains("GhostNumaTopology")
        && smoke.contains("mock_bringup_numa")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Full M5.8 artifact + mock SRAT/SLIT runtime gate (does not run Verus).
pub fn run_m5_numa_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_numa_spec()
        && ept_spec_closes_numa_todo()
        && numa_scripts_present()
        && prop_mock_numa_runtime()
}

#[cfg(test)]
#[path = "m5_numa_gate_test.rs"]
mod m5_numa_gate_test;
