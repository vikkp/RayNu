//! 16550 UART for Stage 46 product ISO (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests)
//!
//! Lab El Torito keeps the guest-UEFI stub UART (LSR 0x60, IIR 0x01). Linux
//! 8250 autoconfig needs a scratch register, FIFO IIR, and MCR loopback.
//! After product-ISO El Torito bootimg, linux earlycon pace LSR THRE so
//! printk cannot outrun iDRAC SOL (iron hush-on-bootimg still cut at e820).
//! COM1 IRQ is ISA GSI 4. Host COM2 (iDRAC SOL) RX feeds guest COM1 RBR so
//! the installer can take serial input. Host/CI never prints `ISO-INSTALL-OK`.
//!
//! THRE is a level: pending while IER.ETBEI is set and the shared TX ring
//! is empty (`guest_tx_guest_lsr_thre`). Iron `f229d14` / `34354322953`
//! SysRq `t`: `apk` slept in `n_tty_write` → `wait_woken` (stdout is the
//! 8250 console) while userspace output advanced exactly one 16-byte
//! `tx_loadsz` per *other* IRQ 4. The IIR read had cleared the THRE latch
//! and the following LSR read said THRE=0 (SOL back-pressure or our own
//! diag bytes in the ring), so `serial8250_handle_irq` skipped
//! `tx_chars` and the interrupt was never re-raised: 4 KiB xmit buffer
//! full, `apk` blocked forever, no more virtio kicks. UART THRE level
//! until stop_tx.
//!
//! A line BREAK (LSR.BI with a NUL in RBR) followed by one key is the 8250
//! console's magic-SysRq path (`uart_handle_break` then
//! `uart_prepare_sysrq_char`). The hypervisor uses it at the apk stall so
//! Linux prints its own task state (`w`/`m`/`t`/`l`). That is an architected
//! serial-line input, not firmware state mutation (ADR-016).
//! UART sysrq break. Not `ISO-INSTALL-OK`.
//!
//! Iron `8b6ed1a` / `34415711199` ran the THRE level and COM2 looked
//! identical to the `f229d14` probe: `apk` still asleep in `n_tty_write`,
//! console text still moving only around SysRq IRQ 4s. Every link of the
//! THRE chain (IER.ETBEI on the device, `thre_irq`, TX ring empty, COM2
//! THRE, PIC IRR/IMR/ISR, IRQ 4 taken, IIR class, LSR class) is now
//! counted and printed with the stall heartbeat so the next flash names
//! the broken link instead of guessing. UART THRE chain telemetry.
//! Not `ISO-INSTALL-OK`.
//!
//! Iron `916af96` / `34420783162` named it: every counted link agreed
//! (`take4` == `iir` THRE == `lsr_thre` ON, PIC IRR/IMR/ISR sane, ETBEI
//! set) but the level was almost never true — `pend=0` even with `ring=0`
//! because guest THRE was `com2_lsr` bit 5 **and** an empty ring, sampled
//! only at VM exits, and under `idle=poll` the only exits were ~15
//! preemption ticks a second, each draining one byte. Guest THRE now
//! follows **ring room** (`guest_tx_room_has_thre`); the shared ring is the
//! buffer and it drains toward iDRAC SOL on a line-rate pacing timer plus a
//! whole 16550A FIFO per THRE window. guest UART TX ring room.
//! Not `ISO-INSTALL-OK`.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// ISA COM1.
pub const COM1_IRQ: u8 = 4;
const RX_CAP: usize = 16;

