//! Read one Ubuntu initrd out of an installed ext4 filesystem.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016 / ADR-019)
//!
//! `826ca6e9` raised `*BufferSize` on the copy call. COM2 showed the stub
//! already passed the real length (`sz=34338680`, about 32.7 MiB) and GRUB
//! still returned a failure the stub prints as `EFI_LOAD_ERROR`. No
//! `blk fail` landed between that call and the error, so the bytes never
//! arrived through GRUB's `LoadFile`. This reader copies the file whose
//! size is that length. GRUB's size query stays the source of the length.
//! Not `ISO-INSTALL-OK`.

use super::fat::VolumeRead;
use super::gpt::{parse_gpt_header, parse_protective_mbr, GPT_LBA_SIZE, GPT_MAX_ENTRIES};

const EXT4_MAGIC: u16 = 0xEF53;
const EXTENT_MAGIC: u16 = 0xF30A;
const INCOMPAT_FILETYPE: u32 = 0x2;
const INCOMPAT_64BIT: u32 = 0x80;
const EXTENTS_FL: u32 = 0x0008_0000;
const S_IFMT: u16 = 0xF000;
const S_IFDIR: u16 = 0x4000;
const S_IFREG: u16 = 0x8000;
const S_IFLNK: u16 = 0xA000;
const FT_REG: u8 = 1;
const FT_DIR: u8 = 2;
const FT_LNK: u8 = 7;
const MAX_COPY: u64 = 128 * 1024 * 1024;
const MAX_PARTS: usize = 8;
const MAX_ENTS: usize = 16;
const MAX_DIR_BYTES: u64 = 256 * 1024;
const MAX_LINK: usize = 255;
const NAME_CAP: usize = 48;

/// A file copied into the caller's buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitrdFile {
    pub bytes: u64,
    pub name: [u8; NAME_CAP],
    pub name_len: usize,
}

/// Why the copy did not run. `saw_bytes` is the largest initrd-like file
/// that was found, when one was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InitrdMiss {
    pub saw_bytes: u64,
    pub saw_name: [u8; NAME_CAP],
    pub saw_name_len: usize,
    pub why: &'static str,
}

#[derive(Clone, Copy)]
struct Fs {
    part_off: u64,
    block_size: u64,
    inode_size: u64,
    inodes_per_group: u64,
    desc_size: u64,
    incompat: u32,
}

#[derive(Clone, Copy)]
struct Inode {
    mode: u16,
    flags: u32,
    size: u64,
    iblock: [u8; 60],
}

#[derive(Clone, Copy)]
struct Ent {
    ino: u32,
    kind: u8,
    name_len: u8,
    name: [u8; NAME_CAP],
}

struct Saw {
    bytes: u64,
    name: [u8; NAME_CAP],
    name_len: usize,
}

/// Copy the initrd whose size is `want` (GRUB's size query).
///
/// `scratch` must hold one filesystem block (4 KiB). `emit` receives the
/// file in order. A short `emit` or a short disk read is a miss.
pub fn copy_sized_initrd<R: VolumeRead, F: FnMut(&[u8]) -> bool>(
    disk: &R,
    want: u64,
    scratch: &mut [u8],
    mut emit: F,
) -> Result<InitrdFile, InitrdMiss> {
    if want == 0 || want > MAX_COPY {
        return Err(miss("size"));
    }
    if scratch.len() < 4096 {
        return Err(miss("scratch"));
    }
    let mut saw = Saw {
        bytes: 0,
        name: [0; NAME_CAP],
        name_len: 0,
    };
    let (parts, nparts) = match list_parts(disk) {
        Some(p) => p,
        None => return Err(miss("gpt")),
    };
    let mut any_fs = false;
    for i in 0..nparts {
        let Some(fs) = open_fs(disk, parts[i].0, scratch) else {
            continue;
        };
        any_fs = true;
        if let Some(hit) = search_dir(disk, &fs, 2, 0, want, scratch, &mut saw) {
            if stream_file(disk, &fs, &hit.0, scratch, &mut emit)? {
                return Ok(InitrdFile {
                    bytes: hit.0.size,
                    name: hit.1,
                    name_len: hit.2,
                });
            }
            return Err(miss_saw("read", &saw));
        }
    }
    Err(miss_saw(if any_fs { "nofile" } else { "nofs" }, &saw))
}

fn miss(why: &'static str) -> InitrdMiss {
    InitrdMiss {
        saw_bytes: 0,
        saw_name: [0; NAME_CAP],
        saw_name_len: 0,
        why,
    }
}

