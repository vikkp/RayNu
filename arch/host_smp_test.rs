use super::*;

fn cpu(id: u32, package: u32, core: u32, thread: u32, bsp: bool) -> CpuInfo {
    CpuInfo {
        apic_id: id,
        package,
        core,
        thread,
        enabled: true,
        healthy: true,
        bsp,
    }
}

#[test]
fn max_cores_matches_units() {
    assert_eq!(MAX_CORES, crate::guest::slot::MAX_UNITS);
}

#[test]
fn sixteen_threads_on_eight_cores() {
    let mut cpus = Vec::new();
    for core in 0..8u32 {
        cpus.push(cpu(core * 2, 0, core, 0, core == 0));
        cpus.push(cpu(core * 2 + 1, 0, core, 1, false));
    }
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 8);
    assert_eq!(out[0], 0);
    for i in 1..8 {
        assert_eq!(out[i], (i as u32) * 2);
    }
}

#[test]
fn bsp_not_thread_zero_is_first() {
    let cpus = [
        cpu(1, 0, 0, 0, false),
        cpu(8, 0, 0, 1, true),
        cpu(2, 0, 1, 0, false),
    ];
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 2);
    assert_eq!(out[0], 8);
    assert_eq!(out[1], 2);
}

#[test]
fn disabled_cpu_is_skipped() {
    let mut off = cpu(4, 0, 1, 0, false);
    off.enabled = false;
    let cpus = [cpu(0, 0, 0, 0, true), off];
    let mut out = [0u32; MAX_CORES];
    assert_eq!(select_cores(&cpus, true, &mut out), 1);
    assert_eq!(out[0], 0);
}

#[test]
fn forty_cores_cap_at_sixteen() {
    let mut cpus = Vec::new();
    for core in 0..40u32 {
        cpus.push(cpu(core, 0, core, 0, core == 0));
    }
    let mut out = [0u32; MAX_CORES];
    let n = select_cores(&cpus, true, &mut out);
    assert_eq!(n, 16);
    assert_eq!(out[0], 0);
    assert_eq!(out[15], 15);
}

#[test]
fn xapic_id_300_is_skipped() {
    let cpus = [cpu(0, 0, 0, 0, true), cpu(300, 0, 1, 0, false)];
    let mut out = [0u32; MAX_CORES];
    assert_eq!(select_cores(&cpus, false, &mut out), 1);
    assert_eq!(out[0], 0);
    assert_eq!(select_cores(&cpus, true, &mut out), 2);
}

#[test]
fn core_of_rsp_at_block_edges() {
    let base = 0x10_0000_0000u64;
    let n = 3usize;
    assert_eq!(core_of_rsp(base - 1, base, n), 0);
    assert_eq!(core_of_rsp(base, base, n), 1);
    assert_eq!(core_of_rsp(base + BLOCK_BYTES - 1, base, n), 1);
    assert_eq!(core_of_rsp(base + BLOCK_BYTES, base, n), 2);
    assert_eq!(core_of_rsp(base + 3 * BLOCK_BYTES - 1, base, n), 3);
    assert_eq!(core_of_rsp(base + 3 * BLOCK_BYTES, base, n), 0);
    assert_eq!(core_of_rsp(0x1000, 0, n), 0);
}

#[test]
fn stack_top_is_aligned_inside_the_block() {
    let base = 0x20_0000_0000u64;
    let top = stack_top(1, base, 0x8000);
    assert_eq!(top & 0xF, 0);
    assert!(top >= base && top < base + BLOCK_BYTES);
    assert_eq!(core_of_rsp(top, base, 2), 1);
    assert_eq!(stack_top(0, base, 0x8000), 0x8000);
    let top2 = stack_top(2, base, 0x8000);
    assert_eq!(core_of_rsp(top2, base, 2), 2);
}

#[test]
fn x2apic_id_is_not_shifted() {
    assert_eq!(xapic_id(0x0200_0000), 2);
    assert_eq!(x2apic_id(0x2), 2);
    assert_eq!(x2apic_id(0x1e), 0x1e);
    assert_ne!(x2apic_id(0x2), xapic_id(0x2));
}

#[test]
fn tsc_sync_delta_is_the_paired_offset() {
    assert_eq!(tsc_sync_delta(5_000, 1_000), 4_000);
    assert_eq!(tsc_sync_delta(1_000, 1_000), 0);
    assert_eq!(1_000u64.wrapping_add(tsc_sync_delta(5_000, 1_000)), 5_000);
    assert_eq!(5_000u64.wrapping_add(tsc_sync_delta(1_000, 5_000)), 1_000);
}

#[test]
fn cpu_cap_keeps_b7_on_one_core() {
    assert_eq!(cpu_cap(4, 1, false), 4);
    assert_eq!(cpu_cap(4, 8, true), 4);
    assert_eq!(cpu_cap(4, 2, false), 2);
    assert_eq!(cpu_cap(4, 8, false), 4);
    assert_eq!(cpu_cap(4, 3, false), 3);
}

