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

use core::sync::atomic::{AtomicBool, Ordering};

/// ISA COM1.
pub const COM1_IRQ: u8 = 4;
const RX_CAP: usize = 16;

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

fn irq_pending(u: &Uart) -> bool {
    (u.rx_len > 0 && u.ier & 1 != 0) || (u.thre_irq && u.ier & 2 != 0)
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
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    } else {
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
            (out, None, irq_pending(uart))
        } else {
            let thr = uart_write(uart, off, val);
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

/// Re-assert COM1 RX/THRE if Linux left IER set (edge inject consumed IRR).
pub fn reassert_irq() {
    let pending = with_uart(|u| irq_pending(&u.com1));
    if pending {
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
        0 => rx_pop(u),
        1 if dlab(u) => u.dlm,
        1 => u.ier,
        2 => {
            if u.rx_len > 0 && u.ier & 1 != 0 {
                iir_bits(0x04, u.fcr & 1 != 0)
            } else if u.thre_irq && u.ier & 2 != 0 {
                u.thre_irq = false;
                iir_bits(0x02, u.fcr & 1 != 0)
            } else {
                iir_bits(0x01, u.fcr & 1 != 0)
            }
        }
        3 => u.lcr,
        4 => u.mcr,
        5 => {
            // linux earlycon pace LSR THRE (THRE|TEMT follow host COM2).
            let mut m = 0u8;
            if crate::boot::serial::guest_tx_guest_lsr_thre() {
                m |= 0x60;
            }
            if u.rx_len > 0 {
                m |= 0x01;
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
