//! Extended Page Tables (EPT) ownership registry (ADR-004).
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L2 (spec M2.6) + L1 runtime
//! Per ADR-004: every valid GPA→HPA mapping is exclusively owned by one guest
//! and belongs to neither the hypervisor nor any other guest.
//!
//! M2.2 tracks **explicitly claimed** guest pages (code/stack). The 4 GiB
//! identity EPT in `ept_hw` remains a bring-up scaffold; precise per-GPA maps
//! replace it later while this registry stays the ownership source of truth.

use crate::memory::PhysFrame;

/// COM1 marker when ADR-004 ownership self-test passes (M2.2 gate).
pub const M2_OWN_OK_MARKER: &str = "RAYNU-V-M2-OWN-OK";

/// Guest id used by the M2 bring-up single guest.
pub const M2_BRINGUP_GUEST_ID: u64 = 1;

/// Max tracked mappings in the bring-up registry (M3.7 multi-page loads).
const MAP_CAP: usize = 128;

/// Set when [`run_ownership_selftest`] succeeds (read on VMEXIT for marker order).
static mut OWNERSHIP_SELFTEST_OK: bool = false;

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

#[cfg(test)]
#[path = "ept_test.rs"]
mod ept_test;
