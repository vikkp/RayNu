//! PE32+ (x86-64) image loader for RayNu-F.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! Loads a UEFI application (`\EFI\BOOT\BOOTX64.EFI` — GRUB, systemd-boot,
//! the Linux EFI stub, or our own test app) into guest memory: headers +
//! sections copied to `load_base + RVA`, gaps zero-filled, and
//! `IMAGE_REL_BASED_DIR64` base relocations applied for
//! `delta = load_base - ImageBase`. Pure Rust over byte slices so it is
//! host-testable; the hypervisor hands it the slab it owns.
//!
//! Format refs: PE/COFF spec §3 (COFF header), §3.4 (optional header PE32+),
//! §4 (section table), §6.6 (base relocations). Subsystem 10 =
//! `IMAGE_SUBSYSTEM_EFI_APPLICATION`.

/// `e_lfanew` lives at DOS header offset 0x3C.
pub const DOS_E_LFANEW_OFF: usize = 0x3C;
/// COFF header is 20 bytes after the 4-byte `PE\0\0` signature.
pub const COFF_HEADER_SIZE: usize = 20;
/// `IMAGE_FILE_MACHINE_AMD64`.
pub const MACHINE_AMD64: u16 = 0x8664;
/// PE32+ optional-header magic.
pub const PE32PLUS_MAGIC: u16 = 0x20B;
/// `IMAGE_SUBSYSTEM_EFI_APPLICATION`.
pub const SUBSYSTEM_EFI_APPLICATION: u16 = 10;
/// Data-directory index of the base relocation table.
pub const DIR_BASERELOC: usize = 5;
/// Data directories start at this offset inside the PE32+ optional header.
pub const OPT_DATA_DIRS_OFF: usize = 112;
/// Section header size.
pub const SECTION_HEADER_SIZE: usize = 40;
/// `IMAGE_REL_BASED_ABSOLUTE` (padding, skip).
pub const REL_ABSOLUTE: u16 = 0;
/// `IMAGE_REL_BASED_DIR64` (add 64-bit delta).
pub const REL_DIR64: u16 = 10;
/// Refuse absurd images (a UEFI loader is a few MiB at most).
pub const MAX_IMAGE_BYTES: u32 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeError {
    TooShort,
    NotMz,
    NotPe,
    NotAmd64,
    NotPe32Plus,
    BadSectionAlignment,
    ImageTooLarge,
    SectionOutOfFile,
    SectionOutOfImage,
    HeadersOutOfFile,
    EntryOutOfImage,
    DestinationTooSmall,
    RelocOutOfImage,
    RelocUnsupportedType(u16),
    LoadBaseUnaligned,
}

/// Parsed PE32+ headers (everything the loader needs, nothing more).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeImage {
    pub machine: u16,
    pub num_sections: u16,
    pub section_table_off: usize,
    pub entry_rva: u32,
    pub image_base: u64,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub size_of_image: u32,
    pub size_of_headers: u32,
    pub subsystem: u16,
    /// `(rva, size)` of the base relocation directory; `(0, 0)` if none.
    pub reloc_dir: (u32, u32),
}

/// One section header, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub name: [u8; 8],
    pub virtual_size: u32,
    pub virtual_address: u32,
    pub size_of_raw_data: u32,
    pub pointer_to_raw_data: u32,
    pub characteristics: u32,
}

/// Result of a successful load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loaded {
    pub load_base: u64,
    pub entry: u64,
    pub size_of_image: u32,
    pub sections_loaded: u16,
    pub relocs_applied: u32,
}

fn u16_at(b: &[u8], off: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(off)?, *b.get(off + 1)?]))
}

fn u32_at(b: &[u8], off: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(off)?,
        *b.get(off + 1)?,
        *b.get(off + 2)?,
        *b.get(off + 3)?,
    ]))
}

fn u64_at(b: &[u8], off: usize) -> Option<u64> {
    let lo = u32_at(b, off)? as u64;
    let hi = u32_at(b, off + 4)? as u64;
    Some(lo | (hi << 32))
}

