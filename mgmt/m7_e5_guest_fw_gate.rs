//! E5 Stage 3 — guest UEFI firmware envelope (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-003 / ADR-014)
//! VERIFICATION: N/A
//!
//! Host/CI: `box_guest_firmware` validates the PE `.asguefw` envelope under
//! ADR-003 caps. `attach_cdrom_uefi` stays `UnsupportedOnFirmware`. The
//! placeholder is not OVMF. E4 SHELL VMLAUNCH is unchanged. Do not claim
//! Everest E5 / ISO-INSTALL-OK.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_fw::{
    box_guest_firmware, dispatch_guest_fw_rest, guest_fw_bytes, guest_fw_is_boxed, parse_guest_fw,
    reset_guest_fw, write_guest_fw_header, ADR003_GUEST_FW, GUEST_FW_HEADER_LEN,
    GUEST_FW_MAX_COMPRESSED, GUEST_FW_MAX_UNCOMPRESSED, SECTION_GUEST_FW,
};
use super::iso::{attach_cdrom_uefi, IsoError};
use super::m7_e5_cdrom_attach_gate::e4_shell_launch_no_cdrom;
use super::m7_e5_cdrom_firmware_gate::run_m7_e5_cdrom_firmware_gate;
use super::VmTable;

/// Host / CI marker when the E5 Stage 3 guest-firmware envelope gate passes.
pub const M7_E5_GUEST_FW_OK_MARKER: &str = "RAYNU-V-M7-E5-GUEST-FW-OK";

/// Honest residual: boxed envelope ≠ OVMF ≠ VMLAUNCH.
pub const E5_GUEST_FW_RESIDUAL_NOTE: &str =
    "residual: guest FW envelope is ADR-003 boxed; attach_cdrom_uefi stays UnsupportedOnFirmware; not OVMF; no guest UEFI VMLAUNCH";

/// Embedded placeholder parses and boxes under ADR-003 caps.
pub fn prop_guest_fw_box_embedded() -> bool {
    reset_guest_fw();
    let bytes = guest_fw_bytes();
    let parsed = match parse_guest_fw(bytes) {
        Ok(p) => p,
        Err(_) => return false,
    };
    if parsed.payload_len == 0 || parsed.boxed {
        return false;
    }
    if parsed.uncompressed_len > GUEST_FW_MAX_UNCOMPRESSED
        || parsed.compressed_len > GUEST_FW_MAX_COMPRESSED
    {
        return false;
    }
    match box_guest_firmware(bytes) {
        Ok(b) => b.boxed && guest_fw_is_boxed(),
        Err(_) => false,
    }
}

/// Oversized / bad-magic envelopes reject. UEFI stub stays honest.
pub fn prop_guest_fw_box_rejects() -> bool {
    reset_guest_fw();
    let mut oversize = [0u8; GUEST_FW_HEADER_LEN];
    if write_guest_fw_header(
        &mut oversize,
        GUEST_FW_MAX_UNCOMPRESSED + 1,
        GUEST_FW_MAX_COMPRESSED,
        0,
    )
    .is_err()
    {
        return false;
    }
    if parse_guest_fw(&oversize).is_ok() {
        return false;
    }
    let bad = [0u8; GUEST_FW_HEADER_LEN];
    if parse_guest_fw(&bad).is_ok() {
        return false;
    }
    attach_cdrom_uefi(1) == Err(IsoError::UnsupportedOnFirmware)
}

/// Bearer REST: box then GET count=1. No token → 401. `iso=0` SHELL stays 201.
pub fn prop_rest_guest_fw_box() -> bool {
    reset_guest_fw();
    let tok = Some(BRINGUP_AUTH_TOKEN);

    let before = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw",
        auth_token: tok,
    });
    if before.status != 200 {
        return false;
    }

    let boxed = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: tok,
    });
    if boxed.status != 201 || !guest_fw_is_boxed() {
        return false;
    }

    let st = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Get,
        path: "/fw",
        auth_token: tok,
    });
    if st.status != 200 {
        return false;
    }

    let denied = dispatch_guest_fw_rest(RestRequest {
        method: RestMethod::Post,
        path: "/fw/box",
        auth_token: None,
    });
    if denied.status != 401 {
        return false;
    }

    let mut t = VmTable::new();
    let shell = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1/spec/1/512/1024/0",
            auth_token: tok,
        },
    );
    shell.status == 201 && t.get(1).map(|r| r.iso_id) == Some(0)
}

/// SPA + ADR-014 Stage 3 phrases. Envelope is not OVMF.
pub fn guest_fw_surface_present() -> bool {
    let spa = include_str!("../assets/webui.html");
    let adr = include_str!("../docs/adr/ADR-014.md");
    let adr003 = include_str!("../docs/adr/ADR-003.md");
    let http = include_str!("http.rs");
    let src = include_str!("guest_fw.rs");
    let launch = include_str!("../vmx/launch.rs");
    spa.contains("Box guest FW")
        && spa.contains("not OVMF")
        && spa.contains("UEFI-first")
        && spa.contains("extract-boot is lab")
        && spa.contains("not guest UEFI")
        && crate::mgmt::webui::webui_len().saturating_add(256) <= 16384
        && http.contains("is_guest_fw_path")
        && adr.contains("Stage 3")
        && adr.contains("box_guest_firmware")
        && adr003.contains(ADR003_GUEST_FW)
        && src.contains("link_section = \".asguefw\"")
        && SECTION_GUEST_FW == ".asguefw"
        && !launch.contains("box_guest_firmware")
        && !launch.contains("GuestFwBlob")
        && !launch.contains("asguefw")
        && e4_shell_launch_no_cdrom()
}

/// Full E5 Stage 3 package. Host gate only — not iron, not Everest E5.
pub fn run_m7_e5_guest_fw_gate() -> bool {
    let _ = (M7_E5_GUEST_FW_OK_MARKER, E5_GUEST_FW_RESIDUAL_NOTE);
    reset_guest_fw();
    let ok = prop_guest_fw_box_embedded()
        && prop_guest_fw_box_rejects()
        && prop_rest_guest_fw_box()
        && guest_fw_surface_present()
        && run_m7_e5_cdrom_firmware_gate()
        && E5_GUEST_FW_RESIDUAL_NOTE.contains("UnsupportedOnFirmware")
        && E5_GUEST_FW_RESIDUAL_NOTE.contains("not OVMF")
        && M7_E5_GUEST_FW_OK_MARKER == "RAYNU-V-M7-E5-GUEST-FW-OK";
    reset_guest_fw();
    ok
}

#[cfg(test)]
#[path = "m7_e5_guest_fw_gate_test.rs"]
mod m7_e5_guest_fw_gate_test;
