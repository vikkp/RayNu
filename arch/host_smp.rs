//! Host cores for B21 (ADR-024).
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside**.
//!
//! Topology is read from EFI MP Services before ExitBootServices. No core is
//! started here. Core 0 writes the statics below once, before any other core
//! exists. Later cores read them.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};

use crate::arch::cpu;

/// Same bound as `guest::slot::MAX_UNITS`. One schedulable unit per core.
pub const MAX_CORES: usize = 16;
pub const NO_CORE: u8 = 0xFF;
pub const KICK_VECTOR: u8 = 0xF2;
/// 2 MiB per non-zero core: VMXON page, canary, host stack.
pub const BLOCK_PAGES: usize = 512;
pub const BLOCK_BYTES: u64 = (BLOCK_PAGES * 4096) as u64;

/// One logical processor, as MP Services reports it.
#[derive(Clone, Copy)]
pub struct CpuInfo {
    pub apic_id: u32,
    pub package: u32,
    pub core: u32,
    pub thread: u32,
    pub enabled: bool,
    pub healthy: bool,
    pub bsp: bool,
}

/// `AP_BASE` is the block array. Written by core 0 in `probe_pre_ebs`.
static AP_BASE: AtomicU64 = AtomicU64::new(0);
/// Non-zero cores that have a block. Core index `i` (i ≥ 1) uses block `i - 1`.
static N_AP: AtomicUsize = AtomicUsize::new(0);
/// Cores we intend to start, including core 0. 1 until a multi-core probe sticks.
static USE: AtomicUsize = AtomicUsize::new(1);
/// Cores that reached VMXON. Stays 1 until `start_cores` (WP3).
static CORES_ONLINE: AtomicUsize = AtomicUsize::new(1);
static X2APIC: AtomicBool = AtomicBool::new(false);
static LOW_BASE: AtomicU64 = AtomicU64::new(0);
/// APIC id of each selected core, index 0 = BSP. Written by core 0 in the probe.
static APIC_IDS: [AtomicU32; MAX_CORES] = [const { AtomicU32::new(0) }; MAX_CORES];

/// Ticket lock. Core 0 holds ticket 0 from boot: `MON_OWNER` is 0 and the
/// next ticket handed out is 1. Written by whichever core takes or drops it.
/// While core 0 is the only taker, the exit count equals the ticket it holds,
/// so a pulse with `o = x + 1` means that exit's `lock inc` has run.
pub static MON_NEXT: AtomicU32 = AtomicU32::new(1);
pub static MON_OWNER: AtomicU32 = AtomicU32::new(0);
/// TSC at the last VM-exit landing. The landing stub writes it under the lock.
pub static LAND_TSC: AtomicU64 = AtomicU64::new(0);
/// Core that holds the ticket, [`NO_CORE`] when the ticket is free.
/// [`acquire`] and [`claim_holder`] set it. [`release`] and the entry
/// stubs clear it in the same step as the owner increment. Core 0 starts
/// holding ticket 0, so this starts at 0.
pub static MON_HOLDER: AtomicU8 = AtomicU8::new(0);
static ENTER_TSC: AtomicU64 = AtomicU64::new(0);
static HOLD_MAX: AtomicU64 = AtomicU64::new(0);
static WAIT_MAX: AtomicU64 = AtomicU64::new(0);
static KICKS_TAKEN: AtomicU64 = AtomicU64::new(0);
static KICKS_SENT: AtomicU64 = AtomicU64::new(0);
static RECALLS: AtomicU64 = AtomicU64::new(0);
/// Core 0's host stack top, published when the guest VMCS is built.
static CORE0_STACK: AtomicU64 = AtomicU64::new(0);
/// Unit whose VMCS this core last entered. Written by that core as it leaves
/// the monitor, read by that core as it enters.
static CORE_UNIT: [AtomicU8; MAX_CORES] = [const { AtomicU8::new(0) }; MAX_CORES];
static CORE_SPEC: [AtomicU64; MAX_CORES] = [const { AtomicU64::new(0) }; MAX_CORES];
static CORE_APIC: [AtomicU64; MAX_CORES] = [const { AtomicU64::new(0) }; MAX_CORES];
static IN_GUEST: [AtomicBool; MAX_CORES] = [const { AtomicBool::new(false) }; MAX_CORES];
/// Last checkpoint core 0 stored on its exit path. 0 until the first exit.
static EXIT_PHASE: AtomicU8 = AtomicU8::new(0);
/// `rdtsc` of that checkpoint. Core 1 treats a silent beat as a stalled exit.
static EXIT_BEAT: AtomicU64 = AtomicU64::new(0);
/// Low byte of the exit reason stored at checkpoint 1. Core 0 only.
static EXIT_REASON8: AtomicU8 = AtomicU8::new(0);
/// Guest RIP stored at checkpoint 1. Core 0 only.
static EXIT_RIP: AtomicU64 = AtomicU64::new(0);
/// Exits that stored checkpoint 1 on core 0.
static CORE0_EXITS: AtomicU64 = AtomicU64::new(0);
/// Set by the core that changed EPT. The target core flushes before its next entry.
static INVEPT_PENDING: [AtomicBool; MAX_CORES] = [const { AtomicBool::new(false) }; MAX_CORES];
/// Core whose queue holds the unit. 0 until placement. The monitor writes it.
static ASSIGNED: [AtomicU8; MAX_CORES] = [const { AtomicU8::new(0) }; MAX_CORES];
/// Core 0 asked this core to VMCLEAR its VMCS and stop entering it.
static RECALL: [AtomicBool; MAX_CORES] = [const { AtomicBool::new(false) }; MAX_CORES];
/// The core has finished the recall. Starts finished so a wait is not stuck.
static RECALL_DONE: [AtomicBool; MAX_CORES] = [const { AtomicBool::new(true) }; MAX_CORES];
/// This core has VMPTRLDed a guest VMCS it must VMCLEAR before the VMCS moves.
static VMCS_LOADED: [AtomicBool; MAX_CORES] = [const { AtomicBool::new(false) }; MAX_CORES];

/// Take the monitor. Fair: callers run in ticket order.
/// The winner is [`MON_HOLDER`] until [`release`].
pub fn acquire() {
    let ticket = MON_NEXT.fetch_add(1, Ordering::AcqRel);
    while MON_OWNER.load(Ordering::Acquire) != ticket {
        core::hint::spin_loop();
    }
    claim_holder();
}

