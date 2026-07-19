//! UEFI entry for `r640-hypervisor.efi` [Z].
//!
//! M0: COM1 banner + boot marker.
//! M1.0: ExitBootServices + frame pool.
//! M1.1: VMXON (requires VT-x; QEMU needs KVM nested).
//! M1.2: VMLAUNCH → one HLT VMEXIT.
//! M2.0: EPT identity map (`RAYNU-V-M2-EPT-OK`).
//! M2.1: guest store + loop (`RAYNU-V-M2-GUEST-OK`).
//! M2.2: ADR-004 ownership (`RAYNU-V-M2-OWN-OK`).
//! M2.3: Proven Core frame allocator (`RAYNU-V-M2-ALLOC-OK`).
//! M2.4: inject IRQ → guest ISR (`RAYNU-V-M2-IRQ-OK`).
//! M2.5: LAPIC timer → external-IRQ VMEXIT → EOI → re-inject (`RAYNU-V-M2-TIMER-OK`).
//! M3.0: guest COM1 OUT → I/O VMEXIT (`RAYNU-V-M3-IO-OK`).
//! M3.1: guest CPUID filter hide VMX (`RAYNU-V-M3-CPUID-OK`).
//! M3.2: synthetic kernel/initrd + `boot_params` load (`RAYNU-V-M3-LOAD-OK`).
//! M3.3: 64-bit proto-kernel entry + early serial (`RAYNU-V-M3-EARLY-OK`).
//! M3.4: post-proto guest timer → inject (`RAYNU-V-M3-GTIMER-OK`).
//! M3.5: proto-init shell marker (`RAYNU-V-M3-SHELL-OK`).
//! M3.6: continuous HLT exit loop (`RAYNU-V-M3-LOOP-OK`).
//! M3.7: bzImage load (`RAYNU-V-M3-BZIMAGE-OK`).
//! M3.8: real Linux earlyprintk (`RAYNU-V-M3-LINUX-EARLY-OK`).
//! M3.9: MSR firewall + post-banner LAPIC (`RAYNU-V-M3-GTIMER2-OK`).

#![no_main]
#![no_std]

extern crate alloc;

use r640_hypervisor::{arch, audit, boot, guest, memory, vmx, BOOT_BANNER};
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

    // M3.7: stage ESP bzImage before ExitBootServices tears down file I/O.
    boot::esp_assets::probe_bzimage();
    if boot::esp_assets::bzimage_bytes().is_none() {
        boot::serial::write_line("boot: ESP BZIMAGE missing — will use embedded minimal");
    } else {
        boot::serial::write_line("boot: ESP BZIMAGE staged");
    }

    // SAFETY: no live protocol refs beyond helpers (disabled inside exit path).
    let handoff = unsafe { boot::handoff::leave_firmware() };
    let mut bump = handoff.frames;

    boot::serial::write_str("boot: handoff pool remaining_pages=");
    write_dec(bump.remaining_pages());
    boot::serial::write_byte(b'\n');

    let mut alloc = match memory::boot_alloc::bootstrap_from_bump(&mut bump) {
        Ok(a) => a,
        Err(_) => {
            boot::serial::write_line("boot: ERROR — frame allocator bootstrap failed");
            boot::serial::qemu_exit_failure();
            loop {
                core::hint::spin_loop();
            }
        }
    };
    boot::serial::write_str("boot: FrameAllocator capacity_pages=");
    write_dec(alloc.capacity());
    boot::serial::write_str(" base=0x");
    write_hex(alloc.base_phys());
    boot::serial::write_byte(b'\n');

    match memory::run_allocator_selftest(&mut alloc) {
        Ok(()) => boot::serial::write_line("boot: allocator selftest ok"),
        Err(_) => {
            boot::serial::write_line("boot: ERROR — allocator selftest failed");
            boot::serial::qemu_exit_failure();
            loop {
                core::hint::spin_loop();
            }
        }
    }

    boot::serial::write_line("boot: M1.0 complete — entering M1.1/M2 VMX+EPT");
    run_m1_vmx(&mut alloc);

    boot::serial::write_line("boot: M1 path finished without VMEXIT marker");
    boot::serial::qemu_exit_success();

    loop {
        core::hint::spin_loop();
    }
}

