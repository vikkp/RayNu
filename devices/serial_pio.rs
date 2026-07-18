//! Guest COM1 (0x3F8) PIO emulation / passthrough (M3.0).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! On I/O VMEXIT, OUT to the COM1 data port is forwarded to the host UART.
//! Tracks a magic guest string for the `RAYNU-V-M3-IO-OK` gate.

/// COM1 marker when guest OUT magic is observed (M3.0 gate).
pub const M3_IO_OK_MARKER: &str = "RAYNU-V-M3-IO-OK";

/// Bytes the synthetic guest writes via `out 0x3f8, al`.
pub const GUEST_IO_MAGIC: &[u8] = b"RAYNU-V-M3-IO";

pub const COM1_DATA: u16 = 0x3F8;
pub const COM1_IER: u16 = 0x3F9;
pub const COM1_LSR: u16 = 0x3FD;

/// Set when [`GUEST_IO_MAGIC`] has been fully received from guest OUTs.
static mut IO_MAGIC_OK: bool = false;
static mut MAGIC_POS: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoExitInfo {
    pub port: u16,
    pub size: u8,
    pub is_in: bool,
    pub string: bool,
    pub rep: bool,
}

/// Parse EXIT_QUALIFICATION for an I/O-instruction VMEXIT (SDM Vol. 3C).
pub fn parse_qualification(qual: u64) -> IoExitInfo {
    let size = ((qual & 7) as u8) + 1;
    IoExitInfo {
        port: ((qual >> 16) & 0xFFFF) as u16,
        size: if size == 3 { 4 } else { size },
        is_in: (qual & (1 << 3)) != 0,
        string: (qual & (1 << 4)) != 0,
        rep: (qual & (1 << 5)) != 0,
    }
}

pub fn is_com1_port(port: u16) -> bool {
    (COM1_DATA..=COM1_DATA + 7).contains(&port)
}

/// Handle a guest PIO access. `rax` is the guest RAX at VMEXIT.
///
/// For OUT: forwards COM1 data bytes to host serial and updates the magic latch.
/// For IN on LSR: returns THR-empty so a polling guest would not spin (unused in M3.0).
///
/// Returns `Some(new_rax)` when guest RAX must be updated (IN); `None` for OUT.
pub fn handle_pio(info: &IoExitInfo, rax: u64) -> Result<Option<u64>, ()> {
    if info.string || info.rep || info.size != 1 {
        return Err(());
    }
    if !is_com1_port(info.port) {
        return Err(());
    }

    if info.is_in {
        let val = match info.port {
            COM1_LSR => 0x60u64, // THR empty + transmitter empty
            COM1_IER => 0,
            _ => 0,
        };
        Ok(Some((rax & !0xFF) | val))
    } else {
        let byte = (rax & 0xFF) as u8;
        if info.port == COM1_DATA {
            note_magic_byte(byte);
            // Passthrough so the magic is visible on the QEMU serial log.
            // Skip port I/O under host `cargo test` (no COM1).
            #[cfg(not(test))]
            crate::boot::serial::write_byte(byte);
        }
        Ok(None)
    }
}

fn note_magic_byte(byte: u8) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if IO_MAGIC_OK {
            return;
        }
        if MAGIC_POS < GUEST_IO_MAGIC.len() && byte == GUEST_IO_MAGIC[MAGIC_POS] {
            MAGIC_POS += 1;
            if MAGIC_POS == GUEST_IO_MAGIC.len() {
                IO_MAGIC_OK = true;
            }
        } else {
            MAGIC_POS = if byte == GUEST_IO_MAGIC[0] { 1 } else { 0 };
        }
    }
}

pub fn guest_io_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after magic completes.
    unsafe { IO_MAGIC_OK }
}

/// Trap COM1 ports in an I/O bitmap A page (ports 0x0000–0x7FFF).
///
/// SAFETY: `bitmap_a` is a writable zeroed 4K frame.
pub unsafe fn trap_com1_in_bitmap_a(bitmap_a: u64) {
    let base = bitmap_a as *mut u8;
    for port in COM1_DATA..=COM1_DATA + 7 {
        let idx = (port / 8) as usize;
        let bit = (port % 8) as u8;
        let cur = core::ptr::read_volatile(base.add(idx));
        core::ptr::write_volatile(base.add(idx), cur | (1 << bit));
    }
}

#[cfg(test)]
#[path = "serial_pio_test.rs"]
mod serial_pio_test;
