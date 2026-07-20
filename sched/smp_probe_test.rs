use super::*;

#[test]
fn marker_stable() {
    assert_eq!(M4_SMP_OK_MARKER, "RAYNU-V-M4-SMP-OK");
}

#[test]
fn both_ready_latches() {
    let mut page = [0u8; 4096];
    // SAFETY: stack buffer as fake flag HPA.
    unsafe {
        init(page.as_mut_ptr() as u64);
        page[OFF_BSP_READY] = READY_MAGIC;
        assert!(note_bsp_ready());
        assert!(!smp_ok());
        page[OFF_AP_READY] = READY_MAGIC;
        assert!(note_ap_ready());
        assert!(smp_ok());
        assert!(take_smp_ok_latch());
        assert!(!take_smp_ok_latch());
    }
}
