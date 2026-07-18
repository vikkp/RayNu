//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0: COM1 banner + boot marker.
//! M1.0: ExitBootServices + frame pool.
//! M1.1: VMXON / VMXOFF (requires VT-x; QEMU needs KVM nested).

#![no_main]
#![no_std]

extern crate alloc;

use r640_hypervisor::{arch, audit, boot, vmx, BOOT_BANNER};
use uefi::prelude::*;
use uefi::println;

#[entry]
fn main() -> Status {
    uefi::helpers::init().expect("uefi helpers init");

    boot::early_init();
    boot::serial::print_m0_banner(BOOT_BANNER);

    println!("{BOOT_BANNER}");
    println!("{}", boot::serial::M0_BOOT_OK_MARKER);

    arch::log_cpu_vendor_stub();

    audit::integrity::record_event(audit::AuditEvent::BootStarted {
        milestone: audit::Milestone::M0,
    });

    boot::serial::write_line("boot: M0 complete — entering M1.0 firmware handoff");

    // SAFETY: no live protocol refs beyond helpers (disabled inside exit path).
    let handoff = unsafe { boot::handoff::leave_firmware() };
    let mut frames = handoff.frames;

    boot::serial::write_str("boot: handoff pool remaining_pages=");
    write_dec(frames.remaining_pages());
    boot::serial::write_byte(b'\n');
    boot::serial::write_line("boot: M1.0 complete — entering M1.1 VMXON");

    run_m1_1_vmxon(&mut frames);

    boot::serial::write_line("boot: M1.1 complete; awaiting M1.2 VMLAUNCH");
    boot::serial::qemu_exit_success();

    loop {
        core::hint::spin_loop();
    }
}

fn run_m1_1_vmxon(frames: &mut boot::mem::FrameBump) {
    if !arch::cpu::vmx_supported() {
        boot::serial::write_line("boot: CPUID.VMX clear — need KVM nested / VT-x");
        boot::serial::write_line(vmx::M1_VMXON_SKIP_MARKER);
        return;
    }

    let Some(region) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for VMXON region");
        return;
    };

    boot::serial::write_str("boot: VMXON region phys=0x");
    write_hex(region.0);
    boot::serial::write_byte(b'\n');

    let mut life = vmx::VmxLifecycle::new();
    match life.enable(region.0) {
        Ok(()) => {
            boot::serial::write_line(vmx::M1_VMXON_OK_MARKER);
            audit::integrity::record_event(audit::AuditEvent::VmxEnabled { vcpu_id: 0 });
            match life.disable() {
                Ok(()) => boot::serial::write_line("boot: VMXOFF ok"),
                Err(_) => boot::serial::write_line("boot: ERROR — VMXOFF failed"),
            }
        }
        Err(e) => {
            boot::serial::write_str("boot: ERROR — VMXON failed: ");
            boot::serial::write_line(vmx_err_name(e));
        }
    }
}

fn vmx_err_name(e: vmx::VmxError) -> &'static str {
    match e {
        vmx::VmxError::InvalidState => "InvalidState",
        vmx::VmxError::NotSupported => "NotSupported",
        vmx::VmxError::FeatureControl => "FeatureControl",
        vmx::VmxError::VmxonFailed => "VmxonFailed",
        vmx::VmxError::VmxoffFailed => "VmxoffFailed",
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

fn write_hex(mut n: u64) {
    let mut buf = [0u8; 16];
    let mut i = buf.len();
    if n == 0 {
        boot::serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        let d = (n & 0xf) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + (d - 10) };
        n >>= 4;
    }
    for &b in &buf[i..] {
        boot::serial::write_byte(b);
    }
}
