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
    BootStarted {
        milestone: Milestone,
    },
    VmxEnabled {
        vcpu_id: u32,
    },
    VmcsCreated {
        vcpu_id: u32,
        vmcs_id: u64,
    },
    EptMapped {
        guest_id: u64,
        gpa: u64,
        hpa: u64,
    },
    EptUnmapped {
        guest_id: u64,
        gpa: u64,
        hpa: u64,
    },
    MsrBlocked {
        vcpu_id: u32,
        msr_index: u32,
    },
    FrameAllocated {
        frame: u64,
    },
    FrameFreed {
        frame: u64,
    },
    /// Management-plane VM created (Defined). M5.0.
    VmCreated {
        guest_id: u64,
    },
    /// Management-plane VM started (Running). M5.0.
    VmStarted {
        guest_id: u64,
    },
    /// Management-plane VM stopped. M5.0.
    VmStopped {
        guest_id: u64,
    },
    /// Management-plane VM destroyed. M5.0.
    VmDestroyed {
        guest_id: u64,
    },
    /// VMware migrate batch started (ADR-007 / M5.5).
    MigrateStarted {
        batch_id: u64,
        count: u32,
    },
    /// VMware migrate batch completed successfully.
    MigrateCompleted {
        batch_id: u64,
        count: u32,
    },
    /// VMware migrate batch failed.
    MigrateFailed {
        batch_id: u64,
        count: u32,
    },
    /// REST control-plane auth allowed (M6.4).
    AuthAllowed {
        method_tag: u8,
    },
    /// REST control-plane auth denied (M6.4).
    AuthDenied {
        method_tag: u8,
    },
    /// Mock HA failover started (M6.6); role tags: 0=Primary, 1=Standby.
    HaFailoverStarted {
        from_role: u8,
        to_role: u8,
    },
    /// Mock HA failover completed with transferred guest count (M6.6).
    HaFailoverCompleted {
        guest_count: u32,
    },
    /// Fault injected (M6.7); kind: 0=KillVcpu,1=CorruptPage,2=DropIrq,3=NetPartition.
    FaultInjected {
        kind: u8,
        detail: u64,
    },
    /// Fault recovered (M6.7).
    FaultRecovered {
        kind: u8,
        detail: u64,
    },
    /// Fault denied / fail-closed path taken (M6.7).
    FaultFailClosed {
        kind: u8,
        detail: u64,
    },
    /// Soak run started (M6.8); detail = target hours.
    SoakStarted {
        target_hours: u32,
    },
    /// Soak run completed within thresholds (M6.8); detail = hours completed.
    SoakCompleted {
        hours: u32,
    },
    /// Soak run failed thresholds (M6.8); detail = hours completed at fail.
    SoakFailed {
        hours: u32,
    },
    /// Evidence / verbose mode activated via ESP flag file (ADR-011).
    /// `source`: 1 = volume root `paperverbose.txt`, 2 = `\\EFI\\RayNu\\paperverbose.txt`.
    EvidenceModeActivated {
        source: u8,
    },
    /// Management-plane listen restarted after `MgmtFatal` (ADR-013 Phase E).
    /// `kind`: 0=Device, 1=Bind, 2=ArenaExhausted, 3=Induced.
    MgmtRestarted {
        generation: u32,
        kind: u8,
    },
    /// Host El Torito CD-ROM attach armed (ADR-014 Stage 1). Not guest UEFI.
    CdromAttached {
        iso_id: u64,
        load_lba: u64,
    },
    /// Firmware-facing CD armed (ADR-014 Stage 2). Not VMLAUNCH / not OVMF.
    CdromFirmwareArmed {
        iso_id: u64,
        load_lba: u64,
    },
    /// Guest-UEFI CD presented on the private VMCS PCI IDE/ATAPI function.
    /// Not full DXE / not installer.
    CdromGuestVisible {
        iso_id: u64,
        load_lba: u64,
    },
    /// Guest UEFI firmware envelope boxed (ADR-014 Stage 3). Not OVMF / not VMLAUNCH.
    GuestFirmwareBoxed {
        uncompressed_len: u64,
        compressed_len: u64,
    },
    /// Guest firmware stub payload lazy-loaded (ADR-014 Stage 4). Not OVMF / not VMLAUNCH.
    GuestFirmwareLoaded {
        payload_len: u64,
    },
    /// OVMF Firmware Volume header probed (ADR-014 Stage 5). Not VMLAUNCH / not embedded EDK2.
    OvmfFirmwareProbed {
        fv_len: u64,
    },
    /// OVMF loaded from ESP split-mode path (ADR-014 Stage 6). Not VMLAUNCH.
    OvmfFirmwareEspLoaded {
        bytes_len: u64,
        fv_len: u64,
    },
    /// Guest firmware slot armed (ADR-014 Stage 7). Not VMLAUNCH.
    OvmfFirmwareSlotArmed {
        slot_id: u64,
    },
    /// Firmware slot bound to a guest (ADR-014 Stage 8). Not VMLAUNCH.
    OvmfFirmwareGuestBound {
        guest_id: u64,
        slot_id: u64,
    },
    /// Firmware launch-prepare after bind (ADR-014 Stage 9). Not VMLAUNCH.
    OvmfFirmwareLaunchPrepared {
        guest_id: u64,
        slot_id: u64,
    },
    /// Size-floor FV staged (ADR-014 Stage 10). Not EDK2 / not VMLAUNCH.
    OvmfFirmwareFloorStaged {
        bytes_len: u64,
    },
    /// EDK2-sized FV staged (ADR-014 Stage 11). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfFirmwareEdk2Staged {
        bytes_len: u64,
    },
    /// ESP-path guest UEFI VMLAUNCH armed (ADR-014 Stage 12). No live OVMF.fd mapping.
    OvmfEspLaunchArmed {
        guest_id: u64,
        slot_id: u64,
    },
    /// Live-sized ESP OVMF map recorded (ADR-014 Stage 13). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfEspLiveMapped {
        bytes_len: u64,
    },
    /// Reset-vector VMCS contract armed (ADR-014 Stage 14). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfResetVectorArmed {
        bytes_len: u64,
    },
    /// Firmware-alias contract armed (ADR-014 Stage 15). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfFirmwareAliasArmed {
        bytes_len: u64,
    },
    /// Alias-EPT program contract recorded (ADR-014 Stage 16). Not a live EPT write / not VMLAUNCH.
    OvmfAliasEptProgrammed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Private alias-EPT install recorded (ADR-014 Stage 17). Not a live E4 SHELL EPT write / not VMLAUNCH.
    OvmfAliasEptInstalled {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real-ESP VMLAUNCH-ready contract recorded (ADR-014 Stage 18). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfRealEspQualified {
        bytes_len: u64,
        gpa: u64,
    },
    /// Guest-UEFI VMLAUNCH insn path armed (ADR-014 Stage 19). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfRealLaunchArmed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Live ESP `\EFI\RayNu\OVMF.fd` required before VMLAUNCH (ADR-014 Stage 20).
    /// Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspRequired {
        bytes_len: u64,
        gpa: u64,
    },
    /// Private guest-UEFI VMCS selected (ADR-014 Stage 21). Not E4 SHELL / not VMLAUNCH.
    OvmfPrivateVmcsArmed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Live-ESP VMLAUNCH issue path armed (ADR-014 Stage 22). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveIssueArmed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Live ESP `\EFI\RayNu\OVMF.fd` bytes probed (ADR-014 Stage 23). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveBytesProbed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` required (ADR-014 Stage 24). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveFdRequired {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` present-attempt (ADR-014 Stage 25). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspPresented {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` admit-attempt (ADR-014 Stage 26). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspAdmitted {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` read-attempt (ADR-014 Stage 27). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspRead {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` copy-attempt (ADR-014 Stage 28). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspCopied {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` place-attempt (ADR-014 Stage 29). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspPlaced {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` apply-attempt (ADR-014 Stage 30). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspApplied {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` commit-attempt (ADR-014 Stage 31). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspCommitted {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` latch-attempt (ADR-014 Stage 32). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspLatched {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` seal-attempt (ADR-014 Stage 33). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspSealed {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` lock-attempt (ADR-014 Stage 34). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspLocked {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` hold-attempt (ADR-014 Stage 35). Not a shipped OVMF.fd / not VMLAUNCH.
    OvmfLiveEspHeld {
        bytes_len: u64,
        gpa: u64,
    },
    /// Real ESP `\EFI\RayNu\OVMF.fd` bytes retained in a pre-EBS buffer (ADR-014 presence rule).
    /// Not a private VMCS and not VMLAUNCH.
    OvmfLiveEspBytesRetained {
        bytes_len: u64,
    },
    /// Private guest-UEFI VMLAUNCH of retained ESP `OVMF.fd` entered (first VMEXIT).
    /// Not Everest E5 / not a shipped installer.
    OvmfGuestUefiVmlaunched {
        exit_reason: u64,
        guest_rip: u64,
    },
    /// Guest UEFI continued past the first triple-fault (CR4.VMXE host-owned).
    /// Not full OVMF boot / not installer.
    OvmfGuestUefiAlive {
        exits: u64,
        last_reason: u64,
    },
    /// Guest UEFI left the OVMF SEC tail (last 64 KiB) with PEI-style evidence.
    /// Not full DXE / not installer.
    OvmfGuestUefiPastSec {
        exits: u64,
        linear: u64,
        com_bytes: u64,
    },
    /// Guest UEFI enumerated or read the presented ATAPI CD.
    /// Not full DXE / not installer.
    OvmfGuestUefiCdrom {
        exits: u64,
        pci_enum: u64,
        sectors: u64,
    },
    /// Guest UEFI left PEI into DXE or attempted a CD boot (ATAPI READ).
    /// Not a completed firmware CD boot / not installer.
    OvmfGuestUefiDxe {
        exits: u64,
        sectors: u64,
        platform: u64,
    },
    /// Guest UEFI enumerated empty virtio-blk with CD then disk boot order.
    /// Not a completed firmware CD boot / not installer.
    OvmfGuestUefiVirtio {
        exits: u64,
        pci_enum: u64,
    },
    /// Guest UEFI enumerated virtio `00:00.0` and IDE `00:00.1` on one boot.
    /// Not ATAPI sectors / not installer.
    OvmfGuestUefiBoth {
        exits: u64,
        virtio: u64,
        ide: u64,
    },
    /// Guest UEFI issued ATAPI READ and `sectors>0`.
    /// Not a completed El Torito CD boot / not installer.
    OvmfGuestUefiAtapi {
        exits: u64,
        sectors: u64,
    },
    /// Guest UEFI loaded and ran the El Torito CD EFI.
    /// Not installer / not `ISO-INSTALL-OK`.
    OvmfGuestUefiEltorito {
        exits: u64,
        catalog: u64,
        boot_image: u64,
    },
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
        AuditEvent::FaultInjected { .. } => 20,
        AuditEvent::FaultRecovered { .. } => 21,
        AuditEvent::FaultFailClosed { .. } => 22,
        AuditEvent::SoakStarted { .. } => 23,
        AuditEvent::SoakCompleted { .. } => 24,
        AuditEvent::SoakFailed { .. } => 25,
        AuditEvent::EvidenceModeActivated { .. } => 26,
        AuditEvent::MgmtRestarted { .. } => 27,
        AuditEvent::CdromAttached { .. } => 28,
        AuditEvent::CdromFirmwareArmed { .. } => 29,
        AuditEvent::CdromGuestVisible { .. } => 67,
        AuditEvent::GuestFirmwareBoxed { .. } => 30,
        AuditEvent::GuestFirmwareLoaded { .. } => 31,
        AuditEvent::OvmfFirmwareProbed { .. } => 32,
        AuditEvent::OvmfFirmwareEspLoaded { .. } => 33,
        AuditEvent::OvmfFirmwareSlotArmed { .. } => 34,
        AuditEvent::OvmfFirmwareGuestBound { .. } => 35,
        AuditEvent::OvmfFirmwareLaunchPrepared { .. } => 36,
        AuditEvent::OvmfFirmwareFloorStaged { .. } => 37,
        AuditEvent::OvmfFirmwareEdk2Staged { .. } => 38,
        AuditEvent::OvmfEspLaunchArmed { .. } => 39,
        AuditEvent::OvmfEspLiveMapped { .. } => 40,
        AuditEvent::OvmfResetVectorArmed { .. } => 41,
        AuditEvent::OvmfFirmwareAliasArmed { .. } => 42,
        AuditEvent::OvmfAliasEptProgrammed { .. } => 43,
        AuditEvent::OvmfAliasEptInstalled { .. } => 44,
        AuditEvent::OvmfRealEspQualified { .. } => 45,
        AuditEvent::OvmfRealLaunchArmed { .. } => 46,
        AuditEvent::OvmfLiveEspRequired { .. } => 47,
        AuditEvent::OvmfPrivateVmcsArmed { .. } => 48,
        AuditEvent::OvmfLiveIssueArmed { .. } => 49,
        AuditEvent::OvmfLiveBytesProbed { .. } => 50,
        AuditEvent::OvmfLiveFdRequired { .. } => 51,
        AuditEvent::OvmfLiveEspPresented { .. } => 52,
        AuditEvent::OvmfLiveEspAdmitted { .. } => 53,
        AuditEvent::OvmfLiveEspRead { .. } => 54,
        AuditEvent::OvmfLiveEspCopied { .. } => 55,
        AuditEvent::OvmfLiveEspPlaced { .. } => 56,
        AuditEvent::OvmfLiveEspApplied { .. } => 57,
        AuditEvent::OvmfLiveEspCommitted { .. } => 58,
        AuditEvent::OvmfLiveEspLatched { .. } => 59,
        AuditEvent::OvmfLiveEspSealed { .. } => 60,
        AuditEvent::OvmfLiveEspLocked { .. } => 61,
        AuditEvent::OvmfLiveEspHeld { .. } => 62,
        AuditEvent::OvmfLiveEspBytesRetained { .. } => 63,
        AuditEvent::OvmfGuestUefiVmlaunched { .. } => 64,
        AuditEvent::OvmfGuestUefiAlive { .. } => 65,
        AuditEvent::OvmfGuestUefiPastSec { .. } => 66,
        AuditEvent::OvmfGuestUefiCdrom { .. } => 68,
        AuditEvent::OvmfGuestUefiDxe { .. } => 69,
        AuditEvent::OvmfGuestUefiVirtio { .. } => 70,
        AuditEvent::OvmfGuestUefiBoth { .. } => 71,
        AuditEvent::OvmfGuestUefiAtapi { .. } => 72,
        AuditEvent::OvmfGuestUefiEltorito { .. } => 73,
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
///
/// On UEFI firmware, also mirrors a one-line summary to COM1 so iDRAC Virtual
/// Console / SOL capture sees audit activity (see `docs/runbooks/idrac_logging.md`).
pub fn record_event(event: AuditEvent) {
    let _ = with_boot_ring(|ring| ring.append(event).map(|_| ()));
    #[cfg(target_os = "uefi")]
    mirror_audit_to_com1(event);
}

/// COM1 mirror for iDRAC capture. Skips high-churn frame events.
#[cfg(target_os = "uefi")]
fn mirror_audit_to_com1(event: AuditEvent) {
    use crate::boot::serial;

    match event {
        AuditEvent::FrameAllocated { .. } | AuditEvent::FrameFreed { .. } => return,
        _ => {}
    }

    // Fixed labels only — no heap; details as decimal via tiny helper.
    match event {
        AuditEvent::BootStarted { milestone } => {
            serial::write_str("RAYNU-V-AUDIT: BootStarted milestone=");
            write_u32(milestone_tag(milestone));
            serial::write_byte(b'\n');
        }
        AuditEvent::VmxEnabled { vcpu_id } => {
            serial::write_str("RAYNU-V-AUDIT: VmxEnabled vcpu_id=");
            write_u32(vcpu_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::VmcsCreated { vcpu_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: VmcsCreated vcpu_id=");
            write_u32(vcpu_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::EptMapped { guest_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: EptMapped guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::EptUnmapped { guest_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: EptUnmapped guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::MsrBlocked { vcpu_id, msr_index } => {
            serial::write_str("RAYNU-V-AUDIT: MsrBlocked vcpu_id=");
            write_u32(vcpu_id);
            serial::write_str(" msr=0x");
            write_u32_hex(msr_index);
            serial::write_byte(b'\n');
        }
        AuditEvent::VmCreated { guest_id } => {
            serial::write_str("RAYNU-V-AUDIT: VmCreated guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::VmStarted { guest_id } => {
            serial::write_str("RAYNU-V-AUDIT: VmStarted guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::VmStopped { guest_id } => {
            serial::write_str("RAYNU-V-AUDIT: VmStopped guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::VmDestroyed { guest_id } => {
            serial::write_str("RAYNU-V-AUDIT: VmDestroyed guest_id=");
            write_u64(guest_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::MigrateStarted { batch_id, count } => {
            serial::write_str("RAYNU-V-AUDIT: MigrateStarted batch=");
            write_u64(batch_id);
            serial::write_str(" count=");
            write_u32(count);
            serial::write_byte(b'\n');
        }
        AuditEvent::MigrateCompleted { batch_id, count } => {
            serial::write_str("RAYNU-V-AUDIT: MigrateCompleted batch=");
            write_u64(batch_id);
            serial::write_str(" count=");
            write_u32(count);
            serial::write_byte(b'\n');
        }
        AuditEvent::MigrateFailed { batch_id, count } => {
            serial::write_str("RAYNU-V-AUDIT: MigrateFailed batch=");
            write_u64(batch_id);
            serial::write_str(" count=");
            write_u32(count);
            serial::write_byte(b'\n');
        }
        AuditEvent::AuthAllowed { method_tag } => {
            serial::write_str("RAYNU-V-AUDIT: AuthAllowed method_tag=");
            write_u32(method_tag as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::AuthDenied { method_tag } => {
            serial::write_str("RAYNU-V-AUDIT: AuthDenied method_tag=");
            write_u32(method_tag as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::HaFailoverStarted { from_role, to_role } => {
            serial::write_str("RAYNU-V-AUDIT: HaFailoverStarted from=");
            write_u32(from_role as u32);
            serial::write_str(" to=");
            write_u32(to_role as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::HaFailoverCompleted { guest_count } => {
            serial::write_str("RAYNU-V-AUDIT: HaFailoverCompleted guests=");
            write_u32(guest_count);
            serial::write_byte(b'\n');
        }
        AuditEvent::FaultInjected { kind, .. } => {
            serial::write_str("RAYNU-V-AUDIT: FaultInjected kind=");
            write_u32(kind as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::FaultRecovered { kind, .. } => {
            serial::write_str("RAYNU-V-AUDIT: FaultRecovered kind=");
            write_u32(kind as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::FaultFailClosed { kind, .. } => {
            serial::write_str("RAYNU-V-AUDIT: FaultFailClosed kind=");
            write_u32(kind as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::SoakStarted { target_hours } => {
            serial::write_str("RAYNU-V-AUDIT: SoakStarted hours=");
            write_u32(target_hours);
            serial::write_byte(b'\n');
        }
        AuditEvent::SoakCompleted { hours } => {
            serial::write_str("RAYNU-V-AUDIT: SoakCompleted hours=");
            write_u32(hours);
            serial::write_byte(b'\n');
        }
        AuditEvent::SoakFailed { hours } => {
            serial::write_str("RAYNU-V-AUDIT: SoakFailed hours=");
            write_u32(hours);
            serial::write_byte(b'\n');
        }
        AuditEvent::EvidenceModeActivated { source } => {
            serial::write_str("RAYNU-V-AUDIT: EvidenceModeActivated source=");
            write_u32(source as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::MgmtRestarted { generation, kind } => {
            serial::write_str("RAYNU-V-AUDIT: MgmtRestarted gen=");
            write_u32(generation);
            serial::write_str(" kind=");
            write_u32(kind as u32);
            serial::write_byte(b'\n');
        }
        AuditEvent::CdromAttached { iso_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: CdromAttached iso_id=");
            write_u64(iso_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::CdromGuestVisible { iso_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: CdromGuestVisible iso_id=");
            write_u64(iso_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::CdromFirmwareArmed { iso_id, .. } => {
            serial::write_str("RAYNU-V-AUDIT: CdromFirmwareArmed iso_id=");
            write_u64(iso_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::GuestFirmwareBoxed {
            uncompressed_len, ..
        } => {
            serial::write_str("RAYNU-V-AUDIT: GuestFirmwareBoxed uncompressed=");
            write_u64(uncompressed_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::GuestFirmwareLoaded { payload_len } => {
            serial::write_str("RAYNU-V-AUDIT: GuestFirmwareLoaded payload=");
            write_u64(payload_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareProbed { fv_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareProbed fv_len=");
            write_u64(fv_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareEspLoaded { bytes_len, fv_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareEspLoaded bytes=");
            write_u64(bytes_len);
            serial::write_str(" fv_len=");
            write_u64(fv_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareSlotArmed { slot_id } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareSlotArmed slot=");
            write_u64(slot_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareGuestBound { guest_id, slot_id } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareGuestBound guest=");
            write_u64(guest_id);
            serial::write_str(" slot=");
            write_u64(slot_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareLaunchPrepared { guest_id, slot_id } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareLaunchPrepared guest=");
            write_u64(guest_id);
            serial::write_str(" slot=");
            write_u64(slot_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareFloorStaged { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareFloorStaged bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareEdk2Staged { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareEdk2Staged bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfEspLaunchArmed { guest_id, slot_id } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfEspLaunchArmed guest=");
            write_u64(guest_id);
            serial::write_str(" slot=");
            write_u64(slot_id);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfEspLiveMapped { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfEspLiveMapped bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfResetVectorArmed { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfResetVectorArmed bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfFirmwareAliasArmed { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfFirmwareAliasArmed bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfAliasEptProgrammed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfAliasEptProgrammed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfAliasEptInstalled { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfAliasEptInstalled bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfRealEspQualified { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfRealEspQualified bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfRealLaunchArmed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfRealLaunchArmed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspRequired { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspRequired bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfPrivateVmcsArmed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfPrivateVmcsArmed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveIssueArmed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveIssueArmed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveBytesProbed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveBytesProbed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveFdRequired { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveFdRequired bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspPresented { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspPresented bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspAdmitted { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspAdmitted bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspRead { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspRead bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspCopied { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspCopied bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspPlaced { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspPlaced bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspApplied { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspApplied bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspCommitted { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspCommitted bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspLatched { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspLatched bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspSealed { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspSealed bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspLocked { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspLocked bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspHeld { bytes_len, gpa } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspHeld bytes=");
            write_u64(bytes_len);
            serial::write_str(" gpa=0x");
            write_u64(gpa);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfLiveEspBytesRetained { bytes_len } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfLiveEspBytesRetained bytes=");
            write_u64(bytes_len);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiVmlaunched {
            exit_reason,
            guest_rip,
        } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiVmlaunched reason=0x");
            write_u64(exit_reason);
            serial::write_str(" rip=0x");
            write_u64(guest_rip);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiAlive { exits, last_reason } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiAlive exits=");
            write_u64(exits);
            serial::write_str(" reason=0x");
            write_u64(last_reason);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiPastSec {
            exits,
            linear,
            com_bytes,
        } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiPastSec exits=");
            write_u64(exits);
            serial::write_str(" linear=0x");
            write_u64(linear);
            serial::write_str(" com=");
            write_u64(com_bytes);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiCdrom {
            exits,
            pci_enum,
            sectors,
        } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiCdrom exits=");
            write_u64(exits);
            serial::write_str(" pci=");
            write_u64(pci_enum);
            serial::write_str(" sectors=");
            write_u64(sectors);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiDxe {
            exits,
            sectors,
            platform,
        } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiDxe exits=");
            write_u64(exits);
            serial::write_str(" sectors=");
            write_u64(sectors);
            serial::write_str(" plat=");
            write_u64(platform);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiVirtio { exits, pci_enum } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiVirtio exits=");
            write_u64(exits);
            serial::write_str(" pci=");
            write_u64(pci_enum);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiBoth { exits, virtio, ide } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiBoth exits=");
            write_u64(exits);
            serial::write_str(" virtio=");
            write_u64(virtio);
            serial::write_str(" ide=");
            write_u64(ide);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiAtapi { exits, sectors } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiAtapi exits=");
            write_u64(exits);
            serial::write_str(" sectors=");
            write_u64(sectors);
            serial::write_byte(b'\n');
        }
        AuditEvent::OvmfGuestUefiEltorito {
            exits,
            catalog,
            boot_image,
        } => {
            serial::write_str("RAYNU-V-AUDIT: OvmfGuestUefiEltorito exits=");
            write_u64(exits);
            serial::write_str(" catalog=");
            write_u64(catalog);
            serial::write_str(" bootimg=");
            write_u64(boot_image);
            serial::write_byte(b'\n');
        }
        AuditEvent::FrameAllocated { .. } | AuditEvent::FrameFreed { .. } => {}
    }
}

#[cfg(target_os = "uefi")]
fn milestone_tag(m: Milestone) -> u32 {
    match m {
        Milestone::M0 => 0,
        Milestone::M1 => 1,
        Milestone::M2 => 2,
        Milestone::M3 => 3,
        Milestone::M4 => 4,
        Milestone::M5 => 5,
        Milestone::M55 => 55,
        Milestone::M6 => 6,
    }
}

#[cfg(target_os = "uefi")]
fn write_u32(n: u32) {
    write_u64(n as u64);
}

#[cfg(target_os = "uefi")]
fn write_u64(mut n: u64) {
    use crate::boot::serial;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        serial::write_byte(buf[i]);
    }
}

#[cfg(target_os = "uefi")]
fn write_u32_hex(n: u32) {
    use crate::boot::serial;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for shift in (0..8).rev() {
        let nib = ((n >> (shift * 4)) & 0xf) as usize;
        serial::write_byte(HEX[nib]);
    }
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

/// Test-only: reset the shared boot ring so length-delta asserts stay reliable
/// across a full `cargo test --lib` suite (fixed [`AUDIT_RING_CAP`]).
#[cfg(test)]
pub fn boot_ring_reset_for_test() {
    with_boot_ring(|ring| *ring = AuditRing::new());
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
