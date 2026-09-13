//! FAT12/16/32 reader for RayNu-F's `SimpleFileSystem`.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! The volume is the El Torito EFI boot image embedded in the product ISO — a
//! FAT filesystem whose extent comes from [`crate::mgmt::el_torito`]. This
//! module reads it through a caller-supplied `read` closure so it works over
//! the CD `BlockIo` backing store, a host `Vec`, or the install disk.
//!
//! Scope is deliberate: enough to resolve `\EFI\BOOT\BOOTX64.EFI` and read it.
//! All three components are valid 8.3 short names, so **long-file-name (LFN)
//! entries are parsed only far enough to skip them** — we match on short
//! names. A loader needing LFN lookup is a later refinement, recorded honestly
//! rather than faked.

/// Directory entry size.
pub const DIR_ENTRY_SIZE: usize = 32;
/// Attribute bits.
pub const ATTR_READ_ONLY: u8 = 0x01;
pub const ATTR_HIDDEN: u8 = 0x02;
pub const ATTR_SYSTEM: u8 = 0x04;
pub const ATTR_VOLUME_ID: u8 = 0x08;
pub const ATTR_DIRECTORY: u8 = 0x10;
pub const ATTR_ARCHIVE: u8 = 0x20;
/// An LFN entry has all four of RO|HIDDEN|SYSTEM|VOLUME_ID set.
pub const ATTR_LONG_NAME: u8 = 0x0F;
/// Deleted / end-of-directory markers.
pub const DIR_ENTRY_FREE: u8 = 0xE5;
pub const DIR_ENTRY_END: u8 = 0x00;
/// Cap on cluster-chain hops so a cyclic FAT cannot spin forever.
pub const MAX_CHAIN_HOPS: u32 = 1 << 20;
/// Longest path we resolve (components).
pub const MAX_PATH_COMPONENTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatKind {
    Fat12,
    Fat16,
    Fat32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatError {
    ShortRead,
    BadBpb,
    NotFound,
    NotADirectory,
    IsADirectory,
    BadChain,
    PathTooLong,
}

/// Parsed BIOS Parameter Block plus derived geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatVolume {
    pub kind: FatKind,
    pub bytes_per_sector: u32,
    pub sectors_per_cluster: u32,
    pub reserved_sectors: u32,
    pub num_fats: u32,
    pub root_entries: u32,
    pub total_sectors: u32,
    pub fat_sectors: u32,
    /// First sector of the FAT region.
    pub fat_start: u32,
    /// First sector of the fixed root directory (FAT12/16 only).
    pub root_start: u32,
    pub root_sectors: u32,
    /// First sector of the data region (cluster 2).
    pub data_start: u32,
    pub cluster_count: u32,
    /// FAT32 root directory first cluster.
    pub root_cluster: u32,
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}

