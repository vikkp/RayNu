//! Host tests for NVMe command encode + mock controller round-trip.
//!
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK` or `RAYNU-V-M8-DISK-PERSIST-OK`.

use super::*;
use crate::mgmt::disk_persist::M8_DISK_PERSIST_OK_MARKER;

const LBA: u32 = 512;
const NS_BYTES: usize = 2 * 1024 * 1024;

struct MockNvme {
    cap: u64,
    cc: u32,
    csts: u32,
    aqa: u32,
    asq: u64,
    acq: u64,
    iosq: u64,
    iocq: u64,
    ns: Vec<u8>,
    acq_tail: u16,
    acq_phase: u16,
    iocq_tail: u16,
    iocq_phase: u16,
}

impl MockNvme {
    fn new(ns: Vec<u8>) -> Self {
        Self {
            // MQES=0x1F (32 entries), DSTRD=0 so doorbell stride is 4.
            cap: 0x1F,
            cc: 0,
            csts: 0,
            aqa: 0,
            asq: 0,
            acq: 0,
            iosq: 0,
            iocq: 0,
            ns,
            acq_tail: 0,
            acq_phase: 1,
            iocq_tail: 0,
            iocq_phase: 1,
        }
    }

    fn complete(&mut self, admin: bool, cid: u16) {
        let mut cpl = [0u8; NVME_CPL_BYTES];
        let (hpa, tail, phase) = if admin {
            (
                self.acq + u64::from(self.acq_tail) * NVME_CPL_BYTES as u64,
                self.acq_tail,
                self.acq_phase,
            )
        } else {
            (
                self.iocq + u64::from(self.iocq_tail) * NVME_CPL_BYTES as u64,
                self.iocq_tail,
                self.iocq_phase,
            )
        };
        let dw3 = u32::from(cid) | (u32::from(phase) << 16);
        cpl[12..16].copy_from_slice(&dw3.to_le_bytes());
        dma_write_raw(hpa, &cpl);
        if admin {
            self.acq_tail = (tail + 1) % NVME_QSIZE;
            if self.acq_tail == 0 {
                self.acq_phase ^= 1;
            }
        } else {
            self.iocq_tail = (tail + 1) % NVME_QSIZE;
            if self.iocq_tail == 0 {
                self.iocq_phase ^= 1;
            }
        }
    }

    fn exec_admin(&mut self, cmd: &[u8]) {
        let op = cmd[0];
        let cid = u16::from_le_bytes(cmd[2..4].try_into().unwrap());
        let prp1 = u64::from_le_bytes(cmd[24..32].try_into().unwrap());
        match op {
            NVME_ADMIN_IDENTIFY => {
                let mut id = [0u8; NVME_IDENTIFY_BYTES];
                let nlb = (self.ns.len() as u64 / u64::from(LBA)) - 1;
                id[0..8].copy_from_slice(&nlb.to_le_bytes());
                id[26] = 0;
                // LBADS=9 → 512
                id[128..132].copy_from_slice(&(9u32 << 16).to_le_bytes());
                dma_write_raw(prp1, &id);
                self.complete(true, cid);
            }
            NVME_ADMIN_CREATE_CQ => {
                self.iocq = prp1;
                self.complete(true, cid);
            }
            NVME_ADMIN_CREATE_SQ => {
                self.iosq = prp1;
                self.complete(true, cid);
            }
            _ => self.complete(true, cid),
        }
    }

    fn exec_io(&mut self, cmd: &[u8]) {
        let op = cmd[0];
        let cid = u16::from_le_bytes(cmd[2..4].try_into().unwrap());
        let prp1 = u64::from_le_bytes(cmd[24..32].try_into().unwrap());
        let slba = u32::from_le_bytes(cmd[40..44].try_into().unwrap()) as u64
            | ((u32::from_le_bytes(cmd[44..48].try_into().unwrap()) as u64) << 32);
        let nlb_m1 = u32::from_le_bytes(cmd[48..52].try_into().unwrap()) as u64;
        let nlb = nlb_m1 + 1;
        let off = slba.saturating_mul(u64::from(LBA)) as usize;
        let n = (nlb * u64::from(LBA)) as usize;
        if off.saturating_add(n) > self.ns.len() {
            self.complete(false, cid);
            return;
        }
        match op {
            NVME_IO_WRITE => {
                let mut tmp = vec![0u8; n];
                dma_read_raw(prp1, &mut tmp);
                self.ns[off..off + n].copy_from_slice(&tmp);
            }
            NVME_IO_READ => {
                dma_write_raw(prp1, &self.ns[off..off + n]);
            }
            _ => {}
        }
        self.complete(false, cid);
    }
}

fn dma_write_raw(hpa: u64, buf: &[u8]) {
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), hpa as *mut u8, buf.len());
    }
}

fn dma_read_raw(hpa: u64, buf: &mut [u8]) {
    unsafe {
        core::ptr::copy_nonoverlapping(hpa as *const u8, buf.as_mut_ptr(), buf.len());
    }
}

impl NvmeHw for MockNvme {
    fn read32(&mut self, off: u32) -> u32 {
        match off {
            NVME_REG_CAP => self.cap as u32,
            0x04 => (self.cap >> 32) as u32,
            NVME_REG_CC => self.cc,
            NVME_REG_CSTS => self.csts,
            NVME_REG_AQA => self.aqa,
            NVME_REG_ASQ => self.asq as u32,
            0x2C => (self.asq >> 32) as u32,
            NVME_REG_ACQ => self.acq as u32,
            0x34 => (self.acq >> 32) as u32,
            _ => 0,
        }
    }

    fn write32(&mut self, off: u32, val: u32) {
        match off {
            NVME_REG_CC => {
                self.cc = val;
                self.csts = if val & NVME_CC_EN != 0 {
                    NVME_CSTS_RDY
                } else {
                    0
                };
            }
            NVME_REG_AQA => self.aqa = val,
            NVME_REG_ASQ => self.asq = (self.asq & !0xFFFF_FFFF) | u64::from(val),
            0x2C => self.asq = (self.asq & 0xFFFF_FFFF) | (u64::from(val) << 32),
            NVME_REG_ACQ => self.acq = (self.acq & !0xFFFF_FFFF) | u64::from(val),
            0x34 => self.acq = (self.acq & 0xFFFF_FFFF) | (u64::from(val) << 32),
            NVME_REG_SQ0TDBL => {
                let tail = val as u16;
                let prev = if self.asq == 0 {
                    0
                } else {
                    // process the entry just submitted: tail is new index
                    let idx = tail.wrapping_sub(1) % NVME_QSIZE;
                    let mut cmd = [0u8; NVME_CMD_BYTES];
                    dma_read_raw(self.asq + u64::from(idx) * NVME_CMD_BYTES as u64, &mut cmd);
                    self.exec_admin(&cmd);
                    idx
                };
                let _ = prev;
            }
            _x if off == NVME_REG_SQ0TDBL + 2 * cap_doorbell_stride(self.cap) => {
                let tail = val as u16;
                let idx = tail.wrapping_sub(1) % NVME_QSIZE;
                let mut cmd = [0u8; NVME_CMD_BYTES];
                dma_read_raw(self.iosq + u64::from(idx) * NVME_CMD_BYTES as u64, &mut cmd);
                self.exec_io(&cmd);
            }
            _ => {}
        }
    }

    fn dma_read(&mut self, hpa: u64, buf: &mut [u8]) {
        dma_read_raw(hpa, buf);
    }

    fn dma_write(&mut self, hpa: u64, buf: &[u8]) {
        dma_write_raw(hpa, buf);
    }
}

fn queues() -> (
    NvmeQueues,
    Box<[u8]>,
    Box<[u8]>,
    Box<[u8]>,
    Box<[u8]>,
    Box<[u8]>,
) {
    let mut asq = vec![0u8; 4096].into_boxed_slice();
    let mut acq = vec![0u8; 4096].into_boxed_slice();
    let mut iosq = vec![0u8; 4096].into_boxed_slice();
    let mut iocq = vec![0u8; 4096].into_boxed_slice();
    let mut bounce = vec![0u8; 4096].into_boxed_slice();
    let q = NvmeQueues {
        asq: asq.as_mut_ptr() as u64,
        acq: acq.as_mut_ptr() as u64,
        iosq: iosq.as_mut_ptr() as u64,
        iocq: iocq.as_mut_ptr() as u64,
        bounce: bounce.as_mut_ptr() as u64,
    };
    (q, asq, acq, iosq, iocq, bounce)
}

#[test]
fn identify_ns_bytes_from_lba512() {
    let mut id = [0u8; NVME_IDENTIFY_BYTES];
    let last = (1024u64 * 1024 * 1024 / 512) - 1;
    id[0..8].copy_from_slice(&last.to_le_bytes());
    id[26] = 0;
    id[128..132].copy_from_slice(&(9u32 << 16).to_le_bytes());
    assert_eq!(identify_ns_bytes(&id), Some(1024 * 1024 * 1024));
    assert_eq!(identify_lba_bytes(&id), Some(512));
    assert_eq!(cap_doorbell_stride(0), 4);
    assert!(NVME_IO_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert_eq!(M8_DISK_PERSIST_OK_MARKER, "RAYNU-V-M8-DISK-PERSIST-OK");
}

#[test]
fn mock_nvme_write_read_efi_part() {
    let ns = vec![0u8; NS_BYTES];
    let mut hw = MockNvme::new(ns);
    let (q, _asq, _acq, _iosq, _iocq, _bounce) = queues();
    let (mut db, bytes, lba) = nvme_bring_up(&mut hw, &q, 1024 * 1024).expect("bring-up");
    assert_eq!(bytes, NS_BYTES as u64);
    assert_eq!(lba, 512);
    let mut sector = [0u8; 512];
    sector[..8].copy_from_slice(b"EFI PART");
    nvme_rw(&mut hw, &q, &mut db, lba, 512, &mut sector, true).expect("write");
    let mut back = [0u8; 512];
    nvme_rw(&mut hw, &q, &mut db, lba, 512, &mut back, false).expect("read");
    assert_eq!(&back[..8], b"EFI PART");
}

#[test]
fn host_nvme_vec_survives_as_durable_lun() {
    clear_nvme_ready();
    let ns = Box::leak(vec![0u8; NS_BYTES].into_boxed_slice());
    host_nvme_attach(ns, 512);
    assert!(nvme_io_ready());
    let mut sig = *b"EFI PART";
    assert!(host_nvme_rw(512, &mut sig, true));
    let mut back = [0u8; 8];
    assert!(host_nvme_rw(512, &mut back, false));
    assert_eq!(&back, b"EFI PART");
    assert!(!NVME_IO_RESIDUAL_NOTE.contains("println!"));
    clear_nvme_ready();
}
