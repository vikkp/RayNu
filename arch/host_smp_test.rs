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
    let land_body = &src[land..land + 4500];
    let xadd = land_body.find("lock xadd").expect("xadd");
    let rax = land_body.find("slot_rax").expect("rax");
    assert!(xadd < rax, "lock is taken before the guest RAX is stored");
    assert!(
        !land_body.contains("call {claim}"),
        "the landing stub does not call out over live guest XMM"
    );
    let enter = include_str!("host_smp.rs");
    let ment = enter.find("fn monitor_enter()").expect("enter");
    let ment_body = &enter[ment..ment + 500];
    assert!(
        ment_body.contains("MON_HOLDER.swap(core as u8"),
        "monitor_enter stores the holder"
    );
    for needle in [
        "fn guest_uefi_vmresume()",
        "fn guest_uefi_vmlaunch()",
        "fn raynu_f_vmlaunch()",
    ] {
        let at = src.find(needle).unwrap_or_else(|| panic!("missing {needle}"));
        let body = &src[at..at + 5000];
        let inc = body.find("lock inc dword ptr [rip + {mon_owner}]").expect(needle);
        let clear = body.find("mov byte ptr [rip + {mon_holder}], 0xff").expect(needle);
        assert!(clear < inc, "{needle} clears the holder as it releases");
        let home = body.find("sub rsp, 40").expect(needle);
        let call = body.find("call {claim}").expect(needle);
        assert!(
            inc < home && home < call,
            "{needle} reserves 32 bytes of home space after the registers are dead"
        );
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
    assert_eq!(pick_core(&[2, 2, 1, 0], 4), 2);
    assert_eq!(pick_core(&[2, 2, 1], 3), 1, "core 2 is the watchdog");
    assert_eq!(pick_core(&[1, 1, 1], 3), 1);
    assert_eq!(pick_core(&[0, 2, 2], 3), 0);
    assert_eq!(pick_core(&[5, 3, 3], 3), 1);
    assert_eq!(pick_core(&[0], 1), 0);
    assert_eq!(watchdog_core(1), None);
    assert_eq!(watchdog_core(16), Some(15));
    assert_eq!(place_vcpu(1, &[0, 9, 0, 0], 4), 2);
    assert_eq!(place_vcpu(1, &[0, 0, 0], 3), 1);
}

#[test]
fn b21_pulse_line_names_the_checkpoint() {
    let mut buf = [0u8; 160];
    let n = format_b21_pulse(
        4, false, 0, 0x12, 0x13, 0x1e, 0xffff_ffff_8100_1234, 12, 4, 0, &mut buf,
    );
    let s = core::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(
        s,
        "boot: B21 pulse p=4 g=0 h=00 o=00000012 n=00000013 r=1e rip=ffffffff81001234 x=12 t=4 d=0\r\n"
    );
    let n = format_b21_pulse(6, true, NO_CORE, 0, 1, 0x0a, 0, 0, 0, 3, &mut buf);
    let s = core::str::from_utf8(&buf[..n]).unwrap();
    assert_eq!(
        s,
        "boot: B21 pulse p=6 g=1 h=ff o=00000000 n=00000001 r=0a rip=0000000000000000 x=0 t=0 d=3\r\n"
    );
}

#[test]
fn core0_checkpoints_are_in_exit_order() {
    let smp = include_str!("host_smp.rs");
    let leave = smp.find("fn leave_hardware").expect("leave");
    let leave_body = &smp[leave..leave + 1600];
    let p5 = leave_body.find("note_exit_phase(5)").expect("phase 5");
    let spin = leave_body.find("wrong host stack").expect("stack");
    let p6 = leave_body.rfind("note_exit_phase(6)").expect("phase 6");
    assert!(p5 < spin && spin < p6, "a stack mismatch stays on phase 5");

    let guest = include_str!("../vmx/guest_uefi.rs");
    let exit = guest.find("fn guest_uefi_vmexit()").expect("exit");
    let head = &guest[exit..exit + 2500];
    let observed = head.find("note_core0_exit(").expect("reason");
    let p1 = head.find("note_exit_phase(1)").expect("phase 1");
    let tick = head.find("maybe_tick_standing_spa()").expect("tick");
    let p2 = head.rfind("note_exit_phase(2)").expect("phase 2");
    assert!(observed < p1 && p1 < tick && tick < p2, "phase 1 stores reason and RIP");
    let dispatch = guest.find("the exit-reason dispatch returned").expect("dispatch");
    assert!(guest[dispatch..dispatch + 160].contains("note_exit_phase(3)"));
    let arm_call = guest[dispatch..].find("arm_preempt_on_resume();").expect("arm call");
    assert!(arm_call > 0);
    let arm = guest.find("fn arm_preempt_on_resume()").expect("arm");
    assert!(guest[arm..arm + 180].contains("note_exit_phase(4)"));

    let start = include_str!("host_smp_start.rs");
    let wait = start.find("fn watchdog_wait(").expect("watchdog");
    let body = &start[wait..wait + 600];
    assert!(body.contains("emit_core0_pulse"));
    assert!(!body.contains("acquire("));
    assert!(!body.contains("mwait"));
}

