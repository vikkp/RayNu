//! M6.7 host-testable fault injection suite (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002) — orchestrates vCPU / EPT / IRQ / vSwitch
//! surfaces without breaking exclusivity proofs.
//! VERIFICATION: N/A
//!
//! Four faults: kill vCPU, corrupt page, drop IRQ, network partition.
//! Each has inject → fail-closed / recover criteria and audit trail.
//! 72-hr soak is M6.8; real QEMU iron demos are optional beyond this gate.

use crate::audit_log;
use crate::audit::AuditEvent;
use crate::memory::ept::{EptMap, EptPermissions};
use crate::memory::frame_allocator::PhysFrame;
use crate::net::{build_eth_frame, VSwitch};
use crate::sched::interrupt::{
    prepare_external_inject, InjectError, INTR_INFO_VALID, M2_IRQ_VECTOR,
};
use crate::sched::vcpu::{Vcpu, VcpuState};

use super::{VmLifecycle, VmTable};

/// Host / CI marker when the M6.7 fault injection gate passes.
pub const M6_FAULT_OK_MARKER: &str = "RAYNU-V-M6-FAULT-OK";

/// Fault injection GAP closed in M6.7.
pub const FAULT_GAP_NOTE: &str = "GAP(CLOSED M6.7): Fault injection";

/// Fault kind tags (stable for audit `kind` field).
pub const KIND_KILL_VCPU: u8 = 0;
pub const KIND_CORRUPT_PAGE: u8 = 1;
pub const KIND_DROP_IRQ: u8 = 2;
pub const KIND_NET_PARTITION: u8 = 3;

/// Scripted fault kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    KillVcpu,
    CorruptPage,
    DropIrq,
    NetPartition,
}

impl FaultKind {
    pub fn tag(self) -> u8 {
        match self {
            FaultKind::KillVcpu => KIND_KILL_VCPU,
            FaultKind::CorruptPage => KIND_CORRUPT_PAGE,
            FaultKind::DropIrq => KIND_DROP_IRQ,
            FaultKind::NetPartition => KIND_NET_PARTITION,
        }
    }
}

/// Error from a fault inject / recover step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultError {
    WrongState,
    Inject(InjectError),
    Surface,
}

/// Host-side IRQ drop latch (wraps `prepare_external_inject`; does not alter Proven Core).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqDropLatch {
    armed: bool,
    drops: u32,
}

impl IrqDropLatch {
    pub const fn new() -> Self {
        Self {
            armed: false,
            drops: 0,
        }
    }

    pub fn arm(&mut self) {
        self.armed = true;
    }

