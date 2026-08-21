use super::{
    adr014_present, BootDevice, GuestBootSpec, GuestFirmware, GuestImageType,
};

#[test]
fn parses_product_types() {
    assert_eq!(GuestImageType::parse("linux_iso"), Some(GuestImageType::LinuxIso));
    assert_eq!(GuestImageType::parse("windows_iso"), Some(GuestImageType::WindowsIso));
    assert_eq!(GuestImageType::parse("generic_uefi"), Some(GuestImageType::GenericUefi));
    assert_eq!(
        GuestImageType::parse("linux_bzimage"),
        Some(GuestImageType::LinuxBzImage)
    );
    assert!(GuestImageType::parse("iso").is_none());
}

#[test]
fn bzimage_is_lab_only() {
    assert!(GuestImageType::LinuxBzImage.is_lab_only());
    assert!(!GuestImageType::LinuxIso.is_lab_only());
    assert!(!GuestImageType::WindowsIso.is_lab_only());
    assert_eq!(
        GuestImageType::LinuxBzImage.firmware(),
        GuestFirmware::LinuxBootProtocol
    );
    assert_eq!(GuestImageType::WindowsIso.firmware(), GuestFirmware::Uefi);
}

#[test]
fn product_iso_rejects_bzimage() {
    assert!(GuestBootSpec::product_iso(GuestImageType::LinuxBzImage, 1, 32).is_none());
    let spec = GuestBootSpec::product_iso(GuestImageType::WindowsIso, 7, 64).unwrap();
    assert_eq!(spec.firmware, GuestFirmware::Uefi);
    assert_eq!(spec.boot_order, [BootDevice::Cdrom, BootDevice::Disk]);
    assert!(spec.is_product_path());
}

#[test]
fn linux_iso_same_uefi_path_as_windows() {
    let linux = GuestBootSpec::product_iso(GuestImageType::LinuxIso, 1, 32).unwrap();
    let win = GuestBootSpec::product_iso(GuestImageType::WindowsIso, 1, 32).unwrap();
    assert_eq!(linux.firmware, win.firmware);
    assert_eq!(linux.boot_order, win.boot_order);
    assert_eq!(linux.firmware, GuestFirmware::Uefi);
}

#[test]
fn adr014_files_the_constraint() {
    assert!(adr014_present());
}
