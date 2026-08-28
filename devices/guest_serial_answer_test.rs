use super::{note_tx, queued, reset, take_rx, BOOTLOADER, GRUB_ENTER, ROOT, SETUP, YES};

#[test]
fn login_queues_root_then_setup_disk() {
    reset();
    assert_eq!(queued(), 0);
    for &b in b"localhost login:" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, ROOT);
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"/ # " {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, SETUP);
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, SETUP);
    assert!(SETUP.len() <= 96);
    assert!(core::str::from_utf8(SETUP).unwrap().contains("/dev/vda"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("BOOTLOADER=grub"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("USE_EFI=1"));
    reset();
    assert_eq!(queued(), 0);
}

#[test]
fn gnu_grub_queues_enter_once() {
    reset();
    for &b in b"GNU GRUB" {
        note_tx(b);
    }
    assert_eq!(take_rx(), Some(GRUB_ENTER[0]));
    assert_eq!(queued(), 0);
    for &b in b"GNU GRUB" {
        note_tx(b);
    }
    assert_eq!(take_rx(), None);
    reset();
}

#[test]
fn confirm_queues_yes_then_stops() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for _ in 0..6 {
        for &b in b"(y/n)" {
            note_tx(b);
        }
    }
    let mut n = 0u32;
    while take_rx() == Some(b'y') {
        assert_eq!(take_rx(), Some(b'\r'));
        n += 1;
    }
    assert_eq!(n, 4);
    assert_eq!(queued(), 0);
    for &b in b"Which bootloader?" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, BOOTLOADER);
    reset();
}

#[test]
fn bootloader_prompt_queues_grub() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"Which bootloader?" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, BOOTLOADER);
    reset();
}

#[test]
fn confirm_bracket_yn_queues_yes() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"WARNING: Erase the above disk(s) and continue? [y/N]: " {
        note_tx(b);
    }
    assert_eq!(take_rx(), Some(b'y'));
    assert_eq!(take_rx(), Some(b'\r'));
    assert_eq!(queued(), 0);
    reset();
}

#[test]
fn never_prints_iso_install_ok() {
    let s = include_str!("guest_serial_answer.rs");
    assert!(!s.contains("RAYNU-V-M7-ISO-INSTALL-OK"));
    assert!(s.contains("fn note_tx("));
    let _ = YES;
}