/// Parse PE32+ headers without loading anything.
pub fn parse_pe32plus(file: &[u8]) -> Result<PeImage, PeError> {
    if file.len() < 0x40 {
        return Err(PeError::TooShort);
    }
    if &file[0..2] != b"MZ" {
        return Err(PeError::NotMz);
    }
    let pe_off = u32_at(file, DOS_E_LFANEW_OFF).ok_or(PeError::TooShort)? as usize;
    if pe_off + 4 + COFF_HEADER_SIZE > file.len() || &file[pe_off..pe_off + 4] != b"PE\0\0" {
        return Err(PeError::NotPe);
    }
    let coff = pe_off + 4;
    let machine = u16_at(file, coff).ok_or(PeError::TooShort)?;
    if machine != MACHINE_AMD64 {
        return Err(PeError::NotAmd64);
    }
    let num_sections = u16_at(file, coff + 2).ok_or(PeError::TooShort)?;
    let size_of_opt = u16_at(file, coff + 16).ok_or(PeError::TooShort)? as usize;
    let opt = coff + COFF_HEADER_SIZE;
    if opt + size_of_opt > file.len() || size_of_opt < OPT_DATA_DIRS_OFF {
        return Err(PeError::NotPe32Plus);
    }
    if u16_at(file, opt).ok_or(PeError::TooShort)? != PE32PLUS_MAGIC {
        return Err(PeError::NotPe32Plus);
    }
    let entry_rva = u32_at(file, opt + 16).ok_or(PeError::TooShort)?;
    let image_base = u64_at(file, opt + 24).ok_or(PeError::TooShort)?;
    let section_alignment = u32_at(file, opt + 32).ok_or(PeError::TooShort)?;
    let file_alignment = u32_at(file, opt + 36).ok_or(PeError::TooShort)?;
    let size_of_image = u32_at(file, opt + 56).ok_or(PeError::TooShort)?;
    let size_of_headers = u32_at(file, opt + 60).ok_or(PeError::TooShort)?;
    let subsystem = u16_at(file, opt + 68).ok_or(PeError::TooShort)?;
    let num_dirs = u32_at(file, opt + 108).ok_or(PeError::TooShort)? as usize;
    if section_alignment == 0 || !section_alignment.is_power_of_two() {
        return Err(PeError::BadSectionAlignment);
    }
    if size_of_image == 0 || size_of_image > MAX_IMAGE_BYTES {
        return Err(PeError::ImageTooLarge);
    }
    if size_of_headers as usize > file.len() {
        return Err(PeError::HeadersOutOfFile);
    }
    if entry_rva >= size_of_image {
        return Err(PeError::EntryOutOfImage);
    }
    let mut reloc_dir = (0u32, 0u32);
    if num_dirs > DIR_BASERELOC {
        let d = opt + OPT_DATA_DIRS_OFF + DIR_BASERELOC * 8;
        if d + 8 <= opt + size_of_opt {
            reloc_dir = (
                u32_at(file, d).ok_or(PeError::TooShort)?,
                u32_at(file, d + 4).ok_or(PeError::TooShort)?,
            );
        }
    }
    let section_table_off = opt + size_of_opt;
    if section_table_off + num_sections as usize * SECTION_HEADER_SIZE > file.len() {
        return Err(PeError::SectionOutOfFile);
    }
    Ok(PeImage {
        machine,
        num_sections,
        section_table_off,
        entry_rva,
        image_base,
        section_alignment,
        file_alignment,
        size_of_image,
        size_of_headers,
        subsystem,
        reloc_dir,
    })
}

/// Decode section header `i`.
pub fn section(file: &[u8], pe: &PeImage, i: u16) -> Option<Section> {
    if i >= pe.num_sections {
        return None;
    }
    let off = pe.section_table_off + i as usize * SECTION_HEADER_SIZE;
    let mut name = [0u8; 8];
    name.copy_from_slice(file.get(off..off + 8)?);
    Some(Section {
        name,
        virtual_size: u32_at(file, off + 8)?,
        virtual_address: u32_at(file, off + 12)?,
        size_of_raw_data: u32_at(file, off + 16)?,
        pointer_to_raw_data: u32_at(file, off + 20)?,
        characteristics: u32_at(file, off + 36)?,
    })
}

/// Largest PE header region we will pull into a stack buffer to parse.
pub const MAX_HEADER_BYTES: usize = 4096;