/// Parse the BPB from the volume's first 512 bytes.
pub fn parse_bpb(boot: &[u8]) -> Result<FatVolume, FatError> {
    if boot.len() < 512 {
        return Err(FatError::ShortRead);
    }
    let bytes_per_sector = u32::from(u16_at(boot, 11));
    let sectors_per_cluster = u32::from(boot[13]);
    let reserved_sectors = u32::from(u16_at(boot, 14));
    let num_fats = u32::from(boot[16]);
    let root_entries = u32::from(u16_at(boot, 17));
    let total16 = u32::from(u16_at(boot, 19));
    let fat16_sectors = u32::from(u16_at(boot, 22));
    let total32 = u32_at(boot, 32);
    let fat32_sectors = u32_at(boot, 36);
    let root_cluster = u32_at(boot, 44);

    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096)
        || !sectors_per_cluster.is_power_of_two()
        || sectors_per_cluster == 0
        || sectors_per_cluster > 128
        || reserved_sectors == 0
        || !(1..=2).contains(&num_fats)
    {
        return Err(FatError::BadBpb);
    }
    let total_sectors = if total16 != 0 { total16 } else { total32 };
    let fat_sectors = if fat16_sectors != 0 {
        fat16_sectors
    } else {
        fat32_sectors
    };
    if total_sectors == 0 || fat_sectors == 0 {
        return Err(FatError::BadBpb);
    }
    // Root dir sectors (0 for FAT32).
    let root_sectors =
        (root_entries * DIR_ENTRY_SIZE as u32 + bytes_per_sector - 1) / bytes_per_sector;
    let fat_start = reserved_sectors;
    let root_start = fat_start + num_fats * fat_sectors;
    let data_start = root_start + root_sectors;
    if data_start >= total_sectors {
        return Err(FatError::BadBpb);
    }
    let cluster_count = (total_sectors - data_start) / sectors_per_cluster;
    // FAT type is defined by cluster count (Microsoft FAT spec).
    let kind = if cluster_count < 4085 {
        FatKind::Fat12
    } else if cluster_count < 65525 {
        FatKind::Fat16
    } else {
        FatKind::Fat32
    };
    if kind == FatKind::Fat32 && (root_entries != 0 || root_cluster < 2) {
        return Err(FatError::BadBpb);
    }
    if kind != FatKind::Fat32 && root_entries == 0 {
        return Err(FatError::BadBpb);
    }
    Ok(FatVolume {
        kind,
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        root_entries,
        total_sectors,
        fat_sectors,
        fat_start,
        root_start,
        root_sectors,
        data_start,
        cluster_count,
        root_cluster,
    })
}

impl FatVolume {
    pub const fn cluster_bytes(&self) -> u32 {
        self.bytes_per_sector * self.sectors_per_cluster
    }

    /// Byte offset of a data cluster within the volume.
    pub const fn cluster_offset(&self, cluster: u32) -> u64 {
        let sector = self.data_start + (cluster - 2) * self.sectors_per_cluster;
        sector as u64 * self.bytes_per_sector as u64
    }

    /// End-of-chain test for this FAT width.
    pub const fn is_end_of_chain(&self, entry: u32) -> bool {
        match self.kind {
            FatKind::Fat12 => entry >= 0xFF8,
            FatKind::Fat16 => entry >= 0xFFF8,
            FatKind::Fat32 => (entry & 0x0FFF_FFFF) >= 0x0FFF_FFF8,
        }
    }

    /// Whether a cluster number can name real data.
    pub const fn cluster_is_valid(&self, cluster: u32) -> bool {
        cluster >= 2 && cluster < self.cluster_count + 2
    }
}

/// One resolved directory entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatEntry {
    /// 8.3 name, upper-case, no padding, dot only when an extension exists.
    pub name: [u8; 12],
    pub name_len: usize,
    pub attr: u8,
    pub first_cluster: u32,
    pub size: u32,
}

impl FatEntry {
    pub fn is_dir(&self) -> bool {
        self.attr & ATTR_DIRECTORY != 0
    }
    pub fn name_bytes(&self) -> &[u8] {
        &self.name[..self.name_len]
    }
}

/// Render a raw 11-byte 8.3 field as `NAME.EXT`.
pub fn short_name(raw: &[u8]) -> ([u8; 12], usize) {
    let mut out = [0u8; 12];
    let mut n = 0usize;
    for &c in raw[..8].iter() {
        if c == b' ' {
            break;
        }
        out[n] = c.to_ascii_uppercase();
        n += 1;
    }
    let has_ext = raw[8..11].iter().any(|&c| c != b' ');
    if has_ext {
        out[n] = b'.';
        n += 1;
        for &c in raw[8..11].iter() {
            if c == b' ' {
                break;
            }
            out[n] = c.to_ascii_uppercase();
            n += 1;
        }
    }
    (out, n)
}