fn miss_saw(why: &'static str, saw: &Saw) -> InitrdMiss {
    InitrdMiss {
        saw_bytes: saw.bytes,
        saw_name: saw.name,
        saw_name_len: saw.name_len,
        why,
    }
}

fn note_saw(saw: &mut Saw, size: u64, name: &[u8]) {
    if size > saw.bytes {
        saw.bytes = size;
        saw.name = [0; NAME_CAP];
        let n = name.len().min(NAME_CAP);
        saw.name[..n].copy_from_slice(&name[..n]);
        saw.name_len = n;
    }
}

fn size_matches(file: u64, want: u64) -> bool {
    file == want || (want > file && want - file < 4)
}

fn is_initrd_name(name: &[u8]) -> bool {
    name.windows(6).any(|w| w == b"initrd")
}

fn list_parts<R: VolumeRead>(disk: &R) -> Option<([(u64, u64); MAX_PARTS], usize)> {
    let mut lba0 = [0u8; 512];
    if !disk.read_at(0, &mut lba0) || parse_protective_mbr(&lba0).is_err() {
        return None;
    }
    let mut lba1 = [0u8; 512];
    if !disk.read_at(u64::from(GPT_LBA_SIZE), &mut lba1) || parse_gpt_header(&lba1).is_err() {
        return None;
    }
    let hdr = parse_gpt_header(&lba1).ok()?;
    let n = hdr.number_of_entries.min(GPT_MAX_ENTRIES);
    let es = hdr.size_of_entry as usize;
    if es < 128 || es > 256 {
        return None;
    }
    let mut entry = [0u8; 256];
    let mut out = [(0u64, 0u64); MAX_PARTS];
    let mut count = 0usize;
    for i in 0..n {
        let off = hdr
            .partition_entry_lba
            .saturating_mul(u64::from(GPT_LBA_SIZE))
            .saturating_add(u64::from(i) * hdr.size_of_entry as u64);
        if !disk.read_at(off, &mut entry[..es]) {
            return None;
        }
        if entry[..16].iter().all(|&b| b == 0) {
            continue;
        }
        let start = u64_at(&entry, 32);
        let end = u64_at(&entry, 40);
        if start == 0 || end < start {
            continue;
        }
        if count < MAX_PARTS {
            out[count] = (start * 512, (end - start + 1) * 512);
            count += 1;
        }
    }
    Some((out, count))
}

fn open_fs<R: VolumeRead>(disk: &R, part_off: u64, scratch: &mut [u8]) -> Option<Fs> {
    if !disk.read_at(part_off.saturating_add(1024), &mut scratch[..1024]) {
        return None;
    }
    let sb = &scratch[..1024];
    if u16_at(sb, 0x38) != EXT4_MAGIC {
        return None;
    }
    let log_bs = u32_at(sb, 0x18);
    if log_bs > 2 {
        return None;
    }
    let block_size = 1024u64 << log_bs;
    if block_size > scratch.len() as u64 {
        return None;
    }
    let incompat = u32_at(sb, 0x60);
    let inode_size = u16_at(sb, 0x58) as u64;
    if inode_size < 128 || inode_size > 512 {
        return None;
    }
    let inodes_per_group = u32_at(sb, 0x28) as u64;
    if inodes_per_group == 0 {
        return None;
    }
    let desc_size = if incompat & INCOMPAT_64BIT != 0 {
        let d = u16_at(sb, 0xFE) as u64;
        if d < 32 || d > 128 {
            return None;
        }
        d
    } else {
        32
    };
    Some(Fs {
        part_off,
        block_size,
        inode_size,
        inodes_per_group,
        desc_size,
        incompat,
    })
}

fn read_inode<R: VolumeRead>(disk: &R, fs: &Fs, ino: u32, scratch: &mut [u8]) -> Option<Inode> {
    if ino < 2 {
        return None;
    }
    let group = (u64::from(ino) - 1) / fs.inodes_per_group;
    let index = (u64::from(ino) - 1) % fs.inodes_per_group;
    let table = inode_table_block(disk, fs, group, scratch)?;
    let off = fs
        .part_off
        .saturating_add(table.saturating_mul(fs.block_size))
        .saturating_add(index.saturating_mul(fs.inode_size));
    if !disk.read_at(off, &mut scratch[..128]) {
        return None;
    }
    let raw = &scratch[..128];
    let mut iblock = [0u8; 60];
    iblock.copy_from_slice(&raw[40..100]);
    let lo = u32_at(raw, 4) as u64;
    let hi = u32_at(raw, 108) as u64;
    Some(Inode {
        mode: u16_at(raw, 0),
        flags: u32_at(raw, 32),
        size: lo | (hi << 32),
        iblock,
    })
}

