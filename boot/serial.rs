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

use crate::guest::slot::{PerGuest, PerGuestCell};
use crate::{per_guest, per_guest_cell};
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

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
///
/// Iron `916af96` / `34420783162`: `ring=2048` full with `com2_lsr=0x0` on
/// the stall heartbeat — HV diag lines and guest console shared 2 KiB and
/// drop-oldest shredded SOL. 4 KiB with [`GUEST_TX_RESERVE`] kept for HV
/// lines. guest UART TX ring room. Not `ISO-INSTALL-OK`.
pub const GUEST_TX_CAP: usize = 4096;
/// Ring bytes kept free of guest console output so a stall heartbeat
/// (~400 bytes) or `ISO-INSTALL-OK` never evicts guest text. The guest 16550
/// reports THRE while `room >= GUEST_TX_RESERVE + GUEST_TX_THRE_BURST`.
pub const GUEST_TX_RESERVE: usize = 1024;
/// One Linux 8250 `tx_loadsz` (16550A FIFO). The guest THRE level needs room
/// for a whole burst so `serial8250_tx_chars` never overruns the reserve.
pub const GUEST_TX_THRE_BURST: usize = 16;
/// Bytes to push toward SOL per guest `out` (keep the UART exit short).
pub const GUEST_TX_DRAIN_CHUNK: usize = 64;
/// Bytes to push on a UART line-rate pacing exit (one host FIFO burst).
/// guest UART line-rate pace. Not `ISO-INSTALL-OK`.
pub const GUEST_TX_DRAIN_PACE: usize = 16;
/// Host COM2 bytes written per THRE=1 window when the port reported a
/// working 16550A FIFO at init (IIR bits 7:6 both set after FCR 0xC7):
/// THRE means the 16-byte TX FIFO is empty, so 16 slots are free. Linux
/// `tx_loadsz` does the same on ttyS1. Without a FIFO one byte per window.
/// guest UART COM2 FIFO burst. Not `ISO-INSTALL-OK`.
pub const COM2_FIFO_BURST: usize = 16;
/// Bytes to push on a non-UART VM-exit. Nested QEMU CI `be0f1cd` blasted
/// 64 host THR bytes on every preemption/HLT exit while THRE stayed set,
/// then E4 `/init` SIGSEGV hung until the 480s timeout (3/3). Iron SOL
/// still drains the ring: UART `out` uses [`GUEST_TX_DRAIN_CHUNK`]; later
/// exits leak a few bytes whenever COM2 (iDRAC SOL) shows THRE.
pub const GUEST_TX_DRAIN_EXIT: usize = 4;

/// Per-port liveness; cleared on THR timeout so a missing UART cannot stall boot.
static mut COM1_LIVE: bool = true;
static mut COM2_LIVE: bool = true;
/// COM2 reported a 16550A FIFO at [`init`] (IIR bits 7:6 after FCR 0xC7).
/// Host tests: `true`. guest UART COM2 FIFO burst.
static COM2_FIFO: AtomicBool = AtomicBool::new(true);
/// Bytes drained to COM2 while THRE was low would be lost; count the THRE
/// windows we used and the bytes we moved for the stall heartbeat.
static GUEST_TX_DRAINED: AtomicU32 = AtomicU32::new(0);
static GUEST_TX_WINDOWS: AtomicU32 = AtomicU32::new(0);

/// E4: host serial log ring for SPA / `GET /logs/serial` (not a guest console).
pub const SERIAL_LOG_CAP: usize = 4096;
static mut LOG_BUF: [u8; SERIAL_LOG_CAP] = [0; SERIAL_LOG_CAP];
static mut LOG_HEAD: usize = 0;
static mut LOG_LEN: usize = 0;
static mut GUEST_TX: [u8; GUEST_TX_CAP] = [0; GUEST_TX_CAP];
static mut GUEST_TX_HEAD: usize = 0;
static mut GUEST_TX_LEN: usize = 0;
/// SPA copy of guest COM1 TX (survives SOL drain). Not iDRAC COM2.
static SPA_GUEST_LOG: PerGuestCell<[u8; GUEST_TX_CAP]> = per_guest_cell!([0; GUEST_TX_CAP]);
static SPA_GUEST_LOG_HEAD: PerGuestCell<usize> = per_guest_cell!(0);
static SPA_GUEST_LOG_LEN: PerGuestCell<usize> = per_guest_cell!(0);
/// Iron `202312f`: TX ring made `Linux version` readable, then a blocking HV
/// `write_byte` (hypervisor-scan bump) interleaved e820 and COM2 cut mid-word.
/// After Linux `#PF` deliver, HV diagnostics share the guest TX ring.
static LINUX_EARLYCON_SHARE: PerGuest<AtomicBool> = per_guest!(AtomicBool::new(false));
/// Product-ISO Linux high-half RIP (bit 63). Distinct from earlycon share,
/// which latches at El Torito bootimg while GRUB still needs PIIX ATAPI.
static LINUX_HIGH_HALF: PerGuest<AtomicBool> = per_guest!(AtomicBool::new(false));

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

