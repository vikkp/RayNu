//! M8.7 host pack. Not a doorbell. Not iron `RAYNU-V-M8-PERC-LUN-OK`.

use super::{
    cdb_is_single_read16, classify_ld_bytes, dcmd_is_allowed, frame_is_read,
    fw_state_allows_mailbox, h740p_mini_singleton, host_never_prints_iron_perc_ok,
    pack_ld_get_list, pack_ld_read16, parse_ld_list, pick_spare, prop_perc_host_package,
    FrameError, LdClass, LdEntry, LAB_SPARE_BYTES, LAB_UBUNTU0_BYTES, M8_PERC_HOST_OK_MARKER,
    M8_PERC_LUN_OK_MARKER, MFI_CMD_DCMD, MFI_CMD_LD_SCSI_IO, MFI_CMD_LD_WRITE, MR_DCMD_LD_GET_LIST,
    PCI_DEVICE_H740P_HARPOON, PCI_VENDOR_LSI, PERC_HOST_RESIDUAL_NOTE, SCSI_READ_16, SCSI_WRITE_16,
};

fn lab_list_bytes() -> [u8; 8 + 32] {
    let mut buf = [0u8; 40];
    buf[0] = 2;
    buf[8] = 0;
    let ubuntu_blocks = LAB_UBUNTU0_BYTES / 512;
    buf[16..24].copy_from_slice(&ubuntu_blocks.to_le_bytes());
    buf[24] = 1;
    let spare_blocks = LAB_SPARE_BYTES / 512;
    buf[32..40].copy_from_slice(&spare_blocks.to_le_bytes());
    buf
}

#[test]
fn lab_ld_list_picks_the_spare_and_refuses_ubuntu() {
    let buf = lab_list_bytes();
    let list = parse_ld_list(&buf, 512).unwrap();
    assert_eq!(list.count, 2);
    assert_eq!(
        classify_ld_bytes(list.entries[0].size_bytes),
        LdClass::Ubuntu
    );
    assert_eq!(
        classify_ld_bytes(list.entries[1].size_bytes),
        LdClass::Spare
    );
    let pick = pick_spare(&list.entries[..list.count]).unwrap();
    assert_eq!(pick.target_id, 1);
    assert_eq!(pick.size_bytes, LAB_SPARE_BYTES);
    assert!(host_never_prints_iron_perc_ok());
    assert!(PERC_HOST_RESIDUAL_NOTE.contains("skip PERC"));
}

#[test]
fn dcmd_frame_is_the_ld_list_read() {
    let frame = pack_ld_get_list(7, 0x1000, 40);
    assert_eq!(frame[0], MFI_CMD_DCMD);
    assert_eq!(frame[7], 1);
    assert!(frame_is_read(&frame));
    assert_eq!(
        u32::from_le_bytes(frame[0x18..0x1C].try_into().unwrap()),
        MR_DCMD_LD_GET_LIST
    );
    assert!(dcmd_is_allowed(MR_DCMD_LD_GET_LIST));
    assert_eq!(
        u64::from_le_bytes(frame[0x28..0x30].try_into().unwrap()),
        0x1000
    );
}

#[test]
fn read16_is_one_block_on_the_spare_target() {
    let frame = pack_ld_read16(1, 0, 1, 0x2000, 512).unwrap();
    assert_eq!(frame[0], MFI_CMD_LD_SCSI_IO);
    assert_eq!(frame[4], 1);
    assert!(frame_is_read(&frame));
    assert!(cdb_is_single_read16(&frame[0x20..0x30]));
    assert_eq!(frame[0x20], SCSI_READ_16);
    assert_ne!(frame[0], MFI_CMD_LD_WRITE);
    assert!(pack_ld_read16(1, 0, 2, 0x2000, 512).is_err());
    assert_eq!(
        pack_ld_read16(1, 0, 1, 0x2000, 4096),
        Err(FrameError::Length)
    );
    let mut write = [0u8; 16];
    write[0] = SCSI_WRITE_16;
    write[13] = 1;
    assert!(!cdb_is_single_read16(&write));
}

#[test]
fn two_harpoons_and_a_faulted_fw_stop() {
    assert!(h740p_mini_singleton(
        PCI_VENDOR_LSI,
        PCI_DEVICE_H740P_HARPOON,
        1
    ));
    assert!(!h740p_mini_singleton(
        PCI_VENDOR_LSI,
        PCI_DEVICE_H740P_HARPOON,
        2
    ));
    assert!(!fw_state_allows_mailbox(super::MFI_STATE_FAULT));
    assert!(prop_perc_host_package());
    assert_ne!(M8_PERC_HOST_OK_MARKER, M8_PERC_LUN_OK_MARKER);
    let _ = LdEntry {
        target_id: 0,
        size_bytes: LAB_UBUNTU0_BYTES,
    };
    println!("{M8_PERC_HOST_OK_MARKER}");
}
