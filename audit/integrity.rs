//! Audit log integrity (ring + hash chain).
//!
//! Pillar: [A] [V] · Proven Core · VERIFICATION: L0
//! Tampered audit log collapses the [A] pillar (ADR-002).

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Host / CI marker when the M5.3 audit integrity gate passes.
pub const M5_AUDIT_OK_MARKER: &str = "RAYNU-V-M5-AUDIT-OK";

/// Genesis previous-hash for an empty chain ("RAYNU-V0" marker).
pub const AUDIT_GENESIS_HASH: u64 = 0x5241_594E_552D_5630;

/// Fixed slot count for the append-only ring (host suite + firmware).
pub const AUDIT_RING_CAP: usize = 256;

/// Milestone tag for boot / gate events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    M0,
    M1,
    M2,
    M3,
    M4,
    M5,
    M55,
    M6,
}

/// Security-relevant events that MUST be audited (CLAUDE.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEvent {
    BootStarted { milestone: Milestone },
    VmxEnabled { vcpu_id: u32 },
    VmcsCreated { vcpu_id: u32, vmcs_id: u64 },
    EptMapped { guest_id: u64, gpa: u64, hpa: u64 },
    EptUnmapped { guest_id: u64, gpa: u64, hpa: u64 },
    MsrBlocked { vcpu_id: u32, msr_index: u32 },
    FrameAllocated { frame: u64 },
    FrameFreed { frame: u64 },
    /// Management-plane VM created (Defined). M5.0.
    VmCreated { guest_id: u64 },
    /// Management-plane VM started (Running). M5.0.
    VmStarted { guest_id: u64 },
    /// Management-plane VM stopped. M5.0.
    VmStopped { guest_id: u64 },
    /// Management-plane VM destroyed. M5.0.
    VmDestroyed { guest_id: u64 },
    /// VMware migrate batch started (ADR-007 / M5.5).
    MigrateStarted { batch_id: u64, count: u32 },
    /// VMware migrate batch completed successfully.
    MigrateCompleted { batch_id: u64, count: u32 },
    /// VMware migrate batch failed.
    MigrateFailed { batch_id: u64, count: u32 },
    /// REST control-plane auth allowed (M6.4).
    AuthAllowed { method_tag: u8 },
    /// REST control-plane auth denied (M6.4).
    AuthDenied { method_tag: u8 },
    /// Mock HA failover started (M6.6); role tags: 0=Primary, 1=Standby.
    HaFailoverStarted { from_role: u8, to_role: u8 },
    /// Mock HA failover completed with transferred guest count (M6.6).
    HaFailoverCompleted { guest_count: u32 },
}

/// One sealed audit record in the hash chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditRecord {
    pub seq: u64,
    pub event: AuditEvent,
    pub prev_hash: u64,
    pub hash: u64,
}

/// Fixed-capacity audit ring with hash chaining.
///
/// INVARIANTS:
///   - `records[i].prev_hash == records[i-1].hash` (or genesis for i==0)
///   - `hash` is a deterministic function of (seq, event, prev_hash)
///   - Overflow rejects append in this stub (no silent drop)
///
/// VERIFICATION: L0 — see integrity_spec.rs
pub struct AuditRing {
    records: [Option<AuditRecord>; AUDIT_RING_CAP],
    len: usize,
    next_seq: u64,
    tip_hash: u64,
}

impl AuditRing {
    pub const fn new() -> Self {
        Self {
            records: [None; AUDIT_RING_CAP],
            len: 0,
            next_seq: 0,
            tip_hash: AUDIT_GENESIS_HASH,
        }
    }