#[test]
fn place_vcpu_keeps_vcpu0_on_core0() {
    let loads = [1u8, 0, 0];
    assert_eq!(place_vcpu(0, &loads, 3), 0);
    assert_eq!(place_vcpu(1, &loads, 1), 0);
    assert_eq!(place_vcpu(1, &loads, 3), 1);
    assert_eq!(place_vcpu(2, &[0, 1, 0, 0], 4), 2);
}

#[test]
fn exits_stay_on_core0_until_a_vcpu_is_placed() {
    for u in 0..MAX_CORES {
        assign_unit(u, 0);
    }
    assert!(!remote_placed());
    assert!(!running_remote(4));
    assign_unit(1, 2);
    assert!(remote_placed());
    assert!(running_remote(2));
    assert!(!running_remote(0));
    assign_unit(1, 0);
    assert!(!remote_placed());
}

#[test]
fn on_ap_wake_calls_enter_guest_while_it_holds() {
    let g = include_str!("../vmx/guest_uefi.rs");
    let wake = g.find("pub unsafe fn on_ap_wake(").expect("wake");
    let body = &g[wake..wake + 500];
    let acq = body.find("acquire()").expect("acquire");
    let launch = body.find("launch_on_core").expect("launch");
    assert!(acq < launch, "on_ap_wake takes the ticket before launch");
    let enter = g.find("unsafe fn enter_guest()").expect("enter");
    assert!(g[enter..enter + 240].contains("monitor_leave()"));
    let smp = include_str!("host_smp.rs");
    let leave = smp.find("fn monitor_leave()").expect("leave");
    let lb = &smp[leave..leave + 800];
    assert!(lb.contains("holder_is(core)"));
    assert!(!lb.contains("swap(NO_CORE"), "leave does not clear the holder");
}

#[test]
fn on_ap_wake_enter_guest_does_not_two_in_monitor() {
    // Core 0 holds ticket 0 from boot. Its entry stub releases. on_ap_wake
    // then acquire()s, and enter_guest's monitor_leave requires the holder
    // to be that core. The old acquire left the holder at NO_CORE.
    reset_monitor_for_test();
    MON_NEXT.store(1, Ordering::Relaxed);
    MON_OWNER.store(0, Ordering::Relaxed);
    MON_HOLDER.store(0, Ordering::Relaxed);
    set_test_core(0);
    release();
    assert!(!holder_is(0), "the entry stub clears the holder");
    assert_eq!(MON_HOLDER.load(Ordering::Relaxed), NO_CORE);
    set_test_core(1);
    acquire();
    assert!(
        holder_is(1),
        "enter_guest would two_in_monitor"
    );
    release();
    assert_eq!(MON_HOLDER.load(Ordering::Relaxed), NO_CORE);
    assert_eq!(MON_OWNER.load(Ordering::Relaxed), 2);
}

#[test]
fn stall_pulse_is_off_during_firmware_and_rearms() {
    assert!(!stall_report_due(true, true, 1, 1, 10, false));
    assert!(stall_report_due(true, false, 1, 2, 0, false));
    assert!(!stall_report_due(false, false, 1, 2, 0, false));
    assert!(stall_report_due(false, false, 4, 4, 1, false));
    assert!(stall_rearm(4, 5));
    assert!(!stall_rearm(4, 4));
    assert!(!stall_rearm(u64::MAX, 1));
}