/// Load a PE32+ that lives in **guest memory** at `src` (`src_len` bytes) to
/// `dst` in guest memory, using a `GuestMem` for every access. Mirrors
/// [`load_pe32plus`] for the `LoadImage(SourceBuffer)` path, where both the
/// file and the destination are the guest's.
///
/// The header region is copied into a bounded stack buffer to parse; sections
/// and relocations stream through 4 KiB chunks so nothing unbounded is held.
pub fn load_pe32plus_guest(
    mem: &dyn super::services::GuestMem,
    src: u64,
    src_len: u64,
    dst: u64,
    dst_len: u64,
) -> Result<Loaded, PeError> {
    if dst & 0xfff != 0 {
        return Err(PeError::LoadBaseUnaligned);
    }
    // Parse from a bounded header window.
    let hdr_take = (src_len as usize).min(MAX_HEADER_BYTES);
    if hdr_take < 0x40 {
        return Err(PeError::TooShort);
    }
    let mut hdr = [0u8; MAX_HEADER_BYTES];
    if mem.read(src, &mut hdr[..hdr_take]) < hdr_take {
        return Err(PeError::TooShort);
    }
    let pe = parse_pe32plus(&hdr[..hdr_take])?;
    let image_len = pe.size_of_image as u64;
    if dst_len < image_len {
        return Err(PeError::DestinationTooSmall);
    }
    if u64::from(pe.size_of_headers) > src_len {
        return Err(PeError::HeadersOutOfFile);
    }

    // Zero the destination image.
    let zero = [0u8; 4096];
    let mut done = 0u64;
    while done < image_len {
        let chunk = (image_len - done).min(4096) as usize;
        if mem.write(dst + done, &zero[..chunk]) != chunk {
            return Err(PeError::DestinationTooSmall);
        }
        done += chunk as u64;
    }

    // Copy a guest→guest byte range through a bounded buffer.
    let mut copy = |from: u64, to: u64, len: u64| -> bool {
        let mut buf = [0u8; 4096];
        let mut n = 0u64;
        while n < len {
            let chunk = (len - n).min(4096) as usize;
            if mem.read(from + n, &mut buf[..chunk]) != chunk
                || mem.write(to + n, &buf[..chunk]) != chunk
            {
                return false;
            }
            n += chunk as u64;
        }
        true
    };

    // Headers.
    if !copy(src, dst, u64::from(pe.size_of_headers)) {
        return Err(PeError::HeadersOutOfFile);
    }

    // Sections.
    let mut loaded = 0u16;
    for i in 0..pe.num_sections {
        let s = section(&hdr[..hdr_take], &pe, i).ok_or(PeError::SectionOutOfFile)?;
        let va = u64::from(s.virtual_address);
        let vsz = u64::from(s.virtual_size);
        let raw = u64::from(s.size_of_raw_data);
        let ptr = u64::from(s.pointer_to_raw_data);
        if va >= image_len || va.saturating_add(vsz.max(raw)) > image_len {
            return Err(PeError::SectionOutOfImage);
        }
        if raw > 0 {
            if ptr.saturating_add(raw) > src_len {
                return Err(PeError::SectionOutOfFile);
            }
            if !copy(src + ptr, dst + va, raw) {
                return Err(PeError::SectionOutOfFile);
            }
        }
        loaded += 1;
    }

    // Base relocations, read back from the loaded image.
    let mut relocs_applied = 0u32;
    let delta = dst.wrapping_sub(pe.image_base);
    let (rd_rva, rd_size) = pe.reloc_dir;
    if delta != 0 && rd_size != 0 {
        let end = u64::from(rd_rva).saturating_add(u64::from(rd_size));
        if end > image_len {
            return Err(PeError::RelocOutOfImage);
        }
        let mut off = u64::from(rd_rva);
        while off + 8 <= end {
            let mut blk = [0u8; 8];
            if mem.read(dst + off, &mut blk) < 8 {
                return Err(PeError::RelocOutOfImage);
            }
            let page_rva = u64::from(u32::from_le_bytes([blk[0], blk[1], blk[2], blk[3]]));
            let block = u64::from(u32::from_le_bytes([blk[4], blk[5], blk[6], blk[7]]));
            if block < 8 || off + block > end {
                return Err(PeError::RelocOutOfImage);
            }
            let mut e = off + 8;
            while e + 2 <= off + block {
                let mut eb = [0u8; 2];
                if mem.read(dst + e, &mut eb) < 2 {
                    return Err(PeError::RelocOutOfImage);
                }
                let entry = u16::from_le_bytes(eb);
                let typ = entry >> 12;
                let at = page_rva + u64::from(entry & 0xfff);
                match typ {
                    REL_ABSOLUTE => {}
                    REL_DIR64 => {
                        if at + 8 > image_len {
                            return Err(PeError::RelocOutOfImage);
                        }
                        let mut v = [0u8; 8];
                        if mem.read(dst + at, &mut v) < 8 {
                            return Err(PeError::RelocOutOfImage);
                        }
                        let nv = u64::from_le_bytes(v).wrapping_add(delta);
                        if mem.write(dst + at, &nv.to_le_bytes()) != 8 {
                            return Err(PeError::RelocOutOfImage);
                        }
                        relocs_applied += 1;
                    }
                    other => return Err(PeError::RelocUnsupportedType(other)),
                }
                e += 2;
            }
            off += block;
        }
    }

    Ok(Loaded {
        load_base: dst,
        entry: dst + u64::from(pe.entry_rva),
        size_of_image: pe.size_of_image,
        sections_loaded: loaded,
        relocs_applied,
    })
}

