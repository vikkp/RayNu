//! M8.7 host pack. Not a doorbell. Not iron `RAYNU-V-M8-PERC-LUN-OK`.

use super::{
    cdb_is_single_read16, classify_ld_bytes, dcmd_is_allowed, frame_is_read,
    fw_state_allows_mailbox, fwstate_may_load, h740p_mini_singleton,
    host_never_prints_iron_perc_ok, memory_bar64, pack_ld_get_list, pack_ld_read16, parse_ld_list,
    pci_cmd_for_fwstate_load, pci_cmd_newly_bus_master, pick_spare, prop_perc_host_package,
    FrameError, LdClass, LdEntry, LAB_SPARE_BYTES, LAB_UBUNTU0_BYTES, M8_PERC_HOST_OK_MARKER,
    M8_PERC_LUN_OK_MARKER, MFI_CMD_DCMD, MFI_CMD_LD_SCSI_IO, MFI_CMD_LD_WRITE, MR_DCMD_LD_GET_LIST,
    PCI_CMD_BUS_MASTER, PCI_CMD_MEMORY, PCI_DEVICE_H740P_HARPOON, PCI_VENDOR_LSI,
    PERC_HOST_RESIDUAL_NOTE, PERC_SCAN_BUS_LAST, SCSI_READ_16, SCSI_WRITE_16,
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
fn iron_scratch_pad_is_ready_and_ioc_init_is_one_mfa() {
    use super::{
        pack_ioc_init_request, pack_ld_get_list_polled, pack_mfa_descriptor, pack_mfi_init_frame,
        pack_rdpq_entry, IRON_FUSION_S0, IRON_FUSION_S1, LD_LIST_FW_BYTES, MFA_REQUEST_FLAGS,
        MFI_CMD_INIT, MFI_CMD_STATUS_POLL, MFI_FRAME_DIR_READ, MFI_FRAME_DIR_WRITE,
        MFI_FRAME_DONT_POST_IN_REPLY_QUEUE, MFI_INBOUND_LOW_QUEUE_PORT, MPI2_FUNCTION_IOC_INIT,
        MPI2_HEADER_VERSION, MPI2_IOC_INIT_BYTES, MPI2_VERSION, MPI2_WHOINIT_HOST_DRIVER,
    };
    assert_eq!(super::fw_state(IRON_FUSION_S0), super::MFI_STATE_READY);
    assert!(!super::fw_state_allows_mailbox(IRON_FUSION_S0));
    assert!(super::fusion_post_is_allowed(IRON_FUSION_S0));
    assert!(!super::fusion_post_is_allowed(super::MFI_STATE_OPERATIONAL));
    assert_eq!(super::fusion_max_cmds(IRON_FUSION_S0), 0x0fed);
    assert_eq!(super::fusion_reply_queues_ventura(IRON_FUSION_S1), 128);
    assert!(super::fusion_rdpq(IRON_FUSION_S1));
    assert_eq!(MFI_INBOUND_LOW_QUEUE_PORT, 0xC0);
    assert_eq!(
        pack_mfa_descriptor(0x20_0000),
        Some(0x20_0000 | MFA_REQUEST_FLAGS)
    );
    let msg = pack_ioc_init_request(0x30_0000, 0x40_0000).unwrap();
    assert_eq!(msg.len(), MPI2_IOC_INIT_BYTES);
    assert_eq!(msg[0x00], MPI2_WHOINIT_HOST_DRIVER);
    assert_eq!(msg[0x02], 0);
    assert_eq!(msg[0x03], MPI2_FUNCTION_IOC_INIT);
    assert_eq!(msg[0x07], 0x01);
    assert_eq!(
        u16::from_le_bytes(msg[0x0C..0x0E].try_into().unwrap()),
        MPI2_VERSION
    );
    assert_eq!(
        u16::from_le_bytes(msg[0x0E..0x10].try_into().unwrap()),
        MPI2_HEADER_VERSION
    );
    assert_eq!(msg[0x16], 12);
    assert_eq!(msg[0x17], 1);
    assert_eq!(u16::from_le_bytes(msg[0x1A..0x1C].try_into().unwrap()), 64);
    assert_eq!(u16::from_le_bytes(msg[0x1C..0x1E].try_into().unwrap()), 16);
    assert_eq!(
        u64::from_le_bytes(msg[0x28..0x30].try_into().unwrap()),
        0x30_0000
    );
    assert_eq!(
        u64::from_le_bytes(msg[0x30..0x38].try_into().unwrap()),
        0x40_0000
    );
    assert!(pack_ioc_init_request(0x1_0000_0000, 0x40_0000).is_none());
    let entry = pack_rdpq_entry(0x50_0000).unwrap();
    assert_eq!(
        u64::from_le_bytes(entry[0..8].try_into().unwrap()),
        0x50_0000
    );
    let init = pack_mfi_init_frame(0x30_0100).unwrap();
    assert_eq!(init[0], MFI_CMD_INIT);
    assert_eq!(init[2], MFI_CMD_STATUS_POLL);
    assert_eq!(init[4], 0, "driver_operations stays 0");
    let flags = u16::from_le_bytes(init[0x10..0x12].try_into().unwrap());
    assert_eq!(flags, MFI_FRAME_DONT_POST_IN_REPLY_QUEUE);
    assert_eq!(flags & MFI_FRAME_DIR_WRITE, 0);
    assert_eq!(
        u32::from_le_bytes(init[0x14..0x18].try_into().unwrap()),
        MPI2_IOC_INIT_BYTES as u32
    );
    let dcmd = pack_ld_get_list_polled(1, 0x60_0000, LD_LIST_FW_BYTES).unwrap();
    assert_eq!(dcmd[2], MFI_CMD_STATUS_POLL);
    let dflags = u16::from_le_bytes(dcmd[0x10..0x12].try_into().unwrap());
    assert_ne!(dflags & MFI_FRAME_DONT_POST_IN_REPLY_QUEUE, 0);
    assert_ne!(dflags & MFI_FRAME_DIR_READ, 0);
    assert_eq!(dflags & MFI_FRAME_DIR_WRITE, 0);
    assert_eq!(LD_LIST_FW_BYTES, 4104);
    assert!(pack_ld_get_list_polled(1, 0x60_0000, 2048).is_none());
    assert!(super::fusion_poll_cycles(2_095_684_400, super::FUSION_POLL_SECS) > 4_000_000_000);
    assert!(!super::adapter_reset_is_allowed());
    assert!(!super::doorbell_transition_is_allowed());
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

fn image_header_sector(alt: u64) -> [u8; 512] {
    use crate::raynu_f::gpt::{GPT_HEADER_SIZE_MIN, GPT_REVISION_1_0, GPT_SIGNATURE};
    use crate::raynu_f::tables::crc32;
    let mut hdr = [0u8; 512];
    hdr[0..8].copy_from_slice(GPT_SIGNATURE);
    hdr[8..12].copy_from_slice(&GPT_REVISION_1_0.to_le_bytes());
    hdr[12..16].copy_from_slice(&GPT_HEADER_SIZE_MIN.to_le_bytes());
    hdr[24..32].copy_from_slice(&1u64.to_le_bytes());
    hdr[32..40].copy_from_slice(&alt.to_le_bytes());
    hdr[72..80].copy_from_slice(&2u64.to_le_bytes());
    hdr[80..84].copy_from_slice(&128u32.to_le_bytes());
    hdr[84..88].copy_from_slice(&128u32.to_le_bytes());
    let c = crc32(&hdr[..GPT_HEADER_SIZE_MIN as usize]);
    hdr[16..20].copy_from_slice(&c.to_le_bytes());
    hdr
}

#[test]
fn fwstate_load_is_one_harpoon_and_does_not_bus_master() {
    assert!(fwstate_may_load(1, memory_bar64(0xF000_0000, 0)));
    assert!(!fwstate_may_load(0, memory_bar64(0xF000_0000, 0)));
    assert!(!fwstate_may_load(2, memory_bar64(0xF000_0000, 0)));
    assert!(!fwstate_may_load(1, None));
    let above4g = memory_bar64(0x0000_0004, 0x1).unwrap();
    assert_eq!(above4g, 0x1_0000_0000);
    assert!(fwstate_may_load(1, Some(above4g)));
    assert!(memory_bar64(0x0000_1000, 0).is_none());
    assert!(memory_bar64(0x0000_0001, 0).is_none());
    let enabled = pci_cmd_for_fwstate_load(0);
    assert_eq!(enabled, PCI_CMD_MEMORY);
    assert!(!pci_cmd_newly_bus_master(0, enabled));
    let already = pci_cmd_for_fwstate_load(PCI_CMD_BUS_MASTER | PCI_CMD_MEMORY);
    assert_eq!(already & PCI_CMD_BUS_MASTER, PCI_CMD_BUS_MASTER);
    assert!(!pci_cmd_newly_bus_master(
        PCI_CMD_BUS_MASTER,
        pci_cmd_for_fwstate_load(PCI_CMD_BUS_MASTER)
    ));
    assert!(PERC_SCAN_BUS_LAST >= 0x18);
    assert!(PERC_SCAN_BUS_LAST < 0xFF);
    assert!(PERC_HOST_RESIDUAL_NOTE.contains("no doorbell"));
    assert!(PERC_HOST_RESIDUAL_NOTE.contains("outbound_msg_0"));
    assert!(PERC_HOST_RESIDUAL_NOTE.contains("scratch_pad_0"));
    assert_eq!(super::harpoon_fw_status_offset(), super::MFI_SCRATCH_PAD_0);
    assert!(super::fusion_dma_base_ok(0x1000000));
    assert!(super::fusion_dma_base_ok(0xFFFF_8000));
    assert!(!super::fusion_dma_base_ok(0));
    assert!(!super::fusion_dma_base_ok(0x1_0000_0000));
    assert!(!super::fusion_dma_base_ok(0xFFFF_F000));
    assert_eq!(super::FUSION_DMA_PAGES, 8);
    assert_eq!(
        super::classify_ld_bytes(super::IRON_LD0_BYTES),
        super::LdClass::Ubuntu
    );
    assert_eq!(
        super::classify_ld_bytes(super::IRON_LD1_BYTES),
        super::LdClass::Spare
    );
    assert_eq!(
        super::spare_read_target(super::pick_spare(&[
            super::LdEntry {
                target_id: 0,
                size_bytes: super::IRON_LD0_BYTES,
            },
            super::LdEntry {
                target_id: 1,
                size_bytes: super::IRON_LD1_BYTES,
            },
        ])),
        Some(1)
    );
    assert!(super::spare_read_target(Err(super::PickError::UbuntuOnly)).is_none());
    let rd = super::pack_ld_read16_polled(1, 0, 0x1006000).unwrap();
    assert_eq!(rd[0], super::MFI_CMD_LD_SCSI_IO);
    assert_eq!(rd[1], 32);
    assert_eq!(rd[2], super::MFI_CMD_STATUS_POLL);
    assert_eq!(rd[4], 1);
    assert!(super::cdb_is_single_read16(&rd[0x20..0x30]));
    assert!(super::frame_is_read(&rd));
    let flags = u16::from_le_bytes(rd[0x10..0x12].try_into().unwrap());
    assert_eq!(flags & super::MFI_FRAME_DIR_WRITE, 0);
    assert_ne!(flags & super::MFI_FRAME_DIR_READ, 0);
    assert_ne!(flags & super::MFI_FRAME_DONT_POST_IN_REPLY_QUEUE, 0);
    assert_eq!(
        u64::from_le_bytes(rd[0x18..0x20].try_into().unwrap()),
        0x1006200
    );
    assert_eq!(
        u64::from_le_bytes(rd[0x30..0x38].try_into().unwrap()),
        0x1006000
    );
    assert_eq!(u32::from_le_bytes(rd[0x38..0x3C].try_into().unwrap()), 512);
    assert!(super::pack_ld_read16_polled(1, 0, 0).is_none());
    let last = super::spare_last_lba(super::IRON_LD1_BYTES).unwrap();
    assert!(last > 0);
    let tail = super::pack_ld_read16_polled(1, last, 0x1006000).unwrap();
    assert_eq!(
        u64::from_be_bytes(tail[0x22..0x2A].try_into().unwrap()),
        last
    );
    let wr = super::pack_ld_write16_last_polled(1, super::IRON_LD1_BYTES, 0x1006000).unwrap();
    assert_eq!(wr[0], super::MFI_CMD_LD_SCSI_IO);
    assert_eq!(wr[2], super::MFI_CMD_STATUS_POLL);
    assert!(super::cdb_is_single_write16(&wr[0x20..0x30]));
    assert!(super::frame_is_write(&wr));
    assert!(!super::frame_is_read(&wr));
    assert_eq!(u64::from_be_bytes(wr[0x22..0x2A].try_into().unwrap()), last);
    let wflags = u16::from_le_bytes(wr[0x10..0x12].try_into().unwrap());
    assert_ne!(wflags & super::MFI_FRAME_DIR_WRITE, 0);
    assert_eq!(wflags & super::MFI_FRAME_DIR_READ, 0);
    assert!(super::pack_ld_write16_last_polled(1, super::IRON_LD0_BYTES, 0x1006000).is_none());
    assert_eq!(
        super::pack_ld_write16(1, 0, 1, 0x1006000, 512),
        Err(super::FrameError::Lba0)
    );
    let sector = super::perc_write_sector();
    assert!(super::perc_write_readback_matches(&sector));
    assert_eq!(&sector[..16], b"RAYNU-SPARE-WR16");
    assert!(sector[16..].iter().all(|b| *b == 0));
    assert!(!super::perc_write_readback_matches(&[0u8; 512]));
    let plan = super::image_copy_plan(super::IRON_LD1_BYTES, super::PERC_IMAGE_BYTES).unwrap();
    assert_eq!(plan.sectors, 16_777_216);
    assert_eq!(plan.chunk_sectors, 8);
    assert!(plan.sectors - 1 < last);
    assert!(super::image_copy_plan(super::IRON_LD0_BYTES, super::PERC_IMAGE_BYTES).is_none());
    assert!(super::image_copy_plan(super::IRON_LD1_BYTES, 512).is_none());
    let image = super::pack_ld_write16_image(super::IRON_LD1_BYTES, 1, 0, 8, 0x1006000).unwrap();
    assert_eq!(image[0], super::MFI_CMD_LD_SCSI_IO);
    assert!(super::frame_is_write(&image));
    assert!(!super::frame_is_read(&image));
    assert_eq!(u64::from_be_bytes(image[0x22..0x2A].try_into().unwrap()), 0);
    assert_eq!(u32::from_be_bytes(image[0x2A..0x2E].try_into().unwrap()), 8);
    assert_eq!(
        u64::from_le_bytes(image[0x18..0x20].try_into().unwrap()),
        0x1006000 + 4096
    );
    assert!(super::pack_ld_write16_image(super::IRON_LD0_BYTES, 1, 0, 1, 0x1006000).is_none());
    assert!(super::pack_ld_write16_image(super::IRON_LD1_BYTES, 1, 1, 8, 0x1006000).is_none());
    let mut peek = [0u8; 1024];
    peek[512..520].copy_from_slice(b"EFI PART");
    assert!(super::gpt_header_is_efi_part(&peek));
    assert!(!super::gpt_header_is_efi_part(&[0u8; 1024]));
    let entry0 = super::image_read_slice(1024, 128, 0).unwrap();
    assert_eq!((entry0.lba, entry0.skip, entry0.take), (2, 0, 128));
    let entry1 = super::image_read_slice(1152, 128, 0).unwrap();
    assert_eq!((entry1.lba, entry1.skip, entry1.take), (2, 128, 128));
    let span0 = super::image_read_slice(400, 200, 0).unwrap();
    assert_eq!((span0.lba, span0.skip, span0.take), (0, 400, 112));
    let span1 = super::image_read_slice(400, 200, 112).unwrap();
    assert_eq!((span1.lba, span1.skip, span1.take), (1, 0, 88));
    assert!(super::image_read_in_window(1024, 128));
    assert!(!super::image_io_in_window(1024, 128));
    assert!(super::image_read_slice(0, 0, 0).is_none());
    assert!(super::image_read_slice(super::PERC_IMAGE_BYTES - 1, 2, 0).is_none());
    let ten = 10_240u64 * 1024 * 1024;
    let place =
        super::place_in_free_tail(super::IRON_LD1_BYTES, super::PERC_IMAGE_BYTES, true, ten)
            .unwrap();
    assert_eq!(place.spare_off, super::PERC_IMAGE_BYTES);
    assert_eq!(place.disk_bytes, ten);
    assert_eq!(
        super::tail_guest_to_lba(place, 0, 512),
        Some(super::PERC_IMAGE_BYTES / 512)
    );
    assert!(super::tail_guest_to_lba(place, 0, 511).is_none());
    assert!(super::tail_guest_to_lba(place, ten, 512).is_none());
    assert!(super::place_in_free_tail(
        super::IRON_LD1_BYTES,
        super::PERC_IMAGE_BYTES,
        true,
        super::PERC_IMAGE_BYTES
    )
    .is_none());
    assert!(
        super::place_in_free_tail(super::IRON_LD1_BYTES, super::PERC_IMAGE_BYTES, false, ten)
            .is_none()
    );
    assert!(super::place_in_free_tail(
        super::LAB_UBUNTU0_BYTES,
        super::PERC_IMAGE_BYTES,
        true,
        ten
    )
    .is_none());
    let free = super::IRON_LD1_BYTES - super::PERC_IMAGE_BYTES;
    assert!(
        super::place_in_free_tail(super::IRON_LD1_BYTES, super::PERC_IMAGE_BYTES, true, free)
            .is_some()
    );
    assert!(super::place_in_free_tail(
        super::IRON_LD1_BYTES,
        super::PERC_IMAGE_BYTES,
        true,
        free + 512
    )
    .is_none());
    let tail_lba = super::PERC_IMAGE_BYTES / 512;
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        0,
        1,
        0x1006000,
        place.spare_off,
        place.disk_bytes
    )
    .is_none());
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba - 1,
        1,
        0x1006000,
        place.spare_off,
        place.disk_bytes
    )
    .is_none());
    let packed = super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba,
        1,
        0x1006000,
        place.spare_off,
        place.disk_bytes,
    )
    .unwrap();
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba,
        super::PERC_IMAGE_CHUNK_SECTORS,
        0x1006000,
        place.spare_off,
        place.disk_bytes,
    )
    .is_some());
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba + 1,
        super::PERC_IMAGE_CHUNK_SECTORS,
        0x1006000,
        place.spare_off,
        place.disk_bytes,
    )
    .is_none());
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba,
        super::LIBRARY_WRITE_SECTORS,
        0x1006000,
        place.spare_off,
        place.disk_bytes,
    )
    .is_some());
    assert!(super::pack_ld_write16_tail(
        super::IRON_LD1_BYTES,
        1,
        tail_lba + 1,
        super::LIBRARY_WRITE_SECTORS,
        0x1006000,
        place.spare_off,
        place.disk_bytes,
    )
    .is_none());
    assert_eq!(
        u64::from_be_bytes(packed[0x22..0x2A].try_into().unwrap()),
        tail_lba
    );
    assert!(super::pack_ld_write16_tail(
        super::LAB_UBUNTU0_BYTES,
        1,
        tail_lba,
        1,
        0x1006000,
        place.spare_off,
        place.disk_bytes
    )
    .is_none());
    let guest0 = super::image_guest_read_slice(ten, super::PERC_IMAGE_BYTES, 0, 512, 0).unwrap();
    assert_eq!(guest0.lba, tail_lba);
    assert!(
        super::image_guest_read_slice(ten, 0, 0, 512, 0).is_none()
            || super::image_guest_read_slice(ten, 0, 0, 512, 0)
                .unwrap()
                .lba
                == 0
    );
    assert!(super::image_guest_read_slice(
        super::PERC_IMAGE_BYTES,
        super::PERC_IMAGE_BYTES / 2,
        0,
        512,
        0
    )
    .is_none());
    let alt = (super::PERC_IMAGE_BYTES / 512) - 1;
    let lba1 = image_header_sector(alt);
    assert_eq!(super::image_backup_lba(&lba1), Some(alt));
    assert!(super::image_backup_lba(&[0u8; 512]).is_none());
    assert!(super::image_backup_lba(&image_header_sector(1)).is_none());
    assert!(super::image_backup_lba(&image_header_sector(super::PERC_IMAGE_BYTES / 512)).is_none());
    assert!(super::image_backup_is_efi_part(b"EFI PART"));
    assert!(!super::image_backup_is_efi_part(&[0u8; 8]));
    assert!(!super::perc_image_boot_latched());
    assert!(super::spare_virtio_offered(512, false));
    assert!(super::spare_virtio_offered(3_169_417_691_136, false));
    assert!(!super::spare_virtio_offered(3_169_417_691_136, true));
    assert!(!super::spare_virtio_offered(511, false));
    assert!(!super::spare_virtio_offered(0, true));
    assert_eq!(super::perc_spare_bytes(), 0);
    let mut scratch = [0u8; 512];
    assert!(!super::perc_spare_read(0, &mut scratch));
    assert!(super::fusion_post_is_allowed(super::IRON_FUSION_S0));
    assert!(!super::fusion_post_is_allowed(super::MFI_STATE_OPERATIONAL));
    assert!(!super::fusion_post_is_allowed(super::MFI_STATE_FAULT));
    assert!(!super::doorbell_transition_is_allowed());
    assert!(super::fusion_regs_look_unmapped(0, 0, 0, 0));
    assert!(super::h740p_mini_subsys(0x1028, 0x1fcd));
    assert!(super::h740p_mini_subsys(0x1028, 0x1fcf));
    assert!(!super::h740p_mini_subsys(0x1028, 0x1fc9));
    assert!(!super::h740p_mini_subsys(0x1028, 0x1fcb));
}