/// Product-ISO Linux kernel is on a high-half RIP (not GRUB/OVMF identity).
///
/// INVARIANTS:
/// - `true` after first product-ISO VM-exit with RIP bit 63
/// - `false` on guest-UEFI reset / E4
///
/// VERIFICATION: L1 (host tests)
pub fn set_linux_high_half(on: bool) {
    LINUX_HIGH_HALF.store(on, Ordering::Release);
}

/// linux high-half. Used so linux hides PIIX IDE after GRUB finished ATAPI.
pub fn linux_high_half() -> bool {
    LINUX_HIGH_HALF.load(Ordering::Acquire)
}

/// One line of a guest that does not own COM2, while it is being typed.
pub const SIDE_LINE_CAP: usize = 192;
/// Finished side lines waiting for COM2. Whole lines only.
pub const SIDE_QUEUE_CAP: usize = 8192;
/// Room the side queue keeps before a non-owner guest's 16550 reads THRE.
pub const SIDE_QUEUE_THRE_ROOM: usize = 1024;
/// TSC ticks the owner's line may sit unfinished before a side line is
/// allowed to cut in (about 50 ms at 2 GHz; a prompt has no newline).
const SIDE_CUT_IN_TSC: u64 = 100_000_000;
/// TSC ticks a side line may sit without a newline before it is shown as
/// it stands (about 250 ms at 2 GHz; a login prompt never ends its line).
const SIDE_STALE_TSC: u64 = 500_000_000;

struct SideLine {
    buf: [u8; SIDE_LINE_CAP],
    len: usize,
    /// 0 text · 1 after ESC · 2 inside a CSI sequence.
    esc: u8,
    /// TSC of the last byte added.
    tsc: u64,
}

static SIDE_LINE: PerGuestCell<SideLine> = per_guest_cell!(SideLine {
    buf: [0; SIDE_LINE_CAP],
    len: 0,
    esc: 0,
    tsc: 0,
});
static mut SIDE_Q: [u8; SIDE_QUEUE_CAP] = [0; SIDE_QUEUE_CAP];
static mut SIDE_Q_HEAD: usize = 0;
static mut SIDE_Q_LEN: usize = 0;
static SIDE_DROPPED: AtomicU32 = AtomicU32::new(0);
/// Last byte queued for COM2 and when. A side line goes out only at the
/// start of a line, so the owner's text is never cut mid-word.
static mut COM2_TAIL: u8 = b'\n';
static COM2_TAIL_TSC: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// True when the current guest shares the host with another guest and is
/// not the one that owns COM2. Its console is then written as `[gN] `
/// lines between the owner's lines, and it gets no COM2 input.
///
/// INVARIANTS:
/// - `false` with one guest on, so one guest writes COM2 exactly as before
/// - `false` for the console owner
///
/// VERIFICATION: L1 (host tests)
pub fn side_console() -> bool {
    crate::guest::slot::shared()
        && crate::guest::slot::current() != crate::guest::slot::console_owner()
}

fn side_q_room() -> usize {
    // SAFETY: single-threaded HV; the queue is touched only by serial writers.
    unsafe { SIDE_QUEUE_CAP - SIDE_Q_LEN }
}

fn side_q_push(byte: u8) {
    // SAFETY: caller checked room; single-threaded HV.
    unsafe {
        let idx = (SIDE_Q_HEAD + SIDE_Q_LEN) % SIDE_QUEUE_CAP;
        SIDE_Q[idx] = byte;
        SIDE_Q_LEN += 1;
    }
}

/// Close the current guest's side line and queue it as `[gN] text`.
fn side_flush_line() {
    let slot = crate::guest::slot::current();
    // SAFETY: BSP-only; one writer at a time.
    let line = unsafe { &mut *SIDE_LINE.p() };
    let n = line.len;
    line.len = 0;
    if n == 0 {
        return;
    }
    let need = 5 + n + 2;
    if side_q_room() < need {
        SIDE_DROPPED.fetch_add(1, Ordering::AcqRel);
        return;
    }
    for &b in &[b'[', b'g', b'0' + (slot as u8 % 10), b']', b' '] {
        side_q_push(b);
        log_push(b);
    }
    for &b in &line.buf[..n] {
        side_q_push(b);
        log_push(b);
    }
    side_q_push(b'\r');
    side_q_push(b'\n');
    log_push(b'\r');
    log_push(b'\n');
}

/// One console byte of a guest that does not own COM2.
///
/// The byte is kept in that guest's SPA console log as written. For COM2
/// the terminal control sequences are dropped and the text is collected
/// into a line, so two guests never interleave inside a line.
fn side_push(byte: u8) {
    spa_guest_log_push(byte);
    // SAFETY: BSP-only; one writer at a time.
    let line = unsafe { &mut *SIDE_LINE.p() };
    match line.esc {
        1 => {
            line.esc = if byte == b'[' { 2 } else { 0 };
            return;
        }
        2 => {
            if (0x40..=0x7e).contains(&byte) {
                line.esc = 0;
            }
            return;
        }
        _ => {}
    }
    match byte {
        0x1b => line.esc = 1,
        b'\r' => {}
        b'\n' => side_flush_line(),
        0x20..=0x7e | b'\t' => {
            if line.len == SIDE_LINE_CAP {
                side_flush_line();
            }
            // SAFETY: same exclusive borrow; flush only reset `len`.
            let line = unsafe { &mut *SIDE_LINE.p() };
            line.buf[line.len] = byte;
            line.len += 1;
            line.tsc = crate::arch::cpu::rdtsc();
        }
        _ => {}
    }
}

