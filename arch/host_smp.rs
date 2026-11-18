//! Host cores for B21 (ADR-024).
//!
//! Pillar: [D] [Z]
//! Proven Core: **outside**.
//!
//! Topology is read from EFI MP Services before ExitBootServices. No core is
//! started here. Core 0 writes the statics below once, before any other core
//! exists. Later cores read them.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

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
    let mut best = 0usize;
    for i in 1..n {
        if loads[i] < loads[best] || (loads[i] == loads[best] && best == 0) {
            best = i;
        }
    }
    best
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
