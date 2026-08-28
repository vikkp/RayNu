//! Guest-UEFI PIC + IOAPIC for Stage 46 product ISO (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-014)
//! VERIFICATION: L1 (runtime + host tests)
//!
//! Lab El Torito keeps 8259 RAZ/WI. A real distro installer (Linux virtio_blk
//! / libata) waits on interrupts: virtio INTx (i440FX slot 2 INTA → GSI 17,
//! slot 3 INTA → GSI 18, plus PCI interrupt line 11 as IOAPIC pin 11 when
//! the guest has no ACPI `_PRT`), ATA IRQ 14, and PIT IRQ 0 (jiffies under
//! `noapic`). IOAPIC delivery latches `lapic_virt` IRR (product ISO
//! `try_inject_guest_irq`) so Linux EOI matches. This module is live only
//! while the product ISO window is armed. Host/CI never prints `ISO-INSTALL-OK`.

use core::sync::atomic::{AtomicBool, Ordering};

/// IOAPIC MMIO window (QEMU/ICH).
pub const IOAPIC_GPA: u64 = 0xFEC0_0000;
pub const IOAPIC_SIZE: u64 = 0x1000;
pub const IOAPIC_PINS: usize = 24;
/// QEMU version 0x11, 24 pins (`(24-1)<<16 | 0x11`).
pub const IOAPIC_VERSION: u32 = 0x0017_0011;
/// ISA ATA / ATAPI.
pub const ATA_GSI: u8 = 14;
/// i440FX slot 2 INTA: `pirq = (pin + slot - 1) & 3` → PIRQB → GSI 16+1.
pub const VIRTIO_GSI: u8 = 17;
/// i440FX slot 3 INTA: `(0 + 3 - 1) & 3` → PIRQC → GSI 16+2.
pub const VIRTIO_ISO_GSI: u8 = 18;
/// PIC fallback for PCI INTx when the guest has not remapped via IOAPIC.
pub const VIRTIO_PIC_IRQ: u8 = 11;
/// ISA PIT. Linux `noapic` uses PIC IRQ 0 for jiffies / HLT wakeup.
pub const PIT_IRQ: u8 = 0;

const RTE_MASK: u64 = 1 << 16;
/// IOAPIC RTE bit 15: 1 = level, 0 = edge.
const RTE_TRIG_LEVEL: u64 = 1 << 15;
/// IOAPIC RTE bit 14: remote IRR (in-service, wait for LAPIC EOI).
const RTE_REMOTE_IRR: u64 = 1 << 14;
const PIC_SLAVE_IRQ: u8 = 2;

struct Pic {
    imr: u8,
    irr: u8,
    isr: u8,
    vector: u8,
    init: u8,
    expect_icw4: bool,
    ready: bool,
    read_isr: bool,
}

struct Ioapic {
    sel: u32,
    id: u32,
    redir: [u64; IOAPIC_PINS],
    irr: u32,
}

struct IrqChip {
    master: Pic,
    slave: Pic,
    ioapic: Ioapic,
}

impl Pic {
    const fn empty() -> Self {
        Self {
            imr: 0xFF,
            irr: 0,
            isr: 0,
            vector: 0x08,
            init: 0,
            expect_icw4: false,
            ready: false,
            read_isr: false,
        }
    }
}

impl Ioapic {
    const fn empty() -> Self {
        Self {
            sel: 0,
            id: 0,
            redir: [RTE_MASK; IOAPIC_PINS],
            irr: 0,
        }
    }
}

impl IrqChip {
    const fn empty() -> Self {
        Self {
            master: Pic::empty(),
            slave: Pic::empty(),
            ioapic: Ioapic::empty(),
        }
    }
}

struct GuestIrq(core::cell::UnsafeCell<IrqChip>);

// SAFETY: exclusive access is enforced by `IRQ_LOCK`.
// KANI-TARGET: guest-UEFI IRQ chip mutex (outside Proven Core).
unsafe impl Sync for GuestIrq {}

static IRQ: GuestIrq = GuestIrq(core::cell::UnsafeCell::new(IrqChip::empty()));
static IRQ_LOCK: AtomicBool = AtomicBool::new(false);

fn with_irq<R>(f: impl FnOnce(&mut IrqChip) -> R) -> R {
    while IRQ_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access.
    // KANI-TARGET: guest-UEFI IRQ chip mutex (outside Proven Core).
    let out = unsafe { f(&mut *IRQ.0.get()) };
    IRQ_LOCK.store(false, Ordering::Release);
    out
}

