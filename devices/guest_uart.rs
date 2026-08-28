//! 16550 UART for Stage 46 product ISO (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests)
//!
//! Lab El Torito keeps the guest-UEFI stub UART (LSR 0x60, IIR 0x01). Linux
//! 8250 autoconfig needs a scratch register and FIFO IIR. COM1 IRQ is ISA
//! GSI 4. Host/CI never prints `ISO-INSTALL-OK`.

use core::sync::atomic::{AtomicBool, Ordering};

/// ISA COM1.
pub const COM1_IRQ: u8 = 4;

struct Uart {
    lcr: u8,
    ier: u8,
    mcr: u8,
    scr: u8,
    dll: u8,
    dlm: u8,
    fcr: u8,
    thre_irq: bool,
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
}

fn port_uart(u: &mut Uarts, com1: bool) -> &mut Uart {
    if com1 {
        &mut u.com1
    } else {
        &mut u.com2
    }
}

/// Product-ISO 16550 PIO. `thr` is a THR byte to emit on the host serial.
pub fn pio(port: u16, is_in: bool, val: u8) -> (u8, Option<u8>, bool) {
    let com1 = (0x03F8..=0x03FF).contains(&port);
    let off = (port & 7) as u8;
    let (out, thr, irq) = with_uart(|u| {
        let uart = port_uart(u, com1);
        if is_in {
            (uart_read(uart, off), None, uart.thre_irq)
        } else {
            let thr = uart_write(uart, off, val);
            (val, thr, uart.thre_irq)
        }
    });
    if com1 {
        if irq {
            crate::devices::guest_irq::raise_gsi(COM1_IRQ);
        } else {
            crate::devices::guest_irq::lower_gsi(COM1_IRQ);
        }
    }
    (out, thr, irq)
}

/// Re-assert COM1 THRE if Linux left IER.ETBEI set (edge inject consumed IRR).
pub fn reassert_irq() {
    let pending = with_uart(|u| u.com1.thre_irq);
    if pending {
        crate::devices::guest_irq::raise_gsi(COM1_IRQ);
    }
}

fn dlab(u: &Uart) -> bool {
    u.lcr & 0x80 != 0
}

fn uart_read(u: &mut Uart, off: u8) -> u8 {
    match off {
        0 if dlab(u) => u.dll,
        0 => 0,
        1 if dlab(u) => u.dlm,
        1 => u.ier,
        2 => {
            if u.thre_irq && u.ier & 2 != 0 {
                u.thre_irq = false;
                0xC2
            } else if u.fcr & 1 != 0 {
                0xC1
            } else {
                0x01
            }
        }
        3 => u.lcr,
        4 => u.mcr,
        5 => 0x60,
        6 => 0xB0,
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
            if u.ier & 2 != 0 {
                u.thre_irq = true;
            }
            Some(val)
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
