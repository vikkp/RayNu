use super::*;
use crate::mgmt::datastore::ImageTable;
use crate::mgmt::iso::{register_iso, DEFAULT_INSTALL_DISK_BYTES};

#[test]
fn markers_stable() {
    assert_eq!(M7_ISO_INSTALL_OK_MARKER, "RAYNU-V-M7-ISO-INSTALL-OK");
    assert_eq!(
        M7_ISO_INSTALL_SCAFFOLD_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK"
    );
    assert_eq!(LAB_INSTALL_DISK_BYTES, 1024 * 1024);
    assert_eq!(M7_ISO_DISK_WRITTEN_MARKER, "RAYNU-V-M7-ISO-DISK-WRITTEN");
    assert_eq!(
        M7_ISO_INSTALL_LAB_OK_MARKER,
        "RAYNU-V-M7-ISO-INSTALL-LAB-OK"
    );
    assert_eq!(
        M7_ISO_REBOOT_PENDING_MARKER,
        "RAYNU-V-M7-ISO-REBOOT-PENDING"
    );
    assert_eq!(
        M7_ISO_BOOTED_FROM_DISK_MARKER,
        "RAYNU-V-M7-ISO-BOOTED-FROM-DISK"
    );
    assert!(ISO_INSTALL_GAP_NOTE.contains("CLOSED M7.7"));
    assert!(ISO_INSTALL_HOST_LIMIT_NOTE.contains("cannot close"));
    assert!(stage46_host_never_prints_iso_install_ok());
    assert!(!product_iso_continues_past_eltorito(true, true));
    assert!(product_iso_continues_past_eltorito(false, true));
    assert!(!product_iso_continues_past_eltorito(false, false));
    assert!(!product_iso_len_is_window(0));
    assert!(!product_iso_len_is_window(crate::devices::ide_cdrom::GUEST_CD_ISO_CAP));
    assert!(product_iso_len_is_window(
        crate::devices::ide_cdrom::GUEST_CD_ISO_CAP + 2048
    ));
    assert!(!product_iso_len_is_window(PRODUCT_ISO_MAX_BYTES + 1));
    assert_eq!(PRODUCT_ISO_ESP_PATHS[0], "\\EFI\\RayNu\\linux.iso");
    assert!(PRODUCT_ISO_ESP_PATHS.contains(&"\\linux.iso"));
    assert!(PRODUCT_ISO_ESP_PATHS.contains(&"\\EFI\\RayNu\\install.iso"));
    assert_eq!(product_iso_install_disk_bytes(true), LAB_INSTALL_DISK_BYTES as usize);
    assert_eq!(
        product_iso_install_disk_bytes(false),
        PRODUCT_ISO_INSTALL_DISK_IRON_BYTES
    );
    assert!(PRODUCT_ISO_INSTALL_DISK_IRON_BYTES > DEFAULT_INSTALL_DISK_BYTES as usize);
    let iron_sizes = product_iso_install_disk_try_sizes(false);
    assert_eq!(iron_sizes[0], PRODUCT_ISO_INSTALL_DISK_IRON_BYTES);
    assert!(iron_sizes.contains(&(DEFAULT_INSTALL_DISK_BYTES as usize)));
    let i64 = iron_sizes
        .iter()
        .position(|&b| b == DEFAULT_INSTALL_DISK_BYTES as usize)
        .unwrap();
    let i1 = iron_sizes
        .iter()
        .position(|&b| b == LAB_INSTALL_DISK_BYTES as usize)
        .unwrap();
    assert!(i64 < i1);
    assert_eq!(
        product_iso_install_disk_try_sizes(true),
        &[LAB_INSTALL_DISK_BYTES as usize]
    );
    assert_eq!(
        product_iso_frame_pool_prefer_end(false, false),
        crate::guest::linux_boot::GUEST_RAM_BYTES
    );
    assert_eq!(
        product_iso_frame_pool_prefer_end(true, true),
        crate::guest::linux_boot::GUEST_RAM_BYTES
    );
    assert_eq!(
        product_iso_frame_pool_prefer_end(true, false),
        crate::memory::PRECISE_BYTES
    );
    let win = crate::mgmt::guest_image::GuestBootSpec::product_iso(
        crate::mgmt::guest_image::GuestImageType::WindowsIso,
        1,
        64,
    );
    let gen = crate::mgmt::guest_image::GuestBootSpec::product_iso(
        crate::mgmt::guest_image::GuestImageType::GenericUefi,
        2,
        64,
    );
    assert!(win.is_some() && gen.is_some());
}

