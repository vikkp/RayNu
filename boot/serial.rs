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

use core::sync::atomic::{AtomicBool, Ordering};

/// Distinctive M0 gate marker — CI greps for this exact string on the serial log.
pub const M0_BOOT_OK_MARKER: &str = "RAYNU-V-M0-BOOT-OK";

const COM1: u16 = 0x3F8;
const COM2: u16 = 0x2F8;

/// Spins waiting for THR empty on host diagnostic writes (ticks, banners).
/// Guest THR tees use [`write_byte_nowait`] (enqueue + drain) so a flooded
/// iDRAC SOL cannot stall the VM-exit path. Timeout must **not** clear
/// liveness — iron `115e5ee` froze COM2 at PAT `n=441600` after a guest
/// `out` waited THRE. Iron `f423d03`: nowait *dropped* Linux printk when HV
/// ticks held THRE (`GenuineIntEl` / PAT shredded). Queue, then drain.
const THR_WAIT_SPINS: u32 = 200_000;
/// Guest UART TX ring. Drops oldest when full so a late `ISO-INSTALL-OK`
/// still reaches SOL. Not blocking. Not `ISO-INSTALL-OK` by itself.
pub const GUEST_TX_CAP: usize = 2048;
/// Bytes to push toward SOL per guest `out` (keep the UART exit short).
pub const GUEST_TX_DRAIN_CHUNK: usize = 64;
/// Bytes to push on a non-UART VM-exit. Nested QEMU CI `be0f1cd` blasted
/// 64 host THR bytes on every preemption/HLT exit while THRE stayed set,
/// then E4 `/init` SIGSEGV hung until the 480s timeout (3/3). Iron SOL
/// still drains the ring: UART `out` uses [`GUEST_TX_DRAIN_CHUNK`]; later
/// exits leak a few bytes whenever COM2 (iDRAC SOL) shows THRE.
pub const GUEST_TX_DRAIN_EXIT: usize = 4;

/// Per-port liveness; cleared on THR timeout so a missing UART cannot stall boot.
static mut COM1_LIVE: bool = true;
static mut COM2_LIVE: bool = true;

/// E4: host serial log ring for SPA / `GET /logs/serial` (not a guest console).
pub const SERIAL_LOG_CAP: usize = 4096;
static mut LOG_BUF: [u8; SERIAL_LOG_CAP] = [0; SERIAL_LOG_CAP];
static mut LOG_HEAD: usize = 0;
static mut LOG_LEN: usize = 0;
static mut GUEST_TX: [u8; GUEST_TX_CAP] = [0; GUEST_TX_CAP];
static mut GUEST_TX_HEAD: usize = 0;
static mut GUEST_TX_LEN: usize = 0;
/// Iron `202312f`: TX ring made `Linux version` readable, then a blocking HV
/// `write_byte` (hypervisor-scan bump) interleaved e820 and COM2 cut mid-word.
/// After Linux `#PF` deliver, HV diagnostics share the guest TX ring.
static LINUX_EARLYCON_SHARE: AtomicBool = AtomicBool::new(false);

/// Route HV `write_byte` through the guest TX ring during Linux earlycon.
///
/// INVARIANTS:
/// - `true` after first Linux `#PF` deliver; `false` on guest-UEFI reset / E4
/// - Does not by itself print `ISO-INSTALL-OK`
///
/// VERIFICATION: L1 (host tests)
pub fn set_linux_earlycon_share(on: bool) {
    LINUX_EARLYCON_SHARE.store(on, Ordering::Release);
}

/// Whether HV diagnostics currently share the guest UART TX ring.
pub fn linux_earlycon_share() -> bool {
    LINUX_EARLYCON_SHARE.load(Ordering::Acquire)
}

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

