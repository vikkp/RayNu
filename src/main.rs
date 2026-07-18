//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0: COM1 banner + boot marker.
//! M1.0: ExitBootServices + frame pool.
//! M1.1: VMXON (requires VT-x; QEMU needs KVM nested).
//! M1.2: VMLAUNCH → one HLT VMEXIT.
//! M2.0: same path under EPT identity map (`RAYNU-V-M2-EPT-OK`).
//! M2.1: guest store + loop then HLT (`RAYNU-V-M2-GUEST-OK`).
//! M2.2: ADR-004 ownership self-test (`RAYNU-V-M2-OWN-OK`).

#![no_main]
#![no_std]

extern crate alloc;

use r640_hypervisor::{arch, audit, boot, memory, vmx, BOOT_BANNER};
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
    boot::serial::write_line("boot: M1.0 complete — entering M1.1/M2.0 VMX+EPT");

    run_m1_vmx(&mut frames);

    boot::serial::write_line("boot: M1 path finished without VMEXIT marker");
    boot::serial::qemu_exit_success();

    loop {
        core::hint::spin_loop();
    }
}

fn run_m1_vmx(frames: &mut boot::mem::FrameBump) {
    if !arch::cpu::vmx_supported() {
        boot::serial::write_line("boot: CPUID.VMX clear — need KVM nested / VT-x");
        boot::serial::write_line(vmx::M1_VMXON_SKIP_MARKER);
        return;
    }

    let Some(vmxon_region) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for VMXON region");
        return;
    };

    boot::serial::write_str("boot: VMXON region phys=0x");
    write_hex(vmxon_region.0);
    boot::serial::write_byte(b'\n');

    let mut life = vmx::VmxLifecycle::new();
    match life.enable(vmxon_region.0) {
        Ok(()) => {
            boot::serial::write_line(vmx::M1_VMXON_OK_MARKER);
            audit::integrity::record_event(audit::AuditEvent::VmxEnabled { vcpu_id: 0 });
            run_m2_ept_launch(frames, &mut life);
        }
        Err(e) => {
            boot::serial::write_str("boot: ERROR — VMXON failed: ");
            boot::serial::write_line(vmx_err_name(e));
        }
    }
}

