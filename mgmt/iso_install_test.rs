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
    assert_eq!(ISO_GRUB_LINUX_FROM.len(), ISO_GRUB_LINUX_TO.len());
    assert_eq!(ISO_GRUB_LINUX_FROM.len(), 225);
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
    // alpine-virt GRUB stanza + ISO9660 NUL pad: E4 timer skip so delay_loop
    // / TSC vs frozen HPET calibrate do not run.
    let mut grub = b"menuentry ".to_vec();
    grub.extend_from_slice(ISO_GRUB_LINUX_FROM);
    grub.extend_from_slice(&[0u8; 8]);
    assert_eq!(patch_iso_linux_serial_console(&mut grub), 1);
    let g = core::str::from_utf8(&grub[..10 + ISO_GRUB_LINUX_TO.len()]).unwrap();
    assert!(g.contains("lpj=4194304"));
    assert!(g.contains("no_timer_check"));
    assert!(g.contains("tsc=reliable"));
    assert!(g.contains("clocksource=tsc"));
    assert!(g.contains("idle=poll"));
    assert!(g.contains("earlycon=uart8250,io,0x3f8"));
    assert!(g.contains("alpine_dev=vdb"));
    assert!(g.contains("virtio_blk"));
    assert!(g.contains("console=ttyS0"));
    assert!(g.contains("modules=loop,squashfs,virtio_blk"));
    assert!(!g.contains("usb-storage"));
    assert!(!grub.windows(ISO_GRUB_LINUX_FROM.len()).any(|w| w == ISO_GRUB_LINUX_FROM));
    assert_eq!(patch_iso_linux_serial_console(&mut grub), 0);
}

fn write_iso9660_dir_record(buf: &mut [u8], rec: usize, name: &[u8], lba: u32, size: u32) {
    let mut rec_len = 33 + name.len();
    if rec_len % 2 == 1 {
        rec_len += 1;
    }
    buf[rec] = rec_len as u8;
    buf[rec + 2..rec + 6].copy_from_slice(&lba.to_le_bytes());
    buf[rec + 6..rec + 10].copy_from_slice(&lba.to_be_bytes());
    buf[rec + 10..rec + 14].copy_from_slice(&size.to_le_bytes());
    buf[rec + 14..rec + 18].copy_from_slice(&size.to_be_bytes());
    buf[rec + 28..rec + 30].copy_from_slice(&1u16.to_le_bytes());
    buf[rec + 30..rec + 32].copy_from_slice(&1u16.to_be_bytes());
    buf[rec + 32] = name.len() as u8;
    buf[rec + 33..rec + 33 + name.len()].copy_from_slice(name);
}

fn iso_dir_size_le(buf: &[u8], rec: usize) -> u32 {
    u32::from_le_bytes(buf[rec + 10..rec + 14].try_into().unwrap())
}

fn iso_dir_size_be(buf: &[u8], rec: usize) -> u32 {
    u32::from_be_bytes(buf[rec + 14..rec + 18].try_into().unwrap())
}

