//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0 gate path: firmware handoff → COM1 serial banner → QEMU exit / halt.
//! VMX/EPT are not enabled yet (M1).

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

    // Also attempt UEFI ConOut (may be invisible under -display none).
    println!("{BOOT_BANNER}");
    println!("{}", boot::serial::M0_BOOT_OK_MARKER);

    arch::log_cpu_vendor_stub();

    audit::integrity::record_event(audit::AuditEvent::BootStarted {
        milestone: audit::Milestone::M0,
    });

    boot::serial::write_line("boot: early_init complete; awaiting M1 VMX bring-up");

    // Clean exit under QEMU CI; no-op on real hardware.
    boot::serial::qemu_exit_success();

    Status::SUCCESS
}
