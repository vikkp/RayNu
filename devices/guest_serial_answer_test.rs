use super::{
    begin_second_boot, note_tx, queued, reset, second_boot, take_rx, BOOTLOADER, DISK, GRUB_ENTER,
    MOUNT_EXIT, NO, PROVE, REBOOT, ROOT, SETUP, SYS, YES,
};

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
    assert!(SETUP.len() <= 768);
    assert!(core::str::from_utf8(SETUP).unwrap().contains("KERNELOPTS="));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("console=ttyS0"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("earlycon=uart8250,io,0x3f8"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("efi=noruntime"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("/dev/vda"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("virtio_pci"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("sr_mod"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("isofs"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("modprobe -a"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("mdev -s"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("sleep 1"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("[ -b /dev/vda ]"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("mkdir -p /media/cdrom"));
    assert!(
        core::str::from_utf8(SETUP).unwrap().contains("for d in /media/*"),
        "discover nlplug mount (/media/vdb) before remount"
    );
    assert!(
        core::str::from_utf8(SETUP).unwrap().contains("{ mkdir"),
        "BusyBox ash needs space after {{ (nested 50ed61c unexpected }})"
    );
    assert!(
        core::str::from_utf8(SETUP).unwrap().contains("apks; }"),
        "BusyBox ash needs space before }}"
    );
    assert!(
        core::str::from_utf8(SETUP).unwrap().contains("/media/*/apks"),
        "ISO media repo is bare …/apks (apk appends arch; not …/apks/main)"
    );
    assert!(
        !core::str::from_utf8(SETUP).unwrap().contains("apks/main"),
        "4536b72 …/apks/main was a regression (mirror layout)"
    );
    assert!(core::str::from_utf8(SETUP).unwrap().contains("mount -t iso9660 /dev/vdb"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("||mount -t iso9660 /dev/sr0"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("/media/cdrom/apks"));
    assert!(
        core::str::from_utf8(SETUP).unwrap().contains("[ -d $R ]&&echo $R>/etc/apk/repositories"),
        "do not clobber live-init repos when discovery+mount fail"
    );
    assert!(!core::str::from_utf8(SETUP).unwrap().contains(">>"));
    assert!(
        !core::str::from_utf8(SETUP).unwrap().contains("apk update"),
        "setup-disk before apk update"
    );
    assert!(core::str::from_utf8(SETUP).unwrap().contains("BOOTLOADER=grub"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("USE_EFI=1"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("BOOT_SIZE=48"));
    assert!(core::str::from_utf8(SETUP).unwrap().contains("-s 0"));
    reset();
    assert_eq!(queued(), 0);
}

#[test]
fn emergency_shell_without_login_queues_mount_exit() {
    reset();
    for &b in b"/ # " {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, MOUNT_EXIT);
    assert!(core::str::from_utf8(MOUNT_EXIT).unwrap().contains("exit"));
    assert!(core::str::from_utf8(MOUNT_EXIT).unwrap().contains("for d in /media/*"));
    assert!(core::str::from_utf8(MOUNT_EXIT).unwrap().contains("/media/*/apks"));
    assert!(!core::str::from_utf8(MOUNT_EXIT).unwrap().contains("apks/main"));
    assert!(
        !core::str::from_utf8(MOUNT_EXIT).unwrap().contains("setup-disk"),
        "emergency mount+exit: initramfs has no setup-disk"
    );
    for &b in b"/ # " {
        note_tx(b);
    }
    assert_eq!(take_rx(), None, "MOUNT_EXIT once");
    for &b in b"localhost login:" {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, ROOT);
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, SETUP);
    reset();
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, SETUP);
    reset();
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
fn which_disk_queues_vda() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"Which disk" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, DISK);
    reset();
}

#[test]
fn like_to_use_queues_sys() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"How would you like to use it? ('sys', 'data' or 'lvm')" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, SYS);
    for &b in b"Which bootloader?" {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, BOOTLOADER);
    reset();
}

#[test]
fn which_disk_would_you_like_does_not_queue_sys() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"Which disk(s) would you like to use?" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, DISK);
    reset();
}

#[test]
fn no_disks_available_answers_n_not_y() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"No disks available. Try boot media? (y/n)" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, NO);
    reset();
}

#[test]
fn please_reboot_then_shell_queues_reboot_once() {
    reset();
    for &b in b"login:" {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    while take_rx().is_some() {}
    for &b in b"Installation is complete. Please reboot." {
        note_tx(b);
    }
    assert_eq!(queued(), 0, "Please reboot itself does not enqueue");
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, REBOOT);
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    assert_eq!(take_rx(), None, "reboot once");
    reset();
}

#[test]
fn please_reboot_after_bootloader_prompt() {
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
    while take_rx().is_some() {}
    for &b in b"Please reboot." {
        note_tx(b);
    }
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, REBOOT);
    reset();
}

#[test]
fn second_boot_answers_root_then_prove_never_setup() {
    begin_second_boot();
    assert!(second_boot());
    for &b in b"localhost login:" {
        note_tx(b);
    }
    let mut got = Vec::new();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, ROOT);
    for &b in b"localhost:~# " {
        note_tx(b);
    }
    got.clear();
    while let Some(b) = take_rx() {
        got.push(b);
    }
    assert_eq!(got, PROVE);
    assert!(core::str::from_utf8(PROVE).unwrap().contains("/proc/cmdline"));
    assert!(!got.windows(SETUP.len()).any(|w| w == SETUP));
    reset();
    assert!(!second_boot());
}

#[test]
fn never_prints_iso_install_ok() {
    let s = include_str!("guest_serial_answer.rs");
    assert!(!s.contains("RAYNU-V-M7-ISO-INSTALL-OK"));
    assert!(s.contains("fn note_tx("));
    let _ = YES;
}