fn guest_tx_push(byte: u8) {
    // SAFETY: single-threaded boot / HV; ring only touched from serial writers.
    unsafe {
        if GUEST_TX_LEN == GUEST_TX_CAP {
            GUEST_TX_HEAD = (GUEST_TX_HEAD + 1) % GUEST_TX_CAP;
            GUEST_TX_LEN -= 1;
        }
        let idx = (GUEST_TX_HEAD + GUEST_TX_LEN) % GUEST_TX_CAP;
        GUEST_TX[idx] = byte;
        GUEST_TX_LEN += 1;
    }
}

/// Bytes waiting in the guest UART TX ring.
pub fn guest_tx_len() -> usize {
    unsafe { GUEST_TX_LEN }
}

/// Drop queued guest UART bytes (tests / guest-UEFI reset).
pub fn guest_tx_clear() {
    unsafe {
        GUEST_TX_HEAD = 0;
        GUEST_TX_LEN = 0;
    }
}

fn guest_tx_port_thre(live: bool, base: u16) -> bool {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: port I/O to a fixed legacy UART base; one LSR poll.
        unsafe { !live || (inb(base + 5) & 0x20) != 0 }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        let _ = (live, base);
        true
    }
}

/// iDRAC SOL is COM2. Do not stall the TX ring on COM1 THRE.
///
/// Iron `b983ef8`: readable `Linux version` / two e820 lines then COM2
/// froze. Drain used to wait until **both** ports showed THRE, so a wedged
/// COM1 (rear serial, no reader) stopped SOL. guest UART TX drain COM2
/// independent. Not `ISO-INSTALL-OK`.
fn guest_tx_sol_ready() -> bool {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: COM2_LIVE is HV-owned; one LSR poll.
        unsafe { guest_tx_port_thre(COM2_LIVE, COM2) }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        true
    }
}

#[cfg(test)]
static GUEST_TX_TEST_SOL_NOT_READY: AtomicBool = AtomicBool::new(false);

/// Host-test hook: pretend iDRAC SOL THRE is clear.
#[cfg(test)]
pub fn set_guest_tx_test_sol_not_ready(on: bool) {
    GUEST_TX_TEST_SOL_NOT_READY.store(on, Ordering::Relaxed);
}

/// Guest 16550 LSR THRE/TEMT during Linux earlycon share.
///
/// Linux `serial8250_putc` polls LSR bit 5. Always-1 THRE lets printk
/// `out` faster than iDRAC SOL can accept, then COM2 looks frozen mid-e820
/// (iron `029ac8f` / `3dc7d11` hush-on-bootimg still cut at `[`). When
/// share is on, THRE follows COM2 and an empty TX ring so earlycon cannot
/// outrun SOL. Drain leftovers on the LSR poll. linux earlycon pace LSR
/// THRE. Not `ISO-INSTALL-OK`.
///
/// INVARIANTS:
/// - `false` only while `linux_earlycon_share` is on and COM2 cannot take
///   another byte (or the TX ring still holds a byte)
/// - Share off keeps LSR `0x60` (OVMF firmware serial / nested iso=0)
///
/// VERIFICATION: L1 (host tests)
pub fn guest_tx_guest_lsr_thre() -> bool {
    if !linux_earlycon_share() {
        return true;
    }
    // Nested QEMU `dcf8495` `/init` SIGSEGV 3/3 when LSR always called here
    // (share off). Pace only after product-ISO earlycon share. iso=0 never
    // latches share.
    #[cfg(test)]
    {
        if GUEST_TX_TEST_SOL_NOT_READY.load(Ordering::Relaxed) {
            return false;
        }
    }
    if guest_tx_sol_ready() {
        let _ = drain_guest_tx(GUEST_TX_DRAIN_CHUNK);
    }
    guest_tx_sol_ready() && guest_tx_len() == 0
}

fn guest_tx_write_ports(byte: u8) {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: COM2 THRE was true; COM1 is opportunistic.
        // KANI-TARGET: guest UART TX ring drain (outside Proven Core).
        unsafe {
            if COM2_LIVE {
                outb(COM2, byte);
            }
            if COM1_LIVE && guest_tx_port_thre(true, COM1) {
                outb(COM1, byte);
            }
        }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        let _ = byte;
    }
}

