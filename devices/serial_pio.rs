//! Guest COM1 (0x3F8) PIO emulation / passthrough (M3.0 / M3.8).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! On I/O VMEXIT, OUT to the COM1 data port is forwarded to the host UART.
//! Tracks magic guest strings and (M3.8) a real Linux earlyprintk banner.

/// COM1 marker when guest OUT magic is observed (M3.0 gate).
pub const M3_IO_OK_MARKER: &str = "RAYNU-V-M3-IO-OK";

/// Bytes the synthetic guest writes via `out dx, al` (DX=COM1).
pub const GUEST_IO_MAGIC: &[u8] = b"RAYNU-V-M3-IO";

/// Bytes the M3.3 proto-kernel writes after its Linux-style early line.
pub const GUEST_EARLY_MAGIC: &[u8] = b"RAYNU-V-M3-EARLY";

/// COM1 marker when proto-kernel early magic is observed (M3.3 gate).
pub const M3_EARLY_OK_MARKER: &str = "RAYNU-V-M3-EARLY-OK";

/// Bytes the M3.5 proto-init writes on COM1.
pub const GUEST_SHELL_MAGIC: &[u8] = b"RAYNU-V-M3-SHELL";

/// COM1 marker when proto-init shell magic is observed (M3.5 gate).
pub const M3_SHELL_OK_MARKER: &str = "RAYNU-V-M3-SHELL-OK";

/// Prefix of a real Linux banner (`earlyprintk` / `printk`).
pub const LINUX_BANNER_PREFIX: &[u8] = b"Linux version ";

/// COM1 marker when a real Linux banner is observed (M3.8 gate).
pub const M3_LINUX_EARLY_OK_MARKER: &str = "RAYNU-V-M3-LINUX-EARLY-OK";

pub const COM1_DATA: u16 = 0x3F8;
pub const COM1_IER: u16 = 0x3F9;
pub const COM1_IIR_FCR: u16 = 0x3FA;
pub const COM1_LCR: u16 = 0x3FB;
pub const COM1_MCR: u16 = 0x3FC;
pub const COM1_LSR: u16 = 0x3FD;
pub const COM1_MSR: u16 = 0x3FE;
pub const COM1_SCR: u16 = 0x3FF;

const LCR_DLAB: u8 = 1 << 7;
/// IER bit 1: enable THR-empty interrupt (8250 ETBEI).
const IER_ETBEI: u8 = 1 << 1;
/// IIR: no interrupt pending.
const IIR_NO_INT: u8 = 0x01;
/// IIR: THR empty (tx ready) interrupt.
const IIR_THRE: u8 = 0x02;

/// Set when [`GUEST_IO_MAGIC`] has been fully received from guest OUTs.
static mut IO_MAGIC_OK: bool = false;
static mut MAGIC_POS: usize = 0;
/// Set when [`GUEST_EARLY_MAGIC`] has been fully received.
static mut EARLY_MAGIC_OK: bool = false;
static mut EARLY_POS: usize = 0;
/// Set when [`GUEST_SHELL_MAGIC`] has been fully received.
static mut SHELL_MAGIC_OK: bool = false;
static mut SHELL_POS: usize = 0;
/// Real Linux earlyprintk banner latch (digit after [`LINUX_BANNER_PREFIX`]).
static mut LINUX_EARLY_OK: bool = false;
static mut LINUX_POS: usize = 0;
static mut LINUX_NEED_DIGIT: bool = false;

/// Soft 16550 shadow (enough for early_serial_init).
static mut SHADOW_LCR: u8 = 0x03; // 8n1
static mut SHADOW_IER: u8 = 0;
static mut SHADOW_MCR: u8 = 0;
static mut SHADOW_DLL: u8 = 1;
static mut SHADOW_DLM: u8 = 0;
/// THR-empty IRQ pending (Linux ttyS0 TX is interrupt-driven).
static mut TX_IRQ_PENDING: bool = false;
/// Port 0x61 (NMI status / speaker): toggle bit 4 so Linux delay loops advance.
static mut PORT61_SHADOW: u8 = 0;
/// PIT channel-0 latch counter (decrements on data-port reads).
static mut PIT0_COUNT: u16 = 0xFFFF;

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
/// COM1: earlyprintk / magic latches. Other ISA ports: stubbed so real Linux
/// can probe PIT/CMOS/speaker (port 0x61 refresh bit toggles on IN).
///
/// Returns `Some(new_rax)` when guest RAX must be updated (IN); `None` for OUT.
pub fn handle_pio(info: &IoExitInfo, rax: u64) -> Result<Option<u64>, ()> {
    // String/rep I/O: ignore for bring-up (kernel rarely needs it on COM1).
    if info.string || info.rep {
        return Ok(if info.is_in { Some(rax) } else { None });
    }
    if is_com1_port(info.port) {
        return handle_com1_pio(info, rax);
    }
    Ok(handle_misc_pio(info, rax))
}