fn product_live() -> bool {
    crate::devices::ide_cdrom::product_iso_window_armed()
}

pub fn reset() {
    with_irq(|c| *c = IrqChip::empty());
}

/// True when GPA is the product-ISO IOAPIC 4 KiB window (not the HPET sink).
pub fn is_ioapic_gpa(gpa: u64) -> bool {
    product_live() && gpa >= IOAPIC_GPA && gpa < IOAPIC_GPA.wrapping_add(IOAPIC_SIZE)
}

/// Whole 2 MiB at `0xFEC00000` is a split PT when the product window is armed.
pub fn is_hpet_split_2m_gpa(gpa: u64) -> bool {
    product_live() && (gpa & !0x1F_FFFF) == crate::devices::guest_platform::HPET_SINK_PAGE
}

pub fn raise_ata() {
    raise_gsi(ATA_GSI);
}

pub fn lower_ata() {
    lower_gsi(ATA_GSI);
}

pub fn raise_virtio() {
    raise_gsi(VIRTIO_GSI);
    // PCI interrupt line is IRQ 11. Without ACPI `_PRT`, Linux unmasks
    // IOAPIC pin 11 (ISA identity), not pin 17.
    raise_gsi(VIRTIO_PIC_IRQ);
}

pub fn lower_virtio() {
    lower_gsi(VIRTIO_GSI);
    lower_gsi(VIRTIO_PIC_IRQ);
}

pub fn raise_virtio_iso() {
    raise_gsi(VIRTIO_ISO_GSI);
    raise_gsi(VIRTIO_PIC_IRQ);
}

pub fn lower_virtio_iso() {
    lower_gsi(VIRTIO_ISO_GSI);
    lower_gsi(VIRTIO_PIC_IRQ);
}

/// PIT IRQ 0. Latches IRR; PIC injects only after ICW2 ≥ 16 and unmask.
/// Also steps the i8253 channel-0 count so Linux `inb 0x40` sees time pass.
pub fn raise_pit() {
    crate::devices::guest_platform::pit_tick();
    raise_gsi(PIT_IRQ);
}

pub fn raise_gsi(gsi: u8) {
    if !product_live() {
        return;
    }
    with_irq(|c| {
        if (gsi as usize) < IOAPIC_PINS {
            c.ioapic.irr |= 1 << gsi;
        }
        if gsi < 16 {
            raise_pic_locked(c, gsi);
        }
    });
}

pub fn lower_gsi(gsi: u8) {
    with_irq(|c| {
        if (gsi as usize) < IOAPIC_PINS {
            c.ioapic.irr &= !(1 << gsi);
        }
        if gsi < 16 {
            lower_pic_locked(c, gsi);
        }
    });
}

fn raise_pic_locked(c: &mut IrqChip, irq: u8) {
    if irq < 8 {
        c.master.irr |= 1 << irq;
    } else {
        c.slave.irr |= 1 << (irq - 8);
        c.master.irr |= 1 << PIC_SLAVE_IRQ;
    }
}

fn lower_pic_locked(c: &mut IrqChip, irq: u8) {
    if irq < 8 {
        c.master.irr &= !(1 << irq);
    } else {
        c.slave.irr &= !(1 << (irq - 8));
        if c.slave.irr & !c.slave.imr == 0 {
            c.master.irr &= !(1 << PIC_SLAVE_IRQ);
        }
    }
}

pub fn has_deliverable() -> bool {
    if !product_live() {
        return false;
    }
    with_irq(|c| peek_vector(c).is_some())
}

/// Consume one pending vector for VM-entry external-interrupt injection.
///
/// IOAPIC first (Linux). PIC only after ICW2 remaps vectors out of 0..15.
pub fn take_inject_vector() -> Option<u8> {
    if !product_live() {
        return None;
    }
    with_irq(take_vector)
}

fn peek_vector(c: &IrqChip) -> Option<u8> {
    ioapic_peek(c).or_else(|| pic_peek(c))
}

fn take_vector(c: &mut IrqChip) -> Option<u8> {
    if let Some((pin, vec)) = ioapic_peek_pin(c) {
        ioapic_accept(c, pin);
        return Some(vec);
    }
    pic_take(c)
}