fn inode_table_block<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    group: u64,
    scratch: &mut [u8],
) -> Option<u64> {
    let gdt_block: u64 = if fs.block_size == 1024 { 2 } else { 1 };
    let off = fs
        .part_off
        .saturating_add(gdt_block.saturating_mul(fs.block_size))
        .saturating_add(group.saturating_mul(fs.desc_size));
    let n = fs.desc_size.min(scratch.len() as u64) as usize;
    if !disk.read_at(off, &mut scratch[..n]) {
        return None;
    }
    let lo = u32_at(scratch, 8) as u64;
    let hi = if n >= 0x2C {
        u32_at(scratch, 0x28) as u64
    } else {
        0
    };
    let block = lo | (hi << 32);
    if block == 0 {
        None
    } else {
        Some(block)
    }
}

fn search_dir<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    dir_ino: u32,
    depth: u32,
    want: u64,
    scratch: &mut [u8],
    saw: &mut Saw,
) -> Option<(Inode, [u8; NAME_CAP], usize)> {
    if depth > 2 {
        return None;
    }
    let dir = read_inode(disk, fs, dir_ino, scratch)?;
    if dir.mode & S_IFMT != S_IFDIR {
        return None;
    }
    let mut ents = [Ent {
        ino: 0,
        kind: 0,
        name_len: 0,
        name: [0; NAME_CAP],
    }; MAX_ENTS];
    let n = collect_interesting(disk, fs, &dir, scratch, &mut ents);
    for e in ents.iter().take(n) {
        if e.kind != FT_REG && e.kind != 0 {
            continue;
        }
        let name = &e.name[..e.name_len as usize];
        if !is_initrd_name(name) {
            continue;
        }
        let Some(ino) = read_inode(disk, fs, e.ino, scratch) else {
            continue;
        };
        if ino.mode & S_IFMT != S_IFREG {
            continue;
        }
        if size_matches(ino.size, want) {
            return Some((ino, e.name, e.name_len as usize));
        }
        note_saw(saw, ino.size, name);
    }
    for e in ents.iter().take(n) {
        if e.kind != FT_LNK && e.kind != 0 {
            continue;
        }
        let name = &e.name[..e.name_len as usize];
        if !is_initrd_name(name) {
            continue;
        }
        let Some(target) = follow_link(disk, fs, dir_ino, e.ino, scratch) else {
            continue;
        };
        let Some(ino) = read_inode(disk, fs, target, scratch) else {
            continue;
        };
        if ino.mode & S_IFMT != S_IFREG {
            continue;
        }
        if size_matches(ino.size, want) {
            return Some((ino, e.name, e.name_len as usize));
        }
        note_saw(saw, ino.size, name);
    }
    if depth == 0 {
        for e in ents.iter().take(n) {
            if e.kind != FT_DIR {
                continue;
            }
            let name = &e.name[..e.name_len as usize];
            if name != b"boot" {
                continue;
            }
            if let Some(hit) = search_dir(disk, fs, e.ino, depth + 1, want, scratch, saw) {
                return Some(hit);
            }
        }
    }
    None
}

fn collect_interesting<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    dir: &Inode,
    scratch: &mut [u8],
    out: &mut [Ent],
) -> usize {
    let bs = fs.block_size as usize;
    if bs == 0 || bs > scratch.len() || dir.size == 0 {
        return 0;
    }
    let mut n = 0usize;
    let mut off = 0u64;
    let cap = dir.size.min(MAX_DIR_BYTES);
    while off < cap && n < out.len() {
        let lblk = off / fs.block_size;
        let Some(phys) = map_block(disk, fs, &dir.iblock, lblk, scratch) else {
            break;
        };
        if phys == 0 || !read_fs_block(disk, fs, phys, scratch) {
            break;
        }
        let mut at = 0usize;
        while at + 8 <= bs {
            let ino = u32_at(scratch, at);
            let rec = u16_at(scratch, at + 4) as usize;
            if rec < 8 || at + rec > bs {
                break;
            }
            if ino != 0 {
                let (name_len, name_off, kind) = dirent_name(fs, scratch, at, rec);
                if name_len > 0 && at + name_off + name_len <= bs {
                    let name = &scratch[at + name_off..at + name_off + name_len];
                    if name != b"." && name != b".." && keep_name(name, kind) {
                        let mut ent = Ent {
                            ino,
                            kind,
                            name_len: name_len.min(NAME_CAP) as u8,
                            name: [0; NAME_CAP],
                        };
                        let c = ent.name_len as usize;
                        ent.name[..c].copy_from_slice(&name[..c]);
                        out[n] = ent;
                        n += 1;
                        if n == out.len() {
                            break;
                        }
                    }
                }
            }
            at += rec;
        }
        off = off.saturating_add(fs.block_size);
    }
    n
}

