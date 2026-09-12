//! Host tests for USB BOT CBW/CSW + mock mass-storage round-trip.
//!
//! Never prints `RAYNU-V-M7-ISO-INSTALL-OK` or `RAYNU-V-M8-DISK-PERSIST-OK`.

use super::*;
use crate::mgmt::disk_persist::M8_DISK_PERSIST_OK_MARKER;

const LBA: u32 = 512;
const NS_BYTES: usize = 2 * 1024 * 1024;

struct MockUsb {
    ns: Vec<u8>,
    pending_in: Vec<u8>,
    write_off: Option<usize>,
    last_tag: u32,
    after_data: bool,
}

impl MockUsb {
    fn new(ns: Vec<u8>) -> Self {
        Self {
            ns,
            pending_in: Vec::new(),
            write_off: None,
            last_tag: 0,
            after_data: false,
        }
    }

    fn queue_csw(&mut self) {
        let mut csw = [0u8; CSW_LEN];
        put_le_u32(&mut csw, 0, CSW_SIG);
        put_le_u32(&mut csw, 4, self.last_tag);
        csw[12] = 0;
        self.pending_in = csw.to_vec();
        self.after_data = false;
    }

    fn exec_cbw(&mut self, cbw: &[u8]) {
        self.last_tag = get_le_u32(cbw, 4);
        self.write_off = None;
        self.after_data = false;
        let cdb0 = cbw[15];
        match cdb0 {
            SCSI_INQUIRY => {
                let mut inq = vec![0u8; 36];
                inq[0] = 0;
                inq[2] = 5;
                inq[4] = 31;
                inq[8..16].copy_from_slice(b"RAYNU-V ");
                self.pending_in = inq;
                self.after_data = true;
            }
            SCSI_READ_CAPACITY_10 => {
                let last = (self.ns.len() as u32 / LBA) - 1;
                let mut cap = [0u8; 8];
                cap[0..4].copy_from_slice(&last.to_be_bytes());
                cap[4..8].copy_from_slice(&LBA.to_be_bytes());
                self.pending_in = cap.to_vec();
                self.after_data = true;
            }
            SCSI_READ_10 => {
                let lba = get_be_u32(&cbw[15..], 2);
                let nlb = u16::from_be_bytes([cbw[15 + 7], cbw[15 + 8]]);
                let off = (lba as usize).saturating_mul(LBA as usize);
                let n = (nlb as usize).saturating_mul(LBA as usize);
                self.pending_in = self.ns[off..off + n].to_vec();
                self.after_data = true;
            }
            SCSI_WRITE_10 => {
                let lba = get_be_u32(&cbw[15..], 2);
                self.write_off = Some((lba as usize).saturating_mul(LBA as usize));
            }
            _ => {}
        }
    }
}

impl UsbBulk for MockUsb {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.len() == CBW_LEN && get_le_u32(data, 0) == CBW_SIG {
            self.exec_cbw(data);
            return Ok(());
        }
        if let Some(off) = self.write_off.take() {
            if off + data.len() > self.ns.len() {
                return Err(UsbBotError::Xfer);
            }
            self.ns[off..off + data.len()].copy_from_slice(data);
            self.queue_csw();
            return Ok(());
        }
        Err(UsbBotError::Xfer)
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        if self.pending_in.is_empty() && self.after_data {
            self.queue_csw();
        }
        if self.pending_in.is_empty() {
            return Err(UsbBotError::Bot);
        }
        let n = self.pending_in.len().min(data.len());
        data[..n].copy_from_slice(&self.pending_in[..n]);
        self.pending_in.clear();
        Ok(n)
    }
}

#[test]
fn capacity10_1gib_lba512() {
    let last = (1024u32 * 1024 * 1024 / 512) - 1;
    let mut cap = [0u8; 8];
    cap[0..4].copy_from_slice(&last.to_be_bytes());
    cap[4..8].copy_from_slice(&512u32.to_be_bytes());
    assert_eq!(capacity10_bytes(&cap), Some((1024 * 1024 * 1024, 512)));
    assert!(USB_BOT_RESIDUAL_NOTE.contains("ISO-INSTALL-OK"));
    assert_eq!(M8_DISK_PERSIST_OK_MARKER, "RAYNU-V-M8-DISK-PERSIST-OK");
    assert!(!USB_BOT_RESIDUAL_NOTE.contains("println!"));
}

#[test]
fn mock_bot_write_read_efi_part() {
    let ns = vec![0u8; NS_BYTES];
    let mut hw = MockUsb::new(ns);
    let (bytes, lba) = usb_bot_bring_up(&mut hw, 1024 * 1024).expect("bring-up");
    assert_eq!(bytes, NS_BYTES as u64);
    assert_eq!(lba, 512);
    let mut sector = [0u8; 512];
    sector[..8].copy_from_slice(b"EFI PART");
    let mut tag = 10u32;
    usb_bot_rw(&mut hw, &mut tag, lba, 512, &mut sector, true).expect("write");
    let mut back = [0u8; 512];
    usb_bot_rw(&mut hw, &mut tag, lba, 512, &mut back, false).expect("read");
    assert_eq!(&back[..8], b"EFI PART");
}

#[test]
fn host_usb_vec_survives_as_durable_lun() {
    clear_usb_bot_ready();
    let ns = Box::leak(vec![0u8; NS_BYTES].into_boxed_slice());
    host_usb_attach(ns, 512);
    assert!(usb_bot_io_ready());
    let mut sig = *b"EFI PART";
    assert!(host_usb_rw(512, &mut sig, true));
    let mut back = [0u8; 8];
    assert!(host_usb_rw(512, &mut back, false));
    assert_eq!(&back, b"EFI PART");
    clear_usb_bot_ready();
}
