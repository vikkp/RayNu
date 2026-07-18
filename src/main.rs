//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0: COM1 banner + boot marker.
//! M1.0: ExitBootServices, own conventional memory pool, COM1 still alive.
//! VMXON / VMLAUNCH land in M1.1+.

#![no_main]
#![no_std]

extern crate alloc;

use r640_hypervisor::{arch, audit, boot, BOOT_BANNER};
use uefi::prelude::*;
use uefi::println;

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("uefi helpers init");

    // COM1 first: reliable on QEMU `-serial stdio` and iDRAC virtual console [D].
    boot::early_init();
    boot::serial::print_m0_banner(BOOT_BANNER);

    // UEFI ConOut (may be invisible under -display none). Must happen before EBS.
    println!("{BOOT_BANNER}");
    println!("{}", boot::serial::M0_BOOT_OK_MARKER);

    arch::log_cpu_vendor_stub();

    audit::integrity::record_event(audit::AuditEvent::BootStarted {
        milestone: audit::Milestone::M0,
    });

    boot::serial::write_line("boot: M0 complete — entering M1.0 firmware handoff");

    // SAFETY: no live protocol refs beyond helpers (disabled inside exit path).
    let handoff = unsafe { boot::handoff::leave_firmware() };

    boot::serial::write_str("boot: handoff pool remaining_pages=");
    write_dec(handoff.frames.remaining_pages());
    boot::serial::write_byte(b'\n');
    boot::serial::write_line("boot: M1.0 complete; awaiting M1.1 VMXON");

    // Clean exit under QEMU CI; no-op on real hardware.
    boot::serial::qemu_exit_success();

    // On bare metal we would halt here; Status is unreachable after EBS in practice.
    loop {
        core::hint::spin_loop();
    }
}

fn write_dec(mut n: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    if n == 0 {
        boot::serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        boot::serial::write_byte(b);
    }
}
