//! Durable image of the audit hash chain (ADR-021 G4).
//!
//! Pillar: [A]
//! Proven Core: **outside** the append path. This file only copies sealed
//! links out and rebuilds them. The hash still ignores event payloads, so a
//! restored link keeps its kind and its hash, with zeroed fields.
//!
//! The image is 16 sectors. It sits immediately before the NIC catalog on
//! RAYNU-SPARE. The catalog LBA does not move.

use crate::audit::integrity::{
    event_discriminant, hash_record, AuditEvent, AuditRing, Milestone, AUDIT_GENESIS_HASH,
    AUDIT_RING_CAP,
};

pub const IMAGE_BYTES: usize = 8192;
const MAGIC: &[u8; 8] = b"RAYNUAUD";
const VERSION: u8 = 1;
const HEADER: usize = 32;
const REC: usize = 24;
const CHECK_OFF: usize = IMAGE_BYTES - 4;

/// Write `ring` into `buf`. `false` when `buf` is short.
pub fn encode_ring(ring: &AuditRing, buf: &mut [u8]) -> bool {
    if buf.len() < IMAGE_BYTES || ring.len() > AUDIT_RING_CAP {
        return false;
    }
    buf[..IMAGE_BYTES].fill(0);
    buf[..8].copy_from_slice(MAGIC);
    buf[8] = VERSION;
    buf[10..12].copy_from_slice(&(ring.len() as u16).to_le_bytes());
    buf[12..20].copy_from_slice(&ring_next_seq(ring).to_le_bytes());
    buf[20..28].copy_from_slice(&ring.tip_hash().to_le_bytes());
    for i in 0..ring.len() {
        let rec = ring.get(i).unwrap();
        let at = HEADER + i * REC;
        buf[at..at + 8].copy_from_slice(&rec.seq.to_le_bytes());
        buf[at + 8..at + 16].copy_from_slice(&event_discriminant(rec.event).to_le_bytes());
        buf[at + 16..at + 24].copy_from_slice(&rec.prev_hash.to_le_bytes());
        // hash is checked on decode by re-appending; stored so a torn
        // image cannot claim a different tip.
        let _ = rec.hash;
    }
    // The stored hash sits in the last 8 bytes of each record. REC is 24,
    // so prev occupies 16..24 and the hash is not in the record. Re-append
    // recomputes it. Keep the tip in the header as the check.
    let sum = checksum(&buf[..CHECK_OFF]);
    buf[CHECK_OFF..CHECK_OFF + 4].copy_from_slice(&sum.to_le_bytes());
    let _ = HEADER;
    true
}

fn ring_next_seq(ring: &AuditRing) -> u64 {
    ring.get(ring.len().saturating_sub(1))
        .map(|r| r.seq.saturating_add(1))
        .unwrap_or(0)
}

/// Rebuild a ring from an image. `None` when the magic, checksum, or chain fails.
pub fn decode_ring(image: &[u8]) -> Option<AuditRing> {
    if image.len() < IMAGE_BYTES || &image[..8] != MAGIC || image[8] != VERSION {
        return None;
    }
    let sum = u32::from_le_bytes(image[CHECK_OFF..CHECK_OFF + 4].try_into().ok()?);
    if sum != checksum(&image[..CHECK_OFF]) {
        return None;
    }
    let count = u16::from_le_bytes(image[10..12].try_into().ok()?) as usize;
    if count > AUDIT_RING_CAP {
        return None;
    }
    let mut ring = AuditRing::new();
    for i in 0..count {
        let at = HEADER + i * REC;
        let seq = u64::from_le_bytes(image[at..at + 8].try_into().ok()?);
        let disc = u64::from_le_bytes(image[at + 8..at + 16].try_into().ok()?);
        let prev = u64::from_le_bytes(image[at + 16..at + 24].try_into().ok()?);
        let event = event_from_disc(disc)?;
        let got = ring.append(event).ok()?;
        if got.seq != seq || got.prev_hash != prev {
            return None;
        }
        if got.hash != hash_record(seq, event, prev) {
            return None;
        }
    }
    let tip = u64::from_le_bytes(image[20..28].try_into().ok()?);
    if ring.tip_hash() != tip {
        return None;
    }
    if count == 0 && tip != AUDIT_GENESIS_HASH {
        return None;
    }
    Some(ring)
}

