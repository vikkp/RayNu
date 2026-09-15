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
            SCSI_TEST_UNIT_READY => {
                self.queue_csw();
            }
            SCSI_REQUEST_SENSE => {
                let mut s = vec![0u8; 18];
                s[0] = 0x70;
                s[7] = 10;
                self.pending_in = s;
                self.after_data = true;
            }
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
            SCSI_START_STOP => {
                self.queue_csw();
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
fn start_stop_and_read_probe_named() {
    assert_eq!(SCSI_START_STOP, 0x1B);
    assert_eq!(USB_BOT_RW_TRIES, 3);
    assert_eq!(usb_bot_stage_name(BOT_STAGE_CBW), "cbw");
    assert_eq!(usb_bot_stage_name(BOT_STAGE_DATA), "data");
    assert_eq!(usb_bot_stage_name(BOT_STAGE_CSW), "csw");
    assert_eq!(usb_bot_scsi_name(SCSI_TAG_INQUIRY), "inquiry");
    assert_eq!(usb_bot_scsi_name(SCSI_TAG_TUR), "tur");
    assert_eq!(usb_bot_scsi_name(SCSI_TAG_CAPACITY), "capacity");
    assert_eq!(usb_bot_scsi_name(SCSI_TAG_READ), "read");
    assert_eq!(cdb_start_stop(true)[0], SCSI_START_STOP);
    assert_eq!(cdb_start_stop(true)[4], 1);
    assert_eq!(cdb_start_stop(false)[4], 0);
    assert!(usb_bot_keep_xfer_diag(UsbBotError::Xfer as u8));
    assert!(usb_bot_keep_xfer_diag(UsbBotError::Bot as u8));
    assert!(usb_bot_keep_xfer_diag(UsbBotError::Capacity as u8));
    assert!(usb_bot_keep_xfer_diag(UsbBotError::Enum as u8));
    assert!(!usb_bot_keep_xfer_diag(UsbBotError::Hub as u8));
    assert!(!usb_bot_keep_xfer_diag(UsbBotError::Reset as u8));
}

struct CountingUsb {
    inner: MockUsb,
    start_stop: u32,
    tur: u32,
    recovers: u32,
}

impl UsbBulk for CountingUsb {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.len() == CBW_LEN && get_le_u32(data, 0) == CBW_SIG && data[15] == SCSI_START_STOP {
            self.start_stop = self.start_stop.saturating_add(1);
        }
        if data.len() == CBW_LEN
            && get_le_u32(data, 0) == CBW_SIG
            && data[15] == SCSI_TEST_UNIT_READY
        {
            self.tur = self.tur.saturating_add(1);
        }
        self.inner.bulk_out(data)
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        self.inner.bulk_in(data)
    }

    fn recover_pipes(&mut self) {
        self.recovers = self.recovers.saturating_add(1);
    }
}

#[test]
fn bring_up_does_not_send_start_stop_or_tur() {
    let ns = vec![0u8; NS_BYTES];
    let mut hw = CountingUsb {
        inner: MockUsb::new(ns),
        start_stop: 0,
        tur: 0,
        recovers: 0,
    };
    usb_bot_bring_up(&mut hw, 1024 * 1024).expect("bring-up");
    assert_eq!(hw.start_stop, 0);
    assert_eq!(hw.tur, 0);
    assert_eq!(hw.recovers, 0);
    assert_eq!(usb_bot_last_scsi(), SCSI_TAG_READ);
}

struct CbwFailUsb {
    inner: MockUsb,
    fail_cbw: u32,
    recovers: u32,
}

impl UsbBulk for CbwFailUsb {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.len() == CBW_LEN && get_le_u32(data, 0) == CBW_SIG && data[15] == SCSI_READ_10 {
            if self.fail_cbw > 0 {
                self.fail_cbw -= 1;
                return Err(UsbBotError::Xfer);
            }
        }
        self.inner.bulk_out(data)
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        self.inner.bulk_in(data)
    }

    fn recover_pipes(&mut self) {
        self.recovers = self.recovers.saturating_add(1);
        self.inner.pending_in.clear();
        self.inner.after_data = false;
        self.inner.write_off = None;
    }
}