/// Accept an IOAPIC pin: set remote IRR. Edge clears pin IRR; level keeps
/// it until `lower_gsi` so a lost inject can retry after LAPIC EOI.
fn ioapic_accept(c: &mut IrqChip, pin: u8) {
    let i = pin as usize;
    let mut rte = c.ioapic.redir[i];
    rte |= RTE_REMOTE_IRR;
    c.ioapic.redir[i] = rte;
    if rte & RTE_TRIG_LEVEL == 0 {
        c.ioapic.irr &= !(1 << pin);
    }
}

/// Consume one unmasked IOAPIC pin. Does not touch PIC. Stage 46 product
/// ISO latches this vector into `lapic_virt` IRR so Linux EOI matches.
pub fn take_ioapic_vector() -> Option<u8> {
    if !product_live() {
        return None;
    }
    with_irq(|c| {
        let (pin, vec) = ioapic_peek_pin(c)?;
        ioapic_accept(c, pin);
        Some(vec)
    })
}

/// LAPIC EOI: drop remote IRR on RTEs that match `vec`. Level pins with
/// the line still high become deliverable again.
pub fn ioapic_eoi(vec: u8) {
    if !product_live() {
        return;
    }
    with_irq(|c| {
        for pin in 0..IOAPIC_PINS {
            let rte = c.ioapic.redir[pin];
            if (rte & 0xff) as u8 == vec && rte & RTE_REMOTE_IRR != 0 {
                c.ioapic.redir[pin] = rte & !RTE_REMOTE_IRR;
            }
        }
    });
}

fn ioapic_peek(c: &IrqChip) -> Option<u8> {
    ioapic_peek_pin(c).map(|(_, v)| v)
}

fn ioapic_peek_pin(c: &IrqChip) -> Option<(u8, u8)> {
    for pin in 0..IOAPIC_PINS {
        if (c.ioapic.irr & (1 << pin)) == 0 {
            continue;
        }
        let rte = c.ioapic.redir[pin];
        if rte & RTE_MASK != 0 {
            continue;
        }
        if rte & RTE_REMOTE_IRR != 0 {
            continue;
        }
        let vec = (rte & 0xff) as u8;
        if vec < 16 {
            continue;
        }
        return Some((pin as u8, vec));
    }
    None
}

fn pic_pending_irq(c: &IrqChip) -> Option<u8> {
    if !c.master.ready || c.master.vector < 16 {
        return None;
    }
    let master_req = c.master.irr & !c.master.imr & !c.master.isr;
    if master_req == 0 {
        return None;
    }
    if master_req & (1 << PIC_SLAVE_IRQ) != 0 && c.slave.ready && c.slave.vector >= 16 {
        let slave_req = c.slave.irr & !c.slave.imr & !c.slave.isr;
        if slave_req != 0 {
            return Some(8 + slave_req.trailing_zeros() as u8);
        }
    }
    // UART (IRQ 4) and other master devices beat PIT so timer ticks cannot
    // starve COM1 auto-answer.
    let master_dev = master_req & !1 & !(1 << PIC_SLAVE_IRQ);
    if master_dev != 0 {
        return Some(master_dev.trailing_zeros() as u8);
    }
    let irq = master_req.trailing_zeros() as u8;
    if irq == PIC_SLAVE_IRQ {
        None
    } else {
        Some(irq)
    }
}

fn pic_peek(c: &IrqChip) -> Option<u8> {
    let irq = pic_pending_irq(c)?;
    if irq < 8 {
        Some(c.master.vector.wrapping_add(irq))
    } else {
        Some(c.slave.vector.wrapping_add(irq - 8))
    }
}

fn pic_take(c: &mut IrqChip) -> Option<u8> {
    let irq = pic_pending_irq(c)?;
    if irq < 8 {
        c.master.irr &= !(1 << irq);
        c.master.isr |= 1 << irq;
        Some(c.master.vector.wrapping_add(irq))
    } else {
        let s = irq - 8;
        c.slave.irr &= !(1 << s);
        c.slave.isr |= 1 << s;
        c.master.isr |= 1 << PIC_SLAVE_IRQ;
        if c.slave.irr & !c.slave.imr == 0 {
            c.master.irr &= !(1 << PIC_SLAVE_IRQ);
        }
        Some(c.slave.vector.wrapping_add(s))
    }
}