struct Staged(core::cell::UnsafeCell<[u8; IMAGE_BYTES]>);
// SAFETY: exclusive access is `STAGED_LOCK`.
// KANI-TARGET: host image round-trip, not this cell.
unsafe impl Sync for Staged {}
static STAGED: Staged = Staged(core::cell::UnsafeCell::new([0u8; IMAGE_BYTES]));
static STAGED_LOCK: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);
static STAGED_GEN: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
static STAGED_ON: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

fn with_staged<R>(f: impl FnOnce(&mut [u8; IMAGE_BYTES]) -> R) -> R {
    while STAGED_LOCK.swap(true, core::sync::atomic::Ordering::Acquire) {
        core::hint::spin_loop();
    }
    // SAFETY: lock held.
    let out = unsafe { f(&mut *STAGED.0.get()) };
    STAGED_LOCK.store(false, core::sync::atomic::Ordering::Release);
    out
}

/// Copy the current boot ring into the staged image.
pub fn stage_boot_ring() {
    with_staged(|buf| {
        if crate::audit::integrity::encode_boot_ring(buf) {
            STAGED_GEN.fetch_add(1, core::sync::atomic::Ordering::Release);
            STAGED_ON.store(true, core::sync::atomic::Ordering::Release);
        }
    });
}

/// One 512-byte piece of the staged image, with the generation that wrote it.
pub fn staged_sector(index: usize) -> Option<(u32, [u8; 512])> {
    if !STAGED_ON.load(core::sync::atomic::Ordering::Acquire) || index >= IMAGE_BYTES / 512 {
        return None;
    }
    let mut sector = [0u8; 512];
    let gen = with_staged(|buf| {
        let gen = STAGED_GEN.load(core::sync::atomic::Ordering::Acquire);
        let at = index * 512;
        sector.copy_from_slice(&buf[at..at + 512]);
        gen
    });
    Some((gen, sector))
}

pub fn ack_staged(gen: u32) {
    if STAGED_GEN.load(core::sync::atomic::Ordering::Acquire) == gen {
        STAGED_ON.store(false, core::sync::atomic::Ordering::Release);
    }
}

pub fn staged_on() -> bool {
    STAGED_ON.load(core::sync::atomic::Ordering::Acquire)
}

/// Whether a flush that finished at `last` may run again at `now`.
/// `last == 0` has never flushed. Otherwise `gap` TSC ticks must have passed.
/// Wrapping-safe for a free-running counter.
pub fn flush_interval_elapsed(last: u64, now: u64, gap: u64) -> bool {
    last == 0 || now.wrapping_sub(last) >= gap
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |a, b| a.wrapping_add(u32::from(*b)))
}