fn run_m2_ept_launch(frames: &mut boot::mem::FrameBump, life: &mut vmx::VmxLifecycle) {
    boot::serial::write_line("boot: M1.1 complete — entering M2.2 ownership + guest EPT");

    // SAFETY: VMX root; capability MSR is defined when VMX is present.
    let page_size = match unsafe { memory::ept_hw::select_page_size() } {
        Ok(ps) => ps,
        Err(_) => {
            boot::serial::write_line("boot: ERROR — EPT 1G/2M identity map unsupported");
            let _ = life.disable();
            return;
        }
    };
    let ept_need = memory::ept_hw::frames_required(page_size);
    boot::serial::write_str("boot: EPT page_size=");
    boot::serial::write_line(match page_size {
        memory::EptPageSize::OneGib => "1G",
        memory::EptPageSize::TwoMib => "2M",
    });

    let mut ept_frames = [0u64; 8];
    if ept_need > ept_frames.len() {
        boot::serial::write_line("boot: ERROR — EPT frame budget too small");
        let _ = life.disable();
        return;
    }
    for slot in ept_frames.iter_mut().take(ept_need) {
        let Some(f) = frames.alloc_frame() else {
            boot::serial::write_line("boot: ERROR — no frame for EPT tables");
            let _ = life.disable();
            return;
        };
        *slot = f.0;
    }

    // SAFETY: frames exclusively owned by this path.
    let eptp = match unsafe {
        memory::ept_hw::build_identity_4g(page_size, &mut ept_frames[..ept_need])
    } {
        Ok(v) => v,
        Err(_) => {
            boot::serial::write_line("boot: ERROR — EPT identity build failed");
            let _ = life.disable();
            return;
        }
    };

    let Some(guest_code) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for guest code");
        let _ = life.disable();
        return;
    };
    let Some(guest_stack) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for guest stack");
        let _ = life.disable();
        return;
    };

    // ADR-004: claim guest pages before launch; reject HPA aliasing.
    match memory::run_ownership_selftest(guest_code.0, guest_stack.0) {
        Ok(()) => {
            audit::integrity::record_event(audit::AuditEvent::EptMapped {
                guest_id: memory::M2_BRINGUP_GUEST_ID,
                gpa: guest_code.0,
                hpa: guest_code.0,
            });
            audit::integrity::record_event(audit::AuditEvent::EptMapped {
                guest_id: memory::M2_BRINGUP_GUEST_ID,
                gpa: guest_stack.0,
                hpa: guest_stack.0,
            });
            boot::serial::write_line("boot: ADR-004 ownership selftest ok");
        }
        Err(_) => {
            boot::serial::write_line("boot: ERROR — ADR-004 ownership selftest failed");
            let _ = life.disable();
            return;
        }
    }

    // SAFETY: owned frame, identity-mapped by UEFI; clear NX so guest can fetch.
    unsafe {
        memory::ept_hw::write_guest_store_page(guest_code.0);
        if !arch::cpu::clear_nx_identity(guest_code.0) {
            boot::serial::write_line("boot: ERROR — could not clear NX on guest code page");
            let _ = life.disable();
            return;
        }
    };

    let Some(vmcs) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for VMCS");
        let _ = life.disable();
        return;
    };
    let Some(host_stack) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for host stack");
        let _ = life.disable();
        return;
    };
    let Some(tss) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for TSS");
        let _ = life.disable();
        return;
    };
    let Some(gdt) = frames.alloc_frame() else {
        boot::serial::write_line("boot: ERROR — no frame for GDT");
        let _ = life.disable();
        return;
    };
    let msr_bitmap = frames.alloc_frame();
    let io_a = frames.alloc_frame();
    let io_b = frames.alloc_frame();

    let launch_frames = vmx::LaunchFrames {
        vmcs_phys: vmcs.0,
        guest_stack_phys: guest_stack.0,
        host_stack_phys: host_stack.0,
        tss_phys: tss.0,
        gdt_phys: gdt.0,
        eptp,
        guest_code_phys: guest_code.0,
        msr_bitmap_phys: msr_bitmap.map(|p| p.0),
        io_bitmap_a_phys: io_a.map(|p| p.0),
        io_bitmap_b_phys: io_b.map(|p| p.0),
    };

    boot::serial::write_str("boot: VMCS phys=0x");
    write_hex(vmcs.0);
    boot::serial::write_str(" EPTP=0x");
    write_hex(eptp);
    boot::serial::write_byte(b'\n');

    // SAFETY: VMX root; frames exclusively owned by this bring-up path.
    match unsafe { vmx::launch::run_hlt_guest(&launch_frames) } {
        Ok(()) => {
            boot::serial::write_line("boot: ERROR — run_hlt_guest returned Ok");
        }
        Err(e) => {
            boot::serial::write_str("boot: ERROR — M2.2 launch failed: ");
            boot::serial::write_line(launch_err_name(e));
        }
    }

    match life.disable() {
        Ok(()) => boot::serial::write_line("boot: VMXOFF ok"),
        Err(_) => boot::serial::write_line("boot: ERROR — VMXOFF failed"),
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

fn launch_err_name(e: vmx::LaunchError) -> &'static str {
    match e {
        vmx::LaunchError::PrepareFailed => "PrepareFailed",
        vmx::LaunchError::ClearFailed => "ClearFailed",
        vmx::LaunchError::PtrldFailed => "PtrldFailed",
        vmx::LaunchError::EptUnsupported => "EptUnsupported",
        vmx::LaunchError::VmwriteFailed { .. } => "VmwriteFailed",
        vmx::LaunchError::LaunchFailed { .. } => "LaunchFailed",
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