fn keep_name(name: &[u8], kind: u8) -> bool {
    if kind == FT_DIR && name == b"boot" {
        return true;
    }
    is_initrd_name(name)
}

fn dirent_name(fs: &Fs, block: &[u8], at: usize, rec: usize) -> (usize, usize, u8) {
    if fs.incompat & INCOMPAT_FILETYPE != 0 {
        let name_len = block[at + 6] as usize;
        let kind = block[at + 7];
        (name_len.min(rec.saturating_sub(8)), 8, kind)
    } else {
        let name_len = u16_at(block, at + 6) as usize;
        (name_len.min(rec.saturating_sub(8)), 8, 0)
    }
}

fn follow_link<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    parent: u32,
    link_ino: u32,
    scratch: &mut [u8],
) -> Option<u32> {
    let link = read_inode(disk, fs, link_ino, scratch)?;
    if link.mode & S_IFMT != S_IFLNK || link.size == 0 || link.size > MAX_LINK as u64 {
        return None;
    }
    let mut path = [0u8; MAX_LINK];
    let n = link.size as usize;
    if !read_link_bytes(disk, fs, &link, scratch, &mut path[..n]) {
        return None;
    }
    let absolute = path[0] == b'/';
    let mut ino = if absolute { 2 } else { parent };
    let mut i = 0usize;
    let mut hops = 0u32;
    while i < n {
        while i < n && path[i] == b'/' {
            i += 1;
        }
        if i >= n {
            break;
        }
        let start = i;
        while i < n && path[i] != b'/' {
            i += 1;
        }
        let comp = &path[start..i];
        if comp.is_empty() || comp == b"." {
            continue;
        }
        if comp == b".." {
            return None;
        }
        ino = lookup_component(disk, fs, ino, comp, scratch)?;
        hops += 1;
        if hops > 8 {
            return None;
        }
    }
    Some(ino)
}

fn read_link_bytes<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    link: &Inode,
    scratch: &mut [u8],
    dst: &mut [u8],
) -> bool {
    let fast = link.flags & EXTENTS_FL == 0 || u16_at(&link.iblock, 0) != EXTENT_MAGIC;
    if fast && link.size as usize <= link.iblock.len() && dst.len() <= link.iblock.len() {
        dst.copy_from_slice(&link.iblock[..dst.len()]);
        return true;
    }
    let mut filled = 0usize;
    let _ = stream_file(disk, fs, link, scratch, &mut |chunk| {
        let room = dst.len().saturating_sub(filled);
        let n = chunk.len().min(room);
        dst[filled..filled + n].copy_from_slice(&chunk[..n]);
        filled += n;
        true
    });
    filled == dst.len()
}

fn lookup_component<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    dir_ino: u32,
    name: &[u8],
    scratch: &mut [u8],
) -> Option<u32> {
    let dir = read_inode(disk, fs, dir_ino, scratch)?;
    if dir.mode & S_IFMT != S_IFDIR {
        return None;
    }
    let bs = fs.block_size as usize;
    if bs == 0 || bs > scratch.len() {
        return None;
    }
    let mut off = 0u64;
    let cap = dir.size.min(MAX_DIR_BYTES);
    while off < cap {
        let lblk = off / fs.block_size;
        let phys = map_block(disk, fs, &dir.iblock, lblk, scratch)?;
        if phys == 0 || !read_fs_block(disk, fs, phys, scratch) {
            return None;
        }
        let mut at = 0usize;
        while at + 8 <= bs {
            let ino = u32_at(scratch, at);
            let rec = u16_at(scratch, at + 4) as usize;
            if rec < 8 || at + rec > bs {
                break;
            }
            if ino != 0 {
                let (name_len, name_off, _) = dirent_name(fs, scratch, at, rec);
                if name_len == name.len() && at + name_off + name_len <= bs {
                    let got = &scratch[at + name_off..at + name_off + name_len];
                    if got == name {
                        return Some(ino);
                    }
                }
            }
            at += rec;
        }
        off = off.saturating_add(fs.block_size);
    }
    None
}