/// A zero-payload event of kind `disc`. The hash uses only this kind.
pub fn event_from_disc(disc: u64) -> Option<AuditEvent> {
    Some(match disc {
        1 => AuditEvent::BootStarted { milestone: Milestone::M0 },
        2 => AuditEvent::VmxEnabled { vcpu_id: 0 },
        3 => AuditEvent::VmcsCreated { vcpu_id: 0, vmcs_id: 0 },
        4 => AuditEvent::EptMapped { guest_id: 0, gpa: 0, hpa: 0 },
        5 => AuditEvent::EptUnmapped { guest_id: 0, gpa: 0, hpa: 0 },
        6 => AuditEvent::MsrBlocked { vcpu_id: 0, msr_index: 0 },
        7 => AuditEvent::FrameAllocated { frame: 0 },
        8 => AuditEvent::FrameFreed { frame: 0 },
        9 => AuditEvent::VmCreated { guest_id: 0 },
        10 => AuditEvent::VmStarted { guest_id: 0 },
        11 => AuditEvent::VmStopped { guest_id: 0 },
        12 => AuditEvent::VmDestroyed { guest_id: 0 },
        13 => AuditEvent::MigrateStarted { batch_id: 0, count: 0 },
        14 => AuditEvent::MigrateCompleted { batch_id: 0, count: 0 },
        15 => AuditEvent::MigrateFailed { batch_id: 0, count: 0 },
        16 => AuditEvent::AuthAllowed { method_tag: 0 },
        17 => AuditEvent::AuthDenied { method_tag: 0 },
        18 => AuditEvent::HaFailoverStarted { from_role: 0, to_role: 0 },
        19 => AuditEvent::HaFailoverCompleted { guest_count: 0 },
        20 => AuditEvent::FaultInjected { kind: 0, detail: 0 },
        21 => AuditEvent::FaultRecovered { kind: 0, detail: 0 },
        22 => AuditEvent::FaultFailClosed { kind: 0, detail: 0 },
        23 => AuditEvent::SoakStarted { target_hours: 0 },
        24 => AuditEvent::SoakCompleted { hours: 0 },
        25 => AuditEvent::SoakFailed { hours: 0 },
        26 => AuditEvent::EvidenceModeActivated { source: 0 },
        27 => AuditEvent::MgmtRestarted { generation: 0, kind: 0 },
        28 => AuditEvent::CdromAttached { iso_id: 0, load_lba: 0 },
        29 => AuditEvent::CdromFirmwareArmed { iso_id: 0, load_lba: 0 },
        30 => AuditEvent::GuestFirmwareBoxed { uncompressed_len: 0, compressed_len: 0 },
        31 => AuditEvent::GuestFirmwareLoaded { payload_len: 0 },
        32 => AuditEvent::OvmfFirmwareProbed { fv_len: 0 },
        33 => AuditEvent::OvmfFirmwareEspLoaded { bytes_len: 0, fv_len: 0 },
        34 => AuditEvent::OvmfFirmwareSlotArmed { slot_id: 0 },
        35 => AuditEvent::OvmfFirmwareGuestBound { guest_id: 0, slot_id: 0 },
        36 => AuditEvent::OvmfFirmwareLaunchPrepared { guest_id: 0, slot_id: 0 },
        37 => AuditEvent::OvmfFirmwareFloorStaged { bytes_len: 0 },
        38 => AuditEvent::OvmfFirmwareEdk2Staged { bytes_len: 0 },
        39 => AuditEvent::OvmfEspLaunchArmed { guest_id: 0, slot_id: 0 },
        40 => AuditEvent::OvmfEspLiveMapped { bytes_len: 0 },
        41 => AuditEvent::OvmfResetVectorArmed { bytes_len: 0 },
        42 => AuditEvent::OvmfFirmwareAliasArmed { bytes_len: 0 },
        43 => AuditEvent::OvmfAliasEptProgrammed { bytes_len: 0, gpa: 0 },
        44 => AuditEvent::OvmfAliasEptInstalled { bytes_len: 0, gpa: 0 },
        45 => AuditEvent::OvmfRealEspQualified { bytes_len: 0, gpa: 0 },
        46 => AuditEvent::OvmfRealLaunchArmed { bytes_len: 0, gpa: 0 },
        47 => AuditEvent::OvmfLiveEspRequired { bytes_len: 0, gpa: 0 },
        48 => AuditEvent::OvmfPrivateVmcsArmed { bytes_len: 0, gpa: 0 },
        49 => AuditEvent::OvmfLiveIssueArmed { bytes_len: 0, gpa: 0 },
        50 => AuditEvent::OvmfLiveBytesProbed { bytes_len: 0, gpa: 0 },
        51 => AuditEvent::OvmfLiveFdRequired { bytes_len: 0, gpa: 0 },
        52 => AuditEvent::OvmfLiveEspPresented { bytes_len: 0, gpa: 0 },
        53 => AuditEvent::OvmfLiveEspAdmitted { bytes_len: 0, gpa: 0 },
        54 => AuditEvent::OvmfLiveEspRead { bytes_len: 0, gpa: 0 },
        55 => AuditEvent::OvmfLiveEspCopied { bytes_len: 0, gpa: 0 },
        56 => AuditEvent::OvmfLiveEspPlaced { bytes_len: 0, gpa: 0 },
        57 => AuditEvent::OvmfLiveEspApplied { bytes_len: 0, gpa: 0 },
        58 => AuditEvent::OvmfLiveEspCommitted { bytes_len: 0, gpa: 0 },
        59 => AuditEvent::OvmfLiveEspLatched { bytes_len: 0, gpa: 0 },
        60 => AuditEvent::OvmfLiveEspSealed { bytes_len: 0, gpa: 0 },
        61 => AuditEvent::OvmfLiveEspLocked { bytes_len: 0, gpa: 0 },
        62 => AuditEvent::OvmfLiveEspHeld { bytes_len: 0, gpa: 0 },
        63 => AuditEvent::OvmfLiveEspBytesRetained { bytes_len: 0 },
        64 => AuditEvent::OvmfGuestUefiVmlaunched { exit_reason: 0, guest_rip: 0 },
        65 => AuditEvent::OvmfGuestUefiAlive { exits: 0, last_reason: 0 },
        66 => AuditEvent::OvmfGuestUefiPastSec { exits: 0, linear: 0, com_bytes: 0 },
        67 => AuditEvent::CdromGuestVisible { iso_id: 0, load_lba: 0 },
        68 => AuditEvent::OvmfGuestUefiCdrom { exits: 0, pci_enum: 0, sectors: 0 },
        69 => AuditEvent::OvmfGuestUefiDxe { exits: 0, sectors: 0, platform: 0 },
        70 => AuditEvent::OvmfGuestUefiVirtio { exits: 0, pci_enum: 0 },
        71 => AuditEvent::OvmfGuestUefiBoth { exits: 0, virtio: 0, ide: 0 },
        72 => AuditEvent::OvmfGuestUefiAtapi { exits: 0, sectors: 0 },
        73 => AuditEvent::OvmfGuestUefiEltorito { exits: 0, catalog: 0, boot_image: 0 },
        74 => AuditEvent::HostPowerOff { source: 0 },
        75 => AuditEvent::PercSpaChoice { choice: 0 },
        76 => AuditEvent::GuestNicSaved { spare_off: 0, mode: 0 },
        77 => AuditEvent::GuestNicDetached { spare_off: 0 },
        78 => AuditEvent::HostLinkChanged { func: 0, up: 0, mbps: 0 },
        79 => AuditEvent::HostAddrSaved { mode: 0, prefix: 0 },
        _ => return None,
    })
}

