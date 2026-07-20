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
//! M3.10: real `/init` on initrd → `RAYNU-V-M3-SHELL-OK`.
//! M3.13/M3.20: precise EPT `[0,512MiB)` + range claims (`EPT2`/`EPT3`).
//! M3.22: PE `.askern`/`.asinit` embed prefer + ESP fallback (`ASSETS-OK`).
//! M4.0: second guest under private 2 MiB EPT slab → `RAYNU-V-M4-2VM-OK`.

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

    // M3.22: prefer PE-embedded assets (ADR-003); ESP is split-mode fallback.
    // Probe ESP before EBS so fallback remains available if PE is empty.
    boot::esp_assets::probe_bzimage();
    if boot::pe_assets::embedded_present() {
        boot::serial::write_line("boot: PE assets embedded (.askern/.asinit) — prefer PE");
        boot::serial::write_line(boot::M3_ASSETS_OK_MARKER);
    } else if boot::esp_assets::bzimage_bytes().is_some() {
        boot::serial::write_line("boot: ESP BZIMAGE staged (PE embed missing)");
        if boot::esp_assets::initrd_bytes().is_none() {
            if let Some(initrd) = boot::pe_assets::initrd_bytes() {
                let _ = boot::esp_assets::stage_initrd(initrd);
            }
        }
    } else {
        boot::serial::write_line("boot: no PE/ESP bzImage — will use embedded minimal");
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

    // M3.20: tight precise path always uses 2M leaves (sub-GiB window).
    // SAFETY: VMX root; capability MSR is defined when VMX is present.
    if unsafe { memory::ept_hw::ensure_2m_capable() }.is_err() {
        boot::serial::write_line("boot: ERROR — EPT 2M identity map unsupported");
        let _ = life.disable();
        return;
    }
    let ept_need = memory::ept_hw::frames_required_precise();
    boot::serial::write_line("boot: EPT page_size=2M");

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

    // M3.20: precise identity [0, 512 MiB) — APIC at 0xFEE00000 stays unmapped.
    // SAFETY: frames exclusively owned by this path.
    let eptp = match unsafe { memory::ept_hw::build_precise_identity(&mut ept_frames[..ept_need]) }
    {
        Ok(v) => v,
        Err(_) => {
            boot::serial::write_line("boot: ERROR — EPT precise identity build failed");
            let _ = life.disable();
            return;
        }
    };

    // SAFETY: PML4 from precise build; window edges + APIC must check out.
    let pml4 = ept_frames[0];
    let apic_gpa = arch::apic::DEFAULT_APIC_PHYS;
    let last_in = memory::PRECISE_BYTES - 0x1000;
    if unsafe {
        !memory::ept_hw::gpa_is_mapped(pml4, 0)
            || !memory::ept_hw::gpa_is_mapped(pml4, last_in)
            || memory::ept_hw::gpa_is_mapped(pml4, memory::PRECISE_BYTES)
            || memory::ept_hw::gpa_is_mapped(pml4, apic_gpa)
    } {
        boot::serial::write_line("boot: ERROR — precise EPT window / APIC check failed");
        let _ = life.disable();
        return;
    }
    boot::serial::write_line("boot: precise EPT [0,512MiB); APIC MMIO unmapped");

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

    // M3.22 / M3.7: prefer PE embed → ESP stage → runtime minimal → synthetic.
    let mut kernel = boot::pe_assets::bzimage_bytes().or_else(boot::esp_assets::bzimage_bytes);
    let initrd = boot::pe_assets::initrd_bytes().or_else(boot::esp_assets::initrd_bytes);
    if kernel.is_none() {
        let mut minimal = [0u8; guest::MINIMAL_BZIMAGE_CAP];
        let n = guest::build_minimal_bzimage(&mut minimal);
        if boot::esp_assets::stage_bzimage(&minimal[..n]).is_err() {
            boot::serial::write_line("boot: ERROR — could not stage minimal bzImage");
            let _ = life.disable();
            return;
        }
        kernel = boot::esp_assets::bzimage_bytes();
    }
    let load = if let Some(img) = kernel {
        match guest::load_bzimage_guest(alloc, img, initrd) {
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
                // Prove zeropage e820 survived packing (Latitude: must not be BIOS-e801).
                // SAFETY: boot_params frame just written by load_bzimage_guest.
                let entries = unsafe {
                    core::ptr::read_volatile(
                        (info.boot_params_phys as *const u8).add(guest::linux_boot::OFF_E820_ENTRIES),
                    )
                };
                boot::serial::write_str("boot: e820_entries=");
                write_hex(entries as u64);
                boot::serial::write_byte(b'\n');
            }
            if info.has_real_initrd {
                boot::serial::write_str("boot: real initrd bytes=0x");
                write_hex(info.ramdisk_size as u64);
                boot::serial::write_byte(b'\n');
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
            // Proto-init is executable; real initrd is data only (loaded by kernel).
            if !info.has_real_initrd
                && !unsafe { arch::cpu::clear_nx_identity(info.init_phys) }
            {
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

    // M4.0: private 2 MiB G1 slab in [GUEST_RAM, PRECISE) — above G0 e820 so
    // Linux never touches it. Prefer host RAM outside the FrameAllocator pool
    // (Latitude pool sits in low conventional memory; QEMU -m 512M still has
    // [256MiB,512MiB) identity-mapped and free).
    let g1_base = match pick_g1_slab_hpa(alloc) {
        Some(h) => h,
        None => {
            boot::serial::write_line("boot: ERROR — no G1 slab above G0 guest RAM");
            let _ = life.disable();
            return;
        }
    };
    // SAFETY: precise PML4; punch G1 slab out of G0 identity.
    if unsafe { memory::ept_hw::clear_2m_identity_leaf(pml4, g1_base) }.is_err() {
        boot::serial::write_line("boot: ERROR — could not unmap G1 slab from G0 EPT");
        let _ = life.disable();
        return;
    }
    if memory::claim_precise_with_guest1_hole(g1_base, memory::ept_hw::TWO_MIB).is_err() {
        boot::serial::write_line("boot: ERROR — dual-guest range claim failed");
        let _ = life.disable();
        return;
    }
    boot::serial::write_line(memory::M3_EPT2_OK_MARKER);
    boot::serial::write_line(memory::M3_EPT3_OK_MARKER);
    boot::serial::write_str("boot: M4.0 G1 slab HPA=0x");
    write_hex(g1_base);
    boot::serial::write_byte(b'\n');

    // Zero the slab via host identity map before installing guest code.
    // SAFETY: HPA is in QEMU RAM, outside the allocator, identity-mapped by UEFI.
    unsafe {
        core::ptr::write_bytes(g1_base as *mut u8, 0, memory::ept_hw::TWO_MIB as usize);
    }

    // M4.0: G1 gets its own precise identity EPT (full [0,512MiB)) and shares
    // host CR3 — same shape as G0 bring-up. G0 already had the G1 slab leaf
    // cleared, so G0 cannot touch G1 HPA. Software ownership still exclusive.
    // (Tight private-only EPT + slab CR3 triple-faulted on Latitude; tighten later.)
    let g1_ept_need = memory::ept_hw::frames_required_precise();
    let mut g1_ept_frames = [0u64; 8];
    if g1_ept_need > g1_ept_frames.len() {
        boot::serial::write_line("boot: ERROR — G1 EPT frame budget too small");
        let _ = life.disable();
        return;
    }
    for slot in g1_ept_frames.iter_mut().take(g1_ept_need) {
        let Some(f) = alloc_phys(alloc) else {
            boot::serial::write_line("boot: ERROR — no frame for G1 EPT");
            let _ = life.disable();
            return;
        };
        *slot = f;
    }
    // SAFETY: exclusive EPT frames for G1.
    let g1_eptp = match unsafe {
        memory::ept_hw::build_precise_identity(&mut g1_ept_frames[..g1_ept_need])
    } {
        Ok(v) => v,
        Err(_) => {
            boot::serial::write_line("boot: ERROR — G1 EPT build failed");
            let _ = life.disable();
            return;
        }
    };
    let g1_code = g1_base + memory::ept_hw::G1_SLAB_OFF_CODE;
    let g1_stack = g1_base + memory::ept_hw::G1_SLAB_OFF_STACK;
    let g1_idt = g1_base + memory::ept_hw::G1_SLAB_OFF_IDT;
    // SAFETY: pages inside G1 slab; host identity still maps them for setup.
    unsafe {
        memory::ept_hw::write_guest_shell_cpuid_page(g1_code);
        if !arch::cpu::clear_nx_identity(g1_code) {
            boot::serial::write_line("boot: ERROR — could not clear NX on G1 code");
            let _ = life.disable();
            return;
        }
    }
    audit::integrity::record_event(audit::AuditEvent::EptMapped {
        guest_id: memory::M4_GUEST1_ID,
        gpa: g1_code,
        hpa: g1_code,
    });

    let Some(g1_vmcs) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for G1 VMCS");
        let _ = life.disable();
        return;
    };
    let Some(g1_host_stack) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for G1 host stack");
        let _ = life.disable();
        return;
    };
    let Some(g1_tss) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for G1 TSS");
        let _ = life.disable();
        return;
    };
    let Some(g1_gdt) = alloc_phys(alloc) else {
        boot::serial::write_line("boot: ERROR — no frame for G1 GDT");
        let _ = life.disable();
        return;
    };
    let g1_msr = alloc_phys(alloc);
    let g1_io_a = alloc_phys(alloc);
    let g1_io_b = alloc_phys(alloc);
    vmx::launch::set_second_guest(vmx::LaunchFrames {
        vmcs_phys: g1_vmcs,
        guest_stack_phys: g1_stack,
        host_stack_phys: g1_host_stack,
        tss_phys: g1_tss,
        gdt_phys: g1_gdt,
        eptp: g1_eptp,
        guest_code_phys: g1_code,
        guest_idt_phys: g1_idt,
        guest_cr3_phys: None, // share host CR3; G1 EPT is full precise identity
        msr_bitmap_phys: g1_msr,
        io_bitmap_a_phys: g1_io_a,
        io_bitmap_b_phys: g1_io_b,
    });
    boot::serial::write_line(
        "boot: M4.0 G1 prepared (precise EPT + host CR3 + SHELL CPUID in slab)",
    );

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
        guest_cr3_phys: None,
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

/// Pick a 2 MiB-aligned HPA for G1 in `[GUEST_RAM, PRECISE)` outside the HV pool.
fn pick_g1_slab_hpa(alloc: &memory::FrameAllocator) -> Option<u64> {
    let guest_ram = guest::linux_boot::GUEST_RAM_BYTES;
    let two_m = memory::ept_hw::TWO_MIB;
    let mut hpa = (guest_ram + two_m - 1) & !(two_m - 1);
    while hpa.saturating_add(two_m) <= memory::PRECISE_BYTES {
        if !alloc.owns_phys_range(hpa, two_m) {
            return Some(hpa);
        }
        hpa = hpa.saturating_add(two_m);
    }
    None
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
