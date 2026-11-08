//! ADR-021 B6. One extra VMCS while a second product guest is seated.
//!
//! The page tables map a supervisor page whose first byte is `HLT`.
//! Entering that VMCS and coming back does not mean the seated guest ran.
//! [`crate::mgmt::product_pair`] stays at zero for the companion until that
//! guest itself is entered. A failed entry is permanent. An exit that stole
//! the instruction (external interrupt, interrupt window, preemption) can
//! be tried again, up to [`MAX_TRIES`].
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-021).
//! VERIFICATION: L1 (host tests). The VM-entry instruction is uefi-only.

use core::sync::atomic::{AtomicU8, Ordering};

/// Guest-physical page that holds `HLT`.
pub const CODE_GPA: u64 = 0x1000;
pub const PML4_GPA: u64 = 0x2000;
pub const PDPT_GPA: u64 = 0x3000;
pub const PD_GPA: u64 = 0x4000;
pub const PT_GPA: u64 = 0x5000;
pub const STACK_GPA: u64 = 0x6000;
pub const HLT_OPCODE: u8 = 0xF4;

/// Present, writable, accessed. Supervisor. No NX.
const GUEST_LINK: u64 = 0x23;
/// Present, writable, accessed, dirty. Supervisor. No NX.
const GUEST_LEAF: u64 = 0x63;

const IDLE: u8 = 0;
const IN_FLIGHT: u8 = 1;
const RETURNED: u8 = 2;
const FAILED: u8 = 3;

/// Attempts before a stolen exit is left alone.
pub const MAX_TRIES: u8 = 4;

static PROBE: AtomicU8 = AtomicU8::new(IDLE);
static TRIES: AtomicU8 = AtomicU8::new(0);

pub fn clear_for_test() {
    PROBE.store(IDLE, Ordering::Release);
    TRIES.store(0, Ordering::Release);
}

pub fn probe_in_flight() -> bool {
    PROBE.load(Ordering::Acquire) == IN_FLIGHT
}

pub fn probe_returned() -> bool {
    PROBE.load(Ordering::Acquire) == RETURNED
}

/// Start the one entry. False when one is in flight, already returned, failed,
/// or the retry budget is spent.
pub fn begin_probe() -> bool {
    if PROBE.load(Ordering::Acquire) != IDLE {
        return false;
    }
    if TRIES.load(Ordering::Acquire) >= MAX_TRIES {
        return false;
    }
    if PROBE
        .compare_exchange(IDLE, IN_FLIGHT, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    TRIES.fetch_add(1, Ordering::AcqRel);
    true
}

pub fn note_probe_hlt() {
    let _ = PROBE.compare_exchange(IN_FLIGHT, RETURNED, Ordering::AcqRel, Ordering::Acquire);
}

/// External interrupt, interrupt-window, or preemption stole the `HLT`.
pub fn note_probe_retry() {
    let _ = PROBE.compare_exchange(IN_FLIGHT, IDLE, Ordering::AcqRel, Ordering::Acquire);
}

pub fn note_probe_failed() {
    PROBE.store(FAILED, Ordering::Release);
}

pub fn probe_exit_is_retry(basic: u32) -> bool {
    matches!(basic, 1 | 7 | 52)
}

/// HLT must exit. An interrupt-window or NMI-window exit would steal the instruction.
pub fn primary_controls(cloned: u32) -> u32 {
    const HLT_EXITING: u32 = 1 << 7;
    const INTERRUPT_WINDOW: u32 = 1 << 2;
    const NMI_WINDOW: u32 = 1 << 22;
    (cloned | HLT_EXITING) & !INTERRUPT_WINDOW & !NMI_WINDOW
}

/// Pages for one companion VMCS, its EPT, and a long-mode guest walk.
#[repr(C, align(4096))]
pub struct CompanionPages {
    pub vmcs: [u8; 4096],
    pub ept_pml4: [u8; 4096],
    pub ept_pdpt: [u8; 4096],
    pub ept_pd: [u8; 4096],
    pub ept_pt: [u8; 4096],
    pub guest_pml4: [u8; 4096],
    pub guest_pdpt: [u8; 4096],
    pub guest_pd: [u8; 4096],
    pub guest_pt: [u8; 4096],
    pub code: [u8; 4096],
    pub stack: [u8; 4096],
}

impl CompanionPages {
    pub const ZERO: Self = Self {
        vmcs: [0; 4096],
        ept_pml4: [0; 4096],
        ept_pdpt: [0; 4096],
        ept_pd: [0; 4096],
        ept_pt: [0; 4096],
        guest_pml4: [0; 4096],
        guest_pdpt: [0; 4096],
        guest_pd: [0; 4096],
        guest_pt: [0; 4096],
        code: [0; 4096],
        stack: [0; 4096],
    };
}

pub struct CompanionPlan {
    pub vmcs: u64,
    pub eptp: u64,
    pub guest_cr3: u64,
    pub rip: u64,
    pub rsp: u64,
}

struct Hold {
    gpr: [u64; 15],
    xmm: [u8; 256],
    ubuntu: u64,
    cr2: u64,
}

static mut HOLD: Hold = Hold {
    gpr: [0; 15],
    xmm: [0; 256],
    ubuntu: 0,
    cr2: 0,
};

/// Remember the running guest's registers before a second VMCS overwrites them.
/// RSP stays in the VMCS. The fifteen slots are RAX, RBX, RCX, RDX, RSI, RDI,
/// RBP, then R8 through R15.
///
/// SAFETY: VMX-root, single threaded.
pub unsafe fn hold_guest(gpr: [u64; 15], xmm: [u8; 256], ubuntu: u64, cr2: u64) {
    let hold = &mut *core::ptr::addr_of_mut!(HOLD);
    hold.gpr = gpr;
    hold.xmm = xmm;
    hold.ubuntu = ubuntu;
    hold.cr2 = cr2;
}

/// SAFETY: VMX-root, single threaded. Valid after [`hold_guest`].
pub unsafe fn held_guest() -> ([u64; 15], [u8; 256], u64, u64) {
    let hold = &*core::ptr::addr_of!(HOLD);
    (hold.gpr, hold.xmm, hold.ubuntu, hold.cr2)
}

fn page_addr(page: &[u8; 4096]) -> u64 {
    page.as_ptr() as u64
}

fn write_pte(page: &mut [u8; 4096], index: usize, val: u64) {
    let off = index * 8;
    page[off..off + 8].copy_from_slice(&val.to_le_bytes());
}

fn read_pte(page: &[u8; 4096], index: usize) -> u64 {
    let off = index * 8;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&page[off..off + 8]);
    u64::from_le_bytes(buf)
}

