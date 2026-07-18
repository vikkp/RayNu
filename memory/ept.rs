//! Extended Page Tables (EPT) engine stub.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0
//! Per ADR-004: every valid GPA→HPA mapping is exclusively owned by one guest
//! and belongs to neither the hypervisor nor any other guest.

use crate::memory::PhysFrame;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EptError {
    /// Frame already mapped by this or another guest (exclusive ownership).
    AlreadyOwned,
    /// No mapping present for unmap.
    NotMapped,
    /// Guest id unknown / invalid.
    InvalidGuest,
}

/// Software model of a single GPA→HPA mapping (4K only in scaffold).
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
///   - At most one mapping exists per HPA (PhysFrame) at any time
///   - At most one mapping exists per (guest_id, gpa)
///   - Mapped frames are treated as guest-owned (HV must not alias — enforced later)
///
/// VERIFICATION: L0 — see ept_spec.rs
pub struct EptMap {
    mappings: [Option<EptMapping>; 16],
}

impl EptMap {
    pub const fn new() -> Self {
        Self {
            mappings: [None; 16],
        }
    }

    /// Map GPA → HPA for a guest.
    ///
    /// INVARIANTS (ADR-004):
    ///   - Frame not already exclusively owned by any guest
    ///   - After Ok, frame is exclusively owned by `guest_id`
    ///
    /// VERIFICATION: L0
    pub fn map(
        &mut self,
        guest_id: u64,
        gpa: u64,
        frame: PhysFrame,
        permissions: EptPermissions,
    ) -> Result<(), EptError> {
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
                return Ok(());
            }
        }
        Err(EptError::AlreadyOwned)
    }

    /// Unmap a guest GPA.
    ///
    /// INVARIANTS:
    ///   - Mapping existed for (guest_id, gpa)
    ///   - After Ok, frame is no longer owned via this map
    ///
    /// VERIFICATION: L0
    pub fn unmap(&mut self, guest_id: u64, gpa: u64) -> Result<PhysFrame, EptError> {
        for slot in self.mappings.iter_mut() {
            if let Some(m) = slot {
                if m.guest_id == guest_id && m.gpa == gpa {
                    let frame = m.frame;
                    *slot = None;
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
}

impl Default for EptMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "ept_test.rs"]
mod ept_test;