#[test]
fn install_phase_machine_roundtrip() {
    let _g = iso_install_host_test_lock();
    clear_armed_install_contract();
    let mut store = ImageTable::new();
    let mut install = InstallToDiskPlan::empty();
    register_iso(&mut store, 3, 100, "test.iso").unwrap();
    let c = begin_install_to_disk(&store, &mut install, 3).unwrap();
    assert_eq!(c.install_disk_bytes, DEFAULT_INSTALL_DISK_BYTES);
    assert_eq!(install.phase, InstallPhase::ContractReady);
    assert_eq!(peek_armed_install_contract(), Some(c));
    assert_eq!(
        disk_bytes_for_virtio_launch(),
        DEFAULT_INSTALL_DISK_BYTES as usize
    );
    mark_disk_written(&mut install).unwrap();
    mark_reboot_pending(&mut install).unwrap();
    mark_booted_from_disk(&mut install).unwrap();
    assert!(install.is_install_complete());
    clear_armed_install_contract();
    assert_eq!(disk_bytes_for_virtio_launch(), PROBE_DISK_BYTES);
}

#[test]
fn iso_install_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_install_package());
}

#[test]
fn armed_contract_sizes_launch_disk() {
    let _g = iso_install_host_test_lock();
    clear_armed_install_contract();
    assert!(!install_disk_armed_for_launch());
    arm_install_launch_contract(InstallLaunchContract {
        iso_id: 9,
        extract_bound: true,
        install_disk_bytes: DEFAULT_INSTALL_DISK_BYTES,
    });
    assert!(install_disk_armed_for_launch());
    assert_eq!(
        disk_bytes_for_virtio_launch(),
        DEFAULT_INSTALL_DISK_BYTES as usize
    );
    clear_armed_install_contract();
}

#[test]
fn iso_install_lab_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_install_lab_package());
    println!("RAYNU-V-M7-ISO-INSTALL-LAB-OK");
}

#[test]
fn iso_reboot_lab_package() {
    let _g = iso_install_host_test_lock();
    assert!(prop_iso_reboot_lab_package());
    println!("RAYNU-V-M7-ISO-BOOTED-FROM-DISK");
}

#[test]
fn persist_image_is_marker_only_for_iron_size() {
    assert_eq!(
        persist_image_len_for_contract(LAB_INSTALL_DISK_BYTES),
        INSTALL_MARKER_PERSIST_BYTES
    );
    assert_eq!(
        persist_image_len_for_contract(DEFAULT_INSTALL_DISK_BYTES),
        INSTALL_MARKER_PERSIST_BYTES
    );
    let mut buf = [0u8; INSTALL_MARKER_PERSIST_BYTES];
    assert!(fill_persist_image(&mut buf));
    assert_eq!(
        u32::from_le_bytes(buf[0..4].try_into().unwrap()),
        crate::devices::virtio_blk::DISK_PATTERN
    );
    assert_eq!(
        u32::from_le_bytes(buf[512..516].try_into().unwrap()),
        crate::devices::virtio_blk::INSTALL_DISK_PATTERN
    );
    assert_eq!(parse_decimal_u64(b"67108864\n"), Some(67108864));
    assert_eq!(parse_decimal_u64(b"1048576"), Some(1048576));
}

#[test]
fn product_iso_esp_retain_rejects_lab_size_and_hold_follows_window() {
    let _g = iso_install_host_test_lock();
    clear_product_iso_retain();
    crate::devices::ide_cdrom::reset();
    let lab = vec![0u8; crate::devices::ide_cdrom::GUEST_CD_ISO_CAP];
    assert!(!retain_product_iso_bytes(&lab));
    assert!(product_iso_retained_bytes().is_none());
    assert!(!stage46_hold_e4_shell());
    let extra = crate::devices::ide_cdrom::GUEST_CD_ISO_CAP + 2048;
    let iso = vec![0x5Au8; extra];
    let leaked: &'static [u8] = Box::leak(iso.into_boxed_slice());
    assert!(retain_product_iso_bytes(leaked));
    assert_eq!(product_iso_retained_bytes().map(|b| b.len()), Some(extra));
    assert!(present_product_iso_if_retained());
    assert!(crate::devices::ide_cdrom::product_iso_window_armed());
    assert!(stage46_hold_e4_shell());
    crate::devices::ide_cdrom::reset();
    clear_product_iso_retain();
    assert!(!stage46_hold_e4_shell());
}

