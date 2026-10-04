//! In-memory UEFI variable store for RayNu-F (outside Proven Core).
//!
//! Shim on `0aa0a78d` called `SetVariable` for `MokListRT` and got
//! `EFI_UNSUPPORTED`, then stopped with `import_mok_state() failed`.
//! This store lasts for the firmware boot. It is not written to disk.

use super::memory::{EFI_INVALID_PARAMETER, EFI_NOT_FOUND, EFI_OUT_OF_RESOURCES, EFI_SUCCESS};

/// Slots for the Mok variables shim creates on one boot.
pub const VAR_SLOTS: usize = 16;
/// CHAR16 units including the trailing NUL.
pub const VAR_NAME_MAX: usize = 32;
/// Bytes of variable data. Shim's `MokListRT` on this ISO was 0x464.
pub const VAR_DATA_MAX: usize = 4096;

const ATTR_NV: u32 = 0x1;
const ATTR_BS: u32 = 0x2;
const ATTR_RT: u32 = 0x4;
const ATTR_APPEND: u32 = 0x40;
const ATTR_KNOWN: u32 = ATTR_NV | ATTR_BS | ATTR_RT | 0x8 | 0x10 | 0x20 | ATTR_APPEND;

#[derive(Clone, Copy)]
struct Slot {
    used: bool,
    guid: [u8; 16],
    name: [u16; VAR_NAME_MAX],
    attrs: u32,
    len: u16,
    data: [u8; VAR_DATA_MAX],
}

const EMPTY_SLOT: Slot = Slot {
    used: false,
    guid: [0; 16],
    name: [0; VAR_NAME_MAX],
    attrs: 0,
    len: 0,
    data: [0; VAR_DATA_MAX],
};

/// Volatile variable store. `SetVariable` with a zero size deletes.
pub struct VarStore {
    slots: [Slot; VAR_SLOTS],
}

impl VarStore {
    pub const fn new() -> Self {
        Self {
            slots: [EMPTY_SLOT; VAR_SLOTS],
        }
    }

    pub fn get(&self, guid: &[u8; 16], name: &[u16]) -> Option<(u32, &[u8])> {
        self.find(guid, name).map(|s| {
            let n = s.len as usize;
            (s.attrs, &s.data[..n])
        })
    }

    /// Create, replace, append, or delete. `name` excludes the NUL.
    pub fn set(&mut self, guid: [u8; 16], name: &[u16], attrs: u32, data: &[u8]) -> u64 {
        if name.is_empty() || name.len() >= VAR_NAME_MAX {
            return EFI_INVALID_PARAMETER;
        }
        if data.is_empty() {
            return match self.find_mut_index(guid, name) {
                Some(i) => {
                    self.slots[i] = EMPTY_SLOT;
                    EFI_SUCCESS
                }
                None => EFI_NOT_FOUND,
            };
        }
        if attrs == 0 || attrs & !ATTR_KNOWN != 0 || attrs & ATTR_BS == 0 {
            return EFI_INVALID_PARAMETER;
        }
        if attrs & ATTR_RT != 0 && attrs & ATTR_BS == 0 {
            return EFI_INVALID_PARAMETER;
        }
        let append = attrs & ATTR_APPEND != 0;
        let stored_attrs = attrs & !ATTR_APPEND;
        if let Some(i) = self.find_mut_index(guid, name) {
            let slot = &mut self.slots[i];
            if slot.attrs != stored_attrs {
                return EFI_INVALID_PARAMETER;
            }
            if append {
                let end = slot.len as usize + data.len();
                if end > VAR_DATA_MAX {
                    return EFI_OUT_OF_RESOURCES;
                }
                slot.data[slot.len as usize..end].copy_from_slice(data);
                slot.len = end as u16;
            } else if data.len() > VAR_DATA_MAX {
                return EFI_OUT_OF_RESOURCES;
            } else {
                slot.data[..data.len()].copy_from_slice(data);
                slot.len = data.len() as u16;
            }
            return EFI_SUCCESS;
        }
        if append {
            return EFI_NOT_FOUND;
        }
        if data.len() > VAR_DATA_MAX {
            return EFI_OUT_OF_RESOURCES;
        }
        let Some(i) = self.slots.iter().position(|s| !s.used) else {
            return EFI_OUT_OF_RESOURCES;
        };
        let mut slot = EMPTY_SLOT;
        slot.used = true;
        slot.guid = guid;
        slot.name[..name.len()].copy_from_slice(name);
        slot.attrs = stored_attrs;
        slot.len = data.len() as u16;
        slot.data[..data.len()].copy_from_slice(data);
        self.slots[i] = slot;
        EFI_SUCCESS
    }

    /// Next variable after `name`. An empty `name` returns the first.
    pub fn next(&self, guid: &[u8; 16], name: &[u16]) -> Result<Option<( [u8; 16], &[u16])>, ()> {
        if name.len() >= VAR_NAME_MAX {
            return Err(());
        }
        let start = if name.is_empty() {
            0
        } else {
            match self.find_index(guid, name) {
                Some(i) => i + 1,
                None => return Ok(None),
            }
        };
        for s in self.slots[start..].iter().filter(|s| s.used) {
            let n = s.name.iter().position(|c| *c == 0).unwrap_or(s.name.len());
            return Ok(Some((s.guid, &s.name[..n])));
        }
        Ok(None)
    }

    /// `(maximum storage, remaining, maximum variable size)` in bytes.
    pub fn query(&self, attrs: u32) -> Result<(u64, u64, u64), u64> {
        if attrs == 0 || attrs & !ATTR_KNOWN != 0 || attrs & ATTR_BS == 0 {
            return Err(EFI_INVALID_PARAMETER);
        }
        let cap = (VAR_SLOTS * VAR_DATA_MAX) as u64;
        let used: u64 = self
            .slots
            .iter()
            .filter(|s| s.used)
            .map(|s| u64::from(s.len))
            .sum();
        Ok((cap, cap.saturating_sub(used), VAR_DATA_MAX as u64))
    }

    fn find(&self, guid: &[u8; 16], name: &[u16]) -> Option<&Slot> {
        self.find_index(guid, name).map(|i| &self.slots[i])
    }

    fn find_index(&self, guid: &[u8; 16], name: &[u16]) -> Option<usize> {
        self.slots.iter().position(|s| s.used && s.guid == *guid && name_eq(&s.name, name))
    }

    fn find_mut_index(&mut self, guid: [u8; 16], name: &[u16]) -> Option<usize> {
        self.find_index(&guid, name)
    }
}

fn name_eq(stored: &[u16], name: &[u16]) -> bool {
    let n = stored.iter().position(|c| *c == 0).unwrap_or(stored.len());
    stored[..n] == *name
}
