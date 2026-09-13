//! RayNu-F `EFI_BLOCK_IO_PROTOCOL` (UEFI 2.10 §13.9) for the CD and the
//! virtio-blk install target.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! Two devices are published in the protocol database:
//!
//! | Handle | Media | Blocks | Access | Backing |
//! |--------|-------|--------|--------|---------|
//! | `HANDLE_CD` | id 1, removable, present | 2048 B | read-only | retained product ISO |
//! | `HANDLE_DISK` | id 2, fixed, present | 512 B | read-write | virtio-blk install disk |
//!
//! Both share one set of trampolines; `This`/`MediaId` select the device, so
//! a loader that walks handles the architected way gets the right storage.
//! Backing access goes through plain `fn` pointers in `FirmwareState` — no
//! lifetimes, `no_std`-clean, and host tests supply their own.

/// `EFI_BLOCK_IO_MEDIA` field offsets (x64 C layout) and size.
pub const MEDIA_MEDIA_ID_OFF: usize = 0x00;
pub const MEDIA_REMOVABLE_OFF: usize = 0x04;
pub const MEDIA_PRESENT_OFF: usize = 0x05;
pub const MEDIA_LOGICAL_PARTITION_OFF: usize = 0x06;
pub const MEDIA_READ_ONLY_OFF: usize = 0x07;
pub const MEDIA_WRITE_CACHING_OFF: usize = 0x08;
pub const MEDIA_BLOCK_SIZE_OFF: usize = 0x0C;
pub const MEDIA_IO_ALIGN_OFF: usize = 0x10;
pub const MEDIA_LAST_BLOCK_OFF: usize = 0x18;
pub const MEDIA_LOWEST_ALIGNED_LBA_OFF: usize = 0x20;
pub const MEDIA_BLOCKS_PER_PHYS_OFF: usize = 0x28;
pub const MEDIA_OPTIMAL_GRANULARITY_OFF: usize = 0x2C;
pub const MEDIA_SIZE: usize = 0x30;

/// `EFI_BLOCK_IO_PROTOCOL` field offsets and size.
pub const BLOCKIO_REVISION_OFF: usize = 0x00;
pub const BLOCKIO_MEDIA_OFF: usize = 0x08;
pub const BLOCKIO_RESET_OFF: usize = 0x10;
pub const BLOCKIO_READ_OFF: usize = 0x18;
pub const BLOCKIO_WRITE_OFF: usize = 0x20;
pub const BLOCKIO_FLUSH_OFF: usize = 0x28;
pub const BLOCKIO_SIZE: usize = 0x30;

/// `EFI_BLOCK_IO_PROTOCOL_REVISION2` — `LowestAlignedLba` and
/// `LogicalBlocksPerPhysicalBlock` are valid; we fill them honestly.
pub const BLOCKIO_REVISION2: u64 = 0x0002_0001;

/// Media ids.
pub const MEDIA_ID_CD: u32 = 1;
pub const MEDIA_ID_DISK: u32 = 2;
/// Block sizes.
pub const CD_BLOCK_SIZE: u32 = 2048;
pub const DISK_BLOCK_SIZE: u32 = 512;
/// Largest single transfer we will service (2 MiB).
pub const MAX_TRANSFER_BYTES: u64 = 2 * 1024 * 1024;

pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_DEVICE_ERROR: u64 = 0x8000_0000_0000_0007;
pub const EFI_WRITE_PROTECTED: u64 = 0x8000_0000_0000_0008;
pub const EFI_NO_MEDIA: u64 = 0x8000_0000_0000_000C;
pub const EFI_MEDIA_CHANGED: u64 = 0x8000_0000_0000_000D;
pub const EFI_BAD_BUFFER_SIZE: u64 = 0x8000_0000_0000_0004;

/// One block device's media description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockMedia {
    pub media_id: u32,
    pub removable: bool,
    pub present: bool,
    pub read_only: bool,
    pub block_size: u32,
    /// Last valid LBA (`blocks - 1`); `present == false` when there is none.
    pub last_block: u64,
}

impl BlockMedia {
    /// CD media for a retained ISO of `bytes`.
    pub const fn cd(bytes: u64) -> Self {
        let blocks = bytes / CD_BLOCK_SIZE as u64;
        BlockMedia {
            media_id: MEDIA_ID_CD,
            removable: true,
            present: blocks > 0,
            read_only: true,
            block_size: CD_BLOCK_SIZE,
            last_block: if blocks > 0 { blocks - 1 } else { 0 },
        }
    }