fn handle_com1_pio(info: &IoExitInfo, rax: u64) -> Result<Option<u64>, ()> {
    if info.size != 1 {
        // Widen to byte access on the data port only if needed later.
        return Ok(handle_misc_pio(info, rax));
    }
    if info.is_in {
        // SAFETY: single-threaded VMEXIT path.
        let val = unsafe {
            let dlab = SHADOW_LCR & LCR_DLAB != 0;
            match info.port {
                COM1_DATA if dlab => SHADOW_DLL as u64,
                COM1_IER if dlab => SHADOW_DLM as u64,
                COM1_DATA => 0, // no RX
                COM1_IER => SHADOW_IER as u64,
                COM1_IIR_FCR => {
                    // Reading IIR acknowledges THR-empty IRQ.
                    if TX_IRQ_PENDING && (SHADOW_IER & IER_ETBEI) != 0 {
                        TX_IRQ_PENDING = false;
                        IIR_THRE as u64
                    } else {
                        IIR_NO_INT as u64
                    }
                }
                COM1_LCR => SHADOW_LCR as u64,
                COM1_MCR => SHADOW_MCR as u64,
                COM1_LSR => 0x60u64, // THR empty + TEMT
                COM1_MSR => 0x30,    // DSR+CTS
                COM1_SCR => 0,
                _ => 0,
            }
        };
        Ok(Some((rax & !0xFF) | val))
    } else {
        let byte = (rax & 0xFF) as u8;
        // SAFETY: single-threaded VMEXIT path.
        unsafe {
            let dlab = SHADOW_LCR & LCR_DLAB != 0;
            match info.port {
                COM1_DATA if dlab => SHADOW_DLL = byte,
                COM1_IER if dlab => SHADOW_DLM = byte,
                COM1_DATA => {
                    note_io_magic(byte);
                    note_early_magic(byte);
                    note_shell_magic(byte);
                    note_linux_early(byte);
                    // Passthrough so banner/magic appear on the QEMU serial log.
                    #[cfg(not(test))]
                    crate::boot::serial::write_byte(byte);
                    // tty write kicks TX IRQ for the next byte (else stalls after 1).
                    if (SHADOW_IER & IER_ETBEI) != 0 {
                        TX_IRQ_PENDING = true;
                    }
                }
                COM1_IER => {
                    SHADOW_IER = byte;
                    // Enabling ETBEI while THR empty raises THRE immediately.
                    if (byte & IER_ETBEI) != 0 {
                        TX_IRQ_PENDING = true;
                    } else {
                        TX_IRQ_PENDING = false;
                    }
                }
                COM1_IIR_FCR => {} // FCR write ignored
                COM1_LCR => SHADOW_LCR = byte,
                COM1_MCR => SHADOW_MCR = byte,
                COM1_SCR => {}
                _ => {}
            }
        }
        Ok(None)
    }
}