fn stream_file<R: VolumeRead, F: FnMut(&[u8]) -> bool>(
    disk: &R,
    fs: &Fs,
    inode: &Inode,
    scratch: &mut [u8],
    emit: &mut F,
) -> Result<bool, InitrdMiss> {
    if inode.mode & S_IFMT != S_IFREG && inode.mode & S_IFMT != S_IFLNK {
        return Ok(false);
    }
    let mut off = 0u64;
    while off < inode.size {
        let lblk = off / fs.block_size;
        let within = off % fs.block_size;
        let Some(phys) = map_block(disk, fs, &inode.iblock, lblk, scratch) else {
            return Ok(false);
        };
        let n = ((fs.block_size - within).min(inode.size - off)) as usize;
        if phys == 0 {
            let mut left = n;
            while left > 0 {
                let take = left.min(256);
                let z = [0u8; 256];
                if !emit(&z[..take]) {
                    return Ok(false);
                }
                left -= take;
            }
        } else {
            if !read_fs_block(disk, fs, phys, scratch) {
                return Ok(false);
            }
            if !emit(&scratch[within as usize..within as usize + n]) {
                return Ok(false);
            }
        }
        off += n as u64;
    }
    Ok(true)
}

fn map_block<R: VolumeRead>(
    disk: &R,
    fs: &Fs,
    iblock: &[u8; 60],
    lblk: u64,
    scratch: &mut [u8],
) -> Option<u64> {
    if u16_at(iblock, 0) != EXTENT_MAGIC {
        return None;
    }
    let mut depth = u16_at(iblock, 6);
    if depth > 3 {
        return None;
    }
    if depth == 0 {
        return cover_extents(&iblock[12..], u16_at(iblock, 2), lblk);
    }
    let mut phys = cover_index(&iblock[12..], u16_at(iblock, 2), lblk)?;
    while depth > 0 {
        if !read_fs_block(disk, fs, phys, scratch) {
            return None;
        }
        if u16_at(scratch, 0) != EXTENT_MAGIC {
            return None;
        }
        let entries = u16_at(scratch, 2);
        let child_depth = u16_at(scratch, 6);
        if child_depth == 0 {
            return cover_extents(&scratch[12..fs.block_size as usize], entries, lblk);
        }
        phys = cover_index(&scratch[12..fs.block_size as usize], entries, lblk)?;
        depth -= 1;
        if depth == 0 {
            break;
        }
    }
    None
}

fn cover_extents(raw: &[u8], entries: u16, lblk: u64) -> Option<u64> {
    let n = entries as usize;
    if n.saturating_mul(12) > raw.len() {
        return None;
    }
    for i in 0..n {
        let e = &raw[i * 12..i * 12 + 12];
        let start = u32_at(e, 0) as u64;
        let len_raw = u16_at(e, 4);
        let len = u64::from(len_raw & 0x7FFF);
        let unwritten = len_raw & 0x8000 != 0;
        if len == 0 || lblk < start || lblk >= start + len {
            continue;
        }
        if unwritten {
            return Some(0);
        }
        let phys = (u64::from(u16_at(e, 6)) << 32) | u64::from(u32_at(e, 8));
        return Some(phys.saturating_add(lblk - start));
    }
    Some(0)
}

fn cover_index(raw: &[u8], entries: u16, lblk: u64) -> Option<u64> {
    let n = entries as usize;
    if n == 0 || n.saturating_mul(12) > raw.len() {
        return None;
    }
    let mut best: Option<(u32, u64)> = None;
    for i in 0..n {
        let e = &raw[i * 12..i * 12 + 12];
        let start = u32_at(e, 0);
        if u64::from(start) > lblk {
            continue;
        }
        let leaf = u64::from(u32_at(e, 4)) | (u64::from(u16_at(e, 8)) << 32);
        if best.map_or(true, |(s, _)| start >= s) {
            best = Some((start, leaf));
        }
    }
    best.map(|(_, leaf)| leaf).filter(|&leaf| leaf != 0)
}

fn read_fs_block<R: VolumeRead>(disk: &R, fs: &Fs, phys: u64, scratch: &mut [u8]) -> bool {
    let n = fs.block_size as usize;
    if phys == 0 || n == 0 || n > scratch.len() {
        return false;
    }
    let off = fs
        .part_off
        .saturating_add(phys.saturating_mul(fs.block_size));
    disk.read_at(off, &mut scratch[..n])
}

