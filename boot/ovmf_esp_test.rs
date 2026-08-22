use super::{
    accept_real_ovmf_bytes, bytes_present, clear_retained, retain_ovmf_bytes, retained_bytes,
    E5_OVMF_RETAIN_RESIDUAL_NOTE, M7_E5_LIVE_BYTES_PRESENT_OK_MARKER, MIN_REAL_OVMF_BYTES,
    MIN_REAL_OVMF_NONEMPTY, OVMF_ESP_CAP,
};

fn write_fvh(buf: &mut [u8]) {
    let len = buf.len() as u64;
    buf[0x20..0x28].copy_from_slice(&len.to_le_bytes());
    buf[0x28..0x2C].copy_from_slice(b"_FVH");
    buf[0x30..0x32].copy_from_slice(&0x38u16.to_le_bytes());
}

#[test]
fn retain_rejects_zero_padded_alias_fixture() {
    clear_retained();
    let mut fixture = vec![0u8; OVMF_ESP_CAP];
    write_fvh(&mut fixture);
    fixture[OVMF_ESP_CAP - 16] = 0xEA;
    assert!(!accept_real_ovmf_bytes(&fixture));
    assert_eq!(retain_ovmf_bytes(&fixture), Err(()));
    assert!(!bytes_present());
    assert!(retained_bytes().is_none());
}

#[test]
fn retain_accepts_dense_edk2_sized_image() {
    clear_retained();
    let mut realish = vec![0u8; MIN_REAL_OVMF_BYTES];
    write_fvh(&mut realish);
    for (i, b) in realish.iter_mut().enumerate().skip(0x38) {
        *b = (i % 251) as u8 + 1;
    }
    assert!(accept_real_ovmf_bytes(&realish));
    assert_eq!(retain_ovmf_bytes(&realish), Ok(MIN_REAL_OVMF_BYTES));
    assert!(bytes_present());
    assert_eq!(retained_bytes().map(|b| b.len()), Some(MIN_REAL_OVMF_BYTES));
    clear_retained();
    assert!(!bytes_present());
}

#[test]
fn retain_rejects_too_small_and_missing_fvh() {
    clear_retained();
    let small = vec![0xAAu8; 4096];
    assert!(!accept_real_ovmf_bytes(&small));
    let no_sig = vec![1u8; MIN_REAL_OVMF_BYTES];
    assert!(no_sig.iter().filter(|b| **b != 0).count() >= MIN_REAL_OVMF_NONEMPTY);
    assert!(!accept_real_ovmf_bytes(&no_sig));
    assert_eq!(
        M7_E5_LIVE_BYTES_PRESENT_OK_MARKER,
        "RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK"
    );
    assert!(E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("VMLAUNCH insn not issued"));
    assert!(E5_OVMF_RETAIN_RESIDUAL_NOTE.contains("not allocated"));
}