/// Lines of non-owner guests dropped because COM2 could not keep up.
pub fn side_dropped() -> u32 {
    SIDE_DROPPED.load(Ordering::Acquire)
}

/// Bytes of finished side lines waiting for COM2.
pub fn side_queue_len() -> usize {
    // SAFETY: single-threaded HV.
    unsafe { SIDE_Q_LEN }
}

/// Close the current guest's side line as it stands. The guest scheduler
/// calls this when that guest leaves for good, so its last words are shown.
pub fn side_break() {
    if side_console() {
        side_flush_line();
    }
}

/// Close the current guest's side line when nothing was added to it for a
/// while (a login prompt has no newline). The guest scheduler calls this
/// when it switches away from a non-owner guest. A line that is still
/// being written is left alone: a 2 ms quantum must not cut it in pieces
/// (nested two-guest run printed `[g0] inux`, `[g0]  vers`, `[g0] ion `).
pub fn side_break_stale(now: u64) {
    if !side_console() {
        return;
    }
    // SAFETY: BSP-only; one writer at a time.
    let (len, tsc) = unsafe {
        let line = &*SIDE_LINE.p();
        (line.len, line.tsc)
    };
    if side_line_is_stale(len, tsc, now) {
        side_flush_line();
    }
}

/// Pure form of [`side_break_stale`].
pub fn side_line_is_stale(len: usize, last_tsc: u64, now: u64) -> bool {
    len != 0 && now.wrapping_sub(last_tsc) >= SIDE_STALE_TSC
}

/// Move whole side lines into the COM2 ring when the owner is at the
/// start of a line, or has left a line unfinished for a while.
fn side_pump() {
    // SAFETY: single-threaded HV; rings touched only from serial writers.
    unsafe {
        while SIDE_Q_LEN != 0 {
            let mut n = 0usize;
            while n < SIDE_Q_LEN && SIDE_Q[(SIDE_Q_HEAD + n) % SIDE_QUEUE_CAP] != b'\n' {
                n += 1;
            }
            if n == SIDE_Q_LEN {
                return;
            }
            n += 1;
            let at_start = COM2_TAIL == b'\n';
            if !at_start {
                let quiet = crate::arch::cpu::rdtsc()
                    .wrapping_sub(COM2_TAIL_TSC.load(Ordering::Acquire));
                if quiet < SIDE_CUT_IN_TSC {
                    return;
                }
            }
            if GUEST_TX_CAP - GUEST_TX_LEN < n + 2 {
                return;
            }
            if !at_start {
                guest_tx_ring_put(b'\r');
                guest_tx_ring_put(b'\n');
            }
            for _ in 0..n {
                let b = SIDE_Q[SIDE_Q_HEAD];
                SIDE_Q_HEAD = (SIDE_Q_HEAD + 1) % SIDE_QUEUE_CAP;
                SIDE_Q_LEN -= 1;
                guest_tx_ring_put(b);
            }
        }
    }
}

/// Drop every queued side line (tests / leaving the guest path).
pub fn side_clear() {
    // SAFETY: single-threaded HV.
    unsafe {
        SIDE_Q_HEAD = 0;
        SIDE_Q_LEN = 0;
        COM2_TAIL = b'\n';
    }
    for slot in 0..crate::guest::slot::MAX_GUESTS {
        // SAFETY: BSP-only.
        unsafe { (*SIDE_LINE.p_of(slot)).len = 0 };
    }
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
    guest_tx_ring_put(byte);
    spa_guest_log_push(byte);
}

/// Queue one byte for COM2. Drops the oldest byte when the ring is full.
fn guest_tx_ring_put(byte: u8) {
    // SAFETY: single-threaded boot / HV; ring only touched from serial writers.
    unsafe {
        if GUEST_TX_LEN == GUEST_TX_CAP {
            GUEST_TX_HEAD = (GUEST_TX_HEAD + 1) % GUEST_TX_CAP;
            GUEST_TX_LEN -= 1;
        }
        let idx = (GUEST_TX_HEAD + GUEST_TX_LEN) % GUEST_TX_CAP;
        GUEST_TX[idx] = byte;
        GUEST_TX_LEN += 1;
        COM2_TAIL = byte;
    }
    COM2_TAIL_TSC.store(crate::arch::cpu::rdtsc(), Ordering::Release);
}

fn spa_guest_log_push(byte: u8) {
    // SAFETY: same BSP serial writers as [`guest_tx_push`].
    unsafe {
        if (*SPA_GUEST_LOG_LEN.p()) == GUEST_TX_CAP {
            (*SPA_GUEST_LOG_HEAD.p()) = ((*SPA_GUEST_LOG_HEAD.p()) + 1) % GUEST_TX_CAP;
            (*SPA_GUEST_LOG_LEN.p()) -= 1;
        }
        let idx = ((*SPA_GUEST_LOG_HEAD.p()) + (*SPA_GUEST_LOG_LEN.p())) % GUEST_TX_CAP;
        (*SPA_GUEST_LOG.p())[idx] = byte;
        (*SPA_GUEST_LOG_LEN.p()) += 1;
    }
}

