//! Host tests for the guest scheduler policy (ADR-022).

use super::*;
use crate::guest::slot;

const Q: u64 = 4_000_000;
const STARVE: u64 = 20_000_000;

fn off() -> Other {
    Other::default()
}

fn busy() -> Other {
    Other {
        on: true,
        halted: false,
        work: false,
        waited_tsc: 0,
    }
}

fn halted(work: bool, waited_tsc: u64) -> Other {
    Other {
        on: true,
        halted: true,
        work,
        waited_tsc,
    }
}

#[test]
fn a_failed_switch_does_not_clear_the_guest_that_stays() {
    let s = include_str!("guest_sched.rs");
    let left = s.find("fn on_guest_left()").expect("left");
    let body = &s[left..s.len()];
    let fail = body.find("report_switch_kept(next, home_v)").expect("report");
    let tail = &body[fail..fail + 400];
    assert!(tail.contains("return false"));
    assert!(!tail.contains("set_on(next, false)"));
    assert!(!s.contains("set_on(next, false)"));
    let h = include_str!("../arch/host_smp.rs");
    assert!(h.contains("failed; guest stays on (ADR-024)"));
    assert!(h.contains("com2_bytes_direct"));
}

#[test]
fn turning_a_slot_off_clears_its_units_before_the_on_bit() {
    let s = include_str!("guest_sched.rs");
    let left = s.find("fn on_guest_left()").expect("left");
    let body = &s[left..s.len()];
    let recall = body.find("recall_and_release(cur)").expect("recall");
    let off = body.find("set_on(cur, false)").expect("off");
    assert!(recall < off, "recall and clear before the on-bit drops");
    let g = include_str!("guest_uefi.rs");
    let reset = g.find("fn raynu_f_reset_relaunch(").expect("reset");
    let reset_body = &g[reset..reset + 2500];
    let q = reset_body.find("recall_slot(").expect("quiesce");
    let check = reset_body.find("slot_launch_block(").expect("check");
    assert!(q < check);
    // After a clear, the launch check is quiet.
    let assigned = [0u8; 4];
    let home = [0xFFu8; 4];
    assert_eq!(launch_block(0, &assigned, &home), None);
}

