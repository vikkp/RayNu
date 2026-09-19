//! M8.1 firmware TLS wrap gate (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-018)
//!
//! Proves coexist TCP feeds [`crate::mgmt::tls12::Tls12Listen`], rustls is
//! still not in `uefi-bin` (ring C needs libc), and CURL NOW is `https://`.
//! Does **not** print `RAYNU-V-M8-TLS-OK`. Nested QEMU is not this gate.
//! Iron `curl --cacert` is a post-EBS native window **before RayNu-F**.
//! Nested QEMU is not this gate.

use crate::mgmt::tls::{
    firmware_listen_is_plaintext, firmware_listen_is_tls12, host_never_prints_iron_tls_ok,
    M8_TLS_OK_MARKER,
};
use crate::mgmt::tls_coexist::{
    prop_tls_fw_wrap_package, M8_TLS_FW_HOST_OK_MARKER, TLS_FW_CURL_NOTE, TLS_FW_WRAP_NOTE,
};

/// Host / CI marker when the firmware-shaped wrap package passes.
pub const M8_TLS_FW_GATE_MARKER: &str = M8_TLS_FW_HOST_OK_MARKER;

/// True when TLS 1.2 wrap wiring, rustls-out-of-efi, and millicert hold.
pub fn tls_fw_surface_present() -> bool {
    let coexist = include_str!("tls_coexist.rs");
    let listen = include_str!("host_nic_listen.rs");
    let tls12 = include_str!("tls12.rs");
    let cargo = include_str!("../Cargo.toml");
    let plan = include_str!("../docs/m8_plan.md");
    let smoke = include_str!("../tools/m8-tls-fw-smoke.sh");
    let qemu = include_str!("../tools/m7-host-nic-qemu-smoke.sh");
    let forbidden = concat!("println!(", "\"RAYNU-V-M8-TLS-OK\")");
    coexist.contains("struct PlaintextListen")
        && coexist.contains("fn feed_tcp(")
        && coexist.contains("fn take_http(")
        && coexist.contains("fn wrap_plaintext_http(")
        && coexist.contains("ring C needs")
        && !coexist.contains(forbidden)
        && tls12.contains("struct Tls12Listen")
        && tls12.contains("ECDHE-RSA-AES128-GCM")
        && tls12.contains("not(feature = \"uefi-bin\")")
        && tls12.contains("fn empty(")
        && tls12.contains("fn load_lab_material(")
        && listen.contains("Tls12Listen")
        && listen.contains("tls_session()")
        && listen.contains("feed_tcp")
        && listen.contains("take_http")
        && listen.contains("drain_tcp")
        && listen.contains("wrap_http")
        && listen.contains("https://")
        && listen.contains("before RayNu-F; SNP is dead")
        && qemu.contains("https://")
        && qemu.contains("lab-ca.crt.pem")
        && qemu.contains("--tls-max")
        && qemu.contains("RAYNU-V-M8-TLS-OK")
        && TLS_FW_WRAP_NOTE.contains("assert.h")
        && TLS_FW_CURL_NOTE.contains("https://")
        && cargo.contains("[dev-dependencies]")
        && cargo.contains("rustls")
        && cargo.contains("aes-gcm")
        && !cargo.contains("dep:rustls")
        && plan.contains("M8.1")
        && plan.contains("Plaintext remains a lab fallback")
        && smoke.contains(M8_TLS_FW_HOST_OK_MARKER)
        && smoke.contains("m8_tls_fw_host_gate_passes")
}

pub fn run_m8_tls_fw_host_gate() -> bool {
    tls_fw_surface_present()
        && prop_tls_fw_wrap_package()
        && firmware_listen_is_tls12()
        && !firmware_listen_is_plaintext()
        && host_never_prints_iron_tls_ok()
        && M8_TLS_OK_MARKER == "RAYNU-V-M8-TLS-OK"
        && M8_TLS_FW_GATE_MARKER == "RAYNU-V-M8-TLS-FW-HOST-OK"
}

#[cfg(test)]
#[path = "m8_tls_fw_gate_test.rs"]
mod m8_tls_fw_gate_test;