fn ept_link(hpa: u64) -> u64 {
    0b111 | (hpa & !0xfff)
}

fn ept_leaf(hpa: u64) -> u64 {
    // R/W/X, WB memory type in bits 5:3.
    0b111 | (6 << 3) | (hpa & !0xfff)
}

fn pack_eptp(pml4: u64) -> u64 {
    // WB, 4-level walk.
    6 | (3 << 3) | (pml4 & !0xfff)
}

/// Fill EPT and guest page tables. Addresses are the page pointers.
pub fn program_tables(pages: &mut CompanionPages) -> CompanionPlan {
    pages.ept_pml4.fill(0);
    pages.ept_pdpt.fill(0);
    pages.ept_pd.fill(0);
    pages.ept_pt.fill(0);
    pages.guest_pml4.fill(0);
    pages.guest_pdpt.fill(0);
    pages.guest_pd.fill(0);
    pages.guest_pt.fill(0);
    pages.code.fill(0);
    pages.stack.fill(0);
    pages.code[0] = HLT_OPCODE;

    let ept_pml4 = page_addr(&pages.ept_pml4);
    let ept_pdpt = page_addr(&pages.ept_pdpt);
    let ept_pd = page_addr(&pages.ept_pd);
    let ept_pt = page_addr(&pages.ept_pt);
    let guest_pml4 = page_addr(&pages.guest_pml4);
    let guest_pdpt = page_addr(&pages.guest_pdpt);
    let guest_pd = page_addr(&pages.guest_pd);
    let guest_pt = page_addr(&pages.guest_pt);
    let code = page_addr(&pages.code);
    let stack = page_addr(&pages.stack);
    let vmcs = page_addr(&pages.vmcs);

    write_pte(&mut pages.ept_pml4, 0, ept_link(ept_pdpt));
    write_pte(&mut pages.ept_pdpt, 0, ept_link(ept_pd));
    write_pte(&mut pages.ept_pd, 0, ept_link(ept_pt));
    let ept_map = [
        (CODE_GPA, code),
        (PML4_GPA, guest_pml4),
        (PDPT_GPA, guest_pdpt),
        (PD_GPA, guest_pd),
        (PT_GPA, guest_pt),
        (STACK_GPA, stack),
    ];
    for (gpa, hpa) in ept_map {
        let idx = ((gpa >> 12) & 511) as usize;
        write_pte(&mut pages.ept_pt, idx, ept_leaf(hpa));
    }

    write_pte(&mut pages.guest_pml4, 0, guest_link(PDPT_GPA));
    write_pte(&mut pages.guest_pdpt, 0, guest_link(PD_GPA));
    write_pte(&mut pages.guest_pd, 0, guest_link(PT_GPA));
    write_pte(
        &mut pages.guest_pt,
        (CODE_GPA >> 12) as usize,
        CODE_GPA | GUEST_LEAF,
    );
    write_pte(
        &mut pages.guest_pt,
        (STACK_GPA >> 12) as usize,
        STACK_GPA | GUEST_LEAF,
    );

    CompanionPlan {
        vmcs,
        eptp: pack_eptp(ept_pml4),
        guest_cr3: PML4_GPA,
        rip: CODE_GPA,
        rsp: STACK_GPA + 0xFF0,
    }
}