#[cfg(test)]
mod persist_test {
    use super::*;
    use crate::audit::integrity::AuditEvent;

    #[test]
    fn g4_chain_continues_after_the_image_is_restored() {
        let mut boot1 = AuditRing::new();
        boot1.append(AuditEvent::AuthAllowed { method_tag: 1 }).unwrap();
        boot1
            .append(AuditEvent::GuestNicSaved { spare_off: 99, mode: 2 })
            .unwrap();
        let tip = boot1.tip_hash();
        let mut image = [0u8; IMAGE_BYTES];
        assert!(encode_ring(&boot1, &mut image));
        image[100] ^= 0xff;
        assert!(decode_ring(&image).is_none(), "a flipped byte must not load");
        image[100] ^= 0xff;
        let mut boot2 = AuditRing::new();
        boot2
            .append(AuditEvent::BootStarted { milestone: Milestone::M0 })
            .unwrap();
        assert!(boot2.adopt_image(&image));
        assert!(boot2.verify_chain());
        assert_eq!(boot2.len(), 3);
        assert_eq!(boot2.get(0).unwrap().prev_hash, AUDIT_GENESIS_HASH);
        assert_eq!(boot2.get(2).unwrap().prev_hash, tip);
        assert_eq!(boot2.get(1).unwrap().hash, tip);
    }

    #[test]
    fn flush_waits_out_the_gap_then_runs_again() {
        assert!(flush_interval_elapsed(0, 50, 100));
        assert!(!flush_interval_elapsed(100, 150, 100));
        assert!(flush_interval_elapsed(100, 200, 100));
        assert!(flush_interval_elapsed(u64::MAX - 10, 90, 100));
        assert!(!flush_interval_elapsed(u64::MAX - 10, 50, 100));
    }
}