#[test]
fn patch_iso_linux_grows_grub_cfg_iso9660_data_length() {
    assert_eq!(ISO_GRUB_CFG_ALPINE_VIRT.len(), ISO_GRUB_CFG_ORIG_SIZE as usize);
    assert!(ISO_GRUB_CFG_PATCHED_SIZE > ISO_GRUB_CFG_ORIG_SIZE);
    let lba = 2u32;
    let data = (lba as usize) * 2048;
    let mut iso = vec![0u8; data + 2048];
    iso[data..data + ISO_GRUB_CFG_ALPINE_VIRT.len()].copy_from_slice(ISO_GRUB_CFG_ALPINE_VIRT);
    let pvd = 64usize;
    let joliet = 128usize;
    write_iso9660_dir_record(
        &mut iso,
        pvd,
        ISO_GRUB_CFG_ISO9660_NAME,
        lba,
        ISO_GRUB_CFG_ORIG_SIZE,
    );
    write_iso9660_dir_record(
        &mut iso,
        joliet,
        ISO_GRUB_CFG_JOLIET_NAME,
        lba,
        ISO_GRUB_CFG_ORIG_SIZE,
    );
    // linux grow (1) + PVD bump (1) + Joliet bump (1) + set timeout=1 (1)
    assert_eq!(patch_iso_linux_serial_console(&mut iso), 4);
    assert_eq!(iso_dir_size_le(&iso, pvd), ISO_GRUB_CFG_PATCHED_SIZE);
    assert_eq!(iso_dir_size_be(&iso, pvd), ISO_GRUB_CFG_PATCHED_SIZE);
    assert_eq!(iso_dir_size_le(&iso, joliet), ISO_GRUB_CFG_PATCHED_SIZE);
    assert_eq!(iso_dir_size_be(&iso, joliet), ISO_GRUB_CFG_PATCHED_SIZE);
    let orig_win = &iso[data..data + ISO_GRUB_CFG_ORIG_SIZE as usize];
    assert!(core::str::from_utf8(orig_win).unwrap().contains("tsc="));
    assert!(!orig_win.contains(&b'}'));
    assert!(!core::str::from_utf8(orig_win).unwrap().contains("initrd"));
    let patched = &iso[data..data + ISO_GRUB_CFG_PATCHED_SIZE as usize];
    let s = core::str::from_utf8(patched).unwrap();
    assert!(s.contains("lpj=4194304"));
    assert!(s.contains("tsc=reliable"));
    assert!(s.contains("earlycon=uart8250,io,0x3f8"));
    assert!(s.contains("alpine_dev=vdb"));
    assert!(s.contains("initrd\t/boot/initramfs-virt"));
    assert!(s.ends_with("}\n"));
    assert_eq!(s.bytes().filter(|b| *b == b'{').count(), 1);
    assert_eq!(s.bytes().filter(|b| *b == b'}').count(), 1);
    assert_eq!(patch_iso_linux_serial_console(&mut iso), 0);
}

#[test]
fn bump_iso9660_grub_cfg_size_skips_gzip_false_positive() {
    let mut buf = vec![0xFFu8; 256];
    write_iso9660_dir_record(
        &mut buf,
        32,
        ISO_GRUB_CFG_ISO9660_NAME,
        0,
        ISO_GRUB_CFG_ORIG_SIZE,
    );
    // LBA 0 is not `set timeout=` (0xFF fill).
    let before = iso_dir_size_le(&buf, 32);
    assert_eq!(patch_iso_linux_serial_console(&mut buf), 0);
    assert_eq!(iso_dir_size_le(&buf, 32), before);
}

#[test]
fn patch_in_tree_alpine_virt_iso_grub_cfg_size_if_present() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target/alpine-virt-3.21.3-x86_64.iso"
    );
    let Ok(mut iso) = std::fs::read(path) else {
        return;
    };
    if iso.len() < 2048 {
        return;
    }
    let n = patch_iso_linux_serial_console(&mut iso);
    assert!(n >= 4, "expected linux grow + 2 dir bumps + timeout, got {n}");
    let mut found = 0u32;
    let mut i = 0usize;
    while i + 34 <= iso.len() {
        if iso[i + 32] as usize == ISO_GRUB_CFG_ISO9660_NAME.len()
            && iso[i + 33..i + 33 + ISO_GRUB_CFG_ISO9660_NAME.len()] == *ISO_GRUB_CFG_ISO9660_NAME
        {
            assert_eq!(iso_dir_size_le(&iso, i), ISO_GRUB_CFG_PATCHED_SIZE);
            assert_eq!(iso_dir_size_be(&iso, i), ISO_GRUB_CFG_PATCHED_SIZE);
            found += 1;
        }
        i += 1;
    }
    assert!(found >= 1);
    let cfg_off = 8121usize * 2048;
    let s = core::str::from_utf8(&iso[cfg_off..cfg_off + ISO_GRUB_CFG_PATCHED_SIZE as usize]).unwrap();
    assert!(s.contains("initrd\t/boot/initramfs-virt"));
    assert!(s.ends_with("}\n"));
}
