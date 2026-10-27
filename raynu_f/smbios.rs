//! SMBIOS 3.0 table RayNu-F publishes to the guest (outside Proven Core).
//!
//! Pillar: [Z] [D] · Proven Core: **outside** (ADR-016, ADR-020)
//!
//! One 64-bit entry point (`_SM3_`, SMBIOS 3.0 §5.2.2) in front of a
//! structure table that holds a Type 11 *OEM Strings* record and the
//! Type 127 end marker. The strings carry systemd credentials
//! (`io.systemd.credential.binary:<name>=<base64>`), which Linux exposes at
//! `/sys/firmware/dmi/entries/11-0/raw` and PID 1 imports at boot. That is
//! the architected way a hypervisor hands a guest its first network file:
//! the disk is not written and no guest state is poked (ADR-016).
//!
//! Byte-exact serialization, host-testable. The guest finds the table
//! through the `EFI_SMBIOS3_TABLE_GUID` configuration-table entry.

/// `EFI_SMBIOS3_TABLE_GUID` = `f2fd1544-9794-4a2c-992e-e5bbcf20e394`
/// (UEFI 2.10 §4.6), mixed-endian GUID byte order.
pub const SMBIOS3_TABLE_GUID: [u8; 16] = [
    0x44, 0x15, 0xfd, 0xf2, 0x94, 0x97, 0x2c, 0x4a, 0x99, 0x2e, 0xe5, 0xbb, 0xcf, 0x20, 0xe3,
    0x94,
];

/// `_SM3_` anchor.
pub const SMBIOS3_ANCHOR: &[u8; 5] = b"_SM3_";
/// 64-bit entry point length (SMBIOS 3.0 §5.2.2).
pub const SMBIOS3_ENTRY_BYTES: usize = 0x18;
/// Entry-point field offsets.
pub const ENTRY_CHECKSUM_OFF: usize = 5;
pub const ENTRY_LENGTH_OFF: usize = 6;
pub const ENTRY_MAJOR_OFF: usize = 7;
pub const ENTRY_MINOR_OFF: usize = 8;
pub const ENTRY_DOCREV_OFF: usize = 9;
pub const ENTRY_REVISION_OFF: usize = 10;
pub const ENTRY_TABLE_MAX_OFF: usize = 12;
pub const ENTRY_TABLE_ADDR_OFF: usize = 16;
/// We publish SMBIOS 3.0.0; entry point revision 1 is the 64-bit format.
pub const SMBIOS_MAJOR: u8 = 3;
pub const SMBIOS_MINOR: u8 = 0;
pub const SMBIOS_DOCREV: u8 = 0;
pub const SMBIOS3_ENTRY_REVISION: u8 = 1;

/// The structure table starts here, past the entry point, so both live in
/// one page and the guest's `dmi_len` stays inside it.
pub const STRUCTURE_TABLE_OFF: usize = 0x20;

/// Structure types we write.
pub const TYPE_OEM_STRINGS: u8 = 11;
pub const TYPE_END_OF_TABLE: u8 = 127;
/// Formatted lengths: Type 11 is header + count; Type 127 is the header.
pub const TYPE11_FORMATTED_LEN: u8 = 5;
pub const TYPE127_FORMATTED_LEN: u8 = 4;
/// Handles are arbitrary and unique.
pub const TYPE11_HANDLE: u16 = 0x0B00;
pub const TYPE127_HANDLE: u16 = 0x7F00;

/// systemd credential prefixes it looks for in Type 11 strings
/// (`systemd.directives(7)`, "System and Service Credentials").
pub const CREDENTIAL_TEXT_PREFIX: &[u8] = b"io.systemd.credential:";
pub const CREDENTIAL_BINARY_PREFIX: &[u8] = b"io.systemd.credential.binary:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmbiosError {
    /// Region cannot hold the entry point plus the structures.
    BufferTooSmall,
    /// An SMBIOS string may not be empty and may not contain NUL.
    BadString,
    /// Type 11 counts strings in one byte.
    TooManyStrings,
}

/// Where the table landed, as offsets inside the region handed to
/// [`write_smbios`] and as the byte count a guest will walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmbiosImage {
    /// Offset of the `_SM3_` entry point (always 0).
    pub entry_off: usize,
    /// Offset of the first structure.
    pub table_off: usize,
    /// Bytes from the first structure through the Type 127 terminator.
    pub table_len: usize,
}

