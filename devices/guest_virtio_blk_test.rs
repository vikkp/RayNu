use super::{
    blk_sector_rw, decode_mmio_insn, is_virtio_bar_2m_gpa, is_virtio_bar_gpa, iso_visible,
    mmio_insn_bytes_this_page,
    latch_dxe_virtio_did, mmio_read, mmio_read_iso, mmio_write, pci_addr_selects_owned,
    pci_addr_selects_slot0,
    pci_addr_selects_virtio, pci_addr_selects_virtio_iso, pci_config_addr, pci_config_addr_iso,
    pci_config_addr_slot0, pci_enumerated, pci_read_data, pci_write_addr, pci_write_data,
    pei_host_bridge_did, present, process_blk_queue_in, process_iso_queue_in, queues_armed, reset,
    take_marker, virtio_disk_evidence, GUEST_VIRTIO_BAR0_DEFAULT, GUEST_VIRTIO_BAR0_SIZE_MASK,
    GUEST_VIRTIO_ISO_BAR0_DEFAULT, GUEST_VIRTIO_PCI_DEVICE, GUEST_VIRTIO_PCI_VENDOR,
    M7_E5_OVMF_VIRTIO_OK_MARKER, VIRTIO_BLK_F_RO, VIRTIO_BLK_S_IOERR, VIRTIO_BLK_S_OK,
    VIRTIO_BLK_T_FLUSH, VIRTIO_BLK_T_IN, VIRTIO_BLK_T_OUT, VIRTIO_PCI_CAP_COMMON,
    VIRTIO_PCI_CAP_NOTIFY, VIRTIO_PCI_CAP_VNDR,
};
use crate::devices::guest_platform::{
    boot_order_cd_then_disk, pci_bdf, HOST_BRIDGE_DEVICE, HOST_BRIDGE_VENDOR,
};
use crate::devices::ide_cdrom;

#[test]
fn pci_bdf_is_probe_slot_not_ide() {
    let addr = pci_config_addr();
    assert_eq!(pci_bdf(addr), (0, 2, 0, 0));
    assert!(pci_addr_selects_virtio(addr));
    assert_eq!(pci_config_addr_slot0(), 0x8000_0000);
    assert!(pci_addr_selects_slot0(0x8000_0000));
    assert!(pci_addr_selects_owned(0x8000_0000));
    assert!(!pci_addr_selects_virtio(0x8000_0000));
    assert!(!pci_addr_selects_virtio(0x8000_0900)); // 00:01.1 IDE
    assert!(!pci_addr_selects_virtio(0x8000_0100)); // 00:00.1 IDE
    assert!(!pci_addr_selects_virtio(0x8000_4000)); // 00:08.0 host
    assert!(!pci_addr_selects_virtio(0x8000_0800)); // 00:01.0 ISA
    assert_eq!(pci_bdf(pci_config_addr_iso()), (0, 3, 0, 0));
    assert!(pci_addr_selects_virtio_iso(pci_config_addr_iso()));
    assert!(pci_addr_selects_owned(pci_config_addr_iso()));
}