/// 8259 PIC + ELCR. Lab path does not call this.
pub fn pic_io(port: u16, is_in: bool, size: u8, rax: u64) -> u64 {
    let mask = match size {
        1 => 0xffu64,
        2 => 0xffff,
        _ => 0xffff_ffff,
    };
    with_irq(|c| {
        if is_in {
            let v = pic_read(c, port);
            (rax & !mask) | (u64::from(v) & mask)
        } else {
            pic_write(c, port, rax as u8);
            rax
        }
    })
}

fn pic_of(c: &mut IrqChip, port: u16) -> &mut Pic {
    if port == 0xA0 || port == 0xA1 || port == 0x4D1 {
        &mut c.slave
    } else {
        &mut c.master
    }
}

fn pic_read(c: &mut IrqChip, port: u16) -> u8 {
    match port {
        0x21 => c.master.imr,
        0xA1 => c.slave.imr,
        0x20 => {
            if c.master.read_isr {
                c.master.isr
            } else {
                c.master.irr
            }
        }
        0xA0 => {
            if c.slave.read_isr {
                c.slave.isr
            } else {
                c.slave.irr
            }
        }
        _ => 0,
    }
}

fn pic_write(c: &mut IrqChip, port: u16, val: u8) {
    match port {
        0x20 | 0xA0 => pic_cmd(pic_of(c, port), val),
        0x21 | 0xA1 => pic_data(pic_of(c, port), val),
        _ => {}
    }
}

fn pic_cmd(p: &mut Pic, val: u8) {
    if val & 0x10 != 0 {
        p.init = 1;
        p.expect_icw4 = val & 1 != 0;
        p.imr = 0;
        p.isr = 0;
        p.irr = 0;
        p.ready = false;
        p.read_isr = false;
        return;
    }
    if val & 0x08 != 0 {
        p.read_isr = val & 0x03 == 0x03;
        return;
    }
    // OCW2 EOI
    if val & 0x20 != 0 {
        if p.isr != 0 {
            let bit = 1u8 << (7 - p.isr.leading_zeros() as u8);
            p.isr &= !bit;
        }
    }
}

fn pic_data(p: &mut Pic, val: u8) {
    match p.init {
        1 => {
            p.vector = val;
            p.init = 2;
        }
        2 => {
            p.init = if p.expect_icw4 { 3 } else { 0 };
            if p.init == 0 {
                p.ready = p.vector >= 16;
            }
        }
        3 => {
            p.init = 0;
            p.ready = p.vector >= 16;
        }
        _ => p.imr = val,
    }
}

pub fn ioapic_read(off: u16) -> u32 {
    with_irq(|c| ioapic_read_locked(&c.ioapic, off))
}

pub fn ioapic_write(off: u16, val: u32) {
    with_irq(|c| ioapic_write_locked(&mut c.ioapic, off, val));
}

fn ioapic_read_locked(io: &Ioapic, off: u16) -> u32 {
    match off & 0xff {
        0 => io.sel,
        0x10 => ioapic_win_read(io),
        _ => 0,
    }
}

fn ioapic_write_locked(io: &mut Ioapic, off: u16, val: u32) {
    match off & 0xff {
        0 => io.sel = val,
        0x10 => ioapic_win_write(io, val),
        _ => {}
    }
}

fn ioapic_win_read(io: &Ioapic) -> u32 {
    match io.sel & 0xff {
        0 => io.id,
        1 => IOAPIC_VERSION,
        2 => 0,
        n if (0x10..=0x3F).contains(&n) => {
            let pin = ((n - 0x10) / 2) as usize;
            if pin >= IOAPIC_PINS {
                return 0;
            }
            let rte = io.redir[pin];
            if n & 1 == 0 {
                rte as u32
            } else {
                (rte >> 32) as u32
            }
        }
        _ => 0,
    }
}

fn ioapic_win_write(io: &mut Ioapic, val: u32) {
    match io.sel & 0xff {
        0 => io.id = val & 0x0F00_0000,
        n if (0x10..=0x3F).contains(&n) => {
            let pin = ((n - 0x10) / 2) as usize;
            if pin >= IOAPIC_PINS {
                return;
            }
            let mut rte = io.redir[pin];
            if n & 1 == 0 {
                rte = (rte & !0xFFFF_FFFF) | u64::from(val);
            } else {
                rte = (rte & 0xFFFF_FFFF) | (u64::from(val) << 32);
            }
            io.redir[pin] = rte;
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "guest_irq_test.rs"]
mod guest_irq_test;