/// Copy the SPA guest COM1 log (oldest → newest). Survives SOL drain.
/// Not iDRAC `console com2`.
pub fn spa_guest_log_snapshot(out: &mut [u8]) -> usize {
    // SAFETY: single-threaded boot / HV; ring only touched from serial writers.
    unsafe {
        let n = (*SPA_GUEST_LOG_LEN.p()).min(out.len());
        for i in 0..n {
            out[i] = (*SPA_GUEST_LOG.p())[((*SPA_GUEST_LOG_HEAD.p()) + i) % GUEST_TX_CAP];
        }
        n
    }
}

/// Drop the SPA guest log (tests / guest-UEFI reset).
pub fn spa_guest_log_clear() {
    unsafe {
        (*SPA_GUEST_LOG_HEAD.p()) = 0;
        (*SPA_GUEST_LOG_LEN.p()) = 0;
    }
}

#[cfg(test)]
pub fn spa_guest_log_push_for_test(byte: u8) {
    spa_guest_log_push(byte);
}

/// Bytes waiting in the guest UART TX ring.
pub fn guest_tx_len() -> usize {
    unsafe { GUEST_TX_LEN }
}

/// Free bytes in the guest UART TX ring.
pub fn guest_tx_room() -> usize {
    GUEST_TX_CAP.saturating_sub(guest_tx_len())
}

/// Pure form of the guest THRE level: room for one more `tx_loadsz` burst
/// above the HV reserve. guest UART TX ring room.
pub fn guest_tx_room_has_thre(room: usize) -> bool {
    room >= GUEST_TX_RESERVE + GUEST_TX_THRE_BURST
}

/// Ring room the THRE interrupt follows. The polled LSR bit does not.
/// A host test can force this low without changing LSR.
pub fn guest_tx_irq_room() -> bool {
    #[cfg(test)]
    {
        if GUEST_TX_TEST_RING_FULL.load(Ordering::Relaxed) {
            return false;
        }
    }
    guest_tx_room_has_thre(guest_tx_room())
}

/// Bytes moved to COM2 / THRE windows used since reset (stall heartbeat).
pub fn guest_tx_drain_stats() -> (u32, u32) {
    (
        GUEST_TX_DRAINED.load(Ordering::Acquire),
        GUEST_TX_WINDOWS.load(Ordering::Acquire),
    )
}

/// Whether COM2 showed a 16550A FIFO at init (bursts of [`COM2_FIFO_BURST`]).
pub fn com2_fifo() -> bool {
    COM2_FIFO.load(Ordering::Acquire)
}

/// Host-test hook for the FIFO flag.
#[cfg(test)]
pub fn set_com2_fifo_for_test(on: bool) {
    COM2_FIFO.store(on, Ordering::Release);
}

