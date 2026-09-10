use super::{
    kind_for_record, prop_spa_kind_for_record, prop_spa_start_queues, SpaStartKind,
    M7_E4_SPA_LAUNCH_OK_MARKER, M7_PHASE_B_SPA_RAYNU_F_NOTE, M7_PHASE_B_SPA_WIRE_OK_MARKER,
};
use crate::mgmt::guest_image::GuestImageType;

#[test]
fn spa_start_queues_token() {
    assert!(prop_spa_start_queues());
    assert_eq!(M7_E4_SPA_LAUNCH_OK_MARKER, "RAYNU-V-M7-E4-SPA-LAUNCH-OK");
}

#[test]
fn spa_product_iso_is_raynu_f_shell_is_iso0() {
    assert!(prop_spa_kind_for_record());
    assert_eq!(
        kind_for_record(1, Some(GuestImageType::LinuxIso), 10240),
        SpaStartKind::RayNuF
    );
    assert_eq!(kind_for_record(0, None, 1024), SpaStartKind::Shell);
    assert_eq!(M7_PHASE_B_SPA_WIRE_OK_MARKER, "RAYNU-V-M7-PHASE-B-SPA-WIRE-OK");
    assert!(M7_PHASE_B_SPA_RAYNU_F_NOTE.contains("Phase B"));
    assert!(M7_PHASE_B_SPA_RAYNU_F_NOTE.contains("not SHELL"));
    assert!(M7_PHASE_B_SPA_RAYNU_F_NOTE.contains("not ISO-INSTALL-OK"));
}