    pub fn disarm(&mut self) {
        self.armed = false;
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    pub fn drops(&self) -> u32 {
        self.drops
    }

    /// Inject external IRQ unless drop-armed (fail-closed while armed).
    pub fn try_inject(&mut self, vector: u32, vcpu_running: bool) -> Result<u32, FaultError> {
        if !vcpu_running {
            return Err(FaultError::Inject(InjectError::NotRunning));
        }
        if self.armed {
            self.drops = self.drops.saturating_add(1);
            audit_log!(AuditEvent::FaultFailClosed {
                kind: KIND_DROP_IRQ,
                detail: vector as u64,
            });
            return Err(FaultError::WrongState);
        }
        prepare_external_inject(vector).map_err(FaultError::Inject)
    }
}

impl Default for IrqDropLatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Page-corruption view over an EPT mapping (does not flip real memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorruptPageView {
    pub guest_id: u64,
    pub gpa: u64,
    pub frame: PhysFrame,
    pub corrupt: bool,
}

impl CorruptPageView {
    pub fn allow_access(&self) -> bool {
        !self.corrupt
    }
}

/// Kill vCPU: tear down Running vCPU + stop guest; recover via restart.
pub fn prop_kill_vcpu_recover() -> bool {
    let mut vcpu = Vcpu::new(0);
    vcpu.make_runnable();
    vcpu.enter_running();
    let mut table = VmTable::new();
    let gid = 1u64;
    if table.create(gid).is_err() || table.start(gid).is_err() {
        return false;
    }

    audit_log!(AuditEvent::FaultInjected {
        kind: KIND_KILL_VCPU,
        detail: gid,
    });
    if vcpu.tear_down().is_err() {
        return false;
    }
    if vcpu.state() != VcpuState::TornDown {
        return false;
    }
    // Fail-closed: second kill rejected.
    if vcpu.tear_down().is_ok() {
        return false;
    }
    if table.stop(gid).is_err() {
        return false;
    }
    if table.get(gid).map(|r| r.state) != Some(VmLifecycle::Stopped) {
        return false;
    }

    // Recover: new vCPU + restart guest.
    let mut v2 = Vcpu::new(0);
    v2.make_runnable();
    v2.enter_running();
    if table.start(gid).is_err() {
        return false;
    }
    audit_log!(AuditEvent::FaultRecovered {
        kind: KIND_KILL_VCPU,
        detail: gid,
    });
    v2.state() == VcpuState::Running
        && table.get(gid).map(|r| r.state) == Some(VmLifecycle::Running)
}

/// Corrupt page: mark mapped GPA corrupt → deny access; recover via unmap+remap.
pub fn prop_corrupt_page_fail_closed() -> bool {
    let mut map = EptMap::new();
    let guest = 1u64;
    let gpa = 0x1000u64;
    let frame = PhysFrame(0x2000);
    if map
        .map(guest, gpa, frame, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    let mut view = CorruptPageView {
        guest_id: guest,
        gpa,
        frame,
        corrupt: false,
    };
    if !view.allow_access() || map.owner_of(frame) != Some(guest) {
        return false;
    }

    audit_log!(AuditEvent::FaultInjected {
        kind: KIND_CORRUPT_PAGE,
        detail: gpa,
    });
    view.corrupt = true;
    if view.allow_access() {
        return false;
    }
    audit_log!(AuditEvent::FaultFailClosed {
        kind: KIND_CORRUPT_PAGE,
        detail: gpa,
    });
    // Fail-closed: must not continue with corrupt frame still mapped as "ok".
    if !map.check_invariants() || map.owner_of(frame) != Some(guest) {
        return false;
    }

    // Recover: unmap corrupt mapping; remap fresh frame.
    if map.unmap(guest, gpa).is_err() {
        return false;
    }
    view.corrupt = false;
    let fresh = PhysFrame(0x3000);
    if map
        .map(guest, gpa, fresh, EptPermissions::READ_WRITE)
        .is_err()
    {
        return false;
    }
    view.frame = fresh;
    audit_log!(AuditEvent::FaultRecovered {
        kind: KIND_CORRUPT_PAGE,
        detail: gpa,
    });
    view.allow_access()
        && map.owner_of(fresh) == Some(guest)
        && map.owner_of(frame).is_none()
        && map.check_invariants()
}

/// Drop IRQ: armed latch rejects inject; disarm restores packed inject word.
pub fn prop_drop_irq_fail_closed() -> bool {
    let mut latch = IrqDropLatch::new();
    // Happy path first.
    let ok = match latch.try_inject(M2_IRQ_VECTOR, true) {
        Ok(info) => info & INTR_INFO_VALID != 0,
        Err(_) => false,
    };
    if !ok {
        return false;
    }

    audit_log!(AuditEvent::FaultInjected {
        kind: KIND_DROP_IRQ,
        detail: M2_IRQ_VECTOR as u64,
    });
    latch.arm();
    if latch.try_inject(M2_IRQ_VECTOR, true).is_ok() {
        return false;
    }
    if latch.drops() < 1 {
        return false;
    }
    // Not running → NotRunning even when armed.
    if !matches!(
        latch.try_inject(M2_IRQ_VECTOR, false),
        Err(FaultError::Inject(InjectError::NotRunning))
    ) {
        return false;
    }

    latch.disarm();
    audit_log!(AuditEvent::FaultRecovered {
        kind: KIND_DROP_IRQ,
        detail: M2_IRQ_VECTOR as u64,
    });
    match latch.try_inject(M2_IRQ_VECTOR, true) {
        Ok(info) => info & INTR_INFO_VALID != 0,
        Err(_) => false,
    }
}

/// Network partition: VSwitch drops unicast while partitioned; recovers after clear.
pub fn prop_net_partition_recover() -> bool {
    let mut sw = VSwitch::new(2);
    let mac0 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let mac1 = [0x52, 0x54, 0x00, 0x12, 0x34, 0x57];
    if sw.attach(0, mac0).is_err() || sw.attach(1, mac1).is_err() {
        return false;
    }
    let mut frame = [0u8; 64];
    let n = match build_eth_frame(&mut frame, &mac1, &mac0, 0x88B5, b"RAYNU") {
        Ok(n) => n,
        Err(_) => return false,
    };
    if sw.forward(0, &frame[..n]) != Ok(Some(1)) {
        return false;
    }

    audit_log!(AuditEvent::FaultInjected {
        kind: KIND_NET_PARTITION,
        detail: 1,
    });
    sw.set_partitioned(true);
    if !sw.is_partitioned() {
        return false;
    }
    // Fail-closed: no unicast delivery under partition.
    if sw.forward(0, &frame[..n]) != Ok(None) {
        return false;
    }
    audit_log!(AuditEvent::FaultFailClosed {
        kind: KIND_NET_PARTITION,
        detail: 1,
    });
    // Attach checks still apply.
    if sw.forward(7, &frame[..n]).is_ok() {
        return false;
    }

    sw.set_partitioned(false);
    audit_log!(AuditEvent::FaultRecovered {
        kind: KIND_NET_PARTITION,
        detail: 1,
    });
    sw.forward(0, &frame[..n]) == Ok(Some(1))
}

/// Full suite: all four props + closed GAP + marker.
pub fn prop_fault_suite() -> bool {
    let _ = (FAULT_GAP_NOTE, M6_FAULT_OK_MARKER);
    prop_kill_vcpu_recover()
        && prop_corrupt_page_fail_closed()
        && prop_drop_irq_fail_closed()
        && prop_net_partition_recover()
        && FAULT_GAP_NOTE.contains("CLOSED M6.7")
        && M6_FAULT_OK_MARKER == "RAYNU-V-M6-FAULT-OK"
}

#[cfg(test)]
#[path = "fault_test.rs"]
mod fault_test;