/// Drop the monitor. The holder is cleared before the owner increment, so
/// the next core cannot observe its ticket and then lose the holder byte.
/// The entry stubs do these two stores after the last guest register load.
pub fn release() {
    MON_HOLDER.store(NO_CORE, Ordering::Release);
    MON_OWNER.fetch_add(1, Ordering::Release);
}

/// The ticket is already this core's. [`acquire`] and the entry-failure
/// stubs call this. The landing stub does not: guest XMM is still live
/// there, and a call would let the callee write the 32-byte home area.
/// A failure stub reserves that area with `sub rsp, 40` (32-byte home
/// plus 8 so the call is 16-byte aligned). [`monitor_enter`] stores the
/// holder for a normal exit.
#[inline(never)]
pub extern "C" fn claim_holder() {
    let core = this_core().min(MAX_CORES - 1) as u8;
    MON_HOLDER.store(core, Ordering::Release);
}

/// `true` when `core` is the core recorded for the ticket now held.
pub fn holder_is(core: usize) -> bool {
    core < MAX_CORES && MON_HOLDER.load(Ordering::Acquire) == core as u8
}

/// Checkpoint on core 0's exit path. Core 1 reads it and does not take the lock.
///
/// 1 reason and RIP stored, 2 the page tick returned, 3 exit dispatch
/// returned, 4 arming the preemption timer, 5 leaving the monitor,
/// 6 the host-stack check passed.
pub fn note_exit_phase(phase: u8) {
    EXIT_PHASE.store(phase, Ordering::Release);
    EXIT_BEAT.store(cpu::rdtsc(), Ordering::Release);
}

/// Core 0's exit reason and guest RIP, stored with checkpoint 1.
/// Counts one core 0 exit.
pub fn note_core0_exit(reason: u32, rip: u64) {
    if this_core() != 0 {
        return;
    }
    EXIT_REASON8.store((reason & 0xFF) as u8, Ordering::Release);
    EXIT_RIP.store(rip, Ordering::Release);
    CORE0_EXITS.fetch_add(1, Ordering::Release);
}

pub fn core0_exits() -> u64 {
    CORE0_EXITS.load(Ordering::Acquire)
}

/// Highest online core, when there is one besides core 0. It writes the
/// pulse and is not a place for a vCPU.
pub fn watchdog_core(online: usize) -> Option<usize> {
    if online <= 1 {
        None
    } else {
        Some(online - 1)
    }
}

pub fn is_watchdog_core(core: usize) -> bool {
    watchdog_core(cores_online()) == Some(core)
}

/// One COM2 line once core 0 has been silent for two seconds.
///
/// `p` is the last checkpoint. `g` is core 0's in-guest flag. `h` is the
/// core that holds the ticket (`ff` is none; it changes with the ticket,
/// not in `monitor_leave`). `o` and `n` are the ticket counters:
/// `n - o` is 0 when the lock is free or a release left the counters equal,
/// 1 when one core holds it, and 2 or more when another core is queued.
/// `r` is the low byte of the exit reason at checkpoint 1. `rip` is the
/// guest RIP stored there. `x` is core 0's exit count, `t` is the guest TX
/// ring length, and `d` is bytes an earlier pulse did not get onto COM2.
pub fn format_b21_pulse(
    phase: u8,
    in_guest: bool,
    holder: u8,
    owner: u32,
    next: u32,
    reason: u8,
    rip: u64,
    exits: u64,
    ring: u64,
    dropped: u64,
    out: &mut [u8],
) -> usize {
    let mut n = push_bytes(out, 0, b"boot: B21 pulse p=");
    n = push_byte(out, n, b'0' + phase.min(9));
    n = push_bytes(out, n, b" g=");
    n = push_byte(out, n, if in_guest { b'1' } else { b'0' });
    n = push_bytes(out, n, b" h=");
    n = push_hex2(out, n, holder);
    n = push_bytes(out, n, b" o=");
    n = push_hex32(out, n, owner);
    n = push_bytes(out, n, b" n=");
    n = push_hex32(out, n, next);
    n = push_bytes(out, n, b" r=");
    n = push_hex2(out, n, reason);
    n = push_bytes(out, n, b" rip=");
    n = push_hex64(out, n, rip);
    n = push_bytes(out, n, b" x=");
    n = push_dec(out, n, exits);
    n = push_bytes(out, n, b" t=");
    n = push_dec(out, n, ring);
    n = push_bytes(out, n, b" d=");
    n = push_dec(out, n, dropped);
    push_bytes(out, n, b"\r\n")
}

fn push_bytes(out: &mut [u8], mut n: usize, bytes: &[u8]) -> usize {
    for &b in bytes {
        n = push_byte(out, n, b);
    }
    n
}

fn push_byte(out: &mut [u8], n: usize, b: u8) -> usize {
    if n < out.len() {
        out[n] = b;
    }
    n + 1
}

fn push_hex2(out: &mut [u8], n: usize, v: u8) -> usize {
    let hex = b"0123456789abcdef";
    let n = push_byte(out, n, hex[(v >> 4) as usize]);
    push_byte(out, n, hex[(v & 0xF) as usize])
}

fn push_hex32(out: &mut [u8], mut n: usize, v: u32) -> usize {
    let hex = b"0123456789abcdef";
    for i in (0..8).rev() {
        n = push_byte(out, n, hex[((v >> (i * 4)) & 0xF) as usize]);
    }
    n
}

fn push_hex64(out: &mut [u8], mut n: usize, v: u64) -> usize {
    let hex = b"0123456789abcdef";
    for i in (0..16).rev() {
        n = push_byte(out, n, hex[((v >> (i * 4)) & 0xF) as usize]);
    }
    n
}