/// Decode a 32-byte directory entry. `None` for free/LFN/end entries.
pub fn parse_dir_entry(e: &[u8]) -> Option<FatEntry> {
    if e.len() < DIR_ENTRY_SIZE || e[0] == DIR_ENTRY_END || e[0] == DIR_ENTRY_FREE {
        return None;
    }
    let attr = e[11];
    if attr & ATTR_LONG_NAME == ATTR_LONG_NAME {
        return None; // LFN slot: skipped (we match short names)
    }
    if attr & ATTR_VOLUME_ID != 0 {
        return None; // volume label
    }
    let (name, name_len) = short_name(&e[..11]);
    let hi = u32::from(u16_at(e, 20));
    let lo = u32::from(u16_at(e, 26));
    Some(FatEntry {
        name,
        name_len,
        attr,
        first_cluster: (hi << 16) | lo,
        size: u32_at(e, 28),
    })
}

/// Volume reader: `read(offset, buf) -> bool`, offsets relative to the FAT
/// volume start (the El Torito boot image, not the ISO).
pub trait VolumeRead {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool;
}

/// Next cluster in the chain.
pub fn next_cluster<R: VolumeRead>(
    vol: &FatVolume,
    r: &R,
    cluster: u32,
) -> Result<u32, FatError> {
    let fat_base = vol.fat_start as u64 * vol.bytes_per_sector as u64;
    match vol.kind {
        FatKind::Fat32 => {
            let off = fat_base + cluster as u64 * 4;
            let mut b = [0u8; 4];
            if !r.read_at(off, &mut b) {
                return Err(FatError::ShortRead);
            }
            Ok(u32::from_le_bytes(b) & 0x0FFF_FFFF)
        }
        FatKind::Fat16 => {
            let off = fat_base + cluster as u64 * 2;
            let mut b = [0u8; 2];
            if !r.read_at(off, &mut b) {
                return Err(FatError::ShortRead);
            }
            Ok(u32::from(u16::from_le_bytes(b)))
        }
        FatKind::Fat12 => {
            // 12 bits per entry: byte offset = cluster * 3 / 2.
            let off = fat_base + (cluster as u64 * 3) / 2;
            let mut b = [0u8; 2];
            if !r.read_at(off, &mut b) {
                return Err(FatError::ShortRead);
            }
            let raw = u16::from_le_bytes(b);
            Ok(u32::from(if cluster & 1 == 0 {
                raw & 0x0FFF
            } else {
                raw >> 4
            }))
        }
    }
}

/// Read `buf.len()` bytes starting `offset` bytes into a cluster chain.
pub fn read_chain<R: VolumeRead>(
    vol: &FatVolume,
    r: &R,
    first_cluster: u32,
    offset: u64,
    buf: &mut [u8],
) -> Result<usize, FatError> {
    if buf.is_empty() {
        return Ok(0);
    }
    let cbytes = vol.cluster_bytes() as u64;
    if cbytes == 0 {
        return Err(FatError::BadBpb);
    }
    // Walk to the cluster holding `offset`.
    let mut cluster = first_cluster;
    let mut skip = offset / cbytes;
    let mut hops = 0u32;
    while skip > 0 {
        if !vol.cluster_is_valid(cluster) {
            return Err(FatError::BadChain);
        }
        cluster = next_cluster(vol, r, cluster)?;
        if vol.is_end_of_chain(cluster) {
            return Ok(0); // offset past EOF
        }
        skip -= 1;
        hops += 1;
        if hops > MAX_CHAIN_HOPS {
            return Err(FatError::BadChain);
        }
    }
    let mut done = 0usize;
    let mut within = offset % cbytes;
    while done < buf.len() {
        if !vol.cluster_is_valid(cluster) {
            return Err(FatError::BadChain);
        }
        let take = ((cbytes - within) as usize).min(buf.len() - done);
        let at = vol.cluster_offset(cluster) + within;
        if !r.read_at(at, &mut buf[done..done + take]) {
            return Err(FatError::ShortRead);
        }
        done += take;
        within = 0;
        if done == buf.len() {
            break;
        }
        cluster = next_cluster(vol, r, cluster)?;
        if vol.is_end_of_chain(cluster) {
            break;
        }
        hops += 1;
        if hops > MAX_CHAIN_HOPS {
            return Err(FatError::BadChain);
        }
    }
    Ok(done)
}