fn u16_at(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
fn u32_at(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
fn u64_at(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

#[cfg(test)]
mod ext4_test {
    use super::super::gpt::{GPT_HEADER_SIZE_MIN, GPT_REVISION_1_0, GPT_SIGNATURE, MBR_TYPE_GPT};
    use super::super::tables::crc32;
    use super::*;

    struct SliceDisk<'a>(&'a [u8]);

    impl VolumeRead for SliceDisk<'_> {
        fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
            let Ok(start) = usize::try_from(off) else {
                return false;
            };
            let Some(end) = start.checked_add(buf.len()) else {
                return false;
            };
            if end > self.0.len() {
                return false;
            }
            buf.copy_from_slice(&self.0[start..end]);
            true
        }
    }

    const PART_LBA: u64 = 40;
    const BLOCK: usize = 4096;
    const LINUX: [u8; 16] = [
        0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
        0xE4,
    ];

    fn put_u16(buf: &mut [u8], off: usize, v: u16) {
        buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u32(buf: &mut [u8], off: usize, v: u32) {
        buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put_u64(buf: &mut [u8], off: usize, v: u64) {
        buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    struct Built {
        disk: Vec<u8>,
        payload: Vec<u8>,
    }

    /// `via_symlink` puts the payload behind `kernels/initrd.img-generic`
    /// and a root symlink. Otherwise the payload is `/boot/initrd.img-generic`.
    fn build(via_symlink: bool) -> Built {
        let blocks = 16usize;
        let part_off = PART_LBA as usize * 512;
        let mut disk = vec![0u8; part_off + blocks * BLOCK];
        disk[510] = 0x55;
        disk[511] = 0xAA;
        disk[0x1BE + 4] = MBR_TYPE_GPT;
        let part_blocks = blocks;
        let part_sectors = (part_blocks * BLOCK / 512) as u64;
        let end_lba = PART_LBA + part_sectors - 1;
        let ent = 2 * 512;
        disk[ent..ent + 16].copy_from_slice(&LINUX);
        disk[ent + 16] = 0x11;
        put_u64(&mut disk, ent + 32, PART_LBA);
        put_u64(&mut disk, ent + 40, end_lba);
        let hdr = 512;
        disk[hdr..hdr + 8].copy_from_slice(GPT_SIGNATURE);
        put_u32(&mut disk, hdr + 8, GPT_REVISION_1_0);
        put_u32(&mut disk, hdr + 12, GPT_HEADER_SIZE_MIN);
        put_u64(&mut disk, hdr + 24, 1);
        let alt = (disk.len() / 512) as u64 - 1;
        put_u64(&mut disk, hdr + 32, alt);
        put_u64(&mut disk, hdr + 72, 2);
        put_u32(&mut disk, hdr + 80, 4);
        put_u32(&mut disk, hdr + 84, 128);
        let c = crc32(&disk[hdr..hdr + GPT_HEADER_SIZE_MIN as usize]);
        put_u32(&mut disk, hdr + 16, c);

        let sb = part_off + 1024;
        put_u32(&mut disk, sb + 0x00, 16);
        put_u32(&mut disk, sb + 0x04, blocks as u32);
        put_u32(&mut disk, sb + 0x14, 0);
        put_u32(&mut disk, sb + 0x18, 2);
        put_u32(&mut disk, sb + 0x20, 32768);
        put_u32(&mut disk, sb + 0x28, 16);
        put_u16(&mut disk, sb + 0x38, EXT4_MAGIC);
        put_u32(&mut disk, sb + 0x4C, 1);
        put_u16(&mut disk, sb + 0x58, 256);
        put_u32(
            &mut disk,
            sb + 0x60,
            INCOMPAT_FILETYPE | 0x40 | INCOMPAT_64BIT,
        );
        put_u16(&mut disk, sb + 0xFE, 64);

        let gdt = part_off + BLOCK;
        put_u32(&mut disk, gdt + 8, 4);

        let mut payload = vec![0u8; 5000];
        for (i, b) in payload.iter_mut().enumerate() {
            *b = (i * 17 + 3) as u8;
        }
        let file_block = if via_symlink { 8 } else { 7 };
        let file_off = part_off + file_block * BLOCK;
        disk[file_off..file_off + payload.len()].copy_from_slice(&payload);
        disk[part_off + 10 * BLOCK..part_off + 10 * BLOCK + 4].copy_from_slice(b"old!");

        write_extent_inode(&mut disk, part_off, 2, 0x41ED, BLOCK as u64, 5, 1);
        write_extent_inode(
            &mut disk,
            part_off,
            3,
            0x81A4,
            payload.len() as u64,
            file_block as u32,
            2,
        );
        write_extent_inode(&mut disk, part_off, 4, 0x81A4, 4, 10, 1);

        if via_symlink {
            write_extent_inode(&mut disk, part_off, 6, 0x41ED, BLOCK as u64, 6, 1);
            write_fast_link(&mut disk, part_off, 5, b"kernels/initrd.img-generic");
            let mut root = vec![0u8; BLOCK];
            let mut at = 0usize;
            push_dirent(&mut root, &mut at, 2, FT_DIR, b".");
            push_dirent(&mut root, &mut at, 2, FT_DIR, b"..");
            push_dirent(&mut root, &mut at, 6, FT_DIR, b"kernels");
            push_dirent(&mut root, &mut at, 4, FT_REG, b"initrd.img-old");
            push_dirent(&mut root, &mut at, 5, FT_LNK, b"initrd.img");
            finish_dir(&mut root, at);
            disk[part_off + 5 * BLOCK..part_off + 6 * BLOCK].copy_from_slice(&root);
            let mut sub = vec![0u8; BLOCK];
            at = 0;
            push_dirent(&mut sub, &mut at, 6, FT_DIR, b".");
            push_dirent(&mut sub, &mut at, 2, FT_DIR, b"..");
            push_dirent(&mut sub, &mut at, 3, FT_REG, b"initrd.img-generic");
            finish_dir(&mut sub, at);
            disk[part_off + 6 * BLOCK..part_off + 7 * BLOCK].copy_from_slice(&sub);
        } else {
            write_extent_inode(&mut disk, part_off, 6, 0x41ED, BLOCK as u64, 6, 1);
            let mut root = vec![0u8; BLOCK];
            let mut at = 0usize;
            push_dirent(&mut root, &mut at, 2, FT_DIR, b".");
            push_dirent(&mut root, &mut at, 2, FT_DIR, b"..");
            push_dirent(&mut root, &mut at, 6, FT_DIR, b"boot");
            push_dirent(&mut root, &mut at, 4, FT_REG, b"initrd.img-old");
            finish_dir(&mut root, at);
            disk[part_off + 5 * BLOCK..part_off + 6 * BLOCK].copy_from_slice(&root);
            let mut sub = vec![0u8; BLOCK];
            at = 0;
            push_dirent(&mut sub, &mut at, 6, FT_DIR, b".");
            push_dirent(&mut sub, &mut at, 2, FT_DIR, b"..");
            push_dirent(&mut sub, &mut at, 3, FT_REG, b"initrd.img-generic");
            finish_dir(&mut sub, at);
            disk[part_off + 6 * BLOCK..part_off + 7 * BLOCK].copy_from_slice(&sub);
        }
        Built { disk, payload }
    }

    fn write_extent_inode(
        disk: &mut [u8],
        part_off: usize,
        ino: u32,
        mode: u16,
        size: u64,
        start: u32,
        blocks: u16,
    ) {
        let off = part_off + 4 * BLOCK + (ino as usize - 1) * 256;
        put_u16(disk, off, mode);
        put_u32(disk, off + 4, size as u32);
        put_u32(disk, off + 32, EXTENTS_FL);
        put_u32(disk, off + 108, (size >> 32) as u32);
        let ib = off + 40;
        put_u16(disk, ib, EXTENT_MAGIC);
        put_u16(disk, ib + 2, 1);
        put_u16(disk, ib + 4, 4);
        put_u16(disk, ib + 6, 0);
        put_u32(disk, ib + 12, 0);
        put_u16(disk, ib + 16, blocks);
        put_u16(disk, ib + 18, 0);
        put_u32(disk, ib + 20, start);
    }

    fn write_fast_link(disk: &mut [u8], part_off: usize, ino: u32, path: &[u8]) {
        let off = part_off + 4 * BLOCK + (ino as usize - 1) * 256;
        put_u16(disk, off, 0xA1FF);
        put_u32(disk, off + 4, path.len() as u32);
        disk[off + 40..off + 40 + path.len()].copy_from_slice(path);
    }

    fn push_dirent(block: &mut [u8], at: &mut usize, ino: u32, kind: u8, name: &[u8]) {
        let rec = (8 + name.len() + 3) & !3;
        put_u32(block, *at, ino);
        put_u16(block, *at + 4, rec as u16);
        block[*at + 6] = name.len() as u8;
        block[*at + 7] = kind;
        block[*at + 8..*at + 8 + name.len()].copy_from_slice(name);
        *at += rec;
    }

    fn finish_dir(block: &mut [u8], at: usize) {
        if at < 8 {
            return;
        }
        let mut prev = 0usize;
        let mut cur = 0usize;
        while cur + 8 <= at {
            let rec = u16_at(block, cur + 4) as usize;
            if rec < 8 {
                break;
            }
            prev = cur;
            cur += rec;
        }
        put_u16(block, prev + 4, (block.len() - prev) as u16);
    }

    fn copy_of(disk: &[u8], want: u64) -> Result<Vec<u8>, InitrdMiss> {
        let mut scratch = [0u8; 4096];
        let mut out = Vec::new();
        let file = copy_sized_initrd(&SliceDisk(disk), want, &mut scratch, |chunk| {
            out.extend_from_slice(chunk);
            true
        })?;
        assert_eq!(file.bytes, out.len() as u64);
        let name = &file.name[..file.name_len];
        assert!(
            name.windows(6).any(|w| w == b"initrd"),
            "name {}",
            String::from_utf8_lossy(name)
        );
        Ok(out)
    }

    #[test]
    fn symlink_initrd_matches_the_size_query() {
        let built = build(true);
        let got = copy_of(&built.disk, built.payload.len() as u64).unwrap();
        assert_eq!(got, built.payload);
    }

    #[test]
    fn boot_directory_initrd_matches_the_size_query() {
        let built = build(false);
        let got = copy_of(&built.disk, built.payload.len() as u64).unwrap();
        assert_eq!(got, built.payload);
    }

    #[test]
    fn smaller_initrd_is_the_one_whose_size_matches() {
        let built = build(false);
        let got = copy_of(&built.disk, 4).unwrap();
        assert_eq!(got, b"old!");
    }

    #[test]
    fn wrong_size_reports_the_largest_initrd_seen() {
        let built = build(false);
        let mut scratch = [0u8; 4096];
        let err =
            copy_sized_initrd(&SliceDisk(&built.disk), 3, &mut scratch, |_| true).unwrap_err();
        assert_eq!(err.why, "nofile");
        assert_eq!(err.saw_bytes, built.payload.len() as u64);
    }

    #[test]
    fn depth1_extent_and_second_directory_block_are_copied() {
        let built = build(false);
        let part_off = PART_LBA as usize * 512;
        let mut disk = built.disk;
        // Move the real file's extent to a leaf block so depth 1 is required,
        // and put that directory entry only in the second block of `/boot`.
        let leaf = 11u32;
        let leaf_off = part_off + leaf as usize * BLOCK;
        put_u16(&mut disk, leaf_off, EXTENT_MAGIC);
        put_u16(&mut disk, leaf_off + 2, 1);
        put_u16(&mut disk, leaf_off + 4, 4);
        put_u16(&mut disk, leaf_off + 6, 0);
        put_u32(&mut disk, leaf_off + 12, 0);
        put_u16(&mut disk, leaf_off + 16, 2);
        put_u16(&mut disk, leaf_off + 18, 0);
        put_u32(&mut disk, leaf_off + 20, 7);
        let ino = part_off + 4 * BLOCK + (3 - 1) * 256;
        put_u16(&mut disk, ino + 40 + 2, 1);
        put_u16(&mut disk, ino + 40 + 6, 1);
        put_u32(&mut disk, ino + 40 + 12, 0);
        put_u32(&mut disk, ino + 40 + 16, leaf);
        put_u16(&mut disk, ino + 40 + 20, 0);
        // `/boot` grows to two blocks. Block 6 keeps `.` and `..` only.
        write_extent_inode(&mut disk, part_off, 6, 0x41ED, (BLOCK * 2) as u64, 6, 2);
        let mut first = vec![0u8; BLOCK];
        let mut at = 0usize;
        push_dirent(&mut first, &mut at, 6, FT_DIR, b".");
        push_dirent(&mut first, &mut at, 2, FT_DIR, b"..");
        finish_dir(&mut first, at);
        disk[part_off + 6 * BLOCK..part_off + 7 * BLOCK].copy_from_slice(&first);
        let mut second = vec![0u8; BLOCK];
        at = 0;
        push_dirent(&mut second, &mut at, 3, FT_REG, b"initrd.img-generic");
        finish_dir(&mut second, at);
        disk[part_off + 12 * BLOCK..part_off + 13 * BLOCK].copy_from_slice(&second);
        // Extent of the directory: block 6 then block 12. Depth 0, two
        // extents, because they are not contiguous (block 7 is file data).
        let dib = ino_iblock(&mut disk, part_off, 6);
        put_u16(dib, 2, 2);
        put_u32(dib, 12, 0);
        put_u16(dib, 16, 1);
        put_u16(dib, 18, 0);
        put_u32(dib, 20, 6);
        put_u32(dib, 24, 1);
        put_u16(dib, 28, 1);
        put_u16(dib, 30, 0);
        put_u32(dib, 32, 12);
        let got = copy_of(&disk, built.payload.len() as u64).unwrap();
        assert_eq!(got, built.payload);
    }

    fn ino_iblock(disk: &mut [u8], part_off: usize, ino: u32) -> &mut [u8] {
        let off = part_off + 4 * BLOCK + (ino as usize - 1) * 256 + 40;
        &mut disk[off..off + 60]
    }

    #[test]
    fn disk_without_ext4_is_a_miss() {
        let mut scratch = [0u8; 4096];
        let err =
            copy_sized_initrd(&SliceDisk(&[0u8; 4096]), 100, &mut scratch, |_| true).unwrap_err();
        assert_eq!(err.why, "gpt");
    }
}