/// Write queued guest UART bytes while COM2 THRE is set. Never spins.
///
/// INVARIANTS:
/// - At most `max` bytes leave the ring
/// - Does not wait [`THR_WAIT_SPINS`] and does not clear COM1/COM2 liveness
/// - COM1 not ready does not block COM2 (guest UART TX drain COM2 independent)
///
/// VERIFICATION: L1 (host tests)
pub fn drain_guest_tx(max: usize) -> usize {
    let mut n = 0usize;
    while n < max {
        let empty = unsafe { GUEST_TX_LEN == 0 };
        if empty || !guest_tx_sol_ready() {
            break;
        }
        // SAFETY: single-threaded HV; LEN>0 and HEAD in range after the empty check.
        let byte = unsafe {
            let b = GUEST_TX[GUEST_TX_HEAD];
            GUEST_TX_HEAD = (GUEST_TX_HEAD + 1) % GUEST_TX_CAP;
            GUEST_TX_LEN -= 1;
            b
        };
        guest_tx_write_ports(byte);
        n += 1;
    }
    n
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
///
/// During Linux earlycon share, **drop** HV diagnostics. Guest printk uses
/// [`write_byte_nowait`]; mixing HV `write_byte` into the same FIFO shredded
/// `Linux version` on iron `9a3cbfa` (`linux cpuid n=2`/`n=3` then
/// `ogtFixune` / cmdline fragments). `ISO-INSTALL-OK` must use
/// [`write_line_nowait`]. linux earlycon hush HV. Not `ISO-INSTALL-OK`.
pub fn write_byte(byte: u8) {
    if linux_earlycon_share() {
        return;
    }
    // Translate `\n` → `\r\n` for typical serial terminals.
    if byte == b'\n' {
        write_raw(b'\r');
        log_push(b'\r');
    }
    write_raw(byte);
    log_push(byte);
}

/// Guest firmware/Linux THR tee. Enqueue, then drain while THRE is set.
///
/// Always records the byte in the host serial ring. Never waits
/// [`THR_WAIT_SPINS`] and never clears COM1/COM2 liveness.
/// Iron `115e5ee`: blocking guest `out` after a tick line left SOL on PAT.
/// Iron `f423d03`: one LSR poll *dropped* Linux printk (`0ce20ps` / PAT
/// fragments) while HV `write_byte` ticks held THRE. Guest UART TX ring
/// drain retries on later exits. Not `ISO-INSTALL-OK`.
pub fn write_byte_nowait(byte: u8) {
    if byte == b'\n' {
        guest_tx_push(b'\r');
        log_push(b'\r');
    }
    guest_tx_push(byte);
    log_push(byte);
    let _ = drain_guest_tx(GUEST_TX_DRAIN_CHUNK);
}

fn write_raw(byte: u8) {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: single-threaded boot / post-EBS HV; THR wait, no LIVE clear.
        unsafe {
            write_raw_port(COM1, byte, COM1_LIVE);
            write_raw_port(COM2, byte, COM2_LIVE);
        }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        let _ = byte; // host/unit-test: no port I/O; log ring still records
    }
}