/// COM1 IIR reads that reported RX / THRE / no interrupt.
static STAT_IIR_RX: AtomicU32 = AtomicU32::new(0);
static STAT_IIR_THRE: AtomicU32 = AtomicU32::new(0);
static STAT_IIR_NONE: AtomicU32 = AtomicU32::new(0);
/// COM1 LSR reads while IER.ETBEI was set: THRE reported 1 / 0.
static STAT_LSR_THRE_ON: AtomicU32 = AtomicU32::new(0);
static STAT_LSR_THRE_OFF: AtomicU32 = AtomicU32::new(0);
/// COM1 THR writes; IER writes with ETBEI set / clear.
static STAT_THR_WR: AtomicU32 = AtomicU32::new(0);
static STAT_IER_ETBEI_ON: AtomicU32 = AtomicU32::new(0);
static STAT_IER_ETBEI_OFF: AtomicU32 = AtomicU32::new(0);
/// IRQ 4 raised by [`reassert_irq`]; raised / lowered by a COM1 PIO.
static STAT_REASSERT_RAISE: AtomicU32 = AtomicU32::new(0);
static STAT_PIO_RAISE: AtomicU32 = AtomicU32::new(0);
static STAT_PIO_LOWER: AtomicU32 = AtomicU32::new(0);

/// One snapshot of the COM1 THRE interrupt chain. UART THRE chain telemetry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThreChain {
    pub ier: u8,
    pub thre_irq: bool,
    pub break_pending: bool,
    pub rx_len: u8,
    /// [`thre_pending`] as the inject path sees it right now.
    pub thre_pending: bool,
    pub iir_rx: u32,
    pub iir_thre: u32,
    pub iir_none: u32,
    pub lsr_thre_on: u32,
    pub lsr_thre_off: u32,
    pub thr_wr: u32,
    pub ier_etbei_on: u32,
    pub ier_etbei_off: u32,
    pub reassert_raise: u32,
    pub pio_raise: u32,
    pub pio_lower: u32,
}

fn bump(c: &AtomicU32) {
    c.fetch_add(1, Ordering::AcqRel);
}

/// Snapshot the COM1 THRE chain (device state + counters since reset).
/// UART THRE chain telemetry. Not `ISO-INSTALL-OK`.
pub fn thre_chain() -> ThreChain {
    with_uart(|u| ThreChain {
        ier: u.com1.ier,
        thre_irq: u.com1.thre_irq,
        break_pending: u.com1.break_pending,
        rx_len: u.com1.rx_len,
        thre_pending: thre_pending(&u.com1),
        iir_rx: STAT_IIR_RX.load(Ordering::Acquire),
        iir_thre: STAT_IIR_THRE.load(Ordering::Acquire),
        iir_none: STAT_IIR_NONE.load(Ordering::Acquire),
        lsr_thre_on: STAT_LSR_THRE_ON.load(Ordering::Acquire),
        lsr_thre_off: STAT_LSR_THRE_OFF.load(Ordering::Acquire),
        thr_wr: STAT_THR_WR.load(Ordering::Acquire),
        ier_etbei_on: STAT_IER_ETBEI_ON.load(Ordering::Acquire),
        ier_etbei_off: STAT_IER_ETBEI_OFF.load(Ordering::Acquire),
        reassert_raise: STAT_REASSERT_RAISE.load(Ordering::Acquire),
        pio_raise: STAT_PIO_RAISE.load(Ordering::Acquire),
        pio_lower: STAT_PIO_LOWER.load(Ordering::Acquire),
    })
}

fn stats_reset() {
    for c in [
        &STAT_IIR_RX,
        &STAT_IIR_THRE,
        &STAT_IIR_NONE,
        &STAT_LSR_THRE_ON,
        &STAT_LSR_THRE_OFF,
        &STAT_THR_WR,
        &STAT_IER_ETBEI_ON,
        &STAT_IER_ETBEI_OFF,
        &STAT_REASSERT_RAISE,
        &STAT_PIO_RAISE,
        &STAT_PIO_LOWER,
    ] {
        c.store(0, Ordering::Release);
    }
}

struct Uart {
    lcr: u8,
    ier: u8,
    mcr: u8,
    scr: u8,
    dll: u8,
    dlm: u8,
    fcr: u8,
    thre_irq: bool,
    rx: [u8; RX_CAP],
    rx_head: u8,
    rx_len: u8,
    /// Line BREAK ahead of the RX FIFO: LSR reads `DR|BI`, RBR reads 0x00
    /// once and clears it. UART sysrq break.
    break_pending: bool,
}