/// Called from the watchdog core, which does not hold the monitor. One line
/// every time the caller asks, whether or not core 0 is still checkpointing.
/// Writes straight to COM2, not the guest ring. `d` is drops before this line.
pub fn emit_core0_pulse(_now: u64) {
    let exits = CORE0_EXITS.load(Ordering::Acquire);
    let mut buf = [0u8; 160];
    let n = format_b21_pulse(
        EXIT_PHASE.load(Ordering::Acquire),
        IN_GUEST[0].load(Ordering::Acquire),
        MON_HOLDER.load(Ordering::Acquire),
        MON_OWNER.load(Ordering::Acquire),
        MON_NEXT.load(Ordering::Acquire),
        EXIT_REASON8.load(Ordering::Acquire),
        EXIT_RIP.load(Ordering::Acquire),
        exits,
        crate::boot::serial::guest_tx_len() as u64,
        u64::from(crate::boot::serial::com2_direct_dropped()),
        &mut buf,
    );
    let n = n.min(buf.len());
    let _ = crate::boot::serial::com2_bytes_direct(&buf[..n]);
    note_stall_tx(exits);
}

/// A second pulse with the same exit count is a stall. Dump the guest TX
/// ring once so the lines still queued are visible on COM2.
fn note_stall_tx(exits: u64) {
    static PREV: AtomicU64 = AtomicU64::new(u64::MAX);
    static DONE: AtomicBool = AtomicBool::new(false);
    let prev = PREV.swap(exits, Ordering::AcqRel);
    let ring = crate::boot::serial::guest_tx_len() as u64;
    if !tx_dump_due(prev, exits, ring, DONE.load(Ordering::Acquire)) {
        return;
    }
    if DONE.swap(true, Ordering::AcqRel) {
        return;
    }
    dump_guest_tx();
}

/// `prev` is the exit count from the previous pulse. `u64::MAX` means there
/// was no previous pulse. A dump is due when that count has not moved and
/// either an exit has been recorded or the ring is holding bytes.
pub fn tx_dump_due(prev: u64, exits: u64, ring: u64, already: bool) -> bool {
    !already && prev != u64::MAX && prev == exits && (exits > 0 || ring > 0)
}

fn dump_guest_tx() {
    let mut hdr = [0u8; 24];
    let n = push_bytes(&mut hdr, 0, b"boot: B21 guest tx\r\n");
    let _ = crate::boot::serial::com2_bytes_direct(&hdr[..n.min(hdr.len())]);
    let mut buf = [0u8; 128];
    let mut off = 0usize;
    loop {
        let n = crate::boot::serial::guest_tx_copy_at(off, &mut buf);
        if n == 0 {
            break;
        }
        let _ = crate::boot::serial::com2_bytes_direct(&buf[..n]);
        off += n;
    }
    let _ = crate::boot::serial::com2_bytes_direct(b"\r\n");
}

pub fn kicks_taken() -> u64 {
    KICKS_TAKEN.load(Ordering::Acquire)
}

pub fn kicks_sent() -> u64 {
    KICKS_SENT.load(Ordering::Acquire)
}

pub fn recalls() -> u64 {
    RECALLS.load(Ordering::Acquire)
}

pub fn hold_max_ticks() -> u64 {
    HOLD_MAX.load(Ordering::Acquire)
}

pub fn wait_max_ticks() -> u64 {
    WAIT_MAX.load(Ordering::Acquire)
}

pub fn assigned(unit: usize) -> usize {
    if unit >= MAX_CORES {
        return 0;
    }
    ASSIGNED[unit].load(Ordering::Acquire) as usize
}

/// Home the unit on `core`. The monitor is the only writer.
pub fn assign_unit(unit: usize, core: usize) {
    if unit >= MAX_CORES {
        return;
    }
    ASSIGNED[unit].store(core.min(MAX_CORES - 1) as u8, Ordering::Release);
}

/// The core has a guest VMCS loaded. A later recall VMCLEARs it.
pub fn note_loaded(core: usize) {
    if core < MAX_CORES {
        VMCS_LOADED[core].store(true, Ordering::Release);
    }
}

/// `true` once, when this core had loaded a VMCS. The caller VMCLEARs it.
pub fn take_loaded(core: usize) -> bool {
    core < MAX_CORES && VMCS_LOADED[core].swap(false, Ordering::AcqRel)
}

/// Ask `core` to give its VMCS back. Core 0 is never recalled.
pub fn request_recall(core: usize) {
    if core == 0 || core >= MAX_CORES {
        return;
    }
    RECALL_DONE[core].store(false, Ordering::Release);
    RECALL[core].store(true, Ordering::Release);
    RECALLS.fetch_add(1, Ordering::Relaxed);
    poke_core(core);
}

pub fn recall_waiting(core: usize) -> bool {
    core < MAX_CORES && RECALL[core].load(Ordering::Acquire)
}

pub fn recall_finished(core: usize) -> bool {
    core >= MAX_CORES || RECALL_DONE[core].load(Ordering::Acquire)
}

pub fn finish_recall(core: usize) {
    if core >= MAX_CORES {
        return;
    }
    RECALL[core].store(false, Ordering::Release);
    RECALL_DONE[core].store(true, Ordering::Release);
}

/// A vCPU is homed on a core other than core 0.
pub fn remote_placed() -> bool {
    ASSIGNED.iter().any(|a| a.load(Ordering::Acquire) != 0)
}

pub fn vmcs_loaded_on(core: usize) -> bool {
    core < MAX_CORES && VMCS_LOADED[core].load(Ordering::Acquire)
}

/// This exit is a core that is running a placed vCPU.
///
/// Until that happens, every exit is core 0's. A stack pointer that falls
/// inside an AP block must not divert CPU 0 onto the AP resume path.
pub fn running_remote(core: usize) -> bool {
    core != 0 && (remote_placed() || vmcs_loaded_on(core))
}

/// `true` while some other core is in a guest and still owes an INVEPT.
pub fn invept_owed_by_a_guest() -> bool {
    let me = this_core();
    let n = cores_online().min(MAX_CORES);
    (0..n).any(|c| {
        c != me && IN_GUEST[c].load(Ordering::Acquire) && INVEPT_PENDING[c].load(Ordering::Acquire)
    })
}

/// Flush this core, then the pending bit on every other online core.
/// A core that is in a guest is kicked and this core waits until that
/// guest has exited and flushed.
///
/// # Safety
/// VMX root. The caller holds the monitor. `INVEPT` is valid.
pub unsafe fn invept_everywhere() {
    crate::memory::ept_hw::invept_global();
    if cores_online() <= 1 {
        return;
    }
    let me = this_core();
    let n = cores_online().min(MAX_CORES);
    let mut wait = false;
    for c in 0..n {
        if c == me {
            continue;
        }
        INVEPT_PENDING[c].store(true, Ordering::Release);
        if IN_GUEST[c].load(Ordering::Acquire) {
            poke_core(c);
            wait = true;
        }
    }
    if wait {
        #[cfg(target_os = "uefi")]
        crate::vmx::guest_uefi::yield_monitor_until(None, || !invept_owed_by_a_guest());
    }
}