fn alloc_phys(alloc: &mut memory::FrameAllocator) -> Option<u64> {
    alloc.allocate_frame().map(|f| {
        audit::integrity::record_event(audit::AuditEvent::FrameAllocated { frame: f.0 });
        f.to_phys()
    })
}

fn run_m1_vmx(alloc: &mut memory::FrameAllocator) {
    if !arch::cpu::vmx_supported() {
        boot::serial::write_line("boot: CPUID.VMX clear — need KVM nested / VT-x");
        boot::serial::write_line(vmx::M1_VMXON_SKIP_MARKER);
        return;
    }

    let Some(vmxon_region) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for VMXON region");
        return;
    };

    boot::serial::write_str("boot: VMXON region phys=0x");
    write_hex(vmxon_region);
    boot::serial::write_byte(b'\n');

    let mut life = vmx::VmxLifecycle::new();
    match life.enable(vmxon_region) {
        Ok(()) => {
            boot::serial::write_line(vmx::M1_VMXON_OK_MARKER);
            audit::integrity::record_event(audit::AuditEvent::VmxEnabled { vcpu_id: 0 });
            run_m2_ept_launch(alloc, &mut life);
        }
        Err(e) => {
            boot::serial::write_str("boot: ERROR — VMXON failed: ");
            boot::serial::write_line(vmx_err_name(e));
        }
    }
}

