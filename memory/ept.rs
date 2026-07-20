//! Extended Page Tables (EPT) ownership registry (ADR-004).
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L2 runtime (M2.6); ownership
//! content refined to verified `ept_model` ghost (M3.18) for 4K bring-up
//! map/unmap. Per ADR-004: every valid GPA→HPA mapping is exclusively owned
//! by one guest and belongs to neither the hypervisor nor any other guest.
//!
//! M2.2 tracks **explicitly claimed** 4K guest pages (code/stack).
//! M3.13 adds a durable **range** registry for the precise identity window
//! (`ept_hw::PRECISE_BYTES`) so every mapped GPA span is claimed.

use crate::memory::ept_hw::{self, PRECISE_BYTES};
use crate::memory::PhysFrame;

/// COM1 marker when ADR-004 ownership self-test passes (M2.2 gate).
pub const M2_OWN_OK_MARKER: &str = "RAYNU-V-M2-OWN-OK";

/// Guest id used by the M2/M3 bring-up guest (G0).
pub const M2_BRINGUP_GUEST_ID: u64 = 1;

/// Guest id for the M4.0 second guest (G1) under a private EPT slab.
pub const M4_GUEST1_ID: u64 = 2;
/// M4.2 shell guests G2 / G3.
pub const M4_GUEST2_ID: u64 = 3;
pub const M4_GUEST3_ID: u64 = 4;

/// COM1 marker when both G0 and G1 have latched SHELL under distinct EPT ownership.
pub const M4_2VM_OK_MARKER: &str = "RAYNU-V-M4-2VM-OK";

/// COM1 marker when G1 SHELL CPUID fires (M4.0).
pub const M4_SHELL_G1_MARKER: &str = "RAYNU-V-M4-SHELL-G1";

/// COM1 marker when ≥4 concurrent guests have progressed under the scheduler (M4.2).
pub const M4_NVM_OK_MARKER: &str = "RAYNU-V-M4-NVM-OK";

/// Max tracked 4K mappings in the bring-up registry (M3.8 multi-page bzImage).
/// Under Kani, keep a tiny registry so CBMC can unwind `map` / `owner_of` loops.
#[cfg(not(kani))]
const MAP_CAP: usize = 512;
#[cfg(kani)]
const MAP_CAP: usize = 8;

/// Max identity ranges claimed for the precise EPT (M3.13 / M4.2 multi-hole).
const RANGE_CAP: usize = 16;

/// Set when [`run_ownership_selftest`] succeeds (read on VMEXIT for marker order).
static mut OWNERSHIP_SELFTEST_OK: bool = false;

/// Set when [`claim_precise_identity_ranges`] succeeds (M3.13).
static mut PRECISE_RANGES_OK: bool = false;

/// Durable range registry for the precise EPT identity window.
static mut PRECISE_RANGES: EptRangeMap = EptRangeMap::new();

/// EPT permission bits (subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EptPermissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

impl EptPermissions {
    pub const READ_WRITE: Self = Self {
        read: true,
        write: true,
        execute: false,
    };