/// Flush this core's pending INVEPT. Called with the monitor held.
///
/// # Safety
/// VMX root. `INVEPT` is valid.
pub unsafe fn take_pending_invept() {
    let core = this_core().min(MAX_CORES - 1);
    if INVEPT_PENDING[core].swap(false, Ordering::AcqRel) {
        crate::memory::ept_hw::invept_global();
    }
}

/// Older name. The eight guest-UEFI sites call [`invept_everywhere`].
///
/// # Safety
/// Same as [`invept_everywhere`].
pub unsafe fn invept_guests() {
    invept_everywhere();
}

/// Wake a sleeping core and, when it is in a guest, send the kick IPI.
///
/// One core online does not send. `EFI\\RayNu\\nokick.txt` does not send.
/// The mailbox store still happens, so correctness does not depend on the IPI.
pub fn poke_core(core: usize) {
    wake_core(core);
    if cores_online() <= 1 || core == this_core() || core >= MAX_CORES {
        return;
    }
    if !IN_GUEST[core].load(Ordering::Acquire) {
        return;
    }
    if crate::boot::raynu_f_flag::host_nokick() {
        return;
    }
    #[cfg(target_os = "uefi")]
    unsafe {
        let id = APIC_IDS[core].load(Ordering::Acquire);
        // SAFETY: this core holds the monitor; the destination is another
        // online core's APIC id. KANI-TARGET: host kick IPI (outside Proven Core).
        let _ = crate::arch::apic::send_ipi(id, KICK_VECTOR);
    }
    KICKS_SENT.fetch_add(1, Ordering::Relaxed);
}

/// Kick the core that owns unit `u`.
pub fn poke_unit(unit: usize) {
    if cores_online() <= 1 {
        return;
    }
    poke_core(assigned(unit));
}

/// Wake a sleeping core. A core in guest mode notices on its next exit.
pub fn wake_core(core: usize) {
    #[cfg(feature = "uefi-bin")]
    {
        start::wake_core(core);
    }
    #[cfg(not(feature = "uefi-bin"))]
    {
        let _ = core;
    }
}

/// Sleep until [`wake_core`] stores this core's mailbox.
///
/// # Safety
/// `core` is the caller. The caller does not hold the monitor.
pub unsafe fn wait_mailbox(core: usize) {
    #[cfg(feature = "uefi-bin")]
    {
        start::wait_mailbox(core);
    }
    #[cfg(not(feature = "uefi-bin"))]
    {
        let _ = core;
    }
}

/// Remember core 0's `HOST_RSP`. Called when that stack is reserved.
pub fn note_core0_stack(top: u64) {
    CORE0_STACK.store(top, Ordering::Release);
}

pub fn stack_top_of(core: usize) -> u64 {
    if core == 0 {
        CORE0_STACK.load(Ordering::Acquire)
    } else {
        stack_top(core, ap_base(), 0)
    }
}

/// Last Rust before a guest entry. The entry stub releases the ticket.
/// The holder stays this core until that release.
///
/// # Safety
/// Caller holds the monitor and is about to enter a guest on this core.
pub unsafe fn monitor_leave() {
    let core = this_core().min(MAX_CORES - 1);
    #[cfg(target_os = "uefi")]
    {
        leave_hardware(core);
    }
    note_hold();
    if core < MAX_CORES {
        IN_GUEST[core].store(true, Ordering::Release);
    }
    let prev = MON_HOLDER.load(Ordering::Acquire);
    if !holder_is(core) {
        two_in_monitor(core, prev);
    }
}

/// First line of `guest_uefi_vmexit`. The landing stub already took the
/// ticket. This stores the holder. The landing stub does not call out.
///
/// # Safety
/// Caller is the exit handler on the core that just left the guest.
pub unsafe fn monitor_enter() {
    let core = this_core().min(MAX_CORES - 1);
    let prev = MON_HOLDER.swap(core as u8, Ordering::AcqRel);
    if prev != NO_CORE {
        two_in_monitor(core, prev);
    }
    if core < MAX_CORES && INVEPT_PENDING[core].swap(false, Ordering::AcqRel) {
        // Before IN_GUEST is cleared, so a core waiting in
        // invept_everywhere sees the flush happen first.
        crate::memory::ept_hw::invept_global();
    }
    if core < MAX_CORES {
        IN_GUEST[core].store(false, Ordering::Release);
    }
    let land = LAND_TSC.load(Ordering::Acquire);
    if land != 0 {
        note_wait(cpu::rdtsc().wrapping_sub(land));
    }
    ENTER_TSC.store(cpu::rdtsc(), Ordering::Release);
    if cores_online() == 1 {
        return;
    }
    #[cfg(target_os = "uefi")]
    {
        enter_hardware(core);
    }
}

/// `monitor_enter` without the wait sample. The fail path retook the
/// lock and called [`claim_holder`], so the holder is already this core.
///
/// # Safety
/// Caller holds the monitor after a failed VM-entry.
pub unsafe fn monitor_reenter() {
    let core = this_core().min(MAX_CORES - 1);
    let prev = MON_HOLDER.load(Ordering::Acquire);
    if prev != core as u8 {
        two_in_monitor(core, prev);
    }
    if core < MAX_CORES {
        IN_GUEST[core].store(false, Ordering::Release);
    }
    ENTER_TSC.store(cpu::rdtsc(), Ordering::Release);
    if cores_online() == 1 {
        return;
    }
    #[cfg(target_os = "uefi")]
    {
        enter_hardware(core);
    }
}

fn note_wait(wait: u64) {
    let mut cur = WAIT_MAX.load(Ordering::Acquire);
    while wait > cur {
        match WAIT_MAX.compare_exchange(cur, wait, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(v) => cur = v,
        }
    }
}

fn note_hold() {
    let enter = ENTER_TSC.load(Ordering::Acquire);
    if enter == 0 {
        return;
    }
    let held = cpu::rdtsc().wrapping_sub(enter);
    let mut cur = HOLD_MAX.load(Ordering::Acquire);
    while held > cur {
        match HOLD_MAX.compare_exchange(cur, held, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(v) => cur = v,
        }
    }
}