/// Find `name` (8.3, upper-case) in a directory. `dir_cluster == 0` means the
/// FAT12/16 fixed root.
pub fn find_in_dir<R: VolumeRead>(
    vol: &FatVolume,
    r: &R,
    dir_cluster: u32,
    name: &[u8],
) -> Result<FatEntry, FatError> {
    let mut e = [0u8; DIR_ENTRY_SIZE];
    if dir_cluster == 0 && vol.kind != FatKind::Fat32 {
        let base = vol.root_start as u64 * vol.bytes_per_sector as u64;
        for i in 0..vol.root_entries as u64 {
            if !r.read_at(base + i * DIR_ENTRY_SIZE as u64, &mut e) {
                return Err(FatError::ShortRead);
            }
            if e[0] == DIR_ENTRY_END {
                break;
            }
            if let Some(ent) = parse_dir_entry(&e) {
                if ent.name_bytes().eq_ignore_ascii_case(name) {
                    return Ok(ent);
                }
            }
        }
        return Err(FatError::NotFound);
    }
    let start = if dir_cluster == 0 {
        vol.root_cluster
    } else {
        dir_cluster
    };
    let per_cluster = (vol.cluster_bytes() / DIR_ENTRY_SIZE as u32) as u64;
    let mut cluster = start;
    let mut hops = 0u32;
    loop {
        if !vol.cluster_is_valid(cluster) {
            return Err(FatError::BadChain);
        }
        let base = vol.cluster_offset(cluster);
        for i in 0..per_cluster {
            if !r.read_at(base + i * DIR_ENTRY_SIZE as u64, &mut e) {
                return Err(FatError::ShortRead);
            }
            if e[0] == DIR_ENTRY_END {
                return Err(FatError::NotFound);
            }
            if let Some(ent) = parse_dir_entry(&e) {
                if ent.name_bytes().eq_ignore_ascii_case(name) {
                    return Ok(ent);
                }
            }
        }
        cluster = next_cluster(vol, r, cluster)?;
        if vol.is_end_of_chain(cluster) {
            return Err(FatError::NotFound);
        }
        hops += 1;
        if hops > MAX_CHAIN_HOPS {
            return Err(FatError::BadChain);
        }
    }
}

/// Resolve a `\`-separated path (e.g. `\EFI\BOOT\BOOTX64.EFI`) to its entry.
/// Leading/duplicate separators are ignored; `/` is accepted too.
pub fn resolve_path<R: VolumeRead>(
    vol: &FatVolume,
    r: &R,
    path: &[u8],
) -> Result<FatEntry, FatError> {
    let mut dir_cluster = 0u32; // root
    let mut last: Option<FatEntry> = None;
    let mut comps = 0usize;
    let mut i = 0usize;
    while i < path.len() {
        while i < path.len() && (path[i] == b'\\' || path[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < path.len() && path[i] != b'\\' && path[i] != b'/' {
            i += 1;
        }
        if i == start {
            continue;
        }
        comps += 1;
        if comps > MAX_PATH_COMPONENTS {
            return Err(FatError::PathTooLong);
        }
        // A previous component must have been a directory.
        if let Some(prev) = last {
            if !prev.is_dir() {
                return Err(FatError::NotADirectory);
            }
            dir_cluster = prev.first_cluster;
        }
        let ent = find_in_dir(vol, r, dir_cluster, &path[start..i])?;
        last = Some(ent);
    }
    last.ok_or(FatError::NotFound)
}
