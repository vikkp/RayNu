//! Host diagnostic UART — COM1 (0x3F8) + COM2 (0x2F8).
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Intel 16550-compatible UARTs. We mirror every host boot byte to **both**
//! ports:
//! - **COM1 (0x3F8)** — QEMU `-serial stdio` / PC legacy
//! - **COM2 (0x2F8)** — Dell iDRAC9 SOL (`ssh …` then `console com2`)
//!
//! R640 first light showed `RAYNU-V-M0-BOOT-OK` only on ConOut / BIOS serial
//! redirect. Post-M0 HV progress is port-I/O; without COM2 mirror, SOL stays
//! frozen at M0 after ExitBootServices tears down ConOut.

/// Distinctive M0 gate marker — CI greps for this exact string on the serial log.
pub const M0_BOOT_OK_MARKER: &str = "RAYNU-V-M0-BOOT-OK";

const COM1: u16 = 0x3F8;
const COM2: u16 = 0x2F8;

/// Spins waiting for THR empty before declaring a UART dead (avoid infinite hang).
const THR_WAIT_SPINS: u32 = 200_000;

/// Per-port liveness; cleared on THR timeout so a missing UART cannot stall boot.
static mut COM1_LIVE: bool = true;
static mut COM2_LIVE: bool = true;

/// E4: host serial log ring for SPA / `GET /logs/serial` (not a guest console).
pub const SERIAL_LOG_CAP: usize = 4096;
static mut LOG_BUF: [u8; SERIAL_LOG_CAP] = [0; SERIAL_LOG_CAP];
static mut LOG_HEAD: usize = 0;
static mut LOG_LEN: usize = 0;

fn log_push(byte: u8) {
    // SAFETY: single-threaded boot / HV; ring only touched from serial writers.
    unsafe {
        let idx = (LOG_HEAD + LOG_LEN) % SERIAL_LOG_CAP;
        if LOG_LEN < SERIAL_LOG_CAP {
            LOG_BUF[idx] = byte;
            LOG_LEN += 1;
        } else {
            LOG_BUF[LOG_HEAD] = byte;
            LOG_HEAD = (LOG_HEAD + 1) % SERIAL_LOG_CAP;
        }
    }
}

/// Copy the current serial log ring into `out` (oldest → newest). Returns bytes written.
pub fn serial_log_snapshot(out: &mut [u8]) -> usize {
    unsafe {
        let n = LOG_LEN.min(out.len());
        for i in 0..n {
            out[i] = LOG_BUF[(LOG_HEAD + i) % SERIAL_LOG_CAP];
        }
        n
    }
}

/// Bytes currently retained in the serial log ring.
pub fn serial_log_len() -> usize {
    unsafe { LOG_LEN }
}

/// Clear the serial log ring (tests / new listen window).
pub fn serial_log_clear() {
    unsafe {
        LOG_HEAD = 0;
        LOG_LEN = 0;
    }
}

/// Initialize COM1 + COM2 to 115200 8N1.
///
/// # Safety
/// Port I/O to legacy COM1/COM2. Safe on QEMU and Dell PowerEdge (iDRAC SOL).
///
/// SAFETY: port I/O to fixed legacy UART bases; no memory aliasing.
/// KANI-TARGET: bounded check that init only touches COM1..COM1+7 and COM2..COM2+7.
pub fn init() {
    // SAFETY: single-threaded early boot; reset liveness then program both UARTs.
    unsafe {
        COM1_LIVE = true;
        COM2_LIVE = true;
    }
    init_port(COM1);
    init_port(COM2);
}

fn init_port(base: u16) {
    unsafe {
        outb(base + 1, 0x00); // Disable interrupts
        outb(base + 3, 0x80); // Enable DLAB
        outb(base + 0, 0x01); // Divisor low (115200)
        outb(base + 1, 0x00); // Divisor high
        outb(base + 3, 0x03); // 8N1, DLAB off
        outb(base + 2, 0xC7); // Enable FIFO, clear, 14-byte threshold
        outb(base + 4, 0x0B); // IRQs enabled, RTS/DSR set
    }
}

/// Write a byte to live diagnostic UARTs (COM1 + COM2), waiting for THR.
pub fn write_byte(byte: u8) {
    // Translate `\n` → `\r\n` for typical serial terminals.
    if byte == b'\n' {
        write_raw(b'\r');
        log_push(b'\r');
    }
    write_raw(byte);
    log_push(byte);
}

fn write_raw(byte: u8) {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: single-threaded boot / post-EBS HV; flags only cleared here.
        unsafe {
            write_raw_port(COM1, byte, &mut COM1_LIVE);
            write_raw_port(COM2, byte, &mut COM2_LIVE);
        }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        let _ = byte; // host/unit-test: no port I/O; log ring still records
    }
}