fn two_in_monitor(core: usize, prev: u8) -> ! {
    direct_fault_line(b"boot: B21 two cores in the monitor", core, prev);
    loop {
        core::hint::spin_loop();
    }
}

fn direct_fault_line(prefix: &[u8], core: usize, prev: u8) {
    let mut buf = [0u8; 96];
    let mut n = push_bytes(&mut buf, 0, prefix);
    n = push_bytes(&mut buf, n, b" core=");
    n = push_dec(&mut buf, n, core as u64);
    n = push_bytes(&mut buf, n, b" prev=");
    n = push_hex2(&mut buf, n, prev);
    n = push_bytes(&mut buf, n, b" (ADR-024; not ISO-INSTALL-OK)\r\n");
    let n = n.min(buf.len());
    let _ = crate::boot::serial::com2_bytes_direct(&buf[..n]);
}

#[cfg(target_os = "uefi")]
unsafe fn leave_hardware(core: usize) {
    // 5 is before the host-stack check. A spin on a mismatch stays here.
    note_exit_phase(5);
    if cores_online() <= 1 {
        note_exit_phase(6);
        return;
    }
    if INVEPT_PENDING[core].swap(false, Ordering::AcqRel) {
        crate::memory::ept_hw::invept_global();
    }
    let u = crate::guest::slot::unit(
        crate::guest::slot::current(),
        crate::guest::slot::current_vcpu(),
    );
    CORE_UNIT[core].store(u as u8, Ordering::Release);
    CORE_SPEC[core].store(
        crate::sched::msr_firewall::shadow_read(crate::sched::msr_firewall::MSR_SPEC_CTRL),
        Ordering::Release,
    );
    CORE_APIC[core].store(
        crate::sched::msr_firewall::shadow_read(crate::sched::msr_firewall::MSR_APIC_BASE),
        Ordering::Release,
    );
    let expect = stack_top_of(core);
    if expect != 0 {
        if let Ok(rsp) = crate::vmx::ops::vmread(crate::vmx::fields::HOST_RSP) {
            if rsp != expect {
                direct_fault_line(
                    b"boot: B21 wrong host stack",
                    core,
                    MON_HOLDER.load(Ordering::Acquire),
                );
                loop {
                    core::hint::spin_loop();
                }
            }
        }
    }
    note_exit_phase(6);
}

#[cfg(target_os = "uefi")]
unsafe fn enter_hardware(core: usize) {
    let u = CORE_UNIT[core].load(Ordering::Acquire) as usize;
    crate::guest::slot::set_current(crate::guest::slot::unit_slot(u));
    crate::guest::slot::set_current_vcpu(crate::guest::slot::unit_vcpu(u));
    crate::sched::msr_firewall::shadow_write(
        crate::sched::msr_firewall::MSR_SPEC_CTRL,
        CORE_SPEC[core].load(Ordering::Acquire),
    );
    crate::sched::msr_firewall::shadow_write(
        crate::sched::msr_firewall::MSR_APIC_BASE,
        CORE_APIC[core].load(Ordering::Acquire),
    );
    if INVEPT_PENDING[core].swap(false, Ordering::AcqRel) {
        crate::memory::ept_hw::invept_global();
    }
}

/// Count a kick exit and acknowledge it. Other external interrupts are untouched.
pub fn note_kick_exit(reason: u32, intr: u64) {
    if reason & 0xFFFF != 1 {
        return;
    }
    if intr & (1 << 31) == 0 {
        return;
    }
    if (intr & 0xFF) as u8 != KICK_VECTOR {
        return;
    }
    #[cfg(target_os = "uefi")]
    unsafe {
        let _ = crate::arch::apic::eoi();
    }
    KICKS_TAKEN.fetch_add(1, Ordering::Relaxed);
}

/// One status line on core 0, at the scheduler's beat, once more than one
/// host core is online and a guest is on.
pub fn maybe_core_line(now: u64, period: u64) {
    if this_core() != 0 || cores_online() <= 1 || crate::guest::slot::on_count() == 0 {
        return;
    }
    static LAST: AtomicU64 = AtomicU64::new(0);
    let last = LAST.load(Ordering::Acquire);
    if last != 0 && now.wrapping_sub(last) < period {
        return;
    }
    LAST.store(now, Ordering::Release);
    let hz = crate::boot::raynu_f_flag::tsc_hz().max(1_000_000);
    let us = hz / 1_000_000;
    crate::boot::serial::write_str_nowait("boot: B21 cores");
    let n = cores_online().min(MAX_CORES);
    for c in 0..n {
        crate::boot::serial::write_str_nowait(" ");
        write_dec_local(c as u64);
        crate::boot::serial::write_str_nowait(":[");
        let mut any = false;
        for s in 0..crate::guest::slot::MAX_GUESTS {
            if !crate::guest::slot::is_on(s) {
                continue;
            }
            for v in 0..crate::guest::slot::MAX_VCPUS {
                if !crate::guest::slot::vcpu_up(s, v) {
                    continue;
                }
                let u = crate::guest::slot::unit(s, v);
                if assigned(u) != c {
                    continue;
                }
                if any {
                    crate::boot::serial::write_str_nowait(" ");
                }
                any = true;
                write_dec_local(s as u64);
                crate::boot::serial::write_str_nowait(".");
                write_dec_local(v as u64);
            }
        }
        crate::boot::serial::write_str_nowait("]");
    }
    crate::boot::serial::write_str_nowait(" lock_wait_max_us=");
    write_dec_local(wait_max_ticks() / us);
    crate::boot::serial::write_str_nowait(" hold_max_us=");
    write_dec_local(hold_max_ticks() / us);
    crate::boot::serial::write_str_nowait(" kicks=");
    write_dec_local(kicks_sent());
    crate::boot::serial::write_str_nowait("/");
    write_dec_local(kicks_taken());
    crate::boot::serial::write_str_nowait(" recalls=");
    write_dec_local(recalls());
    crate::boot::serial::write_line_nowait("");
}

