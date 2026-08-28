use super::{
    blk_sector_rw, decode_mmio_insn, is_virtio_bar_2m_gpa, is_virtio_bar_gpa, iso_visible,
    latch_dxe_virtio_did, mmio_insn_bytes_this_page, mmio_read, mmio_read_iso, mmio_write,
    pci_addr_selects_owned, pci_addr_selects_slot0, pci_addr_selects_virtio,
    pci_addr_selects_virtio_iso, pci_config_addr, pci_config_addr_iso, pci_config_addr_slot0,
    pci_enumerated, pci_read_data, pci_write_addr, pci_write_data, pei_host_bridge_did, present,
    process_blk_queue_in, process_iso_queue_in, queues_armed, reset, take_marker,
    virtio_disk_evidence, GUEST_VIRTIO_BAR0_DEFAULT, GUEST_VIRTIO_BAR0_SIZE_MASK,
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
        present as present_iso, reset as reset_cd, write_placeholder_iso, ISO_SECTOR,
        MOCK_EFI_ISO_BYTES,
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
    assert!(is_virtio_bar_2m_gpa(
        u64::from(GUEST_VIRTIO_BAR0_DEFAULT) + 0x1000
    ));
    assert!(iso_visible());
    assert!(pci_addr_selects_virtio_iso(pci_config_addr_iso()));
    pci_write_addr(pci_config_addr_iso());
    let iso_id = pci_read_data(0xCFC, 4);
    assert_eq!(iso_id as u16, GUEST_VIRTIO_PCI_VENDOR);
    assert_eq!((iso_id >> 16) as u16, GUEST_VIRTIO_PCI_DEVICE);
    assert!(is_virtio_bar_gpa(u64::from(GUEST_VIRTIO_ISO_BAR0_DEFAULT)));
    assert_eq!(mmio_read(0x10, 2), 0xFFFF, "msix_config 16-bit");
    assert_eq!(mmio_read(0x10, 4), 0x0001_FFFF, "packed num_queues=1");
    assert_eq!(mmio_read_iso(0x04, 4) & VIRTIO_BLK_F_RO, VIRTIO_BLK_F_RO);
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
    assert_eq!(
        blk_sector_rw(&mut disk, VIRTIO_BLK_T_OUT, 0, &mut buf),
        VIRTIO_BLK_S_OK
    );
    let mut back = [0u8; 512];
    assert_eq!(
        blk_sector_rw(&mut disk, VIRTIO_BLK_T_IN, 0, &mut back),
        VIRTIO_BLK_S_OK
    );
    assert_eq!(back, buf);
    assert_eq!(
        blk_sector_rw(&mut disk, VIRTIO_BLK_T_FLUSH, 0, &mut buf),
        VIRTIO_BLK_S_OK
    );

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
    assert_eq!(
        n, 1024,
        "installer OUT must not drop later bio_vec descriptors"
    );
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
    let add_rm = decode_mmio_insn(&[0x03, 0x01], 2).unwrap();
    assert!(
        add_rm.alu == super::MMIO_ALU_ADD
            && add_rm.alu_reg_left
            && !add_rm.is_write
            && add_rm.size == 4
            && add_rm.zero_ext
    );
    let sub_rm = decode_mmio_insn(&[0x2B, 0x01], 2).unwrap();
    assert!(sub_rm.alu == super::MMIO_ALU_SUB && sub_rm.alu_reg_left && !sub_rm.is_write);
    let and_rm = decode_mmio_insn(&[0x23, 0x01], 2).unwrap();
    assert!(and_rm.alu == super::MMIO_ALU_AND && and_rm.alu_reg_left);
    let incb = decode_mmio_insn(&[0xFE, 0x00], 2).unwrap();
    assert!(incb.alu == super::MMIO_ALU_ADD && incb.is_write && incb.has_imm && incb.imm == 1);
    let decl = decode_mmio_insn(&[0xFF, 0x08], 2).unwrap();
    assert!(decl.alu == super::MMIO_ALU_SUB && decl.is_write && decl.imm == 1 && decl.size == 4);
    let notb = decode_mmio_insn(&[0xF6, 0x10], 2).unwrap();
    assert!(notb.alu == super::MMIO_ALU_NOT && notb.is_write && !notb.has_imm);
    let negl = decode_mmio_insn(&[0xF7, 0x18], 2).unwrap();
    assert!(negl.alu == super::MMIO_ALU_NEG && negl.is_write && negl.size == 4);
    let btimm = decode_mmio_insn(&[0x0F, 0xBA, 0x20, 0x05], 4).unwrap();
    assert!(btimm.bt == super::MMIO_BT && btimm.has_imm && btimm.imm == 5 && !btimm.is_write);
    let btsr = decode_mmio_insn(&[0x0F, 0xAB, 0x01], 3).unwrap();
    assert!(btsr.bt == super::MMIO_BTS && btsr.is_write && btsr.reg == 0 && !btsr.has_imm);
    let (new, was) = super::mmio_bt_apply(0x10, 4, 4, super::MMIO_BTS);
    assert!(was && new == 0x10);
    let (new2, was2) = super::mmio_bt_apply(0, 0, 4, super::MMIO_BTS);
    assert!(!was2 && new2 == 1);
    assert_eq!(super::mmio_bt_rflags(2, true) & 1, 1);
    assert_eq!(super::mmio_bt_rflags(3, false) & 1, 0);
    let cx = decode_mmio_insn(&[0x0F, 0xB1, 0x01], 3).unwrap();
    assert!(cx.atomic == super::MMIO_CMPXCHG && cx.is_write && cx.reg == 0 && cx.size == 4);
    let xa = decode_mmio_insn(&[0x0F, 0xC1, 0x01], 3).unwrap();
    assert!(xa.atomic == super::MMIO_XADD && xa.is_write && xa.zero_ext);
    let c8 = decode_mmio_insn(&[0x0F, 0xC7, 0x09], 3).unwrap();
    assert!(
        c8.atomic == super::MMIO_CMPXCHG8B && c8.is_write && c8.size == 8 && c8.reg == 0
    );
    let c8l = decode_mmio_insn(&[0xF0, 0x0F, 0xC7, 0x09], 4).unwrap();
    assert!(c8l.atomic == super::MMIO_CMPXCHG8B && c8l.size == 8);
    assert!(decode_mmio_insn(&[0x48, 0x0F, 0xC7, 0x09], 4).is_none());
    assert!(decode_mmio_insn(&[0x0F, 0xC7, 0x31], 3).is_none());
    assert!(decode_mmio_insn(&[0x0F, 0xC7, 0x28], 3).is_none());
    let (m, hit) = super::mmio_cmpxchg8b_apply(1, 1, 2);
    assert!(hit && m == 2);
    let (m2, hit2) = super::mmio_cmpxchg8b_apply(1, 0, 2);
    assert!(!hit2 && m2 == 1);
    let adcl = decode_mmio_insn(&[0x11, 0x01], 2).unwrap();
    assert!(adcl.alu == super::MMIO_ALU_ADC && adcl.is_write && !adcl.alu_reg_left);
    let adc_rm = decode_mmio_insn(&[0x13, 0x01], 2).unwrap();
    assert!(
        adc_rm.alu == super::MMIO_ALU_ADC && adc_rm.alu_reg_left && !adc_rm.is_write
    );
    let sbbi = decode_mmio_insn(&[0x83, 0x19, 0x01], 3).unwrap();
    assert!(sbbi.alu == super::MMIO_ALU_SBB && sbbi.has_imm && sbbi.imm == 1);
    let adci = decode_mmio_insn(&[0x80, 0x10, 0x01], 3).unwrap();
    assert!(adci.alu == super::MMIO_ALU_ADC && adci.has_imm && adci.is_write);
    assert_eq!(
        super::mmio_alu_apply_cf(0xff, 0, super::MMIO_ALU_ADC, true) & 0xff,
        0
    );
    assert_eq!(super::mmio_adc_rflags(3, 0xff, 0, 1) & 1, 1);
    assert_eq!(
        super::mmio_alu_apply_cf(1, 1, super::MMIO_ALU_SBB, true) & 0xff,
        0xff
    );
    assert_eq!(super::mmio_sbb_rflags(3, 1, 1, 1) & 1, 1);
    assert_eq!(super::mmio_adc_rflags(2, 1, 1, 1) & 1, 0);
    let shl = decode_mmio_insn(&[0xC1, 0x21, 0x01], 3).unwrap();
    assert!(shl.alu == super::MMIO_ALU_SHL && shl.has_imm && shl.imm == 1 && shl.is_write);
    let shr1 = decode_mmio_insn(&[0xD0, 0x29], 2).unwrap();
    assert!(shr1.alu == super::MMIO_ALU_SHR && shr1.has_imm && shr1.imm == 1 && shr1.size == 1);
    let sarcl = decode_mmio_insn(&[0xD3, 0x39], 2).unwrap();
    assert!(sarcl.alu == super::MMIO_ALU_SAR && !sarcl.has_imm);
    assert_eq!(
        super::mmio_shift_apply(1, 1, super::MMIO_ALU_SHL, 1, false) & 0xff,
        2
    );
    assert_eq!(
        super::mmio_shift_apply(0x80, 1, super::MMIO_ALU_SAR, 1, false) & 0xff,
        0xc0
    );
    assert_eq!(super::mmio_shift_rflags(2, 0x80, 1, 0, super::MMIO_ALU_SHL, 1) & 1, 1);
    assert_eq!(
        super::mmio_shift_apply(0, 1, super::MMIO_ALU_RCL, 1, true) & 0xff,
        1
    );
    let cmovz = decode_mmio_insn(&[0x0F, 0x44, 0x01], 3).unwrap();
    assert!(cmovz.cc == 5 && !cmovz.is_write && cmovz.reg == 0 && cmovz.size == 4);
    let setnz = decode_mmio_insn(&[0x0F, 0x95, 0x01], 3).unwrap();
    assert!(setnz.cc == 6 && setnz.is_write && setnz.size == 1);
    assert!(super::mmio_cc_taken(5, 1 << 6));
    assert!(!super::mmio_cc_taken(6, 1 << 6));
    assert!(super::mmio_cc_taken(6, 2));
    let pref = decode_mmio_insn(&[0x0F, 0x18, 0x00], 3).unwrap();
    assert!(pref.alu == super::MMIO_ALU_HINT && !pref.is_write);
    let clflush = decode_mmio_insn(&[0x0F, 0xAE, 0x38], 3).unwrap();
    assert!(clflush.alu == super::MMIO_ALU_HINT);
    assert!(decode_mmio_insn(&[0x0F, 0xAE, 0x10], 3).is_none());
    let bsf = decode_mmio_insn(&[0x0F, 0xBC, 0x01], 3).unwrap();
    assert!(
        bsf.alu == super::MMIO_ALU_BSF && bsf.alu_reg_left && !bsf.is_write && bsf.size == 4
    );
    let bsr = decode_mmio_insn(&[0x48, 0x0F, 0xBD, 0x01], 4).unwrap();
    assert!(bsr.alu == super::MMIO_ALU_BSR && bsr.size == 8);
    let (idx, z) = super::mmio_scan_apply(0x10, 4, false);
    assert!(!z && idx == 4);
    let (idx2, z2) = super::mmio_scan_apply(0, 4, false);
    assert!(z2 && idx2 == 0);
    let (idx3, z3) = super::mmio_scan_apply(0x80, 1, true);
    assert!(!z3 && idx3 == 7);
    assert_eq!(super::mmio_scan_rflags(2, true) & (1 << 6), 1 << 6);
    assert_eq!(super::mmio_scan_rflags(2 | (1 << 6), false) & (1 << 6), 0);
    assert!(super::mmio_alu_is_hint(super::MMIO_ALU_HINT));
    assert!(super::mmio_alu_is_scan(super::MMIO_ALU_BSF));
    let tz = decode_mmio_insn(&[0xF3, 0x0F, 0xBC, 0x01], 4).unwrap();
    assert!(
        tz.alu == super::MMIO_ALU_TZCNT && tz.alu_reg_left && !tz.is_write && tz.size == 4
    );
    let lz = decode_mmio_insn(&[0xF3, 0x0F, 0xBD, 0x01], 4).unwrap();
    assert!(lz.alu == super::MMIO_ALU_LZCNT && lz.size == 4);
    let pc = decode_mmio_insn(&[0xF3, 0x0F, 0xB8, 0x01], 4).unwrap();
    assert!(pc.alu == super::MMIO_ALU_POPCNT && pc.alu_reg_left && pc.size == 4);
    let pcq = decode_mmio_insn(&[0xF3, 0x48, 0x0F, 0xB8, 0x01], 5).unwrap();
    assert!(pcq.size == 8 && pcq.alu == super::MMIO_ALU_POPCNT);
    let push = decode_mmio_insn(&[0xFF, 0x30], 2).unwrap();
    assert!(!push.is_write && push.alu == super::MMIO_ALU_PUSH && push.size == 4);
    let pushq = decode_mmio_insn(&[0x48, 0xFF, 0x30], 3).unwrap();
    assert!(pushq.size == 8 && pushq.alu == super::MMIO_ALU_PUSH);
    let pushw = decode_mmio_insn(&[0x66, 0xFF, 0x30], 3).unwrap();
    assert!(pushw.size == 2 && pushw.alu == super::MMIO_ALU_PUSH);
    let pop = decode_mmio_insn(&[0x8F, 0x00], 2).unwrap();
    assert!(pop.is_write && pop.alu == super::MMIO_ALU_POP && pop.size == 4);
    let popq = decode_mmio_insn(&[0x48, 0x8F, 0x00], 3).unwrap();
    assert!(popq.size == 8 && popq.alu == super::MMIO_ALU_POP);
    let popw = decode_mmio_insn(&[0x66, 0x8F, 0x00], 3).unwrap();
    assert!(popw.size == 2 && popw.alu == super::MMIO_ALU_POP);
    assert!(decode_mmio_insn(&[0x8F, 0x08], 2).is_none());
    let call = decode_mmio_insn(&[0xFF, 0x10], 2).unwrap();
    assert!(!call.is_write && call.alu == super::MMIO_ALU_CALL && call.size == 4);
    let callq = decode_mmio_insn(&[0x48, 0xFF, 0x10], 3).unwrap();
    assert!(callq.size == 8 && callq.alu == super::MMIO_ALU_CALL);
    let callw = decode_mmio_insn(&[0x66, 0xFF, 0x10], 3).unwrap();
    assert!(callw.size == 2 && callw.alu == super::MMIO_ALU_CALL);
    let jmp = decode_mmio_insn(&[0xFF, 0x20], 2).unwrap();
    assert!(!jmp.is_write && jmp.alu == super::MMIO_ALU_JMP && jmp.size == 4);
    let jmpq = decode_mmio_insn(&[0x48, 0xFF, 0x20], 3).unwrap();
    assert!(jmpq.size == 8 && jmpq.alu == super::MMIO_ALU_JMP);
    assert!(decode_mmio_insn(&[0xFF, 0x18], 2).is_none());
    assert!(decode_mmio_insn(&[0xFF, 0x28], 2).is_none());
    assert!(decode_mmio_insn(&[0xFE, 0x30], 2).is_none());
    assert_eq!(super::mmio_stack_width(4, true), 8);
    assert_eq!(super::mmio_stack_width(2, true), 2);
    assert_eq!(super::mmio_stack_width(4, false), 4);
    assert_eq!(super::mmio_stack_width(2, false), 2);
    assert!(super::mmio_alu_is_push(super::MMIO_ALU_PUSH));
    assert!(super::mmio_alu_is_pop(super::MMIO_ALU_POP));
    assert!(super::mmio_alu_is_call(super::MMIO_ALU_CALL));
    assert!(super::mmio_alu_is_jmp(super::MMIO_ALU_JMP));
    assert!(!super::mmio_alu_is_shift(super::MMIO_ALU_PUSH));
    assert!(!super::mmio_alu_is_shift(super::MMIO_ALU_POP));
    assert!(!super::mmio_alu_is_shift(super::MMIO_ALU_CALL));
    assert!(!super::mmio_alu_is_call(super::MMIO_ALU_PUSH));
    assert!(!super::mmio_alu_is_jmp(super::MMIO_ALU_CALL));
    let movsb = decode_mmio_insn(&[0xA4], 1).unwrap();
    assert!(movsb.alu == super::MMIO_ALU_MOVS && movsb.size == 1 && !movsb.has_imm);
    let rep_movsb = decode_mmio_insn(&[0xF3, 0xA4], 2).unwrap();
    assert!(rep_movsb.has_imm && rep_movsb.alu == super::MMIO_ALU_MOVS && rep_movsb.size == 1);
    let movsd = decode_mmio_insn(&[0xA5], 1).unwrap();
    assert!(movsd.alu == super::MMIO_ALU_MOVS && movsd.size == 4);
    let movsq = decode_mmio_insn(&[0x48, 0xA5], 2).unwrap();
    assert!(movsq.size == 8 && movsq.alu == super::MMIO_ALU_MOVS);
    let movsw = decode_mmio_insn(&[0x66, 0xA5], 2).unwrap();
    assert!(movsw.size == 2 && movsw.alu == super::MMIO_ALU_MOVS);
    let stosb = decode_mmio_insn(&[0xAA], 1).unwrap();
    assert!(stosb.is_write && stosb.alu == super::MMIO_ALU_STOS && stosb.size == 1);
    let stosq = decode_mmio_insn(&[0x48, 0xAB], 2).unwrap();
    assert!(stosq.size == 8 && stosq.alu == super::MMIO_ALU_STOS);
    let lodsd = decode_mmio_insn(&[0xAD], 1).unwrap();
    assert!(!lodsd.is_write && lodsd.alu == super::MMIO_ALU_LODS && lodsd.size == 4);
    let lodsb = decode_mmio_insn(&[0xAC], 1).unwrap();
    assert!(lodsb.alu == super::MMIO_ALU_LODS && lodsb.size == 1);
    let cmpsb = decode_mmio_insn(&[0xA6], 1).unwrap();
    assert!(cmpsb.alu == super::MMIO_ALU_CMPS && !cmpsb.is_write && cmpsb.size == 1);
    let cmpsd = decode_mmio_insn(&[0xA7], 1).unwrap();
    assert!(cmpsd.alu == super::MMIO_ALU_CMPS && cmpsd.size == 4);
    let repe = decode_mmio_insn(&[0xF3, 0xA7], 2).unwrap();
    assert!(repe.has_imm && repe.imm == 0 && repe.alu == super::MMIO_ALU_CMPS);
    let scasb = decode_mmio_insn(&[0xAE], 1).unwrap();
    assert!(scasb.alu == super::MMIO_ALU_SCAS && !scasb.is_write && scasb.size == 1);
    let repne = decode_mmio_insn(&[0xF2, 0xAE], 2).unwrap();
    assert!(repne.has_imm && repne.imm == 1 && repne.alu == super::MMIO_ALU_SCAS);
    assert!(super::mmio_alu_is_cmps(super::MMIO_ALU_CMPS));
    assert!(super::mmio_alu_is_scas(super::MMIO_ALU_SCAS));
    assert!(super::mmio_alu_is_string(super::MMIO_ALU_CMPS));
    assert!(super::mmio_alu_is_string(super::MMIO_ALU_SCAS));
    assert!(super::mmio_alu_is_string(super::MMIO_ALU_MOVS));
    assert!(super::mmio_alu_is_stos(super::MMIO_ALU_STOS));
    assert!(super::mmio_alu_is_lods(super::MMIO_ALU_LODS));
    assert!(!super::mmio_alu_is_string(super::MMIO_ALU_PUSH));
    let shld_ok = decode_mmio_insn(&[0x0F, 0xA4, 0x01, 0x08], 4).unwrap();
    assert!(shld_ok.alu == super::MMIO_ALU_SHLD);
    assert!(decode_mmio_insn(&[0x0F, 0xB8, 0x01], 3).is_none());
    let (t0, z0) = super::mmio_tzcnt_apply(0, 4);
    assert!(z0 && t0 == 32);
    let (t1, z1) = super::mmio_tzcnt_apply(0x10, 4);
    assert!(!z1 && t1 == 4);
    let (l0, lz0) = super::mmio_lzcnt_apply(0, 4);
    assert!(lz0 && l0 == 32);
    let (l1, lz1) = super::mmio_lzcnt_apply(1, 4);
    assert!(!lz1 && l1 == 31);
    let (l2, lz2) = super::mmio_lzcnt_apply(0x8000_0000, 4);
    assert!(!lz2 && l2 == 0);
    assert_eq!(super::mmio_popcnt_apply(0xF0, 1), 4);
    assert_eq!(super::mmio_tzcnt_rflags(2, 32, true) & 1, 1);
    assert_eq!(super::mmio_tzcnt_rflags(2, 0, false) & (1 << 6), 1 << 6);
    assert_eq!(super::mmio_popcnt_rflags(2, true) & (1 << 6), 1 << 6);
    assert!(super::mmio_alu_is_count_zero(super::MMIO_ALU_TZCNT));
    assert!(super::mmio_alu_is_popcnt(super::MMIO_ALU_POPCNT));
    assert!(!super::mmio_alu_is_scan(super::MMIO_ALU_TZCNT));
    let imul = decode_mmio_insn(&[0x0F, 0xAF, 0x01], 3).unwrap();
    assert!(
        imul.alu == super::MMIO_ALU_IMUL && imul.alu_reg_left && !imul.has_imm && imul.size == 4
    );
    let imuli = decode_mmio_insn(&[0x6B, 0x01, 0x03], 3).unwrap();
    assert!(imuli.alu == super::MMIO_ALU_IMUL && imuli.has_imm && imuli.imm == 3);
    let (p, ov) = super::mmio_imul_apply(4, 5, 4);
    assert!(!ov && p == 20);
    let (p2, ov2) = super::mmio_imul_apply(0x7fff_ffff, 2, 4);
    assert!(ov2 && p2 == 0xffff_fffe);
    assert_eq!(super::mmio_imul_rflags(2, true) & 1, 1);
    assert!(super::mmio_alu_is_imul(super::MMIO_ALU_IMUL));
    let mulb = decode_mmio_insn(&[0xF6, 0x20], 2).unwrap();
    assert!(mulb.alu == super::MMIO_ALU_MUL && mulb.size == 1 && mulb.alu_reg_left);
    let imul1 = decode_mmio_insn(&[0xF7, 0x29], 2).unwrap();
    assert!(imul1.alu == super::MMIO_ALU_IMUL1 && imul1.size == 4);
    let (lo, hi, ov) = super::mmio_mul_pair_apply(2, 3, 4, false);
    assert!(!ov && lo == 6 && hi == 0);
    let (lo2, hi2, ov2) = super::mmio_mul_pair_apply(0x8000_0000, 2, 4, false);
    assert!(ov2 && lo2 == 0 && hi2 == 1);
    let (lo3, _, ov3) = super::mmio_mul_pair_apply(0x80, 2, 1, false);
    assert!(ov3 && lo3 == 0x100);
    assert!(super::mmio_alu_is_mul_pair(super::MMIO_ALU_MUL));
    let divb = decode_mmio_insn(&[0xF6, 0x30], 2).unwrap();
    assert!(divb.alu == super::MMIO_ALU_DIV && divb.size == 1 && divb.alu_reg_left);
    let idiv = decode_mmio_insn(&[0xF7, 0x39], 2).unwrap();
    assert!(idiv.alu == super::MMIO_ALU_IDIV && idiv.size == 4);
    let idivq = decode_mmio_insn(&[0x48, 0xF7, 0x31], 3).unwrap();
    assert!(idivq.alu == super::MMIO_ALU_DIV && idivq.size == 8);
    let (q, r) = super::mmio_div_apply(10, 0, 3, 4, false).unwrap();
    assert_eq!(q, 3);
    assert_eq!(r, 1);
    let (axb, _) = super::mmio_div_apply(0x000A, 0, 3, 1, false).unwrap();
    assert_eq!(axb, 0x0103);
    assert!(super::mmio_div_apply(10, 0, 0, 4, false).is_none());
    assert!(super::mmio_div_apply(0x100, 0, 1, 1, false).is_none());
    assert!(super::mmio_div_apply(0, 1, 1, 4, false).is_none());
    let (qs, rs) = super::mmio_div_apply(0xFFFF_FFFF, 0xFFFF_FFFF, 1, 4, true).unwrap();
    assert_eq!(qs as i32, -1);
    assert_eq!(rs, 0);
    assert!(super::mmio_div_apply(0x8000, 0, 0xFF, 1, true).is_none());
    assert_eq!(super::MMIO_DIV_DE_INTR_INFO, 0x8000_0300);
    assert!(super::mmio_alu_is_div_pair(super::MMIO_ALU_DIV));
    let shld = decode_mmio_insn(&[0x0F, 0xA4, 0x01, 0x08], 4).unwrap();
    assert!(
        shld.alu == super::MMIO_ALU_SHLD
            && shld.is_write
            && shld.has_imm
            && shld.imm == 8
            && shld.size == 4
            && shld.reg == 0
    );
    let shrdcl = decode_mmio_insn(&[0x0F, 0xAD, 0x01], 3).unwrap();
    assert!(
        shrdcl.alu == super::MMIO_ALU_SHRD && shrdcl.is_write && !shrdcl.has_imm && shrdcl.size == 4
    );
    let shldq = decode_mmio_insn(&[0x48, 0x0F, 0xA4, 0x01, 0x04], 5).unwrap();
    assert!(shldq.size == 8 && shldq.alu == super::MMIO_ALU_SHLD && shldq.imm == 4);
    let shldw = decode_mmio_insn(&[0x66, 0x0F, 0xAC, 0x01, 0x01], 5).unwrap();
    assert!(shldw.size == 2 && shldw.alu == super::MMIO_ALU_SHRD && shldw.has_imm);
    let shldr = decode_mmio_insn(&[0x44, 0x0F, 0xA4, 0x01, 0x04], 5).unwrap();
    assert!(shldr.reg == 8 && shldr.alu == super::MMIO_ALU_SHLD);
    assert_eq!(
        super::mmio_double_shift_apply(0x1234_5678, 0xABCD_EF00, 8, super::MMIO_ALU_SHLD, 4),
        0x3456_78AB
    );
    assert_eq!(
        super::mmio_double_shift_apply(0x1234_5678, 0xABCD_EF9A, 8, super::MMIO_ALU_SHRD, 4),
        0x9A12_3456
    );
    assert_eq!(
        super::mmio_double_shift_apply(0x11, 0x22, 0, super::MMIO_ALU_SHLD, 4),
        0x11
    );
    assert_eq!(
        super::mmio_double_shift_apply(0xABCD, 0x1234, 8, super::MMIO_ALU_SHLD, 2),
        0xCD12
    );
    assert_eq!(
        super::mmio_double_shift_apply(0xABCD, 0x1234, 16, super::MMIO_ALU_SHLD, 2),
        0x1234
    );
    assert_eq!(
        super::mmio_double_shift_rflags(2, 0x8000_0000, 0, 1, 0, super::MMIO_ALU_SHLD, 4) & 1,
        1
    );
    assert_eq!(
        super::mmio_double_shift_rflags(2, 1, 0, 1, 0, super::MMIO_ALU_SHRD, 4) & 1,
        1
    );
    let bt_a3 = decode_mmio_insn(&[0x0F, 0xA3, 0x01], 3).unwrap();
    assert!(bt_a3.bt == super::MMIO_BT && bt_a3.alu == 0);
    assert!(super::mmio_alu_is_double_shift(super::MMIO_ALU_SHLD));
    assert!(!super::mmio_alu_is_shift(super::MMIO_ALU_SHLD));
    let nt = decode_mmio_insn(&[0x0F, 0xC3, 0x01], 3).unwrap();
    assert!(nt.is_write && nt.size == 4 && nt.reg == 0 && nt.alu == 0);
    let ntq = decode_mmio_insn(&[0x48, 0x0F, 0xC3, 0x01], 4).unwrap();
    assert!(ntq.is_write && ntq.size == 8);
    let nt16 = decode_mmio_insn(&[0x66, 0x0F, 0xC3, 0x01], 4).unwrap();
    assert!(nt16.size == 4);
    assert!(super::mmio_eq(0x100, 0, 1));
    assert!(!super::mmio_eq(1, 0, 1));
    assert_eq!(super::mmio_alu_apply(5, 2, super::MMIO_ALU_SUB), 3);
    assert_eq!(
        super::mmio_alu_apply(0, 0, super::MMIO_ALU_NOT) & 0xff,
        0xff
    );
    assert_eq!(
        super::mmio_alu_apply(1, 0, super::MMIO_ALU_NEG) & 0xff,
        0xff
    );
    assert_eq!(super::mmio_test_rflags(2, 0, 1) & (1 << 6), 1 << 6);
    assert_eq!(super::mmio_test_rflags(2, 1, 1) & (1 << 6), 0);
    assert_eq!(super::mmio_cmp_rflags(2, 1, 2, 1) & 1, 1);
    assert_eq!(super::mmio_add_rflags(2, 0xff, 1, 1) & 1, 1);
    assert_eq!(
        super::mmio_alu_rflags(2, 1, 2, 3, super::MMIO_ALU_ADD, 1) & (1 << 6),
        0
    );
    assert_eq!(super::mmio_alu_apply(0xF0, 0x0F, super::MMIO_ALU_AND), 0x00);
    assert_eq!(super::mmio_alu_apply(1, 2, super::MMIO_ALU_ADD), 3);
    assert_eq!(mmio_insn_bytes_this_page(0x1000, 16), 16);
    assert_eq!(mmio_insn_bytes_this_page(0x1FFC, 16), 4);
}

#[test]
fn mmio_write_queue_desc_keeps_high_half_on_writeq() {
    use crate::devices::ide_cdrom::{
        present as present_iso, reset as reset_cd, write_placeholder_iso, ISO_SECTOR,
        MOCK_EFI_ISO_BYTES,
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
    assert_eq!(
        pci_read_data(0xCFC, 4),
        0xFFFF_FFFF,
        "virtio hidden until latch"
    );
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