/// Serialize the entry point at `buf[0..24]` and the structure table at
/// `buf[STRUCTURE_TABLE_OFF..]`. `base_gpa` is the guest-physical address of
/// `buf[0]`, so the entry point can name the structure table's address.
///
/// `strings` become the Type 11 strings in order. Each must be non-empty
/// and NUL-free. An empty list writes a Type 11 with `Count = 0`.
pub fn write_smbios(buf: &mut [u8], base_gpa: u64, strings: &[&[u8]]) -> Result<SmbiosImage, SmbiosError> {
    if strings.len() > usize::from(u8::MAX) {
        return Err(SmbiosError::TooManyStrings);
    }
    for s in strings {
        if s.is_empty() || s.contains(&0) {
            return Err(SmbiosError::BadString);
        }
    }
    // Type 11 + its string-set (each NUL-terminated, then one more NUL; an
    // empty set is two NULs) + Type 127 + its empty string-set.
    let t11_strings: usize = strings.iter().map(|s| s.len() + 1).sum::<usize>() + 1;
    let t11_strings = t11_strings.max(2);
    let table_len = usize::from(TYPE11_FORMATTED_LEN) + t11_strings + usize::from(TYPE127_FORMATTED_LEN) + 2;
    let end = STRUCTURE_TABLE_OFF + table_len;
    if buf.len() < end {
        return Err(SmbiosError::BufferTooSmall);
    }
    for b in buf[..end].iter_mut() {
        *b = 0;
    }

    let mut p = STRUCTURE_TABLE_OFF;
    buf[p] = TYPE_OEM_STRINGS;
    buf[p + 1] = TYPE11_FORMATTED_LEN;
    buf[p + 2..p + 4].copy_from_slice(&TYPE11_HANDLE.to_le_bytes());
    buf[p + 4] = strings.len() as u8;
    p += usize::from(TYPE11_FORMATTED_LEN);
    if strings.is_empty() {
        p += 2;
    } else {
        for s in strings {
            buf[p..p + s.len()].copy_from_slice(s);
            p += s.len() + 1;
        }
        p += 1;
    }
    buf[p] = TYPE_END_OF_TABLE;
    buf[p + 1] = TYPE127_FORMATTED_LEN;
    buf[p + 2..p + 4].copy_from_slice(&TYPE127_HANDLE.to_le_bytes());
    p += usize::from(TYPE127_FORMATTED_LEN) + 2;
    debug_assert_eq!(p, end);

    buf[..5].copy_from_slice(SMBIOS3_ANCHOR);
    buf[ENTRY_LENGTH_OFF] = SMBIOS3_ENTRY_BYTES as u8;
    buf[ENTRY_MAJOR_OFF] = SMBIOS_MAJOR;
    buf[ENTRY_MINOR_OFF] = SMBIOS_MINOR;
    buf[ENTRY_DOCREV_OFF] = SMBIOS_DOCREV;
    buf[ENTRY_REVISION_OFF] = SMBIOS3_ENTRY_REVISION;
    buf[11] = 0;
    buf[ENTRY_TABLE_MAX_OFF..ENTRY_TABLE_MAX_OFF + 4].copy_from_slice(&(table_len as u32).to_le_bytes());
    buf[ENTRY_TABLE_ADDR_OFF..ENTRY_TABLE_ADDR_OFF + 8]
        .copy_from_slice(&(base_gpa + STRUCTURE_TABLE_OFF as u64).to_le_bytes());
    buf[ENTRY_CHECKSUM_OFF] = 0;
    buf[ENTRY_CHECKSUM_OFF] = checksum_fix(&buf[..SMBIOS3_ENTRY_BYTES]);

    Ok(SmbiosImage {
        entry_off: 0,
        table_off: STRUCTURE_TABLE_OFF,
        table_len,
    })
}

/// Byte that makes `bytes` sum to zero modulo 256 (the SMBIOS checksum rule).
fn checksum_fix(bytes: &[u8]) -> u8 {
    let sum = bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    0u8.wrapping_sub(sum)
}

/// True when `buf` starts with a well-formed 64-bit entry point: anchor,
/// length 0x18, bytes sum to zero. This is the check Linux `dmi_smbios3_present` makes.
pub fn entry_point_valid(buf: &[u8]) -> bool {
    if buf.len() < SMBIOS3_ENTRY_BYTES || &buf[..5] != SMBIOS3_ANCHOR {
        return false;
    }
    if usize::from(buf[ENTRY_LENGTH_OFF]) != SMBIOS3_ENTRY_BYTES {
        return false;
    }
    buf[..SMBIOS3_ENTRY_BYTES]
        .iter()
        .fold(0u8, |a, b| a.wrapping_add(*b))
        == 0
}

/// Structure table length and address named by an entry point.
pub fn entry_point_table(buf: &[u8]) -> Option<(u32, u64)> {
    if !entry_point_valid(buf) {
        return None;
    }
    let len = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let mut a = [0u8; 8];
    a.copy_from_slice(&buf[16..24]);
    Some((len, u64::from_le_bytes(a)))
}

/// Strings of the first Type 11 structure in `table`, in order. `None` when
/// the table is malformed or has no Type 11 before Type 127.
pub fn type11_strings(table: &[u8]) -> Option<([Option<(usize, usize)>; 8], usize)> {
    let mut p = 0usize;
    loop {
        if p + 4 > table.len() {
            return None;
        }
        let typ = table[p];
        let len = usize::from(table[p + 1]);
        if len < 4 || p + len > table.len() {
            return None;
        }
        let strings_at = p + len;
        // Walk the string-set: ends at a double NUL.
        let mut q = strings_at;
        let mut ranges: [Option<(usize, usize)>; 8] = [None; 8];
        let mut n = 0usize;
        loop {
            if q >= table.len() {
                return None;
            }
            if table[q] == 0 {
                // Empty string-set: two NULs.
                q += 2;
                break;
            }
            let start = q;
            while q < table.len() && table[q] != 0 {
                q += 1;
            }
            if q >= table.len() {
                return None;
            }
            if n < ranges.len() {
                ranges[n] = Some((start, q));
            }
            n += 1;
            q += 1;
            if q < table.len() && table[q] == 0 {
                q += 1;
                break;
            }
        }
        if typ == TYPE_OEM_STRINGS {
            if usize::from(table[p + 4]) != n {
                return None;
            }
            return Some((ranges, n));
        }
        if typ == TYPE_END_OF_TABLE {
            return None;
        }
        p = q;
    }
}