#[test]
fn a_slot_is_launched_only_when_its_units_are_clear() {
    let clear = [0xFFu8; 4];
    assert_eq!(launch_block(0, &[0, 0, 0, 0], &clear), None);
    assert_eq!(launch_block(0b01, &[0, 0, 0, 0], &clear), Some(LaunchBlock::Up(0)));
    assert_eq!(
        launch_block(0, &[0, 2, 0, 0], &clear),
        Some(LaunchBlock::Assigned(1, 2))
    );
    assert_eq!(
        launch_block(0, &[0, 0, 0, 0], &[0xFF, 0xFF, 1, 0xFF]),
        Some(LaunchBlock::Homed(2, 1))
    );
    let s = include_str!("guest_sched.rs");
    let g = include_str!("guest_uefi.rs");
    assert!(s.contains("slot_launch_block(s)"));
    assert!(g.contains("slot_launch_block(relaunch_slot)"));
    assert!(g.contains("recall_slot(relaunch_slot)"));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum NoteHold {
    Stopped(usize),
    Started(usize),
    Restarted(usize),
}

#[derive(Clone, Copy)]
struct Guest {
    on: bool,
    up: u8,
    assigned: [u8; 4],
    home: [u8; 4],
    cur: usize,
}

fn two_guests_three_cores() -> [Guest; 2] {
    [
        Guest {
            on: true,
            up: 0b11,
            assigned: [0, 1, 0, 0],
            home: [0, 1, 0xFF, 0xFF],
            cur: 1,
        },
        Guest {
            on: true,
            up: 0b11,
            assigned: [0, 2, 0, 0],
            home: [0, 2, 0xFF, 0xFF],
            cur: 1,
        },
    ]
}

/// Stop, shut down, reboot, or power off `which`. A failed switch does not
/// clear the other guest. Reboot relaunches only `which`, as GuestRestarted.
fn leave_one(gs: &mut [Guest; 2], which: usize, reboot: bool, switch_ok: bool) -> Vec<NoteHold> {
    gs[which].up = 0;
    gs[which].assigned = [0; 4];
    gs[which].home = [0xFF; 4];
    gs[which].cur = 0;
    gs[which].on = false;
    let mut notes = vec![NoteHold::Stopped(which)];
    for i in 0..2 {
        if i == which || !gs[i].on {
            continue;
        }
        if let Some(v) = home_vcpu(gs[i].up, &gs[i].assigned) {
            if switch_ok {
                gs[i].cur = v;
            }
        }
    }
    if reboot {
        assert_eq!(
            launch_block(gs[which].up, &gs[which].assigned, &gs[which].home),
            None
        );
        gs[which].on = true;
        gs[which].up = 0b01;
        gs[which].cur = 0;
        notes.push(NoteHold::Restarted(which));
    }
    notes
}

#[test]
fn stop_shutdown_reboot_and_poweroff_leave_the_other_guest() {
    // Stop, Shut down, and poweroff leave the slot off. Reboot brings
    // that slot back and records GuestRestarted, not VmStarted.
    for (name, reboot) in [("stop", false), ("shutdown", false), ("poweroff", false), ("reboot", true)] {
        let _ = name;
        for which in [0usize, 1] {
            for switch_ok in [false, true] {
                let mut gs = two_guests_three_cores();
                let other = 1 - which;
                let saved_up = gs[other].up;
                let saved_assigned = gs[other].assigned;
                let saved_home = gs[other].home;
                let notes = leave_one(&mut gs, which, reboot, switch_ok);
                assert!(gs[other].on, "guest {other} stays on");
                assert_eq!(gs[other].up, saved_up);
                assert_eq!(gs[other].assigned, saved_assigned);
                assert_eq!(gs[other].home, saved_home);
                assert!(notes.contains(&NoteHold::Stopped(which)), "{name}");
                assert!(!notes.iter().any(|n| matches!(
                    n,
                    NoteHold::Stopped(s) | NoteHold::Started(s) | NoteHold::Restarted(s) if *s == other
                )));
                if reboot {
                    assert!(notes.contains(&NoteHold::Restarted(which)));
                    assert!(gs[which].on);
                    assert_eq!(gs[which].cur, 0);
                } else {
                    assert!(!gs[which].on);
                    assert!(!notes.iter().any(|n| matches!(n, NoteHold::Started(_))));
                }
                if switch_ok {
                    assert_eq!(gs[other].cur, 0, "home_vcpu names core 0");
                } else {
                    assert_eq!(gs[other].cur, 1, "a failed switch does not rewrite it");
                }
            }
        }
    }
}

#[test]
fn a_hypervisor_restart_is_not_vm_started() {
    assert!(start_audit_is_vm_started(true));
    assert!(!start_audit_is_vm_started(false));
    let s = include_str!("guest_sched.rs");
    let launch = s.find("fn launch(start: Start)").expect("launch");
    let body = &s[launch..launch + 900];
    let restarted = body.find("GuestRestarted").expect("restarted");
    let started = body.find("VmStarted").expect("started");
    assert!(body.contains("if start.auto"));
    assert!(restarted < started, "auto path is GuestRestarted");
    let g = include_str!("guest_uefi.rs");
    assert!(g.contains("GuestRestarted"));
    assert!(g.contains("RESTART_REBOOT"));
}

#[test]
fn home_vcpu_is_the_one_on_core_0() {
    // vCPU 1 was left current by core 1. vCPU 0 is still assigned to core 0.
    assert_eq!(home_vcpu(0b11, &[0, 1, 0, 0]), Some(0));
    assert_eq!(home_vcpu(0b10, &[0, 1, 0, 0]), None);
    assert_eq!(home_vcpu(0b01, &[0, 2, 0, 0]), Some(0));
}

#[test]
fn one_guest_always_stays() {
    for idle in [false, true] {
        for used in [0, Q - 1, Q, 10 * Q] {
            assert_eq!(pick(0, idle, used, Q, STARVE, |_| off()), Pick::Stay);
        }
    }
}

#[test]
fn busy_guest_keeps_its_quantum_and_does_not_ask_the_others() {
    let mut asked = 0;
    let p = pick(0, false, Q - 1, Q, STARVE, |_| {
        asked += 1;
        busy()
    });
    assert_eq!(p, Pick::Stay);
    assert_eq!(asked, 0);
}

fn bsp(slot_n: usize) -> usize {
    slot::unit(slot_n, 0)
}

/// vCPU 0 of a slot is on; the other vCPUs of every slot are off.
fn vcpu0_only(u: usize, on: impl Fn(usize) -> bool) -> Other {
    if slot::unit_vcpu(u) == 0 && on(slot::unit_slot(u)) {
        busy()
    } else {
        off()
    }
}

#[test]
fn two_busy_guests_alternate_at_the_quantum() {
    let other = |u: usize| vcpu0_only(u, |s| s <= 1);
    assert_eq!(
        pick(bsp(0), false, Q, Q, STARVE, other),
        Pick::Switch(bsp(1))
    );
    assert_eq!(
        pick(bsp(1), false, Q, Q, STARVE, other),
        Pick::Switch(bsp(0))
    );
}

#[test]
fn idle_guest_yields_at_once_to_a_busy_one() {
    let other = |u: usize| vcpu0_only(u, |s| s == 1);
    assert_eq!(
        pick(bsp(0), true, 0, Q, STARVE, other),
        Pick::Switch(bsp(1))
    );
}

#[test]
fn halted_guest_without_work_is_left_alone_until_the_backstop() {
    let quiet = |u: usize| {
        if u == bsp(1) {
            halted(false, STARVE - 1)
        } else {
            off()
        }
    };
    assert_eq!(pick(bsp(0), false, Q, Q, STARVE, quiet), Pick::Stay);
    assert_eq!(pick(bsp(0), true, 0, Q, STARVE, quiet), Pick::Stay);
    let starved = |u: usize| {
        if u == bsp(1) {
            halted(false, STARVE)
        } else {
            off()
        }
    };
    assert_eq!(
        pick(bsp(0), false, Q, Q, STARVE, starved),
        Pick::Switch(bsp(1))
    );
    assert_eq!(
        pick(bsp(0), true, 0, Q, STARVE, starved),
        Pick::Switch(bsp(1))
    );
}

#[test]
fn halted_guest_with_work_runs_at_the_next_quantum_edge() {
    let other = |u: usize| {
        if u == bsp(1) {
            halted(true, 0)
        } else {
            off()
        }
    };
    assert_eq!(pick(bsp(0), false, Q - 1, Q, STARVE, other), Pick::Stay);
    assert_eq!(
        pick(bsp(0), false, Q, Q, STARVE, other),
        Pick::Switch(bsp(1))
    );
}

#[test]
fn round_robin_starts_after_the_current_slot_and_never_picks_it() {
    let all = |u: usize| vcpu0_only(u, |_| true);
    for cur in 0..MAX_GUESTS {
        assert_eq!(
            pick(bsp(cur), false, Q, Q, STARVE, all),
            Pick::Switch(bsp((cur + 1) % MAX_GUESTS))
        );
    }
    let only_three = |u: usize| vcpu0_only(u, |s| s == 3);
    assert_eq!(
        pick(bsp(0), false, Q, Q, STARVE, only_three),
        Pick::Switch(bsp(3))
    );
    assert_eq!(pick(bsp(3), false, Q, Q, STARVE, only_three), Pick::Stay);
}

#[test]
fn two_units_of_one_slot_alternate_after_a_quantum() {
    let other = |u: usize| {
        if slot::unit_slot(u) == 0 && slot::unit_vcpu(u) <= 1 {
            busy()
        } else {
            off()
        }
    };
    assert_eq!(
        pick(slot::unit(0, 0), false, Q, Q, STARVE, other),
        Pick::Switch(slot::unit(0, 1))
    );
    assert_eq!(
        pick(slot::unit(0, 1), false, Q, Q, STARVE, other),
        Pick::Switch(slot::unit(0, 0))
    );
}

#[test]
fn an_idle_unit_yields_to_a_busy_sibling() {
    let other = |u: usize| {
        if u == slot::unit(0, 1) {
            busy()
        } else {
            off()
        }
    };
    assert_eq!(
        pick(slot::unit(0, 0), true, 0, Q, STARVE, other),
        Pick::Switch(slot::unit(0, 1))
    );
}

#[test]
fn a_halted_sibling_with_no_work_is_skipped_until_starve() {
    let quiet = |u: usize| {
        if u == slot::unit(0, 1) {
            halted(false, STARVE - 1)
        } else {
            off()
        }
    };
    assert_eq!(
        pick(slot::unit(0, 0), false, Q, Q, STARVE, quiet),
        Pick::Stay
    );
    let starved = |u: usize| {
        if u == slot::unit(0, 1) {
            halted(false, STARVE)
        } else {
            off()
        }
    };
    assert_eq!(
        pick(slot::unit(0, 0), false, Q, Q, STARVE, starved),
        Pick::Switch(slot::unit(0, 1))
    );
}

#[test]
fn pick_on_core_ignores_another_cores_vcpu() {
    let home = |u: usize| if u == slot::unit(0, 1) { 1 } else { 0 };
    let both = |u: usize| {
        if u == slot::unit(0, 0) || u == slot::unit(0, 1) {
            busy()
        } else {
            off()
        }
    };
    assert_eq!(
        pick_on_core(0, slot::unit(0, 0), false, Q, Q, STARVE, both, home),
        Pick::Stay
    );
    assert_eq!(
        pick_on_core(1, slot::unit(0, 0), true, Q, Q, STARVE, both, home),
        Pick::Switch(slot::unit(0, 1))
    );
}

#[test]
fn pick_never_returns_the_current_unit() {
    for cur in 0..MAX_UNITS {
        assert_eq!(
            pick(cur, false, Q, Q, STARVE, |_| busy()),
            Pick::Switch((cur + 1) % MAX_UNITS)
        );
    }
}

#[test]
fn no_on_guest_waits_longer_than_the_backstop_plus_the_other_quanta() {
    // Four busy guests, vCPU 0 only, 1 tick per step: every guest gets the
    // CPU again within (MAX_GUESTS - 1) quanta.
    let mut cur = bsp(0);
    let mut used = 0u64;
    let mut waited = [0u64; MAX_GUESTS];
    let mut worst = 0u64;
    let bsp_busy = |u: usize| vcpu0_only(u, |_| true);
    for _ in 0..(Q * 40) / 1000 {
        used += 1000;
        let cur_slot = slot::unit_slot(cur);
        for (s, w) in waited.iter_mut().enumerate() {
            if s != cur_slot {
                *w += 1000;
                worst = worst.max(*w);
            }
        }
        if let Pick::Switch(n) = pick(cur, false, used, Q, STARVE, bsp_busy) {
            cur = n;
            used = 0;
            waited[slot::unit_slot(n)] = 0;
        }
    }
    assert!(worst <= Q * (MAX_GUESTS as u64 - 1), "worst={worst}");
}

#[test]
fn runnable_needs_the_guest_on() {
    assert!(!runnable(off(), 0));
    assert!(runnable(busy(), STARVE));
    assert!(!runnable(halted(false, 0), STARVE));
    assert!(runnable(halted(true, 0), STARVE));
    assert!(runnable(halted(false, STARVE), STARVE));
    let mut o = halted(true, STARVE);
    o.on = false;
    assert!(!runnable(o, STARVE));
}

#[test]
fn tsc_arithmetic() {
    assert_eq!(us_to_tsc(2_000, 2_000_000_000, 1), 4_000_000);
    assert_eq!(us_to_tsc(2_000, 0, 1_000_000_000), 2_000_000);
    assert_eq!(us_to_tsc(u64::MAX, u64::MAX, 1), u64::MAX / 1_000_000);
    // 2 ms at 2 GHz with the Skylake rate (TSC / 32).
    assert_eq!(quantum_ticks(2_000_000_000, 1, 5), 125_000);
    assert_eq!(quantum_ticks(0, 2_000_000_000, 5), 125_000);
    assert_eq!(quantum_ticks(1, 1, 31), 1, "never zero");
    assert_eq!(
        quantum_ticks(2_000_000_000, 1, 99),
        quantum_ticks(2_000_000_000, 1, 31)
    );
}

#[test]
fn idle_is_hlt_with_nothing_to_deliver() {
    assert!(guest_idle(true, 0));
    assert!(
        !guest_idle(true, 0x8000_0020),
        "an interrupt is being delivered"
    );
    assert!(!guest_idle(false, 0));
    assert!(!guest_idle(false, 0x8000_0020));
}

const RELOAD: u32 = 0x10_0000;
const SHIFT: u32 = 5;
const QT: u32 = 125_000;

#[test]
fn shared_arm_starts_a_deadline_and_paces_at_the_quantum() {
    let (v, paced, due) = shared_preempt_arm(RELOAD, 0, 1_000, SHIFT, false, None, QT);
    assert_eq!(due, 1_000 + (u64::from(RELOAD) << SHIFT));
    assert_eq!(v, QT);
    assert!(paced);
}

#[test]
fn shared_arm_keeps_the_deadline_across_quanta_and_ends_on_a_real_tick() {
    let start = 1_000u64;
    let (_, _, due) = shared_preempt_arm(RELOAD, 0, start, SHIFT, false, None, QT);
    let mut now = start;
    let mut real = 0;
    for _ in 0..64 {
        let (v, paced, d) = shared_preempt_arm(RELOAD, due, now, SHIFT, false, None, QT);
        assert_eq!(d, due, "the deadline does not move while it is ahead");
        assert!(v >= 1 && v <= RELOAD);
        if paced {
            assert!(v <= QT);
        } else {
            real += 1;
            assert!(now + (u64::from(v) << SHIFT) + (1 << SHIFT) >= due);
            break;
        }
        now += u64::from(v) << SHIFT;
    }
    assert_eq!(real, 1);
}

#[test]
fn shared_arm_delivers_a_tick_that_came_due_while_the_guest_was_off_the_cpu() {
    let due = 10_000u64;
    let (v, paced, d) = shared_preempt_arm(RELOAD, due, due + 5_000_000, SHIFT, false, None, QT);
    assert_eq!((v, paced, d), (1, false, due));
    // That exit is the real tick; the next arm starts a new period.
    let now = due + 5_000_100;
    let (v, paced, d) = shared_preempt_arm(RELOAD, due, now, SHIFT, true, None, QT);
    assert_eq!(d, now + (u64::from(RELOAD) << SHIFT));
    assert_eq!(v, QT);
    assert!(paced);
}

#[test]
fn shared_arm_uart_pace_only_shortens() {
    let (v, paced, _) = shared_preempt_arm(RELOAD, 0, 0, SHIFT, false, Some(10_000), QT);
    assert_eq!((v, paced), (10_000, true));
    let (v, paced, _) = shared_preempt_arm(RELOAD, 0, 0, SHIFT, false, Some(QT * 4), QT);
    assert_eq!((v, paced), (QT, true));
    let (v, _, _) = shared_preempt_arm(RELOAD, 0, 0, SHIFT, false, Some(0), QT);
    assert_eq!(v, 1, "never zero");
}

#[test]
fn shared_arm_short_reload_is_not_paced() {
    // A reload shorter than the quantum is the guest's own tick.
    let (v, paced, _) = shared_preempt_arm(1_000, 0, 0, SHIFT, false, None, QT);
    assert_eq!((v, paced), (1_000, false));
    let (v, paced, _) = shared_preempt_arm(0, 0, 0, SHIFT, false, None, QT);
    assert_eq!((v, paced), (1, false), "reload 0 is treated as 1");
}

#[test]
fn leave_plan_is_one_restart_for_each_start() {
    assert_eq!(leave_plan(true, 0), Leave::StayOff);
    assert_eq!(leave_plan(true, 9), Leave::StayOff);
    assert_eq!(leave_plan(false, 0), Leave::Restart);
    assert_eq!(leave_plan(false, RESTARTS_PER_START), Leave::Off);
    assert_eq!(leave_plan(false, RESTARTS_PER_START + 1), Leave::Off);
}

#[test]
fn xsave_area_holds_avx512_and_amx_and_refuses_more() {
    assert!(xsave_fits(576), "x87 + SSE + header");
    assert!(xsave_fits(2_696), "Skylake-SP AVX-512");
    assert!(xsave_fits(11_008), "Sapphire Rapids AMX");
    assert!(xsave_fits(XSAVE_AREA_BYTES as u32));
    assert!(!xsave_fits(XSAVE_AREA_BYTES as u32 + 64));
    assert!(!xsave_fits(0));
    assert!(!xsave_fits(512));
}

#[test]
fn slot_overhead_counts_every_region() {
    // 32 MiB slab, 4 MiB flash, sink, hole-zero, 32 scratch, one table chunk.
    let n = slot_overhead_chunks(32 << 20, 4 << 20, 32);
    assert_eq!(n, 16 + 2 + 1 + 1 + 32 + 1);
    assert_eq!(slot_overhead_chunks(1, 1, 0), 1 + 1 + 1 + 1 + 1);
}

#[test]
fn slot_report_ram_is_capped_and_has_a_floor() {
    let over = 53;
    assert_eq!(slot_report_chunks(4096, over, 512, 1008), Some(1008));
    assert_eq!(
        slot_report_chunks(over + 512 + 300, over, 512, 1008),
        Some(300)
    );
    assert_eq!(
        slot_report_chunks(over + SLOT_MIN_REPORT_CHUNKS, over, 0, 1008),
        Some(SLOT_MIN_REPORT_CHUNKS)
    );
    assert_eq!(
        slot_report_chunks(over + SLOT_MIN_REPORT_CHUNKS - 1, over, 0, 1008),
        None
    );
    assert_eq!(slot_report_chunks(10, over, 0, 1008), None, "no underflow");
    assert_eq!(slot_report_chunks(0, 0, 0, 1008), None);
}

#[test]
fn constants_are_sane() {
    assert!(QUANTUM_US < STARVE_US);
    assert!(
        STARVE_US <= 20_000,
        "a halted guest's timer is at most this late"
    );
    assert_eq!(ARENA_CHUNK_BYTES, 2 << 20);
    assert_eq!(NESTED_SLOT_DISK_BYTES % ARENA_CHUNK_BYTES, 0);
    assert_eq!(XSAVE_AREA_BYTES % 64, 0);
    assert!(MAX_GUESTS >= 2);
}

#[test]
fn scheduler_is_wired_into_the_one_exit_path() {
    let g = include_str!("guest_uefi.rs");
    let s = include_str!("guest_sched.rs");
    assert!(g.contains("pub mod guest_sched;"));
    assert!(g.contains("guest_sched::resume("));
    assert!(g.contains("guest_sched::mark_running()"));
    assert!(g.contains("guest_sched::on_guest_left()"));
    let left = s.find("fn on_guest_left()").expect("on_guest_left");
    let end = (left + 8000).min(s.len());
    let body = &s[left..end];
    let home = body.find("select_home_vcpu(next)").expect("home");
    let sw = body.find("switch_in(slot::unit(next, home_v)").expect("switch");
    assert!(home < sw, "name the core-0 vCPU before VMPTRLD");
    assert!(g.contains("guest_sched::start_pending_from_idle()"));
    assert!(g.contains("unsafe fn build_slot_machine("));
    // The seated second-guest copy stays gone: one launch path, one exit path.
    assert!(!g.contains("finish_seat_firmware"));
    assert!(!g.contains("SeatUart"));
    // Every guest's CPU state is switched, not only the registers.
    for needle in [
        "xsave64",
        "xrstor64",
        "MSR_KERNEL_GS_BASE",
        "write_dr(6",
        "MSR_PRED_CMD",
    ] {
        assert!(s.contains(needle), "{needle}");
    }
    // The scheduler never prints an iron marker.
    assert!(!s.contains("RAYNU-V-"));
}
