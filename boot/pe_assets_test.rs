use super::*;

#[test]
fn embedded_assets_non_empty() {
    assert!(embedded_present());
    assert!(bzimage_len() > 64 * 1024, "bzImage should be a real tinyconfig");
    assert!(initrd_len() > 32, "initrd should carry /init");
    assert_eq!(bzimage_bytes().unwrap().len(), bzimage_len());
    assert_eq!(initrd_bytes().unwrap().len(), initrd_len());
}

#[test]
fn section_names_fit_coff_short() {
    assert!(SECTION_KERNEL.len() <= 8);
    assert!(SECTION_INITRD.len() <= 8);
    assert_eq!(SECTION_KERNEL, ".askern");
    assert_eq!(SECTION_INITRD, ".asinit");
}

#[test]
fn marker_stable() {
    assert_eq!(M3_ASSETS_OK_MARKER, "RAYNU-V-M3-ASSETS-OK");
}