unsafe fn write_raw_port(base: u16, byte: u8, live: &mut bool) {
    if !*live {
        return;
    }
    for _ in 0..THR_WAIT_SPINS {
        if inb(base + 5) & 0x20 != 0 {
            outb(base, byte);
            return;
        }
        core::hint::spin_loop();
    }
    // Dead / missing UART — stop spinning on later bytes.
    *live = false;
}

/// Write a UTF-8 string (bytes as-is) to diagnostic UARTs.
pub fn write_str(s: &str) {
    for &b in s.as_bytes() {
        write_byte(b);
    }
}

/// Write a string plus newline.
pub fn write_line(s: &str) {
    write_str(s);
    write_byte(b'\n');
}

/// Revive diagnostic UART liveness without reprogramming baud/FIFO.
///
/// After a THR timeout, [`write_raw_port`] clears COM2_LIVE and later bytes
/// silently skip COM2 (iDRAC SOL). Full [`init`] can glitch SOL mid-session;
/// this only re-enables writes to already-programmed ports.
pub fn revive_ports() {
    // SAFETY: single-threaded boot / post-EBS HV.
    unsafe {
        COM1_LIVE = true;
        COM2_LIVE = true;
    }
}

/// Non-blocking host UART RX. Prefers COM2 (iDRAC SOL) then COM1 (QEMU).
///
/// Missing UARTs often return LSR `0xFF`; those reads are ignored. Host/CI
/// tests have no port I/O and always return `None`.
pub fn try_read_byte() -> Option<u8> {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: port I/O to fixed legacy UART bases; no memory aliasing.
        // KANI-TARGET: host COM1/COM2 LSR+RBR (outside Proven Core).
        unsafe { read_port_rx(COM2).or_else(|| read_port_rx(COM1)) }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        None
    }
}

#[cfg(target_os = "uefi")]
unsafe fn read_port_rx(base: u16) -> Option<u8> {
    let lsr = inb(base + 5);
    if lsr == 0xFF || lsr & 1 == 0 {
        return None;
    }
    Some(inb(base))
}

/// Print the M0 identity banner and gate marker (COM1+COM2).
pub fn print_m0_banner(banner: &str) {
    write_line(banner);
    write_line("pillars: [V] verified · [Z] single-binary · [D] iDRAC · [A] audit");
    write_line("serial: COM1+COM2 mirror (iDRAC SOL = console com2)");
    write_line(M0_BOOT_OK_MARKER);
}

/// Exit QEMU via isa-debug-exit (iobase 0xf4). No-op on real hardware.
///
/// QEMU exit status becomes `((code << 1) | 1)`. We write `0x10` → status 33,
/// which `tools/qemu-boot-test.sh` treats as a clean guest-requested exit.
///
/// SAFETY: port I/O to QEMU-only debug device; ignored on bare metal.
/// KANI-TARGET: outb to 0xf4 only.
pub fn qemu_exit_success() {
    unsafe {
        outb(0xf4, 0x10);
    }
}

/// Exit QEMU with a failure code (`0x21` → status 67). No-op on real hardware.
pub fn qemu_exit_failure() {
    unsafe {
        outb(0xf4, 0x21);
    }
}

#[inline]
unsafe fn outb(port: u16, val: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nomem, nostack, preserves_flags)
    );
}

#[inline]
unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    core::arch::asm!(
        "in al, dx",
        out("al") val,
        in("dx") port,
        options(nomem, nostack, preserves_flags)
    );
    val
}

#[cfg(test)]
mod serial_test {
    use super::*;

    #[test]
    fn marker_is_stable() {
        assert_eq!(M0_BOOT_OK_MARKER, "RAYNU-V-M0-BOOT-OK");
        assert!(M0_BOOT_OK_MARKER.contains("M0"));
    }

    #[test]
    fn dual_uart_bases_documented() {
        assert_eq!(COM1, 0x3F8);
        assert_eq!(COM2, 0x2F8);
        let s = include_str!("serial.rs");
        assert!(s.contains("console com2"));
        assert!(s.contains("COM2"));
    }

    #[test]
    fn serial_log_ring_retains_writes() {
        serial_log_clear();
        write_str("E4-LOG");
        write_byte(b'\n');
        let mut buf = [0u8; 32];
        let n = serial_log_snapshot(&mut buf);
        assert!(n >= 6);
        assert_eq!(&buf[..6], b"E4-LOG");
        serial_log_clear();
        assert_eq!(serial_log_len(), 0);
    }

    #[test]
    fn host_try_read_byte_is_none() {
        assert!(try_read_byte().is_none());
    }
}