fn run_m2_ept_launch(alloc: &mut memory::FrameAllocator, life: &mut vmx::VmxLifecycle) {
    boot::serial::write_line("boot: M1.1 complete — entering M2 EPT + guest");

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
        let Some(f) = alloc_phys(alloc) else {
            boot::serial::write_line("boot: ERROR — no frame for EPT tables");
            let _ = life.disable();
            return;
        };
        *slot = f;
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

    let Some(guest_code) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for guest code");
        let _ = life.disable();
        return;
    };
    let Some(guest_stack) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for guest stack");
        let _ = life.disable();
        return;
    };
    let Some(guest_idt) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for guest IDT");
        let _ = life.disable();
        return;
    };

    // ADR-004: claim guest pages before launch; reject HPA aliasing.
    match memory::run_ownership_selftest(guest_code, guest_stack, guest_idt) {
        Ok(()) => {
            audit::integrity::record_event(audit::AuditEvent::EptMapped {
                guest_id: memory::M2_BRINGUP_GUEST_ID,
                gpa: guest_code,
                hpa: guest_code,
            });
            audit::integrity::record_event(audit::AuditEvent::EptMapped {
                guest_id: memory::M2_BRINGUP_GUEST_ID,
                gpa: guest_stack,
                hpa: guest_stack,
            });
            audit::integrity::record_event(audit::AuditEvent::EptMapped {
                guest_id: memory::M2_BRINGUP_GUEST_ID,
                gpa: guest_idt,
                hpa: guest_idt,
            });
            boot::serial::write_line("boot: ADR-004 ownership selftest ok");
        }
        Err(_) => {
            boot::serial::write_line("boot: ERROR — ADR-004 ownership selftest failed");
            let _ = life.disable();
            return;
        }
    }

    // M3.7 / M3.2: prefer bzImage (ESP or embedded minimal), else synthetic.
    if boot::esp_assets::bzimage_bytes().is_none() {
        let mut minimal = [0u8; guest::MINIMAL_BZIMAGE_CAP];
        let n = guest::build_minimal_bzimage(&mut minimal);
        if boot::esp_assets::stage_bzimage(&minimal[..n]).is_err() {
            boot::serial::write_line("boot: ERROR — could not stage minimal bzImage");
            let _ = life.disable();
            return;
        }
    }
    let load = if let Some(img) = boot::esp_assets::bzimage_bytes() {
        match guest::load_bzimage_guest(alloc, img) {
            Ok(info) => Ok(info),
            Err(()) => {
                boot::serial::write_line("boot: bzImage load failed — synthetic fallback");
                guest::load_synthetic_guest(alloc)
            }
        }
    } else {
        guest::load_synthetic_guest(alloc)
    };
    match load {
        Ok(info) => {
            boot::serial::write_str("boot: load kernel=0x");
            write_hex(info.kernel_phys);
            boot::serial::write_str(" entry=0x");
            write_hex(info.entry_phys);
            boot::serial::write_str(" initrd=0x");
            write_hex(info.initrd_phys);
            boot::serial::write_str(" boot_params=0x");
            write_hex(info.boot_params_phys);
            boot::serial::write_str(" cmdline=0x");
            write_hex(info.cmdline_phys);
            boot::serial::write_str(" magic=0x");
            write_hex(info.setup_magic as u64);
            boot::serial::write_byte(b'\n');
            if info.setup_magic != guest::SETUP_HEADER_MAGIC {
                boot::serial::write_line("boot: ERROR — setup header magic mismatch");
                let _ = life.disable();
                return;
            }
            boot::serial::write_line(guest::M3_LOAD_OK_MARKER);
            if info.from_bzimage {
                boot::serial::write_line(guest::M3_BZIMAGE_OK_MARKER);
            }
            if info.is_real_linux {
                boot::serial::write_line("boot: real Linux bzImage detected");
            }
            vmx::launch::set_linux_load(
                info.entry_phys,
                info.boot_params_phys,
                info.init_phys,
            );
            vmx::launch::set_real_linux(info.is_real_linux);
            // SAFETY: owned kernel / proto-init frames; clear NX for fetch.
            // Stride 2 MiB to cover large-page PTEs across the decompress window.
            {
                let mut a = info.kernel_phys & !0xfff;
                let end = info.kernel_phys.saturating_add(info.kernel_bytes);
                while a < end {
                    if !unsafe { arch::cpu::clear_nx_identity(a) } {
                        boot::serial::write_line("boot: ERROR — could not clear NX on kernel");
                        let _ = life.disable();
                        return;
                    }
                    a = a.saturating_add(0x20_0000);
                }
            }
            if !unsafe { arch::cpu::clear_nx_identity(info.init_phys) } {
                boot::serial::write_line("boot: ERROR — could not clear NX on proto-init");
                let _ = life.disable();
                return;
            }
        }
        Err(()) => {
            boot::serial::write_line("boot: ERROR — kernel load failed");
            let _ = life.disable();
            return;
        }
    }

    // SAFETY: owned frame, identity-mapped by UEFI; clear NX so guest can fetch.
    unsafe {
        memory::ept_hw::write_guest_store_page(guest_code);
        if !arch::cpu::clear_nx_identity(guest_code) {
            boot::serial::write_line("boot: ERROR — could not clear NX on guest code page");
            let _ = life.disable();
            return;
        }
    };

    let Some(vmcs) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for VMCS");
        let _ = life.disable();
        return;
    };
    let Some(host_stack) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for host stack");
        let _ = life.disable();
        return;
    };
    let Some(tss) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for TSS");
        let _ = life.disable();
        return;
    };
    let Some(gdt) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for GDT");
        let _ = life.disable();
        return;
    };
    let msr_bitmap = alloc_phys(alloc);
    let io_a = alloc_phys(alloc);
    let io_b = alloc_phys(alloc);

    let launch_frames = vmx::LaunchFrames {
        vmcs_phys: vmcs,
        guest_stack_phys: guest_stack,
        host_stack_phys: host_stack,
        tss_phys: tss,
        gdt_phys: gdt,
        eptp,
        guest_code_phys: guest_code,
        guest_idt_phys: guest_idt,
        msr_bitmap_phys: msr_bitmap,
        io_bitmap_a_phys: io_a,
        io_bitmap_b_phys: io_b,
    };

    boot::serial::write_str("boot: VMCS phys=0x");
    write_hex(vmcs);
    boot::serial::write_str(" EPTP=0x");
    write_hex(eptp);
    boot::serial::write_byte(b'\n');

    // SAFETY: VMX root; frames exclusively owned by this bring-up path.
    match unsafe { vmx::launch::run_hlt_guest(&launch_frames) } {
        Ok(()) => {
            boot::serial::write_line("boot: ERROR — run_hlt_guest returned Ok");
        }
        Err(e) => {
            boot::serial::write_str("boot: ERROR — M2 launch failed: ");
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
        vmx::LaunchError::CpuidExitingUnsupported => "CpuidExitingUnsupported",
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