#[test]
fn rw_cbw_fail_does_not_reset_endpoint() {
    assert!(!usb_bot_recover_after_fail(BOT_STAGE_CBW));
    assert!(usb_bot_recover_after_fail(BOT_STAGE_DATA));
    assert!(usb_bot_recover_after_fail(BOT_STAGE_CSW));
    let ns = vec![0u8; NS_BYTES];
    let mut hw = CbwFailUsb {
        inner: MockUsb::new(ns),
        fail_cbw: 1,
        recovers: 0,
    };
    let (bytes, lba) = usb_bot_bring_up(&mut hw, 1024 * 1024).expect("bring-up");
    assert_eq!(bytes, NS_BYTES as u64);
    assert_eq!(lba, 512);
    assert_eq!(hw.recovers, 0);
    assert_eq!(hw.fail_cbw, 0);
}

struct FlakyReadUsb {
    inner: MockUsb,
    fail_reads: u32,
    recovers: u32,
    tur: u32,
}

impl UsbBulk for FlakyReadUsb {
    fn bulk_out(&mut self, data: &[u8]) -> Result<(), UsbBotError> {
        if data.len() == CBW_LEN
            && get_le_u32(data, 0) == CBW_SIG
            && data[15] == SCSI_TEST_UNIT_READY
        {
            self.tur = self.tur.saturating_add(1);
        }
        if data.len() == CBW_LEN && get_le_u32(data, 0) == CBW_SIG && data[15] == SCSI_READ_10 {
            if self.fail_reads > 0 {
                self.fail_reads -= 1;
                self.inner.last_tag = get_le_u32(data, 4);
                self.inner.pending_in.clear();
                self.inner.after_data = false;
                return Ok(());
            }
        }
        self.inner.bulk_out(data)
    }

    fn bulk_in(&mut self, data: &mut [u8]) -> Result<usize, UsbBotError> {
        self.inner.bulk_in(data)
    }

    fn recover_pipes(&mut self) {
        self.recovers = self.recovers.saturating_add(1);
        self.inner.pending_in.clear();
        self.inner.after_data = false;
        self.inner.write_off = None;
    }
}

#[test]
fn rw_retry_calls_recover_pipes() {
    let ns = vec![0u8; NS_BYTES];
    let mut hw = FlakyReadUsb {
        inner: MockUsb::new(ns),
        fail_reads: 1,
        recovers: 0,
        tur: 0,
    };
    let (bytes, lba) = usb_bot_bring_up(&mut hw, 1024 * 1024).expect("bring-up");
    assert_eq!(bytes, NS_BYTES as u64);
    assert_eq!(lba, 512);
    assert_eq!(hw.recovers, 1);
    assert_eq!(hw.tur, 0);
    assert_eq!(usb_bot_last_scsi(), SCSI_TAG_READ);
}

#[test]
fn hub_skip_does_not_clobber_xfer_cmpl() {
    store_usb_bot_diag(UsbBotError::Xfer, 0x92b0_0000, 0x0000_000e_0000_0e03, 0xff);
    assert!(usb_bot_keep_xfer_diag(usb_bot_last_err()));
    assert_eq!(usb_bot_last_cmpl(), 0xff);
    assert_eq!(usb_bot_last_portsc(), 0x0000_000e_0000_0e03);
    if !usb_bot_keep_xfer_diag(usb_bot_last_err()) {
        store_usb_bot_diag(UsbBotError::Hub, 0x92b0_0000, 0x0000_000e_0000_0e03, 0);
    }
    assert_eq!(usb_bot_last_err(), UsbBotError::Xfer as u8);
    assert_eq!(usb_bot_last_cmpl(), 0xff);
}

#[test]
fn later_port_reset_does_not_clobber_xfer_cmpl() {
    use crate::mgmt::usb_bot::store_usb_bot_diag_unless_kept;
    store_usb_bot_diag(UsbBotError::Xfer, 0x92b0_0000, 0x0000_000b_0000_0e03, 0);
    assert!(usb_bot_keep_xfer_diag(usb_bot_last_err()));
    // Iron `96024edc`: p14 reset_diag `0x0e801a40` over p11 SET_CONFIG.
    store_usb_bot_diag_unless_kept(
        UsbBotError::Reset,
        0x92b0_0000,
        0x0000_000e_0000_0e03,
        0x0000_0000_0e80_1a40,
    );
    assert_eq!(usb_bot_last_err(), UsbBotError::Xfer as u8);
    assert_eq!(usb_bot_last_cmpl(), 0);
    assert_eq!(usb_bot_last_portsc(), 0x0000_000b_0000_0e03);
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