    pub fn capacity(&self) -> usize {
        AUDIT_RING_CAP
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn tip_hash(&self) -> u64 {
        self.tip_hash
    }

    /// Append an event to the chain.
    ///
    /// INVARIANTS:
    ///   - New record.prev_hash == previous tip
    ///   - tip_hash updates to new record.hash
    ///   - seq increases by 1
    ///
    /// VERIFICATION: L0
    pub fn append(&mut self, event: AuditEvent) -> Result<&AuditRecord, ()> {
        if self.len >= self.records.len() {
            return Err(());
        }
        let prev = self.tip_hash;
        let seq = self.next_seq;
        let hash = hash_record(seq, event, prev);
        let rec = AuditRecord {
            seq,
            event,
            prev_hash: prev,
            hash,
        };
        self.records[self.len] = Some(rec);
        self.len += 1;
        self.next_seq += 1;
        self.tip_hash = hash;
        Ok(self.records[self.len - 1].as_ref().unwrap())
    }

    /// Verify the entire chain from genesis.
    ///
    /// INVARIANTS:
    ///   - Returns true iff every link matches prev_hash/hash rules
    ///
    /// VERIFICATION: L0
    pub fn verify_chain(&self) -> bool {
        let mut prev = AUDIT_GENESIS_HASH;
        for i in 0..self.len {
            let Some(rec) = self.records[i] else {
                return false;
            };
            if rec.prev_hash != prev {
                return false;
            }
            if rec.hash != hash_record(rec.seq, rec.event, rec.prev_hash) {
                return false;
            }
            prev = rec.hash;
        }
        true
    }

    /// Read a sealed record by index (for verify/tamper hosts).
    pub fn get(&self, index: usize) -> Option<&AuditRecord> {
        if index >= self.len {
            return None;
        }
        self.records[index].as_ref()
    }

    /// Corrupt the stored hash at `index` (tamper simulation for [A] gates).
    ///
    /// Returns false if the slot is empty. After success, `verify_chain` is false.
    pub fn tamper_hash_at(&mut self, index: usize) -> bool {
        if index >= self.len {
            return false;
        }
        if let Some(rec) = self.records[index].as_mut() {
            rec.hash ^= 0xDEAD_BEEF_CAFE_BABEu64;
            true
        } else {
            false
        }
    }
}

impl Default for AuditRing {
    fn default() -> Self {
        Self::new()
    }
}

/// FNV-1a style stub hash (not cryptographic — replace for production [A]).
fn hash_record(seq: u64, event: AuditEvent, prev: u64) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    h ^= seq;
    h = h.wrapping_mul(0x100000001b3);
    h ^= event_discriminant(event);
    h = h.wrapping_mul(0x100000001b3);
    h ^= prev;
    h.wrapping_mul(0x100000001b3)
}

fn event_discriminant(event: AuditEvent) -> u64 {
    match event {
        AuditEvent::BootStarted { .. } => 1,
        AuditEvent::VmxEnabled { .. } => 2,
        AuditEvent::VmcsCreated { .. } => 3,
        AuditEvent::EptMapped { .. } => 4,
        AuditEvent::EptUnmapped { .. } => 5,
        AuditEvent::MsrBlocked { .. } => 6,
        AuditEvent::FrameAllocated { .. } => 7,
        AuditEvent::FrameFreed { .. } => 8,
        AuditEvent::VmCreated { .. } => 9,
        AuditEvent::VmStarted { .. } => 10,
        AuditEvent::VmStopped { .. } => 11,
        AuditEvent::VmDestroyed { .. } => 12,
        AuditEvent::MigrateStarted { .. } => 13,
        AuditEvent::MigrateCompleted { .. } => 14,
        AuditEvent::MigrateFailed { .. } => 15,
        AuditEvent::AuthAllowed { .. } => 16,
        AuditEvent::AuthDenied { .. } => 17,
        AuditEvent::HaFailoverStarted { .. } => 18,
        AuditEvent::HaFailoverCompleted { .. } => 19,
    }
}

/// Process-local sink used by `audit_log!`.
///
/// JUSTIFICATION (global state): firmware needs a single boot-time sink.
/// Host `cargo test` runs cases in parallel — `record_event` takes a spinlock.
struct BootRing(UnsafeCell<AuditRing>);