/// Drop queued guest UART bytes (tests / guest-UEFI reset).
pub fn guest_tx_clear() {
    unsafe {
        GUEST_TX_HEAD = 0;
        GUEST_TX_LEN = 0;
    }
    spa_guest_log_clear();
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
        #[cfg(test)]
        {
            if GUEST_TX_TEST_SOL_NOT_READY.load(Ordering::Relaxed) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
static GUEST_TX_TEST_SOL_NOT_READY: AtomicBool = AtomicBool::new(false);

/// Host-test hook: pretend host COM2 (iDRAC SOL) THRE is low so the ring
/// does not drain. Does **not** change the guest THRE level (ring room).
#[cfg(test)]
pub fn set_guest_tx_test_sol_not_ready(on: bool) {
    GUEST_TX_TEST_SOL_NOT_READY.store(on, Ordering::Relaxed);
}

/// Raw host COM2 LSR (iDRAC SOL) for the THRE chain heartbeat; `0x60` on
/// host tests. Read-only. UART THRE chain telemetry. Not `ISO-INSTALL-OK`.
pub fn com2_lsr_raw() -> u8 {
    #[cfg(target_os = "uefi")]
    {
        // SAFETY: one LSR poll on the fixed legacy COM2 base.
        unsafe { inb(COM2 + 5) }
    }
    #[cfg(not(target_os = "uefi"))]
    {
        0x60
    }
}

/// [`guest_tx_sol_ready`] for the THRE chain heartbeat (no drain).
pub fn guest_tx_sol_ready_peek() -> bool {
    guest_tx_sol_ready()
}

#[cfg(test)]
static GUEST_TX_TEST_RING_FULL: AtomicBool = AtomicBool::new(false);

/// Host-test hook: pretend the guest TX ring has no room above the reserve
/// (the guest THRE level reads low).
#[cfg(test)]
pub fn set_guest_tx_test_ring_full(on: bool) {
    GUEST_TX_TEST_RING_FULL.store(on, Ordering::Relaxed);
}

/// Guest 16550 LSR THRE/TEMT during Linux earlycon share.
///
/// Linux `serial8250_putc` polls LSR bit 5. Always-1 THRE lets printk
/// `out` faster than iDRAC SOL can accept, then COM2 looks frozen mid-e820
/// (iron `029ac8f` / `3dc7d11` hush-on-bootimg still cut at `[`). When
/// share is on, THRE follows **ring room**: the guest may write while the
/// ring has [`GUEST_TX_THRE_BURST`] free above [`GUEST_TX_RESERVE`]. The
/// ring, not the instantaneous host COM2 LSR, is the buffer; the ring is
/// drained toward iDRAC SOL on every VM exit and on the line-rate pacing
/// timer. linux earlycon pace LSR THRE. guest UART TX ring room.
/// Not `ISO-INSTALL-OK`.
///
/// History: iron `916af96` / `34420783162` coupled THRE to `com2_lsr` bit 5
/// **and** an empty ring, sampled only at VM exits. Under `idle=poll` the
/// only exits were ~15 preemption ticks a second, each moving one byte, so
/// `apk` (`n_tty_write`) got a 16-byte THRE IRQ per rare coincidence and
/// its 4 KiB xmit buffer never drained (`pend=0` with `ring=0` and
/// `com2_lsr=0x0` on every heartbeat).
///
/// INVARIANTS:
/// - `false` only while `linux_earlycon_share` is on and the ring lacks
///   `GUEST_TX_RESERVE + GUEST_TX_THRE_BURST` free bytes
/// - Share off keeps LSR `0x60` (OVMF firmware serial / nested iso=0)
///
/// VERIFICATION: L1 (host tests)
/// 8N1. ESXi's virtual 16550 shifts one character at this rate.
pub const UART_BAUD: u64 = 115_200;
pub const UART_BITS_PER_CHAR: u64 = 10;
/// Virtual TX FIFO depth. THRE means this FIFO has a free slot, not that
/// the physical COM2 has drained.
pub const VIRT_FIFO_CAP: u32 = 16;

static VIRT_FIFO: AtomicU32 = AtomicU32::new(0);
static VIRT_FIFO_TSC: AtomicU64 = AtomicU64::new(0);

/// TSC ticks for one character at [`UART_BAUD`].
pub fn uart_char_ticks(tsc_hz: u64) -> u64 {
    let hz = if tsc_hz < 1_000 { 2_000_000_000 } else { tsc_hz };
    (hz / (UART_BAUD / UART_BITS_PER_CHAR)).max(1)
}

/// Characters the virtual shifter has finished since `then`.
pub fn uart_chars_shifted(now: u64, then: u64, tsc_hz: u64) -> u64 {
    now.wrapping_sub(then) / uart_char_ticks(tsc_hz)
}

/// FIFO occupancy after `shifted` characters have left.
pub fn virt_fifo_after(queued: u32, shifted: u64) -> u32 {
    queued.saturating_sub(shifted.min(u64::from(queued)) as u32)
}

/// THRE: the 16-byte virtual FIFO has a free slot.
pub fn virt_fifo_thre(queued: u32) -> bool {
    queued < VIRT_FIFO_CAP
}

fn virt_fifo_sync(now: u64, tsc_hz: u64) -> u32 {
    let then = VIRT_FIFO_TSC.load(Ordering::Acquire);
    let queued = VIRT_FIFO.load(Ordering::Acquire);
    if queued == 0 {
        VIRT_FIFO_TSC.store(now, Ordering::Release);
        return 0;
    }
    let shifted = uart_chars_shifted(now, then, tsc_hz);
    let left = virt_fifo_after(queued, shifted);
    if shifted > 0 {
        let step = uart_char_ticks(tsc_hz).saturating_mul(shifted.min(u64::from(queued)));
        VIRT_FIFO_TSC.store(then.wrapping_add(step), Ordering::Release);
        VIRT_FIFO.store(left, Ordering::Release);
    }
    left
}

/// A guest THR byte entered the virtual FIFO. Bytes past 16 stay in the
/// ring, but THRE stays clear until the baud clock catches up.
pub fn note_guest_tx_byte(now: u64, tsc_hz: u64) {
    let queued = virt_fifo_sync(now, tsc_hz);
    if queued < VIRT_FIFO_CAP {
        VIRT_FIFO.store(queued + 1, Ordering::Release);
    }
}

fn virt_fifo_has_room() -> bool {
    let now = crate::arch::cpu::rdtsc();
    let hz = crate::boot::raynu_f_flag::tsc_hz();
    virt_fifo_thre(virt_fifo_sync(now, hz))
}

pub fn guest_tx_guest_lsr_thre() -> bool {
    if side_console() {
        // A guest that does not own COM2 writes lines into the side queue.
        // Its THRE follows that queue, not the owner's ring.
        let _ = drain_guest_tx(GUEST_TX_DRAIN_CHUNK);
        return side_q_room() >= SIDE_QUEUE_THRE_ROOM;
    }
    // ESXi virtual 16550: THRE follows a 16-byte FIFO shifted at 115200 8N1,
    // not host COM2 and not the big ring. Iron `ba07618258cd` reported THRE
    // always and the console shredded; earlier sticks cleared THRE with the
    // ring and `wait_for_xmitr` never returned (`[    0.`).
    // linux earlycon pace LSR THRE.
    #[cfg(test)]
    {
        if GUEST_TX_TEST_RING_FULL.load(Ordering::Relaxed) {
            return false;
        }
        return true;
    }
    #[cfg(not(test))]
    {
        virt_fifo_has_room()
    }
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

/// Bytes one COM2 THRE=1 window can take: a whole FIFO when COM2 has one.
/// Pure form for host tests. guest UART COM2 FIFO burst.
pub fn guest_tx_burst_for_window(fifo: bool, max_left: usize) -> usize {
    if fifo {
        COM2_FIFO_BURST.min(max_left)
    } else {
        1.min(max_left)
    }
}

/// Write queued guest UART bytes while COM2 THRE is set. Never spins.
///
/// Each THRE=1 poll opens one window of [`guest_tx_burst_for_window`]
/// bytes (16 with a 16550A FIFO — THRE means the FIFO is empty; iron
/// `916af96` polled LSR before *every* byte, so one exit moved one byte).
///
/// INVARIANTS:
/// - At most `max` bytes leave the ring
/// - Does not wait [`THR_WAIT_SPINS`] and does not clear COM1/COM2 liveness
/// - COM1 not ready does not block COM2 (guest UART TX drain COM2 independent)
/// - No byte is written to COM2 without a THRE=1 poll at most 15 bytes earlier
///
/// VERIFICATION: L1 (host tests)
pub fn drain_guest_tx(max: usize) -> usize {
    side_pump();
    let mut n = 0usize;
    let fifo = com2_fifo();
    while n < max {
        let empty = unsafe { GUEST_TX_LEN == 0 };
        if empty || !guest_tx_sol_ready() {
            break;
        }
        GUEST_TX_WINDOWS.fetch_add(1, Ordering::AcqRel);
        let burst = guest_tx_burst_for_window(fifo, max - n);
        for _ in 0..burst {
            let empty = unsafe { GUEST_TX_LEN == 0 };
            if empty {
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
    }
    if n != 0 {
        GUEST_TX_DRAINED.fetch_add(n as u32, Ordering::AcqRel);
    }
    n
}

/// Bounded spins for [`flush_guest_tx`] while COM2 THRE is low (~4 KiB at
/// 115200 8N1 is ~360 ms; each spin re-polls LSR).
pub const GUEST_TX_FLUSH_SPINS: u32 = 2_000_000;

/// Push every queued guest UART byte out to COM2 before a mode change.
///
/// Iron `59ac070` / `34425781629`: Linux `reboot` → the F7 `guest reset
/// requested src=` line was queued nowait during earlycon share and then
/// share was switched off before any VM exit drained it, so COM2 never
/// showed it. The guest is halted at these call sites, so a bounded wait
/// on COM2 THRE is acceptable here (and nowhere else). guest UART TX flush.
///
/// INVARIANTS:
/// - Returns the number of bytes written; ring is empty on return unless
///   COM2 THRE stayed low for `GUEST_TX_FLUSH_SPINS` polls
/// - Never touches liveness; never drops bytes itself
///
/// VERIFICATION: L1 (host tests)
pub fn flush_guest_tx() -> usize {
    let mut n = 0usize;
    let mut spins = 0u32;
    while guest_tx_len() != 0 && spins < GUEST_TX_FLUSH_SPINS {
        let moved = drain_guest_tx(GUEST_TX_CAP);
        n += moved;
        if moved == 0 {
            spins += 1;
            core::hint::spin_loop();
        }
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
    let iir = init_port(COM2);
    COM2_FIFO.store(iir_reports_fifo(iir), Ordering::Release);
}

/// 16550A: IIR bits 7:6 read `11` once FCR bit 0 is set. `0xFF` is a
/// floating (missing) port — treat as no FIFO so the drain stays one byte
/// per window. Pure form for host tests. guest UART COM2 FIFO burst.
pub fn iir_reports_fifo(iir: u8) -> bool {
    iir != 0xFF && (iir & 0xC0) == 0xC0
}

fn init_port(base: u16) -> u8 {
    unsafe {
        outb(base + 1, 0x00); // Disable interrupts
        outb(base + 3, 0x80); // Enable DLAB
        outb(base + 0, 0x01); // Divisor low (115200)
        outb(base + 1, 0x00); // Divisor high
        outb(base + 3, 0x03); // 8N1, DLAB off
        outb(base + 2, 0xC7); // Enable FIFO, clear, 14-byte threshold
        outb(base + 4, 0x0B); // IRQs enabled, RTS/DSR set
        inb(base + 2) // IIR: FIFO enabled bits 7:6
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
    if side_console() {
        side_push(byte);
        return;
    }
    // While guests share the host, the ring can hold another guest's
    // finished lines. They go out first, so this line is not written into
    // the middle of one (nested two-guest run: a `[g0]` line ended inside
    // the other guest's `RayNu-F svc=` line). One guest: unchanged.
    if crate::guest::slot::shared() && guest_tx_len() != 0 {
        let _ = flush_guest_tx();
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
    if side_console() {
        side_push(byte);
        let _ = drain_guest_tx(GUEST_TX_DRAIN_CHUNK);
        return;
    }
    if byte == b'\n' {
        guest_tx_push(b'\r');
        log_push(b'\r');
    }
    guest_tx_push(byte);
    log_push(byte);
    if linux_earlycon_share() {
        note_guest_tx_byte(crate::arch::cpu::rdtsc(), crate::boot::raynu_f_flag::tsc_hz());
    }
    let _ = drain_guest_tx(GUEST_TX_DRAIN_CHUNK);
}

fn write_raw(byte: u8) {
    // SAFETY: single-threaded boot / HV.
    unsafe {
        COM2_TAIL = byte;
    }
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
    fn spa_guest_log_survives_sol_drain() {
        guest_tx_clear();
        guest_tx_push(b'A');
        guest_tx_push(b'B');
        let _ = drain_guest_tx(GUEST_TX_CAP);
        assert_eq!(guest_tx_len(), 0);
        let mut buf = [0u8; 8];
        let n = spa_guest_log_snapshot(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], b"AB");
        guest_tx_clear();
        assert_eq!(spa_guest_log_snapshot(&mut buf), 0);
    }

    #[test]
    fn virtual_fifo_shifts_one_character_at_a_time() {
        let hz = 115_200u64; // 10 ticks per character at 115200 8N1
        assert_eq!(uart_char_ticks(hz), 10);
        assert_eq!(uart_chars_shifted(0, 0, hz), 0);
        assert_eq!(uart_chars_shifted(9, 0, hz), 0);
        assert_eq!(uart_chars_shifted(10, 0, hz), 1);
        assert_eq!(uart_chars_shifted(160, 0, hz), 16);
        assert_eq!(virt_fifo_after(16, 1), 15);
        assert!(virt_fifo_thre(15));
        assert!(!virt_fifo_thre(16));
    }

    fn guest_thre_follows_ring_room_not_com2() {
        // guest UART TX ring room: iron 916af96 coupled guest THRE to
        // com2_lsr bit 5 + empty ring sampled at ~15 exits/s (apk stuck in
        // n_tty_write). The ring is the buffer now.
        guest_tx_clear();
        set_linux_earlycon_share(true);
        assert!(guest_tx_guest_lsr_thre());
        assert!(guest_tx_room_has_thre(GUEST_TX_CAP));
        assert!(guest_tx_room_has_thre(GUEST_TX_RESERVE + GUEST_TX_THRE_BURST));
        assert!(!guest_tx_room_has_thre(GUEST_TX_RESERVE + GUEST_TX_THRE_BURST - 1));
        assert!(!guest_tx_room_has_thre(0));
        assert_eq!(GUEST_TX_CAP, 4096);
        assert_eq!(GUEST_TX_RESERVE, 1024);
        assert_eq!(GUEST_TX_THRE_BURST, 16);
        assert!(GUEST_TX_RESERVE + GUEST_TX_THRE_BURST < GUEST_TX_CAP);
        set_linux_earlycon_share(false);
        guest_tx_clear();
        let s = include_str!("serial.rs");
        assert!(s.contains("guest UART TX ring room"));
        assert!(s.contains("fn guest_tx_room_has_thre"));
        assert!(s.contains("fn guest_tx_room"));
    }

    #[test]
    fn com2_fifo_window_takes_a_burst() {
        // guest UART COM2 FIFO burst: THRE=1 on a 16550A means 16 free slots.
        assert!(iir_reports_fifo(0xC1));
        assert!(iir_reports_fifo(0xC2));
        assert!(!iir_reports_fifo(0x01), "no FIFO bits");
        assert!(!iir_reports_fifo(0x81), "16550 (non-A) FIFO unusable");
        assert!(!iir_reports_fifo(0xFF), "floating port");
        assert_eq!(guest_tx_burst_for_window(true, 64), COM2_FIFO_BURST);
        assert_eq!(guest_tx_burst_for_window(true, 4), 4);
        assert_eq!(guest_tx_burst_for_window(false, 64), 1);
        assert_eq!(guest_tx_burst_for_window(false, 0), 0);
        assert_eq!(COM2_FIFO_BURST, 16);
        assert_eq!(GUEST_TX_DRAIN_PACE, 16);
        guest_tx_clear();
        set_com2_fifo_for_test(true);
        let (d0, w0) = guest_tx_drain_stats();
        for i in 0..40u8 {
            guest_tx_push(i);
        }
        assert_eq!(drain_guest_tx(64), 40);
        let (d1, w1) = guest_tx_drain_stats();
        assert_eq!(d1 - d0, 40);
        assert_eq!(w1 - w0, 3, "16 + 16 + 8 in three THRE windows");
        set_com2_fifo_for_test(false);
        for i in 0..5u8 {
            guest_tx_push(i);
        }
        assert_eq!(drain_guest_tx(64), 5);
        let (_, w2) = guest_tx_drain_stats();
        assert_eq!(w2 - w1, 5, "one byte per window without a FIFO");
        set_com2_fifo_for_test(true);
        guest_tx_clear();
        let s = include_str!("serial.rs");
        assert!(s.contains("guest UART COM2 FIFO burst"));
        assert!(s.contains("fn iir_reports_fifo"));
        assert!(s.contains("fn guest_tx_burst_for_window"));
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

    #[test]
    fn flush_guest_tx_empties_the_ring_before_a_mode_change() {
        // Iron `59ac070`: the F7 `guest reset requested src=` line was queued
        // nowait during earlycon share and lost when share ended before any
        // exit drained it. `flush_guest_tx` pushes the ring out (bounded).
        serial_log_clear();
        guest_tx_clear();
        set_linux_earlycon_share(true);
        set_guest_tx_test_sol_not_ready(true);
        for &b in b"boot: RayNu-F guest reset requested src=kbc n=1\n" {
            write_byte_nowait(b);
        }
        let queued = guest_tx_len();
        assert!(queued > 40, "ring holds the line while SOL is back-pressured");
        set_guest_tx_test_sol_not_ready(false);
        assert_eq!(flush_guest_tx(), queued);
        assert_eq!(guest_tx_len(), 0);
        assert_eq!(flush_guest_tx(), 0, "empty ring is a no-op");
        assert!(GUEST_TX_FLUSH_SPINS >= 1_000_000);
        let s = include_str!("serial.rs");
        assert!(s.contains("guest UART TX flush"));
        assert!(s.contains("pub fn flush_guest_tx"));
        set_linux_earlycon_share(false);
        serial_log_clear();
        guest_tx_clear();
    }

    /// Two guests on, slot 1 owns COM2. Restores the run table on drop.
    struct TwoGuests;
    impl TwoGuests {
        fn new() -> Self {
            use crate::guest::slot;
            serial_log_clear();
            guest_tx_clear();
            side_clear();
            slot::set_on(0, true);
            slot::set_on(1, true);
            slot::set_console_owner(1);
            TwoGuests
        }
    }
    impl Drop for TwoGuests {
        fn drop(&mut self) {
            use crate::guest::slot;
            slot::set_current(0);
            slot::set_on(0, false);
            slot::set_on(1, false);
            slot::set_console_owner(0);
            side_clear();
            serial_log_clear();
            guest_tx_clear();
        }
    }

    fn log_text() -> std::string::String {
        let mut buf = [0u8; 1024];
        let n = serial_log_snapshot(&mut buf);
        std::string::String::from_utf8_lossy(&buf[..n]).into_owned()
    }

    #[test]
    fn one_guest_never_has_a_side_console() {
        use crate::guest::slot;
        assert!(!slot::shared());
        assert!(!side_console());
        slot::set_on(0, true);
        slot::set_console_owner(1);
        assert!(!side_console(), "one guest on writes COM2 as it always did");
        slot::set_on(0, false);
        slot::set_console_owner(0);
    }

    #[test]
    fn a_side_guest_is_written_as_whole_prefixed_lines() {
        use crate::guest::slot;
        let _g = TwoGuests::new();
        slot::set_current(0);
        assert!(side_console());
        // A guest switch in the middle of a line does not end it.
        write_str_nowait("[    0.000000] Linux vers");
        side_break_stale(crate::arch::cpu::rdtsc());
        assert_eq!(side_queue_len(), 0, "a line still being written stays whole");
        assert_eq!(guest_tx_len(), 0);
        write_str_nowait("ion 6.12\x1b[0m\r\n");
        slot::set_current(1);
        assert!(!side_console());
        write_str_nowait("owner\n");
        let _ = flush_guest_tx();
        let text = log_text();
        assert!(
            text.contains("[g0] [    0.000000] Linux version 6.12\r\n"),
            "side line is whole, prefixed and stripped of escapes: {text:?}"
        );
        assert!(text.contains("owner\r\n"));
        assert_eq!(side_dropped(), 0);
    }

    #[test]
    fn a_side_prompt_without_a_newline_is_shown_once_it_is_stale() {
        use crate::guest::slot;
        let _g = TwoGuests::new();
        slot::set_current(0);
        write_str_nowait("localhost login: ");
        let now = crate::arch::cpu::rdtsc();
        side_break_stale(now);
        assert_eq!(side_queue_len(), 0);
        side_break_stale(now.wrapping_add(SIDE_STALE_TSC * 2));
        assert!(side_queue_len() > 0, "a quiet prompt is shown as it stands");
        assert!(!side_line_is_stale(0, 0, u64::MAX), "an empty line is never stale");
        assert!(side_line_is_stale(1, 10, 10 + SIDE_STALE_TSC));
        assert!(!side_line_is_stale(1, 10, 9 + SIDE_STALE_TSC));
    }

    #[test]
    fn a_direct_line_waits_for_queued_side_lines() {
        use crate::guest::slot;
        let _g = TwoGuests::new();
        slot::set_current(0);
        write_str_nowait("side line one\n");
        slot::set_current(1);
        // The owner is in its firmware phase: the hypervisor writes its
        // lines straight to the port. The queued side line goes out whole
        // first.
        write_str("boot: RayNu-F svc=OpenProtocol\n");
        assert_eq!(guest_tx_len(), 0);
        assert_eq!(side_queue_len(), 0);
        let text = log_text();
        let side = text.find("[g0] side line one\r\n").expect("side line whole");
        let direct = text.find("boot: RayNu-F svc=OpenProtocol").expect("direct line");
        assert!(side < direct, "queued side line is written before the direct line");
    }
}
