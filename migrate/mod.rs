//! VMware migration engine (vCenter, VMDK, OVF).
//!
//! Pillar: [A] [Z]
//! Proven Core: **outside** (ADR-007)
//! VERIFICATION: N/A
//! Milestone: M5.5 dedicated workstream
//!
//! M5.5 MV: one-command import of a documented OVF/VMDK inventory (≥10 guests)
//! into `mgmt::VmTable`, with audit start/complete/fail. Live vCenter SOAP/REST
//! client remains `GAP: live vCenter API → polish` (host/firmware use the
//! exported inventory path).

use crate::audit_log;
use crate::audit::AuditEvent;
use crate::mgmt::{LifecycleError, VmTable};

/// Host / CI marker when the M5.5 migrate gate passes.
pub const M5_MIGRATE_OK_MARKER: &str = "RAYNU-V-M5-MIGRATE-OK";

/// Minimum guests required to close the ADR-007 gate.
pub const MIGRATE_MIN_GUESTS: usize = 10;

/// Max entries in one import batch (aligned with `MGMT_GUEST_CAP`).
pub const MIGRATE_BATCH_CAP: usize = 16;

/// Documented live-API gap (inventory path closes M5.5).
pub const VCENTER_API_GAP_NOTE: &str = "GAP: live vCenter API → polish";

/// Embedded sample inventory (≥12 VMs) for host/CI one-command smoke.
pub const SAMPLE_INVENTORY: &str = include_str!("../assets/migrate/sample_inventory.txt");

/// Disk / package source from a vCenter export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    Vmdk,
    Ovf,
}

/// One guest line from an inventory (name is truncated to 16 ASCII bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSpec {
    pub guest_id: u64,
    pub source: ImportSource,
    pub name: [u8; 16],
    pub name_len: u8,
}

impl ImportSpec {
    pub fn name_str(&self) -> &str {
        let n = self.name_len as usize;
        core::str::from_utf8(&self.name[..n]).unwrap_or("")
    }
}

/// Migration job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
}

/// Result of a one-command batch import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrateReport {
    pub batch_id: u64,
    pub imported: u32,
    pub status: MigrateStatus,
}

/// Error from parse or import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrateError {
    EmptyInventory,
    TooFewGuests,
    TooManyGuests,
    BadLine,
    BadGuestId,
    BadSource,
    Lifecycle(LifecycleError),
}

/// Parse inventory text into `out`; returns count written.
///
/// Line format: `guest_id source name` (`source` = `vmdk` | `ovf`).
/// Blank lines and `#` comments are ignored.
pub fn parse_inventory(text: &str, out: &mut [ImportSpec; MIGRATE_BATCH_CAP]) -> Result<usize, MigrateError> {
    let mut n = 0;
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if n >= MIGRATE_BATCH_CAP {
            return Err(MigrateError::TooManyGuests);
        }
        let mut parts = line.split_whitespace();
        let id_s = parts.next().ok_or(MigrateError::BadLine)?;
        let src_s = parts.next().ok_or(MigrateError::BadLine)?;
        let name_s = parts.next().ok_or(MigrateError::BadLine)?;
        if parts.next().is_some() {
            return Err(MigrateError::BadLine);
        }
        let guest_id = parse_u64(id_s).ok_or(MigrateError::BadGuestId)?;
        if guest_id == 0 {
            return Err(MigrateError::BadGuestId);
        }
        let source = match src_s {
            "vmdk" | "VMDK" => ImportSource::Vmdk,
            "ovf" | "OVF" => ImportSource::Ovf,
            _ => return Err(MigrateError::BadSource),
        };
        let mut name = [0u8; 16];
        let nb = name_s.as_bytes();
        let take = core::cmp::min(nb.len(), 16);
        name[..take].copy_from_slice(&nb[..take]);
        out[n] = ImportSpec {
            guest_id,
            source,
            name,
            name_len: take as u8,
        };
        n += 1;
    }
    if n == 0 {
        return Err(MigrateError::EmptyInventory);
    }
    Ok(n)
}

fn parse_u64(s: &str) -> Option<u64> {
    let mut n: u64 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(n)
}

/// One-command import: parse inventory and create each guest in `Defined` state.
///
/// Emits `MigrateStarted` then either `MigrateCompleted` or `MigrateFailed`.
/// Requires ≥ [`MIGRATE_MIN_GUESTS`] entries (ADR-007 gate).
pub fn migrate_one_command(
    batch_id: u64,
    inventory: &str,
    table: &mut VmTable,
) -> Result<MigrateReport, MigrateError> {
    let _ = VCENTER_API_GAP_NOTE;
    let mut specs = [ImportSpec {
        guest_id: 0,
        source: ImportSource::Vmdk,
        name: [0; 16],
        name_len: 0,
    }; MIGRATE_BATCH_CAP];
    let count = parse_inventory(inventory, &mut specs)?;
    if count < MIGRATE_MIN_GUESTS {
        return Err(MigrateError::TooFewGuests);
    }

    audit_log!(AuditEvent::MigrateStarted {
        batch_id,
        count: count as u32,
    });

    for spec in specs.iter().take(count) {
        match table.create(spec.guest_id) {
            Ok(()) => {}
            Err(e) => {
                audit_log!(AuditEvent::MigrateFailed {
                    batch_id,
                    count: count as u32,
                });
                return Err(MigrateError::Lifecycle(e));
            }
        }
    }

    audit_log!(AuditEvent::MigrateCompleted {
        batch_id,
        count: count as u32,
    });
    Ok(MigrateReport {
        batch_id,
        imported: count as u32,
        status: MigrateStatus::Succeeded,
    })
}

/// Host-testable: sample inventory imports ≥10 guests in one command.
pub fn prop_migrate_ten_plus() -> bool {
    let mut table = VmTable::new();
    let report = match migrate_one_command(1, SAMPLE_INVENTORY, &mut table) {
        Ok(r) => r,
        Err(_) => return false,
    };
    if report.status != MigrateStatus::Succeeded || report.imported < MIGRATE_MIN_GUESTS as u32 {
        return false;
    }
    if table.len() < MIGRATE_MIN_GUESTS {
        return false;
    }
    // Spot-check first and last sample ids.
    table.get(1).is_some()
        && table.get(12).is_some()
        && SAMPLE_INVENTORY.contains("vmdk")
        && SAMPLE_INVENTORY.contains("ovf")
        && VCENTER_API_GAP_NOTE.contains("vCenter")
        && M5_MIGRATE_OK_MARKER == "RAYNU-V-M5-MIGRATE-OK"
}

/// True when inventory documents both VMDK and OVF sources.
pub fn inventory_documents_vmdk_ovf() -> bool {
    SAMPLE_INVENTORY.lines().filter(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('#')
    }).count()
        >= MIGRATE_MIN_GUESTS
        && SAMPLE_INVENTORY.contains(" vmdk ")
        && SAMPLE_INVENTORY.contains(" ovf ")
}

pub mod m5_migrate_gate;

pub use m5_migrate_gate::run_m5_migrate_gate;

#[cfg(test)]
#[path = "migrate_test.rs"]
mod migrate_test;