// SAFETY: exclusive access is enforced by `BOOT_RING_LOCK` in `record_event`.
// KANI-TARGET: strengthen Sync story when Proven Core audit lands (M5.3+).
unsafe impl Sync for BootRing {}

static BOOT_RING: BootRing = BootRing(UnsafeCell::new(AuditRing::new()));
static BOOT_RING_LOCK: AtomicBool = AtomicBool::new(false);

fn with_boot_ring<R>(f: impl FnOnce(&mut AuditRing) -> R) -> R {
    while BOOT_RING_LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held; exclusive mutable access to the ring.
    let out = unsafe { f(&mut *BOOT_RING.0.get()) };
    BOOT_RING_LOCK.store(false, Ordering::Release);
    out
}

/// Record an event into the boot ring (spinlock; overflow returns without panic).
pub fn record_event(event: AuditEvent) {
    let _ = with_boot_ring(|ring| ring.append(event).map(|_| ()));
}

/// Verify the live boot ring hash chain (tamper-evident path for host/firmware).
pub fn boot_ring_verify() -> bool {
    with_boot_ring(|ring| ring.verify_chain())
}

/// Test-only access to the boot ring length.
#[cfg(test)]
pub fn boot_ring_len_for_test() -> usize {
    with_boot_ring(|ring| ring.len())
}

/// Test-only alias kept for existing callers.
#[cfg(test)]
pub fn boot_ring_verify_for_test() -> bool {
    boot_ring_verify()
}

/// Append the M5.3 mandatory security categories onto `ring` and verify the chain.
///
/// Categories: VMCS · EPT map/unmap · MSR block · lifecycle (M5.0+).
pub fn prop_mandatory_events_chain() -> bool {
    let mut ring = AuditRing::new();
    let ok = ring
        .append(AuditEvent::VmcsCreated {
            vcpu_id: 0,
            vmcs_id: 1,
        })
        .is_ok()
        && ring
            .append(AuditEvent::EptMapped {
                guest_id: 1,
                gpa: 0x1000,
                hpa: 0x2000,
            })
            .is_ok()
        && ring
            .append(AuditEvent::EptUnmapped {
                guest_id: 1,
                gpa: 0x1000,
                hpa: 0x2000,
            })
            .is_ok()
        && ring
            .append(AuditEvent::MsrBlocked {
                vcpu_id: 0,
                msr_index: 0x3A,
            })
            .is_ok()
        && ring.append(AuditEvent::VmCreated { guest_id: 1 }).is_ok()
        && ring.append(AuditEvent::VmStarted { guest_id: 1 }).is_ok()
        && ring.append(AuditEvent::VmStopped { guest_id: 1 }).is_ok()
        && ring.append(AuditEvent::VmDestroyed { guest_id: 1 }).is_ok();
    ok && ring.len() == 8 && ring.verify_chain()
}

/// Tamper-evident: a flipped mid-chain hash makes `verify_chain` fail.
pub fn prop_tamper_detected() -> bool {
    let mut ring = AuditRing::new();
    if ring
        .append(AuditEvent::BootStarted {
            milestone: Milestone::M5,
        })
        .is_err()
        || ring
            .append(AuditEvent::VmcsCreated {
                vcpu_id: 0,
                vmcs_id: 9,
            })
            .is_err()
        || ring
            .append(AuditEvent::EptMapped {
                guest_id: 1,
                gpa: 0,
                hpa: 0x1000,
            })
            .is_err()
    {
        return false;
    }
    if !ring.verify_chain() {
        return false;
    }
    if !ring.tamper_hash_at(1) {
        return false;
    }
    !ring.verify_chain() && M5_AUDIT_OK_MARKER == "RAYNU-V-M5-AUDIT-OK"
}

/// Full M5.3 integrity property bundle (local ring; no boot-ring dependency).
pub fn prop_audit_integrity_gate() -> bool {
    prop_mandatory_events_chain() && prop_tamper_detected() && boot_ring_verify()
}

#[cfg(test)]
#[path = "integrity_test.rs"]
mod integrity_test;