fn guest_link(gpa: u64) -> u64 {
    gpa | GUEST_LINK
}

/// Host-physical address of `gpa` through the EPT just programmed.
pub fn ept_hpa(pages: &CompanionPages, gpa: u64) -> Option<u64> {
    let link_ok = |pte: u64| pte & 0b111 == 0b111;
    if !link_ok(read_pte(&pages.ept_pml4, 0))
        || !link_ok(read_pte(&pages.ept_pdpt, 0))
        || !link_ok(read_pte(&pages.ept_pd, 0))
    {
        return None;
    }
    let idx = ((gpa >> 12) & 511) as usize;
    let leaf = read_pte(&pages.ept_pt, idx);
    if !link_ok(leaf) {
        return None;
    }
    Some(leaf & !0xfff)
}

/// Guest-physical address a long-mode walk of `va` produces.
pub fn guest_leaf_gpa(pages: &CompanionPages, va: u64) -> Option<u64> {
    let present = |pte: u64| pte & 1 == 1;
    if !present(read_pte(&pages.guest_pml4, 0))
        || !present(read_pte(&pages.guest_pdpt, 0))
        || !present(read_pte(&pages.guest_pd, 0))
    {
        return None;
    }
    let idx = ((va >> 12) & 511) as usize;
    let leaf = read_pte(&pages.guest_pt, idx);
    if !present(leaf) || (leaf & 4) != 0 {
        return None;
    }
    Some(leaf & !0xfff)
}

const _: () = assert!(crate::vmx::fields::VMCS_CLONE_FIELDS.len() <= 128);

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn hlt_page_is_supervisor_and_reachable() {
        let mut pages = Box::new(CompanionPages::ZERO);
        let plan = program_tables(&mut pages);
        assert_eq!(plan.rip, CODE_GPA);
        assert_eq!(plan.guest_cr3, PML4_GPA);
        assert_eq!(plan.rsp, STACK_GPA + 0xFF0);
        assert_eq!(plan.vmcs & 0xfff, 0);
        assert_eq!(plan.eptp & 0xfff, 0x1E);
        assert_eq!(pages.code[0], HLT_OPCODE);
        assert_eq!(ept_hpa(&pages, CODE_GPA), Some(page_addr(&pages.code) & !0xfff));
        assert_eq!(ept_hpa(&pages, STACK_GPA), Some(page_addr(&pages.stack) & !0xfff));
        assert_eq!(guest_leaf_gpa(&pages, CODE_GPA), Some(CODE_GPA));
        assert_eq!(guest_leaf_gpa(&pages, STACK_GPA), Some(STACK_GPA));
        assert!(guest_leaf_gpa(&pages, 0).is_none());
    }

    #[test]
    fn a_stolen_exit_retries_and_a_failure_does_not() {
        let _guard = lock();
        clear_for_test();
        assert!(begin_probe());
        assert!(!begin_probe());
        assert!(probe_exit_is_retry(1));
        assert!(probe_exit_is_retry(7));
        assert!(probe_exit_is_retry(52));
        assert!(!probe_exit_is_retry(12));
        assert!(!probe_exit_is_retry(48));
        note_probe_retry();
        assert!(begin_probe());
        note_probe_hlt();
        assert!(probe_returned());
        assert!(!begin_probe());
        clear_for_test();
        assert!(begin_probe());
        note_probe_failed();
        assert!(!begin_probe());
        assert_eq!(primary_controls(0), 1 << 7);
        assert_eq!(primary_controls((1 << 2) | (1 << 22) | 0x100), (1 << 7) | 0x100);
        clear_for_test();
    }

    #[test]
    fn retry_budget_stops() {
        let _guard = lock();
        clear_for_test();
        for _ in 0..MAX_TRIES {
            assert!(begin_probe());
            note_probe_retry();
        }
        assert!(!begin_probe());
        clear_for_test();
    }
}