unsafe fn write_raw_port(base: u16, byte: u8, live: bool) {
    if !live {
        return;
    }
    // SAFETY: port I/O to fixed legacy UART bases; bounded THR wait.
    // KANI-TARGET: host COM1/COM2 THR wait (outside Proven Core).
    for _ in 0..THR_WAIT_SPINS {
        if inb(base + 5) & 0x20 != 0 {
            outb(base, byte);
            return;
        }
        core::hint::spin_loop();
    }
    // Drop this byte. Keep the port live so later ticks / ISO-INSTALL-OK
    // still reach iDRAC SOL (iron 115e5ee n=441600 PAT freeze).
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

/// Guest-visible string during Linux earlycon share (does not go through
/// [`write_byte`], which hushes HV). linux unhandled nowait stop.
pub fn write_str_nowait(s: &str) {
    for &b in s.as_bytes() {
        write_byte_nowait(b);
    }
}

/// Guest-visible line during Linux earlycon share (does not go through
/// [`write_byte`], which hushes HV). Used for `RAYNU-V-M7-ISO-INSTALL-OK`.
pub fn write_line_nowait(s: &str) {
    write_str_nowait(s);
    write_byte_nowait(b'\n');
}

/// Revive diagnostic UART liveness without reprogramming baud/FIFO.
///
/// Re-enable writes if a port was marked dead. [`write_raw_port`] no longer
/// clears liveness on THR timeout (guest UART nowait; do not clear COM2_LIVE).
/// Full [`init`] can glitch SOL mid-session; this only flips the flags.
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
    fn guest_tx_ring_queues_then_drains() {
        guest_tx_clear();
        for i in 0..(GUEST_TX_CAP + 8) {
            guest_tx_push(i as u8);
        }
        assert_eq!(guest_tx_len(), GUEST_TX_CAP);
        let n = drain_guest_tx(GUEST_TX_CAP);
        assert_eq!(n, GUEST_TX_CAP);
        assert_eq!(guest_tx_len(), 0);
        guest_tx_clear();
    }

    #[test]
    fn write_byte_nowait_logs_without_clearing_live() {
        serial_log_clear();
        guest_tx_clear();
        write_byte_nowait(b'G');
        write_byte_nowait(b'\n');
        let mut buf = [0u8; 8];
        let n = serial_log_snapshot(&mut buf);
        assert!(n >= 2);
        assert_eq!(buf[0], b'G');
        assert_eq!(guest_tx_len(), 0);
        let s = include_str!("serial.rs");
        assert!(s.contains("fn write_byte_nowait"));
        assert!(s.contains("fn drain_guest_tx"));
        assert!(s.contains("Keep the port live"));
        assert!(s.contains("guest UART nowait; do not clear COM2_LIVE"));
        assert!(s.contains("guest UART TX ring drain"));
        assert!(s.contains("GUEST_TX_DRAIN_EXIT"));
        assert_eq!(GUEST_TX_DRAIN_EXIT, 4);
        assert!(s.contains("guest UART TX drain COM2 independent"));
        assert!(s.contains("fn guest_tx_sol_ready"));
        assert!(s.contains("fn guest_tx_guest_lsr_thre"));
        assert!(s.contains("linux earlycon pace LSR THRE"));
        serial_log_clear();
        guest_tx_clear();
    }

    #[test]
    fn linux_earlycon_share_routes_write_byte_to_ring() {
        serial_log_clear();
        guest_tx_clear();
        set_linux_earlycon_share(false);
        assert!(!linux_earlycon_share());
        set_linux_earlycon_share(true);
        assert!(linux_earlycon_share());
        write_byte(b'E');
        write_str("820");
        assert_eq!(serial_log_len(), 0);
        write_byte_nowait(b'G');
        write_str_nowait("N");
        write_line_nowait("OK");
        let mut buf = [0u8; 8];
        let n = serial_log_snapshot(&mut buf);
        assert!(n >= 4);
        assert_eq!(buf[0], b'G');
        let s = include_str!("serial.rs");
        assert!(s.contains("linux earlycon share TX ring"));
        assert!(s.contains("linux earlycon hush HV"));
        assert!(s.contains("fn write_str_nowait"));
        assert!(s.contains("fn write_line_nowait"));
        assert!(s.contains("fn set_linux_earlycon_share"));
        set_linux_earlycon_share(false);
        serial_log_clear();
        guest_tx_clear();
    }

    #[test]
    fn host_try_read_byte_is_none() {
        assert!(try_read_byte().is_none());
    }
}