#[test]
fn tx_dump_waits_for_a_second_unchanged_pulse() {
    assert!(!tx_dump_due(u64::MAX, 1, 10, false));
    assert!(!tx_dump_due(1, 2, 10, false));
    assert!(!tx_dump_due(0, 0, 0, false));
    assert!(tx_dump_due(0x01ec_bb54, 0x01ec_bb54, 3450, false));
    assert!(!tx_dump_due(0x01ec_bb54, 0x01ec_bb54, 3450, true));
}

#[test]
fn recall_stays_open_until_the_core_finishes() {
    finish_recall(15);
    assert!(recall_finished(15));
    assert!(!recall_waiting(15));
    request_recall(15);
    assert!(recall_waiting(15));
    assert!(!recall_finished(15));
    finish_recall(15);
    assert!(recall_finished(15));
    assert!(!recall_waiting(15));
    request_recall(0);
    assert!(!recall_waiting(0));
}

#[test]
fn placement_transitions_keep_i1_through_i3() {
    // assigned[unit] = core. loaded[core] = that core holds a VMCS.
    // Steps: assign, load, clear. I1–I3 hold after every step.
    #[derive(Clone, Copy)]
    enum Step {
        Assign(u8, u8),
        Load(u8),
        Clear(u8),
    }
    let steps = [
        Step::Assign(1, 1),
        Step::Load(1),
        Step::Clear(1),
        Step::Assign(1, 0),
        Step::Load(0),
        Step::Assign(2, 2),
        Step::Load(2),
        Step::Clear(2),
        Step::Assign(2, 1),
        Step::Load(1),
    ];
    let mut assigned = [0u8; 4];
    let mut loaded = [false; 4];
    let mut home = [0u8; 4];
    for step in steps {
        match step {
            Step::Assign(u, c) => {
                assigned[u as usize] = c;
            }
            Step::Load(c) => {
                let u = assigned.iter().position(|a| *a == c).unwrap() as u8;
                assert_eq!(assigned[u as usize], c, "I1");
                loaded[c as usize] = true;
                home[u as usize] = c;
            }
            Step::Clear(c) => {
                loaded[c as usize] = false;
                if let Some(u) = home.iter().position(|h| *h == c) {
                    home[u] = 0xFF;
                    assigned[u] = 0;
                }
            }
        }
        for (u, &c) in assigned.iter().enumerate() {
            if loaded[c as usize] && home[u] != 0xFF {
                assert!(home[u] == c || home[u] == 0xFF, "I2/I3 unit {u}");
            }
        }
        assert!(!loaded[0] || home.iter().any(|h| *h == 0) || assigned.iter().any(|a| *a == 0));
    }
    let src = include_str!("../vmx/guest_uefi.rs");
    let sched = include_str!("../vmx/guest_sched.rs");
    let coexist = include_str!("../mgmt/host_nic_coexist.rs");
    for name in [
        "smp_collapse",
        "ap_start",
        "ap_init",
        "launch_uefi",
        "guest_start_waiting",
        "raynu_f_launch_on_stopped_vmcs",
        "launch_slot_guest",
    ] {
        assert!(src.contains(&format!("assert_core0(\"{name}\")")), "{name}");
    }
    for name in ["prepare_start", "launch", "on_guest_left"] {
        assert!(
            sched.contains(&format!("assert_core0(\"{name}\")")),
            "{name}"
        );
    }
    assert!(coexist.contains("assert_core0(\"maybe_tick_standing_spa\")"));
    assert!(src.contains("yield_monitor_until("));
}

#[test]
fn ept_edits_flush_on_every_core() {
    let g = include_str!("../vmx/guest_uefi.rs");
    assert_eq!(
        g.matches("host_smp::invept_everywhere()").count(),
        8,
        "plan WP7: eight guest-UEFI flush sites"
    );
    assert!(!g.contains("invept_guests()"));
    assert!(!invept_owed_by_a_guest());
}

#[test]
fn a_kick_is_counted_only_for_another_core_in_a_guest() {
    use core::sync::atomic::Ordering;
    set_test_core(0);
    set_cores_online(2);
    IN_GUEST[1].store(false, Ordering::Release);
    let base = kicks_sent();
    poke_core(1);
    assert_eq!(kicks_sent(), base, "a core that is not in a guest is not kicked");
    IN_GUEST[1].store(true, Ordering::Release);
    poke_core(1);
    assert_eq!(kicks_sent(), base + 1);
    poke_core(0);
    assert_eq!(kicks_sent(), base + 1, "a core does not kick itself");
    IN_GUEST[1].store(false, Ordering::Release);
    set_cores_online(1);
}