/// Stub non-COM1 ISA ports under unconditional I/O exiting (M3.10).
fn handle_misc_pio(info: &IoExitInfo, rax: u64) -> Option<u64> {
    let mask = match info.size {
        1 => 0xFFu64,
        2 => 0xFFFFu64,
        _ => 0xFFFF_FFFFu64,
    };
    if info.is_in {
        let val = match info.port {
            0x61 => {
                // SAFETY: single-threaded VMEXIT path.
                unsafe {
                    // DRAM refresh toggle (bit 4) — Linux `io_delay` / speaker polls this.
                    PORT61_SHADOW ^= 0x10;
                    PORT61_SHADOW as u64
                }
            }
            0x80 => 0, // POST / io_delay
            0x40 => {
                // PIT ch0 data: return a moving count so calibrate loops advance.
                // SAFETY: single-threaded VMEXIT path.
                unsafe {
                    let v = PIT0_COUNT as u64;
                    PIT0_COUNT = PIT0_COUNT.wrapping_sub(0x40);
                    v & 0xFF
                }
            }
            0x41..=0x43 => 0,                     // PIT ch1/ch2 / command
            0x70 | 0x71 => 0,                     // CMOS
            0x20 | 0x21 | 0xA0 | 0xA1 => 0xFF, // PIC
            _ => 0xFF,
        };
        Some((rax & !mask) | (val & mask))
    } else {
        // SAFETY: single-threaded VMEXIT path.
        unsafe {
            if info.port == 0x61 {
                // Keep toggle bit from reads; accept speaker enable bits from guest.
                PORT61_SHADOW = (rax as u8 & !0x10) | (PORT61_SHADOW & 0x10);
            } else if info.port == 0x40 {
                PIT0_COUNT = (rax as u16) | 0x00FF;
            } else if info.port == 0x43 {
                // Mode command — reset latch high so subsequent reads look alive.
                PIT0_COUNT = 0xFFFF;
            }
        }
        None
    }
}

fn note_io_magic(byte: u8) {
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

fn note_early_magic(byte: u8) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if EARLY_MAGIC_OK {
            return;
        }
        if EARLY_POS < GUEST_EARLY_MAGIC.len() && byte == GUEST_EARLY_MAGIC[EARLY_POS] {
            EARLY_POS += 1;
            if EARLY_POS == GUEST_EARLY_MAGIC.len() {
                EARLY_MAGIC_OK = true;
            }
        } else {
            EARLY_POS = if byte == GUEST_EARLY_MAGIC[0] { 1 } else { 0 };
        }
    }
}

fn note_shell_magic(byte: u8) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if SHELL_MAGIC_OK {
            return;
        }
        if SHELL_POS < GUEST_SHELL_MAGIC.len() && byte == GUEST_SHELL_MAGIC[SHELL_POS] {
            SHELL_POS += 1;
            if SHELL_POS == GUEST_SHELL_MAGIC.len() {
                SHELL_MAGIC_OK = true;
            }
        } else {
            SHELL_POS = if byte == GUEST_SHELL_MAGIC[0] { 1 } else { 0 };
        }
    }
}

fn note_linux_early(byte: u8) {
    // SAFETY: single-threaded VMEXIT path.
    unsafe {
        if LINUX_EARLY_OK {
            return;
        }
        if LINUX_NEED_DIGIT {
            if byte.is_ascii_digit() {
                // Real: "Linux version 6…" — not "Linux version RayNu-V-proto".
                LINUX_EARLY_OK = true;
                LINUX_NEED_DIGIT = false;
            } else {
                LINUX_NEED_DIGIT = false;
                LINUX_POS = if byte == LINUX_BANNER_PREFIX[0] { 1 } else { 0 };
            }
            return;
        }
        if LINUX_POS < LINUX_BANNER_PREFIX.len() && byte == LINUX_BANNER_PREFIX[LINUX_POS] {
            LINUX_POS += 1;
            if LINUX_POS == LINUX_BANNER_PREFIX.len() {
                LINUX_NEED_DIGIT = true;
            }
        } else {
            LINUX_POS = if byte == LINUX_BANNER_PREFIX[0] { 1 } else { 0 };
        }
    }
}

pub fn guest_io_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after magic completes.
    unsafe { IO_MAGIC_OK }
}

pub fn guest_early_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after early magic completes.
    unsafe { EARLY_MAGIC_OK }
}

pub fn guest_shell_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after shell magic completes.
    unsafe { SHELL_MAGIC_OK }
}

pub fn guest_linux_early_ok() -> bool {
    // SAFETY: written on BSP VMEXIT path; read after banner latch.
    unsafe { LINUX_EARLY_OK }
}

/// True when the 8250 guest needs ISA IRQ4 (THR-empty) to continue TX.
pub fn com1_tx_irq_pending() -> bool {
    // SAFETY: single-threaded VMEXIT path.
    unsafe { TX_IRQ_PENDING && (SHADOW_IER & IER_ETBEI) != 0 }
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