fn write_dec_local(mut v: u64) {
    let mut tmp = [0u8; 20];
    let mut k = tmp.len();
    if v == 0 {
        crate::boot::serial::write_byte_nowait(b'0');
        return;
    }
    while v > 0 && k > 0 {
        k -= 1;
        tmp[k] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    for &b in &tmp[k..] {
        crate::boot::serial::write_byte_nowait(b);
    }
}

#[cfg(test)]
pub fn reset_monitor_for_test() {
    MON_NEXT.store(0, Ordering::Relaxed);
    MON_OWNER.store(0, Ordering::Relaxed);
    MON_HOLDER.store(0, Ordering::Relaxed);
}

#[cfg(test)]
thread_local! {
    static TEST_CORE: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Core this thread is running on. Host tests use the override (default 0).
/// Core 0's stacks are outside the block array, so "not in a block" is core 0.
pub fn this_core() -> usize {
    #[cfg(test)]
    {
        return TEST_CORE.with(|c| c.get());
    }
    #[cfg(not(test))]
    {
        let rsp: u64;
        // SAFETY: reads RSP only. No memory, no stack change.
        // KANI-TARGET: host_smp this_core rsp
        unsafe {
            core::arch::asm!(
                "mov {}, rsp",
                out(reg) rsp,
                options(nomem, nostack, preserves_flags)
            );
        }
        core_of_rsp(rsp, ap_base(), n_ap())
    }
}

#[cfg(test)]
pub fn set_test_core(core: usize) {
    TEST_CORE.with(|c| c.set(core));
}

pub fn ap_base() -> u64 {
    AP_BASE.load(Ordering::Acquire)
}

pub fn n_ap() -> usize {
    N_AP.load(Ordering::Acquire)
}

pub fn cores_online() -> usize {
    CORES_ONLINE.load(Ordering::Acquire).max(1)
}

pub(super) fn set_cores_online(n: usize) {
    CORES_ONLINE.store(n.max(1), Ordering::Release);
}

pub fn low_base() -> u64 {
    LOW_BASE.load(Ordering::Acquire)
}

pub fn x2apic() -> bool {
    X2APIC.load(Ordering::Acquire)
}

pub fn apic_id(core: usize) -> u32 {
    if core >= MAX_CORES {
        return 0;
    }
    APIC_IDS[core].load(Ordering::Acquire)
}

/// One thread per `(package, core)`. The BSP's core is index 0 and its
/// sibling is skipped. Disabled or unhealthy CPUs are skipped. In xAPIC mode
/// an APIC ID above 254 is skipped. At most [`MAX_CORES`].
pub fn select_cores(cpus: &[CpuInfo], x2apic: bool, out: &mut [u32; MAX_CORES]) -> usize {
    const CAP: usize = 64;
    #[derive(Clone, Copy)]
    struct Cand {
        apic: u32,
        package: u32,
        core: u32,
        thread: u32,
        bsp: bool,
    }
    let mut cands = [Cand {
        apic: 0,
        package: 0,
        core: 0,
        thread: 0,
        bsp: false,
    }; CAP];
    let mut n = 0usize;
    for cpu in cpus {
        if !cpu.enabled || !cpu.healthy {
            continue;
        }
        if !x2apic && cpu.apic_id > 254 {
            continue;
        }
        let mut found = None;
        for i in 0..n {
            if cands[i].package == cpu.package && cands[i].core == cpu.core {
                found = Some(i);
                break;
            }
        }
        let cand = Cand {
            apic: cpu.apic_id,
            package: cpu.package,
            core: cpu.core,
            thread: cpu.thread,
            bsp: cpu.bsp,
        };
        if let Some(i) = found {
            let keep_bsp = cands[i].bsp && !cpu.bsp;
            let replace = cpu.bsp || (!cands[i].bsp && cpu.thread < cands[i].thread);
            if replace && !keep_bsp {
                cands[i] = cand;
            }
        } else if n < CAP {
            cands[n] = cand;
            n += 1;
        }
    }
    if let Some(i) = (0..n).find(|&i| cands[i].bsp) {
        cands.swap(0, i);
    }
    if n > 1 {
        for i in 1..n {
            let mut j = i;
            while j > 1
                && (cands[j].package, cands[j].core) < (cands[j - 1].package, cands[j - 1].core)
            {
                cands.swap(j, j - 1);
                j -= 1;
            }
        }
    }
    let take = n.min(MAX_CORES);
    for i in 0..take {
        out[i] = cands[i].apic;
    }
    take
}

/// Core index from a stack pointer. 0 unless `rsp` is inside an AP block.
pub fn core_of_rsp(rsp: u64, ap_base: u64, n_ap: usize) -> usize {
    if n_ap == 0 || ap_base == 0 || rsp < ap_base {
        return 0;
    }
    let span = (n_ap as u64).saturating_mul(BLOCK_BYTES);
    if rsp >= ap_base.saturating_add(span) {
        return 0;
    }
    let idx = ((rsp - ap_base) / BLOCK_BYTES) as usize;
    if idx >= n_ap {
        0
    } else {
        idx + 1
    }
}

/// Host stack top for `core`. Core 0 keeps the stack it already has.
/// A non-zero core's top is the highest 16-byte-aligned address inside its
/// block, so the pointer is not the next block's base.
pub fn stack_top(core: usize, ap_base: u64, core0_top: u64) -> u64 {
    if core == 0 || ap_base == 0 {
        return core0_top;
    }
    let end = ap_base.wrapping_add((core as u64).wrapping_mul(BLOCK_BYTES));
    end.saturating_sub(16) & !0xF
}

/// xAPIC ID register: the id is bits 31:24.
pub fn xapic_id(reg: u32) -> u32 {
    reg >> 24
}

/// x2APIC IA32_X2APIC_APICID (MSR 0x802): the id is the low 32 bits.
pub fn x2apic_id(msr: u64) -> u32 {
    msr as u32
}

/// Signed difference added to another core's `IA32_TSC_ADJUST` so a later
/// `rdtsc` on that core matches `bsp`. The two arguments are one paired sample.
pub fn tsc_sync_delta(bsp: u64, ap: u64) -> u64 {
    bsp.wrapping_sub(ap)
}

/// vCPU cap. One core online, and `onecore.txt`, keep the B7 cap (`max_vcpus`).
/// More than one core online: `min(max_vcpus, cores)`.
pub fn cpu_cap(max_vcpus: u8, cores_online: usize, onecore_flag: bool) -> u8 {
    if onecore_flag || cores_online <= 1 {
        max_vcpus
    } else {
        let cores = cores_online.min(usize::from(u8::MAX)) as u8;
        max_vcpus.min(cores)
    }
}

/// Fewest units wins. On a tie a non-zero core wins over core 0, then the
/// lowest index.
pub fn pick_core(loads: &[u8], online: usize) -> usize {
    let n = online.min(loads.len());
    if n == 0 {
        return 0;
    }
    let watch = watchdog_core(n);
    let mut best = 0usize;
    for i in 1..n {
        if Some(i) == watch {
            continue;
        }
        if loads[i] < loads[best] || (loads[i] == loads[best] && best == 0) {
            best = i;
        }
    }
    best
}

/// Core a vCPU is placed on when it comes up. vCPU 0 stays on core 0.
/// A later vCPU uses [`pick_core`] once more than one host core is online.
pub fn place_vcpu(vcpu: usize, loads: &[u8], online: usize) -> usize {
    if vcpu == 0 || online <= 1 {
        0
    } else {
        pick_core(loads, online)
    }
}

/// Read MP Services and reserve per-core blocks. Does not start a core.
///
/// # Safety
/// Core 0, before ExitBootServices, before any other core runs. Writes the
/// statics in this module.
#[cfg(feature = "uefi-bin")]
pub unsafe fn probe_pre_ebs() {
    let onecore = crate::boot::raynu_f_flag::host_onecore();
    let x2 = x2apic_enabled();
    X2APIC.store(x2, Ordering::Release);
    let mut infos = [CpuInfo {
        apic_id: 0,
        package: 0,
        core: 0,
        thread: 0,
        enabled: false,
        healthy: false,
        bsp: false,
    }; 128];
    let (logical, enabled_n, ninfo, mp_ok) = read_mp(&mut infos);
    let mut ids = [0u32; MAX_CORES];
    let selected = if mp_ok {
        select_cores(&infos[..ninfo], x2, &mut ids)
    } else {
        0
    };
    let mut packages = 0u32;
    let mut core_keys = 0u32;
    let mut eligible = 0usize;
    let mut seen_pkg = [0xFFFF_FFFFu32; 16];
    let mut seen_core = [(0xFFFF_FFFFu32, 0xFFFF_FFFFu32); 64];
    let mut np = 0usize;
    let mut nc = 0usize;
    for cpu in &infos[..ninfo] {
        if !cpu.enabled || !cpu.healthy {
            continue;
        }
        if !x2 && cpu.apic_id > 254 {
            continue;
        }
        eligible += 1;
        if np < seen_pkg.len() && !seen_pkg[..np].contains(&cpu.package) {
            seen_pkg[np] = cpu.package;
            np += 1;
        }
        let key = (cpu.package, cpu.core);
        if nc < seen_core.len() && !seen_core[..nc].contains(&key) {
            seen_core[nc] = key;
            nc += 1;
        }
    }
    packages = np as u32;
    core_keys = nc as u32;
    let threads_skipped = eligible.saturating_sub(selected);

    let mut use_n = if selected == 0 { 1 } else { selected };
    let mut reason: Option<&str> = None;
    if !mp_ok {
        reason = Some("no MP Services");
        use_n = 1;
    } else if onecore {
        reason = Some("onecore.txt");
        use_n = 1;
    } else if selected <= 1 {
        use_n = 1;
    } else if cpu::read_cr4() & cpu::CR4_LA57 != 0 {
        reason = Some("LA57");
        use_n = 1;
    } else if crate::boot::raynu_f_flag::tsc_hz() == 0 {
        reason = Some("tsc");
        use_n = 1;
    } else if !reserve_blocks(use_n, &ids) {
        reason = Some("memory");
        use_n = 1;
    }
    if let Some(why) = reason {
        crate::boot::serial::write_str("boot: B21 host cores stay 1: ");
        crate::boot::serial::write_str(why);
        crate::boot::serial::write_line(" (ADR-024)");
    }
    if use_n == 1 {
        for id in &APIC_IDS {
            id.store(0, Ordering::Release);
        }
        if mp_ok && selected >= 1 {
            APIC_IDS[0].store(ids[0], Ordering::Release);
        }
        AP_BASE.store(0, Ordering::Release);
        N_AP.store(0, Ordering::Release);
        LOW_BASE.store(0, Ordering::Release);
    }
    USE.store(use_n, Ordering::Release);
    write_summary(logical, enabled_n, core_keys, packages, threads_skipped, x2, use_n);
}

#[cfg(not(feature = "uefi-bin"))]
pub unsafe fn probe_pre_ebs() {}

#[cfg(feature = "uefi-bin")]
fn x2apic_enabled() -> bool {
    const IA32_APIC_BASE: u32 = 0x1B;
    // SAFETY: RDMSR of the architectural APIC base. Core 0, pre-EBS.
    // KANI-TARGET: host_smp x2apic msr
    let base = unsafe { cpu::rdmsr(IA32_APIC_BASE) };
    base & (1 << 10) != 0
}

#[cfg(feature = "uefi-bin")]
fn read_mp(infos: &mut [CpuInfo; 128]) -> (usize, usize, usize, bool) {
    use uefi::boot;
    let Ok(handle) = boot::get_handle_for_protocol::<uefi::proto::pi::mp::MpServices>() else {
        return (0, 0, 0, false);
    };
    let Ok(mp) = (unsafe { boot::open_protocol_exclusive::<uefi::proto::pi::mp::MpServices>(handle) })
    else {
        return (0, 0, 0, false);
    };
    let Ok(count) = mp.get_number_of_processors() else {
        return (0, 0, 0, false);
    };
    let mut n = 0usize;
    let limit = count.total.min(infos.len());
    for i in 0..limit {
        let Ok(pi) = mp.get_processor_info(i) else {
            continue;
        };
        let id = pi.processor_id;
        if id > u64::from(u32::MAX) {
            continue;
        }
        infos[n] = CpuInfo {
            apic_id: id as u32,
            package: pi.location.package,
            core: pi.location.core,
            thread: pi.location.thread,
            enabled: pi.is_enabled(),
            healthy: pi.is_healthy(),
            bsp: pi.is_bsp(),
        };
        n += 1;
    }
    (count.total, count.enabled, n, true)
}

#[cfg(feature = "uefi-bin")]
fn reserve_blocks(use_n: usize, ids: &[u32; MAX_CORES]) -> bool {
    if use_n < 2 {
        return true;
    }
    let Some(low) = alloc_low() else {
        return false;
    };
    let pages = (use_n - 1).saturating_mul(BLOCK_PAGES);
    let Ok(blocks) = (unsafe {
        uefi::boot::allocate_pages(
            uefi::boot::AllocateType::AnyPages,
            uefi::boot::MemoryType::LOADER_DATA,
            pages,
        )
    }) else {
        return false;
    };
    let base = blocks.as_ptr() as u64;
    let bytes = pages.saturating_mul(4096);
    // SAFETY: the firmware just returned `bytes` of LOADER_DATA at `base`.
    // Core 0, no other core exists. KANI-TARGET: host_smp zero blocks
    unsafe {
        core::ptr::write_bytes(base as *mut u8, 0, bytes);
        for i in 0..(use_n - 1) {
            let block = base + (i as u64) * BLOCK_BYTES;
            let canary = (block + 4096) as *mut u8;
            core::ptr::write_bytes(canary, 0x5A, (1024 * 1024 - 4096) as usize);
        }
    }
    LOW_BASE.store(low, Ordering::Release);
    AP_BASE.store(base, Ordering::Release);
    N_AP.store(use_n - 1, Ordering::Release);
    for i in 0..use_n.min(MAX_CORES) {
        APIC_IDS[i].store(ids[i], Ordering::Release);
    }
    true
}

#[cfg(feature = "uefi-bin")]
fn alloc_low() -> Option<u64> {
    use uefi::boot::{self, AllocateType, MemoryType};
    let first = boot::allocate_pages(AllocateType::MaxAddress(0x9_F000), MemoryType::LOADER_DATA, 4)
        .ok()?;
    let addr = first.as_ptr() as u64;
    if addr != 0 {
        return Some(addr);
    }
    // SAFETY: `first` is the 4 pages just allocated. KANI-TARGET: host_smp free low
    let _ = unsafe { boot::free_pages(first, 4) };
    let second =
        boot::allocate_pages(AllocateType::MaxAddress(0x9_F000), MemoryType::LOADER_DATA, 4)
            .ok()?;
    let addr2 = second.as_ptr() as u64;
    if addr2 == 0 {
        let _ = unsafe { boot::free_pages(second, 4) };
        return None;
    }
    Some(addr2)
}

#[cfg(feature = "uefi-bin")]
fn write_summary(
    logical: usize,
    enabled: usize,
    cores: u32,
    packages: u32,
    threads_skipped: usize,
    x2: bool,
    use_n: usize,
) {
    let mut b = [0u8; 160];
    let mut n = 0usize;
    n = push(&mut b, n, b"boot: B21 host cpus logical=");
    n = push_dec(&mut b, n, logical as u64);
    n = push(&mut b, n, b" enabled=");
    n = push_dec(&mut b, n, enabled as u64);
    n = push(&mut b, n, b" cores=");
    n = push_dec(&mut b, n, u64::from(cores));
    n = push(&mut b, n, b" packages=");
    n = push_dec(&mut b, n, u64::from(packages));
    n = push(&mut b, n, b" threads_skipped=");
    n = push_dec(&mut b, n, threads_skipped as u64);
    n = push(&mut b, n, if x2 { b" x2apic=1 use=" } else { b" x2apic=0 use=" });
    n = push_dec(&mut b, n, use_n as u64);
    n = push(&mut b, n, b" (ADR-024)");
    let s = core::str::from_utf8(&b[..n]).unwrap_or("boot: B21 host cpus (ADR-024)");
    crate::boot::serial::write_line(s);
}

fn push(buf: &mut [u8], n: usize, s: &[u8]) -> usize {
    let mut i = n;
    for &c in s {
        if i < buf.len() {
            buf[i] = c;
            i += 1;
        }
    }
    i
}

fn push_dec(buf: &mut [u8], n: usize, mut v: u64) -> usize {
    let mut tmp = [0u8; 20];
    let mut k = tmp.len();
    if v == 0 {
        k -= 1;
        tmp[k] = b'0';
    }
    while v > 0 {
        k -= 1;
        tmp[k] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    push(buf, n, &tmp[k..])
}

const PTE_P: u64 = 1;
const PTE_RW: u64 = 1 << 1;
const PTE_PS: u64 = 1 << 7;
const PTE_ADDR: u64 = 0x000F_FFFF_FFFF_F000;

/// Clone the map of the first 2 MiB and share the rest with core 0.
/// `pd[0]` is an executable identity 2 MiB page so the trampoline can run
/// even when the firmware mapped low memory no-execute.
pub fn build_boot_tables(
    bsp_cr3: u64,
    pdpt_pa: u64,
    pd_pa: u64,
    read: impl Fn(u64) -> [u64; 512],
) -> Option<([u64; 512], [u64; 512], [u64; 512])> {
    let pml4_pa = bsp_cr3 & PTE_ADDR;
    let mut pml4 = read(pml4_pa);
    let pml4_0 = pml4[0];
    if pml4_0 & PTE_P == 0 {
        return None;
    }
    let pdpt_src = pml4_0 & PTE_ADDR;
    let mut pdpt = read(pdpt_src);
    let pdpt_0 = pdpt[0];
    if pdpt_0 & PTE_P == 0 {
        return None;
    }
    let mut pd = if pdpt_0 & PTE_PS != 0 {
        let base = pdpt_0 & 0x000F_FFFF_C000_0000;
        if base != 0 {
            return None;
        }
        let mut page = [0u64; 512];
        for i in 0..512 {
            page[i] = ((i as u64) << 21) | PTE_P | PTE_RW | PTE_PS;
        }
        page
    } else {
        read(pdpt_0 & PTE_ADDR)
    };
    pd[0] = PTE_P | PTE_RW | PTE_PS;
    pml4[0] = (pdpt_pa & PTE_ADDR) | PTE_P | PTE_RW;
    pdpt[0] = (pd_pa & PTE_ADDR) | PTE_P | PTE_RW;
    Some((pml4, pdpt, pd))
}

#[cfg(feature = "uefi-bin")]
#[path = "host_smp_start.rs"]
mod start;

#[cfg(feature = "uefi-bin")]
pub use start::start_cores;

#[cfg(not(feature = "uefi-bin"))]
pub unsafe fn start_cores() {}

#[cfg(test)]
#[path = "host_smp_test.rs"]
mod host_smp_test;