    pub const READ_WRITE_EXECUTE: Self = Self {
        read: true,
        write: true,
        execute: true,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EptError {
    /// Frame already mapped by this or another guest (exclusive ownership).
    AlreadyOwned,
    /// No mapping present for unmap.
    NotMapped,
    /// Guest id unknown / invalid.
    InvalidGuest,
    /// Registry full.
    Full,
    /// ADR-004 invariant check failed.
    Invariant,
}

/// Software model of a single GPA→HPA mapping (4K ownership unit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EptMapping {
    pub guest_id: u64,
    pub gpa: u64,
    pub frame: PhysFrame,
    pub permissions: EptPermissions,
}

/// In-memory EPT map registry for exclusive-ownership checks (ADR-004).
///
/// INVARIANTS:
///   - At most one mapping exists per HPA (`PhysFrame`) at any time
///   - At most one mapping exists per `(guest_id, gpa)`
///   - Mapped frames are guest-owned (HV must not alias — L1 assert via
///     [`EptMap::check_invariants`])
///
/// VERIFICATION: L1 — see ept_spec.rs
pub struct EptMap {
    mappings: [Option<EptMapping>; MAP_CAP],
    len: usize,
}

impl EptMap {
    pub const fn new() -> Self {
        Self {
            mappings: [None; MAP_CAP],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Map GPA → HPA for a guest.
    ///
    /// INVARIANTS (ADR-004):
    ///   - Frame not already exclusively owned by any guest
    ///   - After Ok, frame is exclusively owned by `guest_id`
    ///
    /// VERIFICATION: L1
    pub fn map(
        &mut self,
        guest_id: u64,
        gpa: u64,
        frame: PhysFrame,
        permissions: EptPermissions,
    ) -> Result<(), EptError> {
        if guest_id == 0 {
            return Err(EptError::InvalidGuest);
        }
        let gpa = gpa & !0xfff;
        for m in self.mappings.iter().flatten() {
            if m.frame == frame {
                return Err(EptError::AlreadyOwned);
            }
            if m.guest_id == guest_id && m.gpa == gpa {
                return Err(EptError::AlreadyOwned);
            }
        }
        for slot in self.mappings.iter_mut() {
            if slot.is_none() {
                *slot = Some(EptMapping {
                    guest_id,
                    gpa,
                    frame,
                    permissions,
                });
                self.len += 1;
                debug_assert!(self.check_invariants());
                return Ok(());
            }
        }
        Err(EptError::Full)
    }

    /// Unmap a guest GPA.
    ///
    /// INVARIANTS:
    ///   - Mapping existed for (guest_id, gpa)
    ///   - After Ok, frame is no longer owned via this map
    ///
    /// VERIFICATION: L1
    pub fn unmap(&mut self, guest_id: u64, gpa: u64) -> Result<PhysFrame, EptError> {
        let gpa = gpa & !0xfff;
        for slot in self.mappings.iter_mut() {
            if let Some(m) = slot {
                if m.guest_id == guest_id && m.gpa == gpa {
                    let frame = m.frame;
                    *slot = None;
                    self.len -= 1;
                    // M5.3: record unmap on firmware path; host unit tests skip
                    // to avoid flooding the shared boot ring.
                    #[cfg(not(test))]
                    crate::audit_log!(crate::audit::AuditEvent::EptUnmapped {
                        guest_id,
                        gpa,
                        hpa: frame.0,
                    });
                    debug_assert!(self.check_invariants());
                    return Ok(frame);
                }
            }
        }
        Err(EptError::NotMapped)
    }

    pub fn owner_of(&self, frame: PhysFrame) -> Option<u64> {
        self.mappings
            .iter()
            .flatten()
            .find(|m| m.frame == frame)
            .map(|m| m.guest_id)
    }

    pub fn owner_of_gpa(&self, guest_id: u64, gpa: u64) -> Option<PhysFrame> {
        let gpa = gpa & !0xfff;
        self.mappings
            .iter()
            .flatten()
            .find(|m| m.guest_id == guest_id && m.gpa == gpa)
            .map(|m| m.frame)
    }

    /// ADR-004 structural check: unique HPA, unique (guest,gpa), len matches.
    pub fn check_invariants(&self) -> bool {
        let mut n = 0usize;
        for (i, a) in self.mappings.iter().enumerate() {
            let Some(ma) = a else { continue };
            n += 1;
            for (j, b) in self.mappings.iter().enumerate() {
                if i == j {
                    continue;
                }
                let Some(mb) = b else { continue };
                if ma.frame == mb.frame {
                    return false;
                }
                if ma.guest_id == mb.guest_id && ma.gpa == mb.gpa {
                    return false;
                }
            }
        }
        n == self.len
    }
}

impl Default for EptMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Identity GPA span claimed for a guest (ADR-004 range unit, M3.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EptRange {
    pub guest_id: u64,
    pub gpa: u64,
    pub len: u64,
}

/// Small registry of exclusive identity ranges (precise EPT windows).
pub struct EptRangeMap {
    ranges: [Option<EptRange>; RANGE_CAP],
    len: usize,
}

impl EptRangeMap {
    pub const fn new() -> Self {
        Self {
            ranges: [None; RANGE_CAP],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Claim an identity span `[gpa, gpa+len)`. Rejects overlap / bad args.
    pub fn claim_range(&mut self, guest_id: u64, gpa: u64, len: u64) -> Result<(), EptError> {
        if guest_id == 0 {
            return Err(EptError::InvalidGuest);
        }
        if len == 0 || (gpa & 0xfff) != 0 || (len & 0xfff) != 0 {
            return Err(EptError::Invariant);
        }
        let end = gpa.checked_add(len).ok_or(EptError::Invariant)?;
        for r in self.ranges.iter().flatten() {
            let r_end = r.gpa.wrapping_add(r.len);
            let overlap = gpa < r_end && r.gpa < end;
            if overlap {
                return Err(EptError::AlreadyOwned);
            }
        }
        for slot in self.ranges.iter_mut() {
            if slot.is_none() {
                *slot = Some(EptRange {
                    guest_id,
                    gpa,
                    len,
                });
                self.len += 1;
                return Ok(());
            }
        }
        Err(EptError::Full)
    }

    pub fn contains_gpa(&self, guest_id: u64, gpa: u64) -> bool {
        self.ranges.iter().flatten().any(|r| {
            r.guest_id == guest_id && gpa >= r.gpa && gpa < r.gpa.wrapping_add(r.len)
        })
    }
}

/// Claim the M3.13 precise identity window for the bring-up guest.
pub fn claim_precise_identity_ranges() -> Result<(), EptError> {
    claim_precise_with_guest1_hole(0, 0)
}

/// Claim precise identity for G0, optionally reserving a private HPA slab for G1.
///
/// When `g1_len == 0`, claims `[0, PRECISE_BYTES)` for [`M2_BRINGUP_GUEST_ID`]
/// (M3.13/M3.20 behavior). When `g1_len > 0`, punches that HPA span out of G0
/// and claims it for [`M4_GUEST1_ID`] (M4.0).
pub fn claim_precise_with_guest1_hole(g1_hpa: u64, g1_len: u64) -> Result<(), EptError> {
    if g1_len == 0 {
        claim_precise_with_shell_holes(&[])
    } else {
        claim_precise_with_shell_holes(&[(g1_hpa, g1_len, M4_GUEST1_ID)])
    }
}

/// Claim precise identity for G0 with zero or more private shell-guest HPA slabs.
///
/// Each hole is `(hpa, len, guest_id)`. Holes must be page-aligned, non-overlapping,
/// inside `[0, PRECISE_BYTES)`, and above G0 guest e820 (when non-empty).
pub fn claim_precise_with_shell_holes(holes: &[(u64, u64, u64)]) -> Result<(), EptError> {
    // SAFETY: single-threaded boot; called once before VMLAUNCH.
    unsafe {
        let ranges = core::ptr::addr_of_mut!(PRECISE_RANGES);
        *ranges = EptRangeMap::new();

        // Insertion-sort a tiny stack copy by HPA (≤3 shell holes for M4.2).
        let mut ordered: [(u64, u64, u64); 8] = [(0, 0, 0); 8];
        if holes.len() > ordered.len() {
            return Err(EptError::Full);
        }
        for (i, h) in holes.iter().enumerate() {
            ordered[i] = *h;
        }
        let n = holes.len();
        for i in 1..n {
            let mut j = i;
            while j > 0 && ordered[j].0 < ordered[j - 1].0 {
                ordered.swap(j, j - 1);
                j -= 1;
            }
        }

        let guest_ram = crate::guest::linux_boot::GUEST_RAM_BYTES;
        if guest_ram > PRECISE_BYTES {
            return Err(EptError::Invariant);
        }

        let mut cursor = 0u64;
        for &(hpa, len, gid) in ordered.iter().take(n) {
            if len == 0 || (hpa & 0xfff) != 0 || (len & 0xfff) != 0 {
                return Err(EptError::Invariant);
            }
            if gid == 0 || gid == M2_BRINGUP_GUEST_ID {
                return Err(EptError::InvalidGuest);
            }
            let end = hpa.checked_add(len).ok_or(EptError::Invariant)?;
            if end > PRECISE_BYTES || hpa < cursor {
                return Err(EptError::Invariant);
            }
            if hpa < guest_ram {
                return Err(EptError::Invariant);
            }
            if hpa > cursor {
                (*ranges).claim_range(M2_BRINGUP_GUEST_ID, cursor, hpa - cursor)?;
            }
            (*ranges).claim_range(gid, hpa, len)?;
            cursor = end;
        }
        if cursor < PRECISE_BYTES {
            (*ranges).claim_range(M2_BRINGUP_GUEST_ID, cursor, PRECISE_BYTES - cursor)?;
        } else if n == 0 {
            (*ranges).claim_range(M2_BRINGUP_GUEST_ID, 0, PRECISE_BYTES)?;
        }

        if !(*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, 0)
            || !(*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, guest_ram - 0x1000)
        {
            return Err(EptError::Invariant);
        }
        for &(hpa, _, gid) in ordered.iter().take(n) {
            if !(*ranges).contains_gpa(gid, hpa) {
                return Err(EptError::Invariant);
            }
            if (*ranges).contains_gpa(M2_BRINGUP_GUEST_ID, hpa) {
                return Err(EptError::Invariant);
            }
        }
        // APIC MMIO must remain outside claimed/mapped precise window.
        if ept_hw::PRECISE_BYTES > crate::arch::apic::DEFAULT_APIC_PHYS {
            return Err(EptError::Invariant);
        }
        // M3.20: live precise window must stay strictly below 1 GiB.
        if ept_hw::PRECISE_BYTES >= (1 << 30) {
            return Err(EptError::Invariant);
        }
        PRECISE_RANGES_OK = true;
    }
    Ok(())
}

/// True after [`claim_precise_identity_ranges`] on this boot.
pub fn precise_ranges_ok() -> bool {
    // SAFETY: written once on BSP before VMLAUNCH.
    unsafe { PRECISE_RANGES_OK }
}

/// Claim bring-up guest pages and prove exclusive ownership (ADR-004).
///
/// Registers `code_phys`, `stack_phys`, and `idt_phys` for
/// [`M2_BRINGUP_GUEST_ID`], then asserts that a second guest cannot alias
/// the same HPA.
///
/// Returns `Ok(())` and sets the VMEXIT marker latch on success.
pub fn run_ownership_selftest(
    code_phys: u64,
    stack_phys: u64,
    idt_phys: u64,
) -> Result<(), EptError> {
    let mut map = EptMap::new();
    let code = PhysFrame::from_phys(code_phys);
    let stack = PhysFrame::from_phys(stack_phys);
    let idt = PhysFrame::from_phys(idt_phys);

    map.map(
        M2_BRINGUP_GUEST_ID,
        code_phys,
        code,
        EptPermissions::READ_WRITE_EXECUTE,
    )?;
    map.map(
        M2_BRINGUP_GUEST_ID,
        stack_phys,
        stack,
        EptPermissions::READ_WRITE,
    )?;
    map.map(
        M2_BRINGUP_GUEST_ID,
        idt_phys,
        idt,
        EptPermissions::READ_WRITE,
    )?;

    // ADR-004: another guest must not obtain the same HPA.
    match map.map(2, 0x9000, code, EptPermissions::READ_WRITE) {
        Err(EptError::AlreadyOwned) => {}
        Ok(()) => return Err(EptError::Invariant),
        Err(e) => return Err(e),
    }

    if map.owner_of(code) != Some(M2_BRINGUP_GUEST_ID) {
        return Err(EptError::Invariant);
    }
    if map.owner_of(stack) != Some(M2_BRINGUP_GUEST_ID) {
        return Err(EptError::Invariant);
    }
    if map.owner_of(idt) != Some(M2_BRINGUP_GUEST_ID) {
        return Err(EptError::Invariant);
    }
    if !map.check_invariants() || map.len() != 3 {
        return Err(EptError::Invariant);
    }

    // Unmap + re-claim proves release restores availability.
    let freed = map.unmap(M2_BRINGUP_GUEST_ID, stack_phys)?;
    if freed != stack {
        return Err(EptError::Invariant);
    }
    map.map(
        M2_BRINGUP_GUEST_ID,
        stack_phys,
        stack,
        EptPermissions::READ_WRITE,
    )?;

    if !map.check_invariants() {
        return Err(EptError::Invariant);
    }

    // SAFETY: single-threaded boot path; latch read on VMEXIT.
    unsafe {
        OWNERSHIP_SELFTEST_OK = true;
    }
    Ok(())
}

/// True after a successful [`run_ownership_selftest`] on this boot.
pub fn ownership_selftest_ok() -> bool {
    // SAFETY: written once on BSP before VMLAUNCH; read after VMEXIT.
    unsafe { OWNERSHIP_SELFTEST_OK }
}

/// Host-visible EPT-violation disposition (mirrors `ept_model::EptViolationDisposition`).
///
/// Live `handle_ept_violation_and_resume` today uses EmulateNoMap (APIC/virtio)
/// or Reject (unexpected GPA). ClaimMap covers demand-fill under exclusivity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationDisposition {
    EmulateNoMap,
    Reject,
    ClaimMap {
        guest_id: u64,
        gpa: u64,
        frame: PhysFrame,
        permissions: EptPermissions,
    },
}

/// Apply a violation disposition to the ownership registry (ADR-004).
///
/// Emulate/Reject leave `map` unchanged. ClaimMap is a normal exclusive map.
pub fn apply_violation_disposition(
    map: &mut EptMap,
    d: ViolationDisposition,
) -> Result<(), EptError> {
    match d {
        ViolationDisposition::EmulateNoMap | ViolationDisposition::Reject => Ok(()),
        ViolationDisposition::ClaimMap {
            guest_id,
            gpa,
            frame,
            permissions,
        } => map.map(guest_id, gpa, frame, permissions),
    }
}

/// M6.3: hand off an HPA frame from `src_guest`@`src_gpa` to `dst_guest`@`dst_gpa`.
///
/// Unmap at source then map the same frame at destination. Fail-closed if the
/// destination map cannot install (restores source mapping when possible).
pub fn transfer_page(
    map: &mut EptMap,
    src_guest: u64,
    src_gpa: u64,
    dst_guest: u64,
    dst_gpa: u64,
    permissions: EptPermissions,
) -> Result<PhysFrame, EptError> {
    if src_guest == 0 || dst_guest == 0 || src_guest == dst_guest {
        return Err(EptError::InvalidGuest);
    }
    let src_gpa = src_gpa & !0xfff;
    let dst_gpa = dst_gpa & !0xfff;
    // Destination GPA must be free before we unmap.
    if map.owner_of_gpa(dst_guest, dst_gpa).is_some() {
        return Err(EptError::AlreadyOwned);
    }
    let frame = map.unmap(src_guest, src_gpa)?;
    match map.map(dst_guest, dst_gpa, frame, permissions) {
        Ok(()) => Ok(frame),
        Err(e) => {
            // Best-effort restore so ownership is not dropped on map failure.
            let _ = map.map(src_guest, src_gpa, frame, permissions);
            Err(e)
        }
    }
}

/// Host-testable: page transfer preserves exclusivity; steal rejected.
pub fn prop_page_transfer_preserves_exclusive() -> bool {
    let mut map = EptMap::new();
    let g0 = M2_BRINGUP_GUEST_ID;
    let g1 = M4_GUEST1_ID;
    let f0 = PhysFrame(70);
    let f1 = PhysFrame(71);

    if map
        .map(g0, 0, f0, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    if map.owner_of(f0) != Some(g0) || !map.check_invariants() {
        return false;
    }

    match transfer_page(
        &mut map,
        g0,
        0,
        g1,
        0x1000,
        EptPermissions::READ_WRITE,
    ) {
        Ok(f) if f == f0 => {}
        _ => return false,
    }
    if map.owner_of(f0) != Some(g1) || !map.check_invariants() {
        return false;
    }
    // Source mapping gone.
    if map.unmap(g0, 0).is_ok() {
        return false;
    }
    // Steal of transferred frame rejected.
    if !matches!(
        map.map(g0, 0x2000, f0, EptPermissions::READ_WRITE),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }
    // Distinct frame still maps for source guest.
    if map
        .map(g0, 0x2000, f1, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    map.check_invariants()
        && map.owner_of(f0) == Some(g1)
        && map.owner_of(f1) == Some(g0)
        && map.len() == 2
}

/// Host-testable: emulate/reject/claim preserve exclusivity; steal rejected.
pub fn prop_violation_preserves_exclusive() -> bool {
    let mut map = EptMap::new();
    let g0 = M2_BRINGUP_GUEST_ID;
    let g1 = M4_GUEST1_ID;
    let gpa = 0x1000u64;
    let f0 = PhysFrame(60);
    let f1 = PhysFrame(61);

    if apply_violation_disposition(&mut map, ViolationDisposition::EmulateNoMap).is_err() {
        return false;
    }
    if !map.is_empty() || !map.check_invariants() {
        return false;
    }
    if apply_violation_disposition(&mut map, ViolationDisposition::Reject).is_err() {
        return false;
    }
    if !map.is_empty() || !map.check_invariants() {
        return false;
    }

    if apply_violation_disposition(
        &mut map,
        ViolationDisposition::ClaimMap {
            guest_id: g0,
            gpa,
            frame: f0,
            permissions: EptPermissions::READ_WRITE,
        },
    )
    .is_err()
    {
        return false;
    }
    if map.owner_of(f0) != Some(g0) || !map.check_invariants() {
        return false;
    }

    // Steal of claimed frame on a second violation must fail (AlreadyOwned).
    if !matches!(
        apply_violation_disposition(
            &mut map,
            ViolationDisposition::ClaimMap {
                guest_id: g1,
                gpa: 0x2000,
                frame: f0,
                permissions: EptPermissions::READ_WRITE,
            },
        ),
        Err(EptError::AlreadyOwned)
    ) {
        return false;
    }

    // Distinct frame claim for another guest succeeds.
    if apply_violation_disposition(
        &mut map,
        ViolationDisposition::ClaimMap {
            guest_id: g1,
            gpa: 0x2000,
            frame: f1,
            permissions: EptPermissions::READ_WRITE,
        },
    )
    .is_err()
    {
        return false;
    }
    map.check_invariants()
        && map.owner_of(f0) == Some(g0)
        && map.owner_of(f1) == Some(g1)
        && map.len() == 2
}

#[cfg(test)]
#[path = "ept_test.rs"]
mod ept_test;