#[test]
fn boot_tables_make_the_low_2mib_executable() {
    let mut tables = std::collections::HashMap::<u64, [u64; 512]>::new();
    let mut pml4 = [0u64; 512];
    pml4[0] = 0x2000 | 1 | (1 << 63);
    let mut pdpt = [0u64; 512];
    pdpt[0] = 0x3000 | 1 | (1 << 63);
    let mut pd = [0u64; 512];
    pd[0] = 0 | 1 | (1 << 7) | (1 << 63);
    pd[1] = (1 << 21) | 1 | (1 << 7);
    tables.insert(0x1000, pml4);
    tables.insert(0x2000, pdpt);
    tables.insert(0x3000, pd);
    let got = build_boot_tables(0x1000, 0x5000, 0x6000, |pa| tables[&(pa & !0xFFF)]);
    let (pml4, pdpt, pd) = got.expect("tables");
    assert_eq!(pml4[0], 0x5000 | 1 | 2);
    assert_eq!(pdpt[0], 0x6000 | 1 | 2);
    assert_eq!(pd[0], 1 | 2 | (1 << 7));
    assert_eq!(pd[1], (1 << 21) | 1 | (1 << 7));
    assert_eq!(pml4[0] >> 63, 0);
}

#[test]
fn boot_tables_reject_a_nonzero_1gib_page() {
    let mut pml4 = [0u64; 512];
    pml4[0] = 0x2000 | 1;
    let mut pdpt = [0u64; 512];
    pdpt[0] = (1 << 30) | 1 | (1 << 7);
    let got = build_boot_tables(0, 0x5000, 0x6000, |pa| {
        if pa & !0xFFF == 0 {
            pml4
        } else {
            pdpt
        }
    });
    assert!(got.is_none());
}

#[test]
fn ticket_lock_counts_exactly() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    reset_monitor_for_test();
    assert_eq!(MON_NEXT.load(Ordering::Relaxed), 0);
    assert_eq!(MON_OWNER.load(Ordering::Relaxed), 0);
    for _ in 0..1000 {
        acquire();
        release();
    }
    assert_eq!(MON_NEXT.load(Ordering::Relaxed), 1000);
    assert_eq!(MON_OWNER.load(Ordering::Relaxed), 1000);
    reset_monitor_for_test();
    let counter = AtomicU64::new(0);
    thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..1000 {
                    acquire();
                    counter.fetch_add(1, Ordering::Relaxed);
                    release();
                }
            });
        }
    });
    assert_eq!(counter.load(Ordering::Relaxed), 4000);
    assert_eq!(
        MON_NEXT.load(Ordering::Relaxed),
        MON_OWNER.load(Ordering::Relaxed)
    );
}

#[test]
fn monitor_stubs_take_the_lock_around_the_registers() {
    let src = include_str!("../vmx/guest_uefi.rs");
    let land = src
        .find("fn guest_uefi_vmexit_landing")
        .expect("landing");
    let land_body = &src[land..land + 1800];
    let xadd = land_body.find("lock xadd").expect("xadd");
    let rax = land_body.find("slot_rax").expect("rax");
    assert!(xadd < rax, "lock is taken before the guest RAX is stored");
    for needle in [
        "fn guest_uefi_vmresume()",
        "fn guest_uefi_vmlaunch()",
        "fn raynu_f_vmlaunch()",
    ] {
        let at = src.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
        let body = &src[at..at + 4500];
        let inc = body.find("lock inc dword ptr [rip + {mon_owner}]").expect(needle);
        let entry = if needle.contains("vmresume") {
            body.find("\"vmresume\"").expect("vmresume")
        } else {
            body.find("\"vmlaunch\"").expect("vmlaunch")
        };
        assert!(inc < entry, "{needle} releases after the registers are loaded");
    }
    let exit = src.find("fn guest_uefi_vmexit()").expect("exit");
    let exit_head = &src[exit..exit + 240];
    assert!(
        exit_head.contains("monitor_enter()"),
        "vmexit starts in the monitor"
    );
    let enter = src.find("fn enter_guest()").expect("enter");
    let enter_head = &src[enter..enter + 180];
    assert!(enter_head.contains("monitor_leave()"));
}

#[test]
fn pick_core_prefers_the_lightest_nonzero() {
    assert_eq!(pick_core(&[2, 2, 1], 3), 2);
    assert_eq!(pick_core(&[1, 1, 1], 3), 1);
    assert_eq!(pick_core(&[0, 2, 2], 3), 0);
    assert_eq!(pick_core(&[5, 3, 3], 3), 1);
    assert_eq!(pick_core(&[0], 1), 0);
}

#[test]
fn place_vcpu_keeps_vcpu0_on_core0() {
    let loads = [1u8, 0, 0];
    assert_eq!(place_vcpu(0, &loads, 3), 0);
    assert_eq!(place_vcpu(1, &loads, 1), 0);
    assert_eq!(place_vcpu(1, &loads, 3), 1);
    assert_eq!(place_vcpu(2, &[0, 1, 0], 3), 2);
}
