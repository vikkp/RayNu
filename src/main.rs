//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0 gate path: firmware handoff → serial banner → halt-friendly return.
//! VMX/EPT are not enabled in this scaffold.

#![no_main]
#![no_std]

extern crate alloc;

use r640_hypervisor::{arch, audit, boot, BOOT_BANNER};
use uefi::prelude::*;
use uefi::println;

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("uefi helpers init");

    println!("{BOOT_BANNER}");
    println!("pillars: [V] verified · [Z] single-binary · [D] iDRAC · [A] audit");

    boot::early_init();
    arch::log_cpu_vendor_stub();

    audit::integrity::record_event(audit::AuditEvent::BootStarted {
        milestone: audit::Milestone::M0,
    });

    println!("boot: early_init complete; awaiting M1 VMX bring-up");
    Status::SUCCESS
}