impl Uart {
    const fn empty() -> Self {
        Self {
            lcr: 0,
            ier: 0,
            mcr: 0,
            scr: 0,
            dll: 0x0C,
            dlm: 0,
            fcr: 0,
            thre_irq: false,
            rx: [0; RX_CAP],
            rx_head: 0,
            rx_len: 0,
            break_pending: false,
        }
    }
}

struct Uarts {
    com1: Uart,
    com2: Uart,
}

struct GuestUart(core::cell::UnsafeCell<Uarts>);

// SAFETY: exclusive access is enforced by `UART_LOCK`.
// KANI-TARGET: guest-UEFI UART mutex (outside Proven Core).
unsafe impl Sync for GuestUart {}

static UARTS: GuestUart = GuestUart(core::cell::UnsafeCell::new(Uarts {
    com1: Uart::empty(),
    com2: Uart::empty(),
}));
static UART_LOCK: AtomicBool = AtomicBool::new(false);

fn with_uart<R>(f: impl FnOnce(&mut Uarts) -> R) -> R {
    while UART_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI UART mutex (outside Proven Core).
    let out = unsafe { f(&mut *UARTS.0.get()) };
    UART_LOCK.store(false, Ordering::Release);
    out
}

pub fn reset() {
    with_uart(|u| *u = Uarts {
        com1: Uart::empty(),
        com2: Uart::empty(),
    });
    stats_reset();
    crate::devices::guest_serial_answer::reset();
}

fn port_uart(u: &mut Uarts, com1: bool) -> &mut Uart {
    if com1 {
        &mut u.com1
    } else {
        &mut u.com2
    }
}

fn loopback(u: &Uart) -> bool {
    u.mcr & 0x10 != 0
}

fn rx_ready(u: &Uart) -> bool {
    u.rx_len > 0 || u.break_pending
}

/// THR empty for interrupt purposes: follows shared TX ring **room** when
/// Linux earlycon share is on (not host COM2 LSR), else always empty.
/// UART THRE level until stop_tx. guest UART TX ring room.
fn tx_empty() -> bool {
    crate::boot::serial::guest_tx_guest_lsr_thre()
}

fn thre_pending(u: &Uart) -> bool {
    u.thre_irq && u.ier & 2 != 0 && tx_empty()
}

fn irq_pending(u: &Uart) -> bool {
    (rx_ready(u) && u.ier & 1 != 0) || thre_pending(u)
}

fn rx_push(u: &mut Uart, b: u8) -> bool {
    if u.rx_len as usize >= RX_CAP {
        return false;
    }
    let i = (u.rx_head.wrapping_add(u.rx_len) as usize) % RX_CAP;
    u.rx[i] = b;
    u.rx_len = u.rx_len.saturating_add(1);
    true
}

fn rx_pop(u: &mut Uart) -> u8 {
    if u.rx_len == 0 {
        return 0;
    }
    let b = u.rx[u.rx_head as usize];
    u.rx_head = ((u.rx_head as usize + 1) % RX_CAP) as u8;
    u.rx_len = u.rx_len.saturating_sub(1);
    b
}

fn iir_bits(id: u8, fifo: bool) -> u8 {
    if fifo {
        0xC0 | id
    } else {
        id
    }
}

fn sync_com1_irq(pending: bool) {
    if pending {
        bump(&STAT_PIO_RAISE);
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    } else {
        bump(&STAT_PIO_LOWER);
        crate::devices::guest_irq::lower_gsi(COM1_IRQ);
    }
}

/// Product-ISO 16550 PIO. `thr` is a THR byte to emit on the host serial.
pub fn pio(port: u16, is_in: bool, val: u8) -> (u8, Option<u8>, bool) {
    let com1 = (0x03F8..=0x03FF).contains(&port);
    let off = (port & 7) as u8;
    let (out, thr, irq) = with_uart(|u| {
        let uart = port_uart(u, com1);
        if is_in {
            let out = uart_read(uart, off);
            if com1 {
                count_com1_read(uart, off, out);
            }
            (out, None, irq_pending(uart))
        } else {
            let dlab_before = dlab(uart);
            let thr = uart_write(uart, off, val);
            if com1 {
                count_com1_write(off, val, dlab_before, thr.is_some());
            }
            (val, thr, irq_pending(uart))
        }
    });
    if com1 {
        sync_com1_irq(irq);
        if let Some(b) = thr {
            crate::devices::guest_serial_answer::note_tx(b);
        }
        drain_answers();
    }
    (out, thr, irq)
}

