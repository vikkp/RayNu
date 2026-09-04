//! RayNu-F scaffold gate (ADR-016). Host-only. Asserts the honesty invariants:
//! the subsystem is declared, outside the Proven Core, and does **not** yet
//! run boot services or boot an ISO. Prints the scaffold marker (not the iron
//! `ISO-INSTALL-OK`).

use super::{
    raynu_f_boots_iso, raynu_f_is_functional, raynu_f_mutates_foreign_firmware_state,
    raynu_f_planned_protocol_count, GuestFwProtocol, RAYNU_F_PLANNED_PROTOCOLS,
    RAYNU_F_RESIDUAL_NOTE, RAYNU_F_SCAFFOLD_OK_MARKER,
};

#[test]
fn raynu_f_scaffold_is_honest() {
    // Marker is a scaffold marker, not the iron close.
    assert_eq!(RAYNU_F_SCAFFOLD_OK_MARKER, "RAYNU-V-RAYNU-F-SCAFFOLD-OK");
    assert_ne!(RAYNU_F_SCAFFOLD_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");

    // Not functional, does not boot an ISO, never puppets foreign firmware.
    assert!(!raynu_f_is_functional());
    assert!(!raynu_f_boots_iso());
    assert!(!raynu_f_mutates_foreign_firmware_state());

    // The planned boot-service surface is declared and ordered by dependency:
    // the system table comes first, LoadImage/StartImage last.
    assert_eq!(raynu_f_planned_protocol_count(), 7);
    assert_eq!(RAYNU_F_PLANNED_PROTOCOLS.len(), 7);
    assert_eq!(RAYNU_F_PLANNED_PROTOCOLS[0], GuestFwProtocol::SystemTable);
    assert_eq!(
        RAYNU_F_PLANNED_PROTOCOLS[6],
        GuestFwProtocol::LoadStartImage
    );
    assert!(RAYNU_F_PLANNED_PROTOCOLS.contains(&GuestFwProtocol::BlockIo));
    assert!(RAYNU_F_PLANNED_PROTOCOLS.contains(&GuestFwProtocol::TimerTick));

    // Residual note is honest about scope and the ADR-016 governance rule.
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("ADR-016"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("No third-party firmware state mutation"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("not ISO-INSTALL-OK"));
    assert!(RAYNU_F_RESIDUAL_NOTE.contains("2b795a0"));

    // Scaffold gate marker (host/CI only).
    #[cfg(not(target_os = "uefi"))]
    println!("{RAYNU_F_SCAFFOLD_OK_MARKER}");
}