#[test]
fn lab_stub_keeps_enum_cap_product_iso_gets_vendor_caps() {
    use crate::devices::ide_cdrom::{
        present as present_iso, reset as reset_cd, write_placeholder_iso, MOCK_EFI_ISO_BYTES,
        ISO_SECTOR,
    };
    reset();
    reset_cd();
    assert!(present());
    assert!(latch_dxe_virtio_did());
    pci_write_addr(pci_config_addr() | 0x40);
    assert_eq!(pci_read_data(0xCFC, 4), 0x0001_0010, "lab enum stub cap");
    assert!(!queues_armed());
    reset();
    reset_cd();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    assert!(present_iso(&iso, 9));
    assert!(present());
    assert!(queues_armed());
    assert!(latch_dxe_virtio_did());
    pci_write_addr(pci_config_addr() | 0x40);
    let cap0 = pci_read_data(0xCFC, 4);
    assert_eq!(cap0 as u8, VIRTIO_PCI_CAP_VNDR);
    assert_eq!((cap0 >> 24) as u8, VIRTIO_PCI_CAP_COMMON);
    pci_write_addr(pci_config_addr() | 0x50);
    let cap1 = pci_read_data(0xCFC, 4);
    assert_eq!(cap1 as u8, VIRTIO_PCI_CAP_VNDR);
    assert_eq!((cap1 >> 24) as u8, VIRTIO_PCI_CAP_NOTIFY);
    pci_write_addr(pci_config_addr() | 0x10);
    pci_write_data(0xCFC, 4, 0xFFFF_FFFF);
    let sz = pci_read_data(0xCFC, 4);
    assert_eq!(sz, GUEST_VIRTIO_BAR0_SIZE_MASK);
    pci_write_data(0xCFC, 4, GUEST_VIRTIO_BAR0_DEFAULT);
    assert!(is_virtio_bar_gpa(u64::from(GUEST_VIRTIO_BAR0_DEFAULT)));
    pci_write_addr(pci_config_addr() | 0x3C);
    assert_eq!(
        pci_read_data(0xCFC, 1) as u8,
        crate::devices::guest_irq::VIRTIO_PIC_IRQ
    );
    assert!(is_virtio_bar_2m_gpa(u64::from(GUEST_VIRTIO_BAR0_DEFAULT) + 0x1000));
    assert!(iso_visible());
    assert!(pci_addr_selects_virtio_iso(pci_config_addr_iso()));
    pci_write_addr(pci_config_addr_iso());
    let iso_id = pci_read_data(0xCFC, 4);
    assert_eq!(iso_id as u16, GUEST_VIRTIO_PCI_VENDOR);
    assert_eq!((iso_id >> 16) as u16, GUEST_VIRTIO_PCI_DEVICE);
    assert!(is_virtio_bar_gpa(u64::from(GUEST_VIRTIO_ISO_BAR0_DEFAULT)));
    assert_eq!(mmio_read(0x10, 2), 0xFFFF, "msix_config 16-bit");
    assert_eq!(mmio_read(0x10, 4), 0x0001_FFFF, "packed num_queues=1");
    assert_eq!(
        mmio_read_iso(0x04, 4) & VIRTIO_BLK_F_RO,
        VIRTIO_BLK_F_RO
    );
    let cap = mmio_read_iso(0x200, 8);
    assert_eq!(cap, (extra / 512) as u64);
    assert!(!crate::devices::guest_platform::is_platform_sink_gpa(
        u64::from(GUEST_VIRTIO_BAR0_DEFAULT)
    ));
    reset();
    reset_cd();
    assert!(!queues_armed());
    assert!(!iso_visible());
    assert!(!is_virtio_bar_gpa(u64::from(GUEST_VIRTIO_BAR0_DEFAULT)));
}