/// Classify a COM1 register read for the THRE chain counters.
/// UART THRE chain telemetry.
fn count_com1_read(u: &Uart, off: u8, out: u8) {
    match off {
        2 => match out & 0x07 {
            0x04 => bump(&STAT_IIR_RX),
            0x02 => bump(&STAT_IIR_THRE),
            _ => bump(&STAT_IIR_NONE),
        },
        5 if u.ier & 2 != 0 => {
            if out & 0x20 != 0 {
                bump(&STAT_LSR_THRE_ON);
            } else {
                bump(&STAT_LSR_THRE_OFF);
            }
        }
        _ => {}
    }
}

/// Classify a COM1 register write for the THRE chain counters.
/// UART THRE chain telemetry.
fn count_com1_write(off: u8, val: u8, dlab_before: bool, thr: bool) {
    match off {
        0 if thr => bump(&STAT_THR_WR),
        1 if !dlab_before => {
            if val & 2 != 0 {
                bump(&STAT_IER_ETBEI_ON);
            } else {
                bump(&STAT_IER_ETBEI_OFF);
            }
        }
        _ => {}
    }
}

/// Re-assert COM1 RX / THRE if Linux left IER set (edge inject consumed IRR).
///
/// History: UART reassert RX not THRE — an ungated THRE (always-empty THR)
/// re-raised IRQ 4 on every resume ahead of PIT so `idle=poll` never saw
/// jiffies and Alpine `sleep` after virtio DRIVER_OK hung. THRE is now
/// re-asserted only while [`tx_empty`] is true and IER.ETBEI is set, i.e.
/// only when Linux will either load the next `tx_loadsz` bytes or
/// `__stop_tx` (clear ETBEI). Rate is bounded by SOL drain (one IRQ per 16
/// bytes at 115200 baud). Iron `f229d14` proved the lost-THRE deadlock
/// (`apk` in `n_tty_write`). UART THRE level until stop_tx.
/// Not `ISO-INSTALL-OK`.
pub fn reassert_irq() {
    let pending = with_uart(|u| irq_pending(&u.com1));
    if pending {
        bump(&STAT_REASSERT_RAISE);
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    }
}

/// Host SOL / QEMU stdio byte into guest COM1 RBR. Returns false if the FIFO is full.
pub fn push_host_rx(b: u8) -> bool {
    let (ok, pending) = with_uart(|u| {
        let ok = rx_push(&mut u.com1, b);
        (ok, irq_pending(&u.com1))
    });
    if pending {
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    }
    ok
}

/// Magic SysRq into guest COM1: a line BREAK then `key`. Returns false when
/// a BREAK is still unread or the FIFO is full (caller retries later).
/// Linux 8250 console: `uart_handle_break` arms `port->sysrq` for 5 s of
/// jiffies; the next RBR byte goes to `handle_sysrq`. The BREAK and the key
/// sit back to back so the window cannot lapse. `sysrq_always_enabled` is
/// on the patched product-ISO cmdline. UART sysrq break.
/// Not `ISO-INSTALL-OK`.
pub fn inject_sysrq(key: u8) -> bool {
    let (ok, pending) = with_uart(|u| {
        if u.com1.break_pending || (u.com1.rx_len as usize) >= RX_CAP {
            return (false, false);
        }
        u.com1.break_pending = true;
        let ok = rx_push(&mut u.com1, key);
        (ok, irq_pending(&u.com1))
    });
    if pending {
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    }
    ok
}

