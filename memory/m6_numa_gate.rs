//! M6.2 host verification gate (NUMA affinity L3).
//!
//! Pillar: [V]
//! Proven Core: companion to `ept_model` + `memory/numa` (not a boot path).
//!
//! Checks that `ept_model` discharges `theorem_numa_map_unmap_affinity`
//! (no `admit`), embeds `RAYNU-V-M6-NUMA-L3-OK`, that `ept_proof` / `ept_spec`
//! close the NUMA affinity L3 GAP, and that the host mock affinity prop holds.
//! Runtime verify is exercised by `tools/verus-numa-l3-smoke.sh`.

use crate::memory::numa::{prop_numa_affinity_l3, M6_NUMA_L3_OK_MARKER};

/// Host / CI marker when the M6.2 NUMA affinity L3 gate passes.
pub const M6_NUMA_L3_GATE_MARKER: &str = M6_NUMA_L3_OK_MARKER;

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

/// True when NUMA affinity L3 artifacts are present (no admit; marker).
pub fn ept_model_has_numa_l3() -> bool {
    let s = include_str!("../ept_model/src/lib.rs");
    s.contains("guest_frames_on_node")
        && s.contains("lemma_numa_map_establishes_affinity")
        && s.contains("lemma_numa_unmap_preserves_affinity")
        && s.contains("theorem_numa_map_unmap_affinity")
        && s.contains("lemma_mock_numa_map_unmap_affinity")
        && s.contains("lemma_empty_guest_frames_on_node")
        && s.contains(M6_NUMA_L3_OK_MARKER)
        && !source_has_admit_call(s)
}

/// True when ept_spec / ept_proof close the NUMA affinity L3 GAP.
pub fn ept_spec_closes_numa_l3() -> bool {
    let spec = include_str!("ept_spec.rs");
    let proof = include_str!("ept_proof.rs");
    spec.contains("TODO(M6.2 CLOSED): NUMA affinity / exclusivity L3")
        && spec.contains("theorem_numa_map_unmap_affinity")
        && proof.contains("GAP(CLOSED M6.2): NUMA affinity / exclusivity L3")
        && proof.contains("theorem_numa_map_unmap_affinity")
        && !proof.contains("GAP: NUMA affinity / exclusivity L3 (M6)")
}

/// True when the NUMA-L3 smoke script is present.
pub fn numa_l3_scripts_present() -> bool {
    let smoke = include_str!("../tools/verus-numa-l3-smoke.sh");
    smoke.contains("cargo verus verify -p ept_model")
        && smoke.contains(M6_NUMA_L3_OK_MARKER)
        && smoke.contains("theorem_numa_map_unmap_affinity")
        && smoke.contains("install-verus.sh")
        && smoke.contains("0 errors")
}

/// Full M6.2 artifact + host affinity gate (does not run Verus).
pub fn run_m6_numa_gate() -> bool {
    ept_model_opts_into_verus()
        && ept_model_has_numa_l3()
        && ept_spec_closes_numa_l3()
        && numa_l3_scripts_present()
        && prop_numa_affinity_l3()
}

#[cfg(test)]
#[path = "m6_numa_gate_test.rs"]
mod m6_numa_gate_test;