#[test]
fn patch_iso_linux_serial_console_same_length_and_idempotent() {
    assert_eq!(ISO_SERIAL_CONSOLE_FROM.len(), ISO_SERIAL_CONSOLE_TO.len());
    assert_eq!(ISO_ALPINE_DEV_FROM.len(), ISO_ALPINE_DEV_TO.len());
    assert_eq!(ISO_TTY0_FROM.len(), ISO_TTY0_TO.len());
    assert_eq!(ISO_GRUB_TIMEOUT1_FROM.len(), ISO_GRUB_TIMEOUT1_TO.len());
    assert_eq!(ISO_GRUB_INSMOD_GOP_FROM.len(), ISO_GRUB_INSMOD_GOP_TO.len());
    assert_eq!(ISO_GRUB_INSMOD_UGA_FROM.len(), ISO_GRUB_INSMOD_UGA_TO.len());
    assert_eq!(ISO_GRUB_INSMOD_ALLVID_FROM.len(), ISO_GRUB_INSMOD_ALLVID_TO.len());
    assert_eq!(ISO_GRUB_TERM_CONSOLE_FROM.len(), ISO_GRUB_TERM_CONSOLE_TO.len());
    let mut buf = b"insmod gfxterm terminal_output gfxterm terminal_output console insmod efi_gop insmod efi_uga insmod all_video linux modules=loop,squashfs,sd-mod,usb-storage quiet alpine_dev=cdrom initrd set timeout=10 set timeout=1".to_vec();
    assert_eq!(patch_iso_linux_serial_console(&mut buf), 10);
    let s = core::str::from_utf8(&buf).unwrap();
    assert!(s.contains("console=ttyS0"));
    assert!(s.contains("virtio_blk"));
    assert!(s.contains("modules=loop,squashfs,virtio_blk console=ttyS0"));
    assert!(!s.contains("nolapic"));
    assert!(s.contains("timeout=0 "));
    assert!(s.contains("set timeout=0"));
    assert!(!s.contains("timeout=00"));
    assert!(!s.contains("efi_gop"));
    assert!(!s.contains("efi_uga"));
    assert!(!s.contains("all_video"));
    assert!(s.contains("alpine_dev=vdb"));
    assert!(s.contains("terminal_output serial "));
    assert!(s.contains("insmod serial "));
    assert!(!s.contains("usb-storage"));
    assert!(!s.contains("gfxterm"));
    assert!(!s.contains("alpine_dev=cdrom"));
    assert!(!s.contains("modules=loop,loop"));
    assert!(!buf.windows(ISO_SERIAL_CONSOLE_FROM.len()).any(|w| w == ISO_SERIAL_CONSOLE_FROM));
    assert_eq!(patch_iso_linux_serial_console(&mut buf), 0);
    let mut already = b"console=ttyS0 usb-storage quiet".to_vec();
    assert_eq!(patch_iso_linux_serial_console(&mut already), 0);
    let mut tty0 = b"linux console=tty0 modules=loop,squashfs,sd-mod,usb-storage quiet".to_vec();
    assert_eq!(patch_iso_linux_serial_console(&mut tty0), 2);
    let t = core::str::from_utf8(&tty0).unwrap();
    assert!(t.contains("noapic"));
    assert!(t.contains("squashfs,virtio_blk console=ttyS0"));
    assert!(!t.contains("console=tty0"));
    // Iron COM2: EFI stub `uncompression error` after `Linux virt`. A needle
    // inside gzip (0xFF neighborhood) must not be rewritten.
    let mut gz = vec![0xFFu8; 96];
    let needle = ISO_GRUB_TIMEOUT1_FROM;
    gz[32..32 + needle.len()].copy_from_slice(needle);
    assert_eq!(patch_iso_linux_serial_console(&mut gz), 0);
    assert_eq!(&gz[32..32 + needle.len()], needle);
    // ISO9660 pads the last sector with NULs; still patch cfg text.
    let mut pad = vec![b' '; 32];
    pad.extend_from_slice(needle);
    pad.extend_from_slice(&[0u8; 32]);
    assert_eq!(patch_iso_linux_serial_console(&mut pad), 1);
    assert_eq!(&pad[32..32 + needle.len()], ISO_GRUB_TIMEOUT1_TO);
    // alpine-virt grub.cfg starts after sector NULs (`set timeout=1` first).
    let mut start = vec![0u8; 32];
    start.extend_from_slice(b"set timeout=1\n\nmenuentry \"Linux virt\" {\n");
    assert_eq!(patch_iso_linux_serial_console(&mut start), 1);
    assert_eq!(&start[32..32 + needle.len()], ISO_GRUB_TIMEOUT1_TO);
    // Needle in a sea of NULs is not cfg (do not rewrite stored gzip).
    let mut z = vec![0u8; 32];
    z.extend_from_slice(needle);
    z.extend_from_slice(&[0u8; 32]);
    assert_eq!(patch_iso_linux_serial_console(&mut z), 0);
    assert_eq!(&z[32..32 + needle.len()], needle);
}