/// Drain host COM2 (iDRAC SOL) then COM1 into guest COM1. Product ISO only.
pub fn poll_host_rx() {
    for _ in 0..RX_CAP {
        let Some(b) = crate::boot::serial::try_read_byte() else {
            break;
        };
        if !push_host_rx(b) {
            break;
        }
    }
    drain_answers();
}

fn rx_free() -> usize {
    with_uart(|u| RX_CAP.saturating_sub(u.com1.rx_len as usize))
}

fn drain_answers() {
    while rx_free() > 0 {
        let Some(b) = crate::devices::guest_serial_answer::take_rx() else {
            break;
        };
        let _ = push_host_rx(b);
    }
}

fn dlab(u: &Uart) -> bool {
    u.lcr & 0x80 != 0
}

fn uart_read(u: &mut Uart, off: u8) -> u8 {
    match off {
        0 if dlab(u) => u.dll,
        0 => {
            if u.break_pending {
                // The BREAK's NUL is consumed before the queued sysrq key.
                u.break_pending = false;
                0
            } else {
                rx_pop(u)
            }
        }
        1 if dlab(u) => u.dlm,
        1 => u.ier,
        2 => {
            if rx_ready(u) && u.ier & 1 != 0 {
                iir_bits(0x04, u.fcr & 1 != 0)
            } else if thre_pending(u) {
                // Not cleared here: the shared ring can refill between this
                // IIR read and Linux's LSR read, and a 16550 only re-asserts
                // on a THR-empty transition we cannot replay. Linux ends
                // every THRE service with a THR write or `__stop_tx`
                // (ETBEI clear), so the level cannot storm. UART THRE level
                // until stop_tx.
                iir_bits(0x02, u.fcr & 1 != 0)
            } else {
                iir_bits(0x01, u.fcr & 1 != 0)
            }
        }
        3 => u.lcr,
        4 => u.mcr,
        5 => {
            // Nested QEMU `dcf8495` `/init` SIGSEGV 3/3 when every LSR IN
            // called into serial (share off). Keep the 0x60/0x61 path until
            // product-ISO earlycon share. linux earlycon pace LSR THRE.
            let mut m = if !crate::boot::serial::linux_earlycon_share() {
                if rx_ready(u) {
                    0x61
                } else {
                    0x60
                }
            } else {
                let mut m = 0u8;
                if crate::boot::serial::guest_tx_guest_lsr_thre() {
                    m |= 0x60;
                }
                if rx_ready(u) {
                    m |= 0x01;
                }
                m
            };
            if u.break_pending {
                // LSR.BI: 8250 `serial8250_read_char` calls
                // `uart_handle_break` and drops the NUL. UART sysrq break.
                m |= 0x10;
            }
            m
        }
        6 => {
            if loopback(u) {
                let mut m = 0u8;
                if u.mcr & 0x02 != 0 {
                    m |= 0x10;
                }
                if u.mcr & 0x01 != 0 {
                    m |= 0x20;
                }
                if u.mcr & 0x04 != 0 {
                    m |= 0x40;
                }
                if u.mcr & 0x08 != 0 {
                    m |= 0x80;
                }
                m
            } else {
                0xB0
            }
        }
        7 => u.scr,
        _ => 0,
    }
}

fn uart_write(u: &mut Uart, off: u8, val: u8) -> Option<u8> {
    match off {
        0 if dlab(u) => {
            u.dll = val;
            None
        }
        0 => {
            if loopback(u) {
                let _ = rx_push(u, val);
                None
            } else {
                if u.ier & 2 != 0 {
                    u.thre_irq = true;
                }
                Some(val)
            }
        }
        1 if dlab(u) => {
            u.dlm = val;
            None
        }
        1 => {
            u.ier = val;
            u.thre_irq = val & 2 != 0;
            None
        }
        2 => {
            u.fcr = val;
            if val & 2 != 0 {
                u.rx_len = 0;
                u.rx_head = 0;
                u.break_pending = false;
            }
            None
        }
        3 => {
            u.lcr = val;
            None
        }
        4 => {
            u.mcr = val;
            None
        }
        7 => {
            u.scr = val;
            None
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "guest_uart_test.rs"]
mod guest_uart_test;