/// Load `file` so that RVA 0 lands at `dst[0]`, which the caller maps at
/// guest address `load_base` (page-aligned). Applies DIR64 relocations.
pub fn load_pe32plus(file: &[u8], load_base: u64, dst: &mut [u8]) -> Result<Loaded, PeError> {
    let pe = parse_pe32plus(file)?;
    if load_base & 0xfff != 0 {
        return Err(PeError::LoadBaseUnaligned);
    }
    let image_len = pe.size_of_image as usize;
    if dst.len() < image_len {
        return Err(PeError::DestinationTooSmall);
    }
    let img = &mut dst[..image_len];
    for b in img.iter_mut() {
        *b = 0;
    }

    // Headers.
    let hdr = pe.size_of_headers as usize;
    img[..hdr].copy_from_slice(&file[..hdr]);

    // Sections.
    let mut loaded = 0u16;
    for i in 0..pe.num_sections {
        let s = section(file, &pe, i).ok_or(PeError::SectionOutOfFile)?;
        let va = s.virtual_address as usize;
        let vsz = s.virtual_size as usize;
        let raw = s.size_of_raw_data as usize;
        let src = s.pointer_to_raw_data as usize;
        if va >= image_len || va.saturating_add(vsz.max(raw)) > image_len {
            return Err(PeError::SectionOutOfImage);
        }
        // Only the initialized part comes from the file; `.bss`-style tails
        // (virtual_size > raw) stay zero from the fill above.
        let copy = raw.min(vsz.max(raw));
        if copy > 0 {
            if src.saturating_add(copy) > file.len() {
                return Err(PeError::SectionOutOfFile);
            }
            img[va..va + copy].copy_from_slice(&file[src..src + copy]);
        }
        loaded += 1;
    }

    // Base relocations.
    let mut relocs_applied = 0u32;
    let delta = load_base.wrapping_sub(pe.image_base);
    let (rd_rva, rd_size) = pe.reloc_dir;
    if delta != 0 && rd_size != 0 {
        let mut off = rd_rva as usize;
        let end = (rd_rva as usize).saturating_add(rd_size as usize);
        if end > image_len {
            return Err(PeError::RelocOutOfImage);
        }
        while off + 8 <= end {
            let page_rva = u32_at(img, off).ok_or(PeError::RelocOutOfImage)? as usize;
            let block = u32_at(img, off + 4).ok_or(PeError::RelocOutOfImage)? as usize;
            if block < 8 || off + block > end {
                return Err(PeError::RelocOutOfImage);
            }
            let mut e = off + 8;
            while e + 2 <= off + block {
                let entry = u16_at(img, e).ok_or(PeError::RelocOutOfImage)?;
                let typ = entry >> 12;
                let at = page_rva + (entry & 0xfff) as usize;
                match typ {
                    REL_ABSOLUTE => {}
                    REL_DIR64 => {
                        if at + 8 > image_len {
                            return Err(PeError::RelocOutOfImage);
                        }
                        let v = u64_at(img, at).ok_or(PeError::RelocOutOfImage)?;
                        img[at..at + 8].copy_from_slice(&v.wrapping_add(delta).to_le_bytes());
                        relocs_applied += 1;
                    }
                    other => return Err(PeError::RelocUnsupportedType(other)),
                }
                e += 2;
            }
            off += block;
        }
    }

    Ok(Loaded {
        load_base,
        entry: load_base + pe.entry_rva as u64,
        size_of_image: pe.size_of_image,
        sections_loaded: loaded,
        relocs_applied,
    })
}