#[test]
fn blk_sector_rw_roundtrip_and_queue_out() {
    let mut disk = vec![0u8; 4096];
    let mut buf = [0xABu8; 512];
    assert_eq!(blk_sector_rw(&mut disk, VIRTIO_BLK_T_OUT, 0, &mut buf), VIRTIO_BLK_S_OK);
    let mut back = [0u8; 512];
    assert_eq!(blk_sector_rw(&mut disk, VIRTIO_BLK_T_IN, 0, &mut back), VIRTIO_BLK_S_OK);
    assert_eq!(back, buf);
    assert_eq!(blk_sector_rw(&mut disk, VIRTIO_BLK_T_FLUSH, 0, &mut buf), VIRTIO_BLK_S_OK);

    // Split virtqueue: header + data + status in a flat GPA image.
    let mut guest = vec![0u8; 4096];
    let qsize = 4u16;
    let desc = 0u64;
    let avail = 256u64;
    let used = 512u64;
    // desc0 header at GPA 0x300
    let hdr_gpa = 0x300u64;
    guest[hdr_gpa as usize..hdr_gpa as usize + 4].copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
    guest[hdr_gpa as usize + 8..hdr_gpa as usize + 16].copy_from_slice(&0u64.to_le_bytes());
    // desc1 data at 0x400
    let data_gpa = 0x400u64;
    guest[data_gpa as usize..data_gpa as usize + 512].fill(0x5A);
    // desc2 status at 0x700
    let st_gpa = 0x700u64;
    guest[st_gpa as usize] = 0xFF;
    fn put_desc(mem: &mut [u8], i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let o = (i as usize) * 16;
        mem[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        mem[o + 8..o + 12].copy_from_slice(&len.to_le_bytes());
        mem[o + 12..o + 14].copy_from_slice(&flags.to_le_bytes());
        mem[o + 14..o + 16].copy_from_slice(&next.to_le_bytes());
    }
    put_desc(&mut guest, 0, hdr_gpa, 16, 1, 1);
    put_desc(&mut guest, 1, data_gpa, 512, 1, 2);
    put_desc(&mut guest, 2, st_gpa, 1, 2, 0);
    guest[avail as usize + 2..avail as usize + 4].copy_from_slice(&1u16.to_le_bytes());
    guest[avail as usize + 4..avail as usize + 6].copy_from_slice(&0u16.to_le_bytes());
    let mut last = 0u16;
    let mut disk2 = vec![0u8; 4096];
    let n = process_blk_queue_in(&mut guest, &mut disk2, qsize, &mut last, desc, avail, used);
    assert_eq!(n, 512);
    assert_eq!(guest[st_gpa as usize], VIRTIO_BLK_S_OK);
    assert_eq!(disk2[0], 0x5A);
}

#[test]
fn blk_queue_out_writes_full_8k() {
    let mut guest = vec![0u8; 0x3200];
    let qsize = 4u16;
    let desc = 0u64;
    let avail = 256u64;
    let used = 512u64;
    let hdr_gpa = 0x300u64;
    guest[hdr_gpa as usize..hdr_gpa as usize + 4].copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
    guest[hdr_gpa as usize + 8..hdr_gpa as usize + 16].copy_from_slice(&0u64.to_le_bytes());
    let data_gpa = 0x1000u64;
    guest[data_gpa as usize..data_gpa as usize + 8192].fill(0x3C);
    let st_gpa = 0x3100u64;
    guest[st_gpa as usize] = 0xFF;
    fn put_desc(mem: &mut [u8], i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let o = (i as usize) * 16;
        mem[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        mem[o + 8..o + 12].copy_from_slice(&len.to_le_bytes());
        mem[o + 12..o + 14].copy_from_slice(&flags.to_le_bytes());
        mem[o + 14..o + 16].copy_from_slice(&next.to_le_bytes());
    }
    put_desc(&mut guest, 0, hdr_gpa, 16, 1, 1);
    put_desc(&mut guest, 1, data_gpa, 8192, 1, 2);
    put_desc(&mut guest, 2, st_gpa, 1, 2, 0);
    guest[avail as usize + 2..avail as usize + 4].copy_from_slice(&1u16.to_le_bytes());
    guest[avail as usize + 4..avail as usize + 6].copy_from_slice(&0u16.to_le_bytes());
    let mut last = 0u16;
    let mut disk = vec![0u8; 16384];
    let n = process_blk_queue_in(&mut guest, &mut disk, qsize, &mut last, desc, avail, used);
    assert_eq!(n, 8192, "installer OUT must not truncate at 4KiB");
    assert_eq!(guest[st_gpa as usize], VIRTIO_BLK_S_OK);
    assert_eq!(disk[0], 0x3C);
    assert_eq!(disk[8191], 0x3C);
}

#[test]
fn blk_queue_out_writes_split_data_descriptors() {
    let mut guest = vec![0u8; 4096];
    let qsize = 8u16;
    let desc = 0u64;
    let avail = 256u64;
    let used = 512u64;
    let hdr_gpa = 0x300u64;
    guest[hdr_gpa as usize..hdr_gpa as usize + 4].copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
    guest[hdr_gpa as usize + 8..hdr_gpa as usize + 16].copy_from_slice(&0u64.to_le_bytes());
    let d0 = 0x400u64;
    let d1 = 0x600u64;
    guest[d0 as usize..d0 as usize + 512].fill(0x5A);
    guest[d1 as usize..d1 as usize + 512].fill(0xA5);
    let st_gpa = 0x800u64;
    guest[st_gpa as usize] = 0xFF;
    fn put_desc(mem: &mut [u8], i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let o = (i as usize) * 16;
        mem[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        mem[o + 8..o + 12].copy_from_slice(&len.to_le_bytes());
        mem[o + 12..o + 14].copy_from_slice(&flags.to_le_bytes());
        mem[o + 14..o + 16].copy_from_slice(&next.to_le_bytes());
    }
    put_desc(&mut guest, 0, hdr_gpa, 16, 1, 1);
    put_desc(&mut guest, 1, d0, 512, 1, 2);
    put_desc(&mut guest, 2, d1, 512, 1, 3);
    put_desc(&mut guest, 3, st_gpa, 1, 2, 0);
    guest[avail as usize + 2..avail as usize + 4].copy_from_slice(&1u16.to_le_bytes());
    guest[avail as usize + 4..avail as usize + 6].copy_from_slice(&0u16.to_le_bytes());
    let mut last = 0u16;
    let mut disk = vec![0u8; 4096];
    let n = process_blk_queue_in(&mut guest, &mut disk, qsize, &mut last, desc, avail, used);
    assert_eq!(n, 1024, "installer OUT must not drop later bio_vec descriptors");
    assert_eq!(guest[st_gpa as usize], VIRTIO_BLK_S_OK);
    assert_eq!(disk[0], 0x5A);
    assert_eq!(disk[511], 0x5A);
    assert_eq!(disk[512], 0xA5);
    assert_eq!(disk[1023], 0xA5);
}

#[test]
fn install_disk_partition_table_gpt_and_mbr() {
    use super::install_disk_has_partition_table;
    let mut z = vec![0u8; 4096];
    assert!(!install_disk_has_partition_table(&z));
    z[510] = 0x55;
    z[511] = 0xAA;
    z[0x1BE + 4] = 0x83;
    assert!(install_disk_has_partition_table(&z));
    let mut gpt = vec![0u8; 4096];
    gpt[512..520].copy_from_slice(b"EFI PART");
    assert!(install_disk_has_partition_table(&gpt));
    let mut esp = vec![0u8; 4096];
    esp[512..520].copy_from_slice(b"EFI PART");
    esp[1024..1040].copy_from_slice(&[
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
        0x3B,
    ]);
    assert!(install_disk_has_partition_table(&esp));
    reset();
    assert!(!super::take_iso_install_ok());
}

#[test]
fn decode_mmio_mov_encodings() {
    let r32 = decode_mmio_insn(&[0x8B, 0x01], 2).unwrap();
    assert!(!r32.is_write && r32.size == 4 && r32.reg == 0 && r32.zero_ext);
    let w32 = decode_mmio_insn(&[0x89, 0x11], 2).unwrap();
    assert!(w32.is_write && w32.size == 4 && w32.reg == 2);
    let r16 = decode_mmio_insn(&[0x66, 0x8B, 0x01], 3).unwrap();
    assert!(!r16.is_write && r16.size == 2 && !r16.zero_ext);
    let w8 = decode_mmio_insn(&[0x88, 0x01], 2).unwrap();
    assert!(w8.is_write && w8.size == 1 && w8.reg == 0);
    let r64 = decode_mmio_insn(&[0x48, 0x8B, 0x01], 3).unwrap();
    assert!(!r64.is_write && r64.size == 8 && !r64.zero_ext);
    let imm = decode_mmio_insn(&[0xC7, 0x01, 0x78, 0x56, 0x34, 0x12], 6).unwrap();
    assert!(imm.is_write && imm.has_imm && imm.imm == 0x1234_5678);
    let r8 = decode_mmio_insn(&[0x44, 0x8B, 0x01], 3).unwrap();
    assert!(!r8.is_write && r8.reg == 8 && r8.zero_ext);
    let gs = decode_mmio_insn(&[0x65, 0x8B, 0x01], 3).unwrap();
    assert!(!gs.is_write && gs.reg == 0 && gs.size == 4);
    let zx = decode_mmio_insn(&[0x0F, 0xB6, 0x01], 3).unwrap();
    assert!(!zx.is_write && zx.size == 1 && zx.zero_ext && !zx.sign_ext && !zx.xchg);
    let sx = decode_mmio_insn(&[0x0F, 0xBE, 0x01], 3).unwrap();
    assert!(!sx.is_write && sx.size == 1 && sx.sign_ext && !sx.zero_ext);
    let xchg = decode_mmio_insn(&[0x87, 0x01], 2).unwrap();
    assert!(xchg.xchg && xchg.is_write && xchg.size == 4 && xchg.reg == 0);
    let moffs = decode_mmio_insn(&[0xA1, 0, 0, 0, 0, 0, 0, 0, 0], 9).unwrap();
    assert!(!moffs.is_write && moffs.reg == 0 && moffs.size == 4 && moffs.zero_ext);
    let ah = decode_mmio_insn(&[0x88, 0x21], 2).unwrap();
    assert!(ah.is_write && ah.size == 1 && ah.reg == 4 && !ah.rex);
    let spl = decode_mmio_insn(&[0x40, 0x88, 0x21], 3).unwrap();
    assert!(spl.rex && spl.reg == 4 && spl.size == 1);
    let andb = decode_mmio_insn(&[0x80, 0x21, 0x0F], 3).unwrap();
    assert!(andb.is_write && andb.has_imm && andb.alu == super::MMIO_ALU_AND);
    assert_eq!(andb.imm, 0x0F);
    assert_eq!(andb.size, 1);
    let orl = decode_mmio_insn(&[0x83, 0x09, 0x01], 3).unwrap();
    assert!(orl.alu == super::MMIO_ALU_OR && orl.size == 4 && orl.imm == 1);
    let orb = decode_mmio_insn(&[0x08, 0x01], 2).unwrap();
    assert!(orb.is_write && orb.alu == super::MMIO_ALU_OR && orb.size == 1 && !orb.has_imm);
    let addl = decode_mmio_insn(&[0x81, 0x01, 0x01, 0, 0, 0], 6).unwrap();
    assert!(addl.alu == super::MMIO_ALU_ADD && addl.has_imm && addl.imm == 1);
    let testb = decode_mmio_insn(&[0xF6, 0x00, 0x01], 3).unwrap();
    assert!(testb.test && testb.has_imm && testb.imm == 1 && !testb.is_write && !testb.cmp);
    let testr = decode_mmio_insn(&[0x84, 0x01], 2).unwrap();
    assert!(testr.test && testr.reg == 0 && testr.size == 1);
    let cmpb = decode_mmio_insn(&[0x80, 0x38, 0x00], 3).unwrap();
    assert!(cmpb.cmp && cmpb.has_imm && !cmpb.is_write && !cmpb.test && !cmpb.cmp_reg_left);
    let cmpr = decode_mmio_insn(&[0x3B, 0x01], 2).unwrap();
    assert!(cmpr.cmp && cmpr.cmp_reg_left && cmpr.size == 4 && !cmpr.is_write);
    let subl = decode_mmio_insn(&[0x83, 0x29, 0x01], 3).unwrap();
    assert!(subl.alu == super::MMIO_ALU_SUB && subl.is_write && subl.imm == 1);
    assert_eq!(super::mmio_alu_apply(5, 2, super::MMIO_ALU_SUB), 3);
    assert_eq!(super::mmio_test_rflags(2, 0, 1) & (1 << 6), 1 << 6);
    assert_eq!(super::mmio_test_rflags(2, 1, 1) & (1 << 6), 0);
    assert_eq!(super::mmio_cmp_rflags(2, 1, 2, 1) & 1, 1);
    assert_eq!(
        super::mmio_alu_apply(0xF0, 0x0F, super::MMIO_ALU_AND),
        0x00
    );
    assert_eq!(super::mmio_alu_apply(1, 2, super::MMIO_ALU_ADD), 3);
    assert_eq!(mmio_insn_bytes_this_page(0x1000, 16), 16);
    assert_eq!(mmio_insn_bytes_this_page(0x1FFC, 16), 4);
}

#[test]
fn mmio_write_queue_desc_keeps_high_half_on_writeq() {
    use crate::devices::ide_cdrom::{
        present as present_iso, reset as reset_cd, write_placeholder_iso, MOCK_EFI_ISO_BYTES,
        ISO_SECTOR,
    };
    reset();
    reset_cd();
    let extra = MOCK_EFI_ISO_BYTES + ISO_SECTOR;
    let mut iso = vec![0u8; extra];
    write_placeholder_iso(&mut iso[..MOCK_EFI_ISO_BYTES]);
    assert!(present_iso(&iso, 9));
    assert!(present());
    assert!(queues_armed());
    let gpa = 0x0000_0001_2345_6000u64;
    mmio_write(0x20, 8, gpa);
    assert_eq!(mmio_read(0x20, 8), gpa);
    mmio_write(0x20, 4, 0xABCD_0000);
    mmio_write(0x24, 4, 0x0000_0002);
    assert_eq!(mmio_read(0x20, 8), 0x0000_0002_ABCD_0000);
    reset();
    reset_cd();
}

#[test]
fn present_enumerates_virtio_and_cd_then_disk() {
    reset();
    ide_cdrom::reset();
    assert!(boot_order_cd_then_disk());
    assert!(!virtio_disk_evidence(false, true, true));
    assert!(!virtio_disk_evidence(true, false, true));
    assert!(!virtio_disk_evidence(true, true, false));
    assert!(present());
    pci_write_addr(pci_config_addr_slot0());
    let pei_id = pci_read_data(0xCFC, 4);
    assert!(pei_host_bridge_did());
    assert_eq!(pei_id as u16, HOST_BRIDGE_VENDOR);
    assert_eq!((pei_id >> 16) as u16, HOST_BRIDGE_DEVICE);
    // PEI DID probe: CF8=0x80000002 + inw(CFC) at 00:00.0 — i440FX HostBridgeDevId.
    pci_write_addr(0x8000_0002);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(HOST_BRIDGE_DEVICE)
    );
    assert!(!pci_enumerated(), "PEI i440FX DID is not virtio enum");
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF, "virtio hidden until latch");
    assert!(latch_dxe_virtio_did());
    assert!(!pei_host_bridge_did());
    // CpuDxe AcpiTimerLib still reads OVMF_HOSTBRIDGE_DID at 00:00.0.
    pci_write_addr(0x8000_0002);
    assert_eq!(
        pci_read_data(0xCFC, 2) & 0xffff,
        u32::from(HOST_BRIDGE_DEVICE)
    );
    pci_write_addr(pci_config_addr());
    let id = pci_read_data(0xCFC, 4);
    assert_eq!(id as u16, GUEST_VIRTIO_PCI_VENDOR);
    assert_eq!((id >> 16) as u16, GUEST_VIRTIO_PCI_DEVICE);
    assert!(virtio_disk_evidence(true, true, true));
    assert!(take_marker());
    assert!(!take_marker());
    assert_eq!(M7_E5_OVMF_VIRTIO_OK_MARKER, "RAYNU-V-M7-E5-OVMF-VIRTIO-OK");
    reset();
}

#[test]
fn virtio_fn0_is_multifunction() {
    use crate::devices::guest_platform::pci_header_is_multifunction;
    reset();
    assert!(present());
    assert!(latch_dxe_virtio_did());
    pci_write_addr(pci_config_addr() | 0x0C);
    let ht = pci_read_data(0xCFC, 4);
    assert!(pci_header_is_multifunction(ht));
    pci_write_addr(pci_config_addr_slot0() | 0x0C);
    let slot0_ht = pci_read_data(0xCFC, 4);
    assert!(pci_header_is_multifunction(slot0_ht));
    reset();
}

#[test]
fn unpresented_pci_is_empty() {
    reset();
    pci_write_addr(pci_config_addr());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    pci_write_addr(pci_config_addr_iso());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}

#[test]
fn lab_stub_hides_iso_slot3() {
    reset();
    ide_cdrom::reset();
    assert!(present());
    assert!(latch_dxe_virtio_did());
    assert!(!iso_visible());
    pci_write_addr(pci_config_addr_iso());
    assert_eq!(pci_read_data(0xCFC, 4), 0xFFFF_FFFF);
    reset();
}

#[test]
fn iso_queue_in_copies_and_rejects_out() {
    let mut guest = vec![0u8; 4096];
    let mut iso = vec![0u8; 2048];
    iso[..512].fill(0xAB);
    let qsize = 4u16;
    let desc = 0u64;
    let avail = 256u64;
    let used = 512u64;
    let hdr_gpa = 0x300u64;
    guest[hdr_gpa as usize..hdr_gpa as usize + 4].copy_from_slice(&VIRTIO_BLK_T_IN.to_le_bytes());
    let data_gpa = 0x400u64;
    let st_gpa = 0x700u64;
    guest[st_gpa as usize] = 0xFF;
    fn put_desc(mem: &mut [u8], i: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let o = (i as usize) * 16;
        mem[o..o + 8].copy_from_slice(&addr.to_le_bytes());
        mem[o + 8..o + 12].copy_from_slice(&len.to_le_bytes());
        mem[o + 12..o + 14].copy_from_slice(&flags.to_le_bytes());
        mem[o + 14..o + 16].copy_from_slice(&next.to_le_bytes());
    }
    put_desc(&mut guest, 0, hdr_gpa, 16, 1, 1);
    put_desc(&mut guest, 1, data_gpa, 512, 3, 2); // NEXT | WRITE (device fills guest)
    put_desc(&mut guest, 2, st_gpa, 1, 2, 0);
    guest[avail as usize + 2..avail as usize + 4].copy_from_slice(&1u16.to_le_bytes());
    guest[avail as usize + 4..avail as usize + 6].copy_from_slice(&0u16.to_le_bytes());
    let mut last = 0u16;
    let n = process_iso_queue_in(&mut guest, &mut iso, qsize, &mut last, desc, avail, used);
    assert_eq!(n, 512);
    assert_eq!(guest[st_gpa as usize], VIRTIO_BLK_S_OK);
    assert_eq!(guest[data_gpa as usize], 0xAB);
    guest[hdr_gpa as usize..hdr_gpa as usize + 4].copy_from_slice(&VIRTIO_BLK_T_OUT.to_le_bytes());
    put_desc(&mut guest, 1, data_gpa, 512, 1, 2); // NEXT, guest-to-device
    guest[avail as usize + 2..avail as usize + 4].copy_from_slice(&2u16.to_le_bytes());
    guest[avail as usize + 6..avail as usize + 8].copy_from_slice(&0u16.to_le_bytes());
    guest[st_gpa as usize] = 0xFF;
    iso[0] = 0xAB;
    let n2 = process_iso_queue_in(&mut guest, &mut iso, qsize, &mut last, desc, avail, used);
    assert_eq!(n2, 0);
    assert_eq!(guest[st_gpa as usize], VIRTIO_BLK_S_IOERR);
    assert_eq!(iso[0], 0xAB, "read-only ISO must not take OUT");
}

#[test]
fn virtio_gpa_copy_stops_at_4k_so_report_ram_slots_do_not_bleed() {
    let mut lo = [0u8; 4096];
    let mut hi = [0u8; 4096];
    lo[4090..].fill(0x11);
    hi[..6].fill(0x22);
    let lo_p = lo.as_mut_ptr() as u64;
    let hi_p = hi.as_mut_ptr() as u64;
    let translate = |gpa: u64| {
        if gpa < 4096 {
            Some(lo_p + gpa)
        } else if gpa < 8192 {
            Some(hi_p + (gpa - 4096))
        } else {
            None
        }
    };
    let mut dst = [0u8; 12];
    assert!(super::read_bytes(&translate, 4090, &mut dst));
    assert_eq!(&dst[..6], &[0x11; 6]);
    assert_eq!(&dst[6..], &[0x22; 6]);
    let src = [0xABu8; 12];
    assert!(super::write_bytes(&translate, 4090, &src));
    assert_eq!(&lo[4090..], &[0xAB; 6]);
    assert_eq!(&hi[..6], &[0xAB; 6]);
}
