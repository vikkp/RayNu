//! COM1 (0x3F8) 16550 UART — Tier 1 serial for QEMU and iDRAC virtual console.
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Intel 16550-compatible UART. Port map: COM1 = 0x3F8 (PC legacy).
//! Used so the M0 banner is visible on `-serial stdio` after firmware handoff,
//! independent of the UEFI ConOut console (which often does not reach COM1).

/// Distinctive M0 gate marker — CI greps for this exact string on the serial log.
pub const M0_BOOT_OK_MARKER: &str = "RAYNU-V-M0-BOOT-OK";

const COM1: u16 = 0x3F8;

/// Initialize COM1 to 115200 8N1.
///
/// # Safety
/// Performs port I/O to the legacy COM1 registers. Safe on QEMU and standard
/// PC firmware that exposes a 16550 at 0x3F8 (R640 iDRAC virtual COM1).
///
/// SAFETY: port I/O to fixed legacy COM1; no memory aliasing.
/// KANI-TARGET: bounded check that init only touches COM1..COM1+7.
pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // Disable interrupts
        outb(COM1 + 3, 0x80); // Enable DLAB
        outb(COM1 + 0, 0x01); // Divisor low (115200)
        outb(COM1 + 1, 0x00); // Divisor high
        outb(COM1 + 3, 0x03); // 8N1, DLAB off
        outb(COM1 + 2, 0xC7); // Enable FIFO, clear, 14-byte threshold
        outb(COM1 + 4, 0x0B); // IRQs enabled, RTS/DSR set
    }
}

/// Write a byte to COM1, waiting for the transmitter holding register.
pub fn write_byte(byte: u8) {
    // Translate `\n` → `\r\n` for typical serial terminals.
    if byte == b'\n' {
        write_raw(b'\r');
    }
    write_raw(byte);
}

fn write_raw(byte: u8) {
    unsafe {
        while inb(COM1 + 5) & 0x20 == 0 {
            core::hint::spin_loop();
        }
        outb(COM1, byte);
    }
}

/// Write a UTF-8 string (bytes as-is) to COM1.
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

/// Print the M0 identity banner and gate marker to COM1.
pub fn print_m0_banner(banner: &str) {
    write_line(banner);
    write_line("pillars: [V] verified · [Z] single-binary · [D] iDRAC · [A] audit");
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
}
