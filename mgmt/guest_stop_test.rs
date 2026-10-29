//! Host tests for the last-stop record (ADR-021 B20).

use super::{kind_from_exit, kind_from_raynu_f, last, note_exit, note_operator_if_empty, note_raynu_f, reset_for_test, StopKind};

#[test]
fn exit_kind_follows_tf_then_entry_then_cap() {
    assert_eq!(kind_from_exit(true, true, 99, 10), StopKind::TripleFault);
    assert_eq!(kind_from_exit(false, true, 99, 10), StopKind::EntryFail);
    assert_eq!(kind_from_exit(false, false, 10, 10), StopKind::ExitCap);
    assert_eq!(kind_from_exit(false, false, 9, 10), StopKind::Unhandled);
    assert_eq!(kind_from_exit(false, false, 9, 0), StopKind::Unhandled);
}

#[test]
fn raynu_f_why_maps_onto_a_sentence() {
    assert_eq!(kind_from_raynu_f("spa-stop"), StopKind::OperatorStop);
    assert_eq!(kind_from_raynu_f("exit-cap"), StopKind::ExitCap);
    assert_eq!(kind_from_raynu_f("reset-cap"), StopKind::ExitCap);
    assert_eq!(kind_from_raynu_f("wall-cap"), StopKind::WallCap);
    assert_eq!(kind_from_raynu_f("triple-fault"), StopKind::TripleFault);
    assert_eq!(kind_from_raynu_f("entry-fail"), StopKind::EntryFail);
    assert_eq!(kind_from_raynu_f("hlt"), StopKind::Unhandled);
    assert_eq!(
        StopKind::WallCap.sentence(),
        "Wall-clock cap reached."
    );
    assert!(!StopKind::OperatorStop.sentence().contains('"'));
}

#[test]
fn operator_idle_does_not_clobber_a_recorded_stop() {
    reset_for_test();
    assert!(last().is_none());
    note_raynu_f("wall-cap", 0x1c, 0xffff_fe94, 40, 1 << 30, 0);
    note_operator_if_empty();
    let rec = last().unwrap();
    assert_eq!(rec.kind, StopKind::WallCap);
    assert_eq!(rec.reason, 0x1c);
    assert_eq!(rec.rip, 0xffff_fe94);
    note_exit(0x1c, 0x1000, 8, 8, false, false, 0);
    assert_eq!(last().unwrap().kind, StopKind::ExitCap);
    reset_for_test();
    note_operator_if_empty();
    assert_eq!(last().unwrap().kind, StopKind::OperatorStop);
    reset_for_test();
}