    /// Install-disk media for a virtio-blk target of `bytes`.
    pub const fn disk(bytes: u64) -> Self {
        let blocks = bytes / DISK_BLOCK_SIZE as u64;
        BlockMedia {
            media_id: MEDIA_ID_DISK,
            removable: false,
            present: blocks > 0,
            read_only: false,
            block_size: DISK_BLOCK_SIZE,
            last_block: if blocks > 0 { blocks - 1 } else { 0 },
        }
    }

    /// Serialize into the `EFI_BLOCK_IO_MEDIA` layout.
    pub fn encode(&self, out: &mut [u8; MEDIA_SIZE]) {
        for b in out.iter_mut() {
            *b = 0;
        }
        out[MEDIA_MEDIA_ID_OFF..MEDIA_MEDIA_ID_OFF + 4]
            .copy_from_slice(&self.media_id.to_le_bytes());
        out[MEDIA_REMOVABLE_OFF] = self.removable as u8;
        out[MEDIA_PRESENT_OFF] = self.present as u8;
        out[MEDIA_LOGICAL_PARTITION_OFF] = 0; // whole device, not a partition
        out[MEDIA_READ_ONLY_OFF] = self.read_only as u8;
        out[MEDIA_WRITE_CACHING_OFF] = 0; // writes reach the backing store
        out[MEDIA_BLOCK_SIZE_OFF..MEDIA_BLOCK_SIZE_OFF + 4]
            .copy_from_slice(&self.block_size.to_le_bytes());
        out[MEDIA_IO_ALIGN_OFF..MEDIA_IO_ALIGN_OFF + 4].copy_from_slice(&1u32.to_le_bytes());
        out[MEDIA_LAST_BLOCK_OFF..MEDIA_LAST_BLOCK_OFF + 8]
            .copy_from_slice(&self.last_block.to_le_bytes());
        // Revision 2 fields: no alignment constraint, 1 logical per physical.
        out[MEDIA_LOWEST_ALIGNED_LBA_OFF..MEDIA_LOWEST_ALIGNED_LBA_OFF + 8]
            .copy_from_slice(&0u64.to_le_bytes());
        out[MEDIA_BLOCKS_PER_PHYS_OFF..MEDIA_BLOCKS_PER_PHYS_OFF + 4]
            .copy_from_slice(&1u32.to_le_bytes());
        out[MEDIA_OPTIMAL_GRANULARITY_OFF..MEDIA_OPTIMAL_GRANULARITY_OFF + 4]
            .copy_from_slice(&0u32.to_le_bytes());
    }
}

/// Validate a `ReadBlocks`/`WriteBlocks` request against media (spec §13.9).
/// Returns `EFI_SUCCESS` when the transfer may proceed.
pub fn validate_transfer(
    media: &BlockMedia,
    media_id: u32,
    lba: u64,
    buffer_size: u64,
    buffer: u64,
    write: bool,
) -> u64 {
    if !media.present {
        return EFI_NO_MEDIA;
    }
    if media_id != media.media_id {
        return EFI_MEDIA_CHANGED;
    }
    if write && media.read_only {
        return EFI_WRITE_PROTECTED;
    }
    if buffer_size == 0 {
        return EFI_SUCCESS;
    }
    if buffer == 0 {
        return EFI_INVALID_PARAMETER;
    }
    if buffer_size % media.block_size as u64 != 0 {
        return EFI_BAD_BUFFER_SIZE;
    }
    if buffer_size > MAX_TRANSFER_BYTES {
        return EFI_INVALID_PARAMETER;
    }
    let blocks = buffer_size / media.block_size as u64;
    // `lba + blocks - 1` must be a valid LBA; guard the add.
    let Some(end) = lba.checked_add(blocks) else {
        return EFI_INVALID_PARAMETER;
    };
    if end > media.last_block + 1 {
        return EFI_INVALID_PARAMETER;
    }
    EFI_SUCCESS
}

/// Byte offset of `lba` in the backing store.
pub fn lba_offset(media: &BlockMedia, lba: u64) -> u64 {
    lba.saturating_mul(media.block_size as u64)
}
