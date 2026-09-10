//! Phase B host wire — SPA/REST product-ISO start → RayNu-F (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014 / ADR-016)
//! VERIFICATION: N/A (host `include_str!` + unit tests)
//!
//! Host/CI: `POST /vms/{id}/start` of a typed product ISO queues RayNu-F
//! instead of the E4 SHELL stub. `iso=0` stays SHELL. Never prints
//! `RAYNU-V-M7-ISO-INSTALL-OK`. Iron Phase B (SPA start on COM2) is not claimed.

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::spa_launch::{
    kind_for_record, prop_spa_kind_for_record, prop_spa_start_queues, take_spa_start_kind,
    SpaStartKind, M7_PHASE_B_SPA_RAYNU_F_NOTE, M7_PHASE_B_SPA_WIRE_OK_MARKER,
};
use super::guest_image::GuestImageType;
use super::VmTable;
use crate::boot::raynu_f_flag;

/// Host / CI marker. Not an iron close.
pub const M7_PHASE_B_SPA_WIRE_GATE_MARKER: &str = "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK";

/// Honest residual: host wire ≠ iron SPA start of the installed disk.
pub const PHASE_B_RESIDUAL_NOTE: &str =
    "residual: host SPA/REST product-ISO start queues RayNu-F (P0-63 wire); iron Phase B is not claimed — COM2 must show SPA start launching RayNu-F without the ESP raynuf.txt auto-path; iso=0 stays E4 SHELL; ISO-INSTALL-OK is never printed from host/CI";

/// REST create+start of `linux_iso` queues RayNu-F and arms the flag.
pub fn prop_rest_product_iso_start_queues_raynu_f() -> bool {
    let _ = take_spa_start_kind();
    raynu_f_flag::clear_request();
    let mut t = VmTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/4/spec/2/2048/10240/1/linux_iso",
            auth_token: tok,
        },
    );
    if created.status != 201 {
        raynu_f_flag::clear_request();
        return false;
    }
    let started = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/4/start",
            auth_token: tok,
        },
    );
    let kind = take_spa_start_kind();
    let armed = raynu_f_flag::requested();
    raynu_f_flag::clear_request();
    started.status == 200 && kind == Some((4, SpaStartKind::RayNuF)) && armed
}

/// REST start of `iso=0` still queues SHELL and does not arm RayNu-F.
pub fn prop_rest_iso0_start_queues_shell() -> bool {
    let _ = take_spa_start_kind();
    raynu_f_flag::clear_request();
    let mut t = VmTable::new();
    let tok = Some(BRINGUP_AUTH_TOKEN);
    let created = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1/spec/1/512/1024/0",
            auth_token: tok,
        },
    );
    if created.status != 201 {
        return false;
    }
    let started = dispatch_rest(
        &mut t,
        RestRequest {
            method: RestMethod::Post,
            path: "/vms/1/start",
            auth_token: tok,
        },
    );
    let kind = take_spa_start_kind();
    let armed = raynu_f_flag::requested();
    raynu_f_flag::clear_request();
    started.status == 200 && kind == Some((1, SpaStartKind::Shell)) && !armed
}

/// Surfaces exist: API, scheduler, SPA HTML, RayNu-F launch entry.
pub fn phase_b_surface_present() -> bool {
    let api = include_str!("api.rs");
    let launch = include_str!("../vmx/launch.rs");
    let guest = include_str!("../vmx/guest_uefi.rs");
    let flag = include_str!("../boot/raynu_f_flag.rs");
    let spa = include_str!("spa_launch.rs");
    let html = include_str!("../assets/webui.html");
    api.contains("note_spa_start_kind")
        && api.contains("SpaStartKind::RayNuF")
        && api.contains("request_from_spa")
        && launch.contains("take_spa_start_kind")
        && launch.contains("SpaStartKind::RayNuF")
        && launch.contains("try_spa_product_iso_start")
        && launch.contains("M7_PHASE_B_SPA_RAYNU_F_NOTE")
        && guest.contains("fn try_spa_product_iso_start")
        && guest.contains("Phase B")
        && flag.contains("fn request_from_spa")
        && spa.contains("enum SpaStartKind")
        && spa.contains(M7_PHASE_B_SPA_RAYNU_F_NOTE)
        && html.contains("data-raynu-phase-b")
        && html.contains("RayNu-F")
        && !api.contains("RAYNU-V-M7-ISO-INSTALL-OK")
        && kind_for_record(1, Some(GuestImageType::LinuxIso), 64) == SpaStartKind::RayNuF
        && kind_for_record(0, None, 1024) == SpaStartKind::Shell
}

pub fn run_m7_phase_b_spa_wire_gate() -> bool {
    PHASE_B_RESIDUAL_NOTE.contains("not claimed")
        && PHASE_B_RESIDUAL_NOTE.contains("iso=0")
        && M7_PHASE_B_SPA_WIRE_GATE_MARKER == M7_PHASE_B_SPA_WIRE_OK_MARKER
        && M7_PHASE_B_SPA_WIRE_OK_MARKER == "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK"
        && prop_spa_start_queues()
        && prop_spa_kind_for_record()
        && prop_rest_iso0_start_queues_shell()
        && prop_rest_product_iso_start_queues_raynu_f()
        && phase_b_surface_present()
        && M7_PHASE_B_SPA_RAYNU_F_NOTE.contains("not SHELL")
        && M7_PHASE_B_SPA_RAYNU_F_NOTE.contains("not ISO-INSTALL-OK")
}

#[cfg(test)]
#[path = "m7_e5_phase_b_gate_test.rs"]
mod m7_e5_phase_b_gate_test;
