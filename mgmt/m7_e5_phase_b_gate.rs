//! Phase B host wire — SPA/REST product-ISO start → RayNu-F (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-014 / ADR-016)
//! VERIFICATION: N/A (host `include_str!` + unit tests)
//!
//! Host/CI: `POST /vms/{id}/start` of a typed product ISO queues RayNu-F
//! instead of the E4 SHELL stub. `iso=0` stays SHELL. Never prints
//! `RAYNU-V-M7-ISO-INSTALL-OK`. Iron Phase B closed on COM2 `f72b4276` /
//! `--run 34552377351` (SPA Start of RayNu-F ISO, no `raynuf.txt`).

use super::api::{dispatch_rest, RestMethod, RestRequest, BRINGUP_AUTH_TOKEN};
use super::guest_image::GuestImageType;
use super::spa_launch::{
    kind_for_record, prop_spa_kind_for_record, prop_spa_start_queues, take_spa_start_kind,
    SpaStartKind, M7_PHASE_B_SPA_RAYNU_F_NOTE, M7_PHASE_B_SPA_WIRE_OK_MARKER,
};
use super::VmTable;
use crate::boot::raynu_f_flag;

/// Host / CI marker. Iron close is COM2 `f72b4276` / `34552377351`, not this string.
pub const M7_PHASE_B_SPA_WIRE_GATE_MARKER: &str = "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK";

/// Honest residual after iron P0-63 close: polish, not “SPA still SHELL”.
pub const PHASE_B_RESIDUAL_NOTE: &str =
    "residual: P0-63 iron CLOSED f72b4276 / 34552377351 — SPA Start of RayNu-F ISO without raynuf.txt; HOST-NIC-HTTP-OK on 10.99.99.145:8443; ISO-INSTALL-OK (iron-only) + DISK-BOOT-OK; iso=0 stays E4 SHELL; product ISO without raynuf.txt skips OVMF to coexist HTTP (not CpuSleep ticks, not Stage 46 hold, not G0 BAR/shell); TSC Instant not MILLIS+=10 (7f8dc0a9 curl: (7) fixed); leftover-DRAM disk does not survive HV reboot; TLS deferred; ISO-INSTALL-OK is never printed from host/CI";

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
    let iso = include_str!("iso_install.rs");
    let listen = include_str!("host_nic_listen.rs");
    let main = include_str!("../src/main.rs");
    api.contains("note_spa_start_kind")
        && api.contains("SpaStartKind::RayNuF")
        && api.contains("request_from_spa")
        && launch.contains("take_spa_start_kind")
        && launch.contains("SpaStartKind::RayNuF")
        && launch.contains("try_spa_product_iso_start")
        && launch.contains("M7_PHASE_B_SPA_RAYNU_F_NOTE")
        && guest.contains("fn try_spa_product_iso_start")
        && guest.contains("fn guest_uefi_phase_b_skip_ovmf_to_e4")
        && guest.contains("M7_E5_PHASE_B_SKIP_OVMF_NOTE")
        && guest.contains("phase_b_continue_e4_for_spa")
        && iso.contains("fn phase_b_continue_e4_for_spa")
        && iso.contains("fn phase_b_e4_for_spa")
        && main.contains("M7_PHASE_B_E4_CONTINUE_OK_MARKER")
        && main.contains("enter_phase_b_coexist_idle")
        && launch.contains("fn enter_phase_b_coexist_idle")
        && listen.contains("coexist_millis_from_tsc")
        && !listen.contains("saturating_add(10)")
        && iso.contains("M7_PHASE_B_COEXIST_IDLE_NOTE")
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
    PHASE_B_RESIDUAL_NOTE.contains("CLOSED")
        && PHASE_B_RESIDUAL_NOTE.contains("f72b4276")
        && PHASE_B_RESIDUAL_NOTE.contains("iso=0")
        && PHASE_B_RESIDUAL_NOTE.contains("curl: (7)")
        && PHASE_B_RESIDUAL_NOTE.contains("TSC Instant")
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
