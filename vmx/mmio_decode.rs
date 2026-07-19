//! Minimal guest instruction decode for APIC MMIO EPT violations (M3.11).
//!
//! Supports 32-bit `mov r/m, r` / `mov r, r/m` (opcodes 0x89 / 0x8B) with
//! optional REX — enough for Linux `native_apic_mem_{read,write}`.

/// Result of decoding a simple GPR↔memory MOV at guest RIP.
#[derive(Debug, Clone, Copy)]
pub struct MovMmio {
    pub is_write: bool,
    pub reg: u8,
    pub len: u8,
}

fn read_gpr(reg: u8, gprs: &[u64; 16]) -> u64 {
    gprs[(reg & 15) as usize]
}

fn write_gpr(reg: u8, val: u64, gprs: &mut [u64; 16]) {
    let i = (reg & 15) as usize;
    // APIC accesses are 32-bit; merge into low half.
    gprs[i] = (gprs[i] & !0xFFFF_FFFF) | (val & 0xFFFF_FFFF);
}

/// Decode `mov` at `insn` (guest identity-mapped bytes).
pub fn decode_mov_mmio(insn: &[u8]) -> Option<MovMmio> {
    if insn.len() < 2 {
        return None;
    }
    let mut i = 0usize;
    let mut rex_r = 0u8;
    let b0 = insn[i];
    if b0 & 0xF0 == 0x40 {
        rex_r = (b0 >> 2) & 1;
        i += 1;
        if i >= insn.len() {
            return None;
        }
    }
    let op = insn[i];
    i += 1;
    let is_write = match op {
        0x89 => true,  // mov r/m32, r32
        0x8B => false, // mov r32, r/m32
        _ => return None,
    };
    if i >= insn.len() {
        return None;
    }
    let modrm = insn[i];
    i += 1;
    let mod_ = modrm >> 6;
    let reg = ((modrm >> 3) & 7) | (rex_r << 3);
    let rm = modrm & 7;

    // SIB byte
    if mod_ != 3 && rm == 4 {
        if i >= insn.len() {
            return None;
        }
        i += 1; // SIB
    }
    // Displacement
    match mod_ {
        0 => {
            if rm == 5 {
                // disp32 (RIP-relative or abs in 32-bit — both skip 4)
                i += 4;
            }
        }
        1 => i += 1,
        2 => i += 4,
        3 => return None, // register-register — not MMIO
        _ => {}
    }
    if i > 15 {
        return None;
    }
    Some(MovMmio {
        is_write,
        reg,
        len: i as u8,
    })
}

/// Apply decoded MMIO mov using GPA-selected APIC access.
pub fn apply_apic_mov(
    mov: MovMmio,
    gpa: u64,
    gprs: &mut [u64; 16],
) -> Result<(), ()> {
    if mov.is_write {
        let val = read_gpr(mov.reg, gprs) as u32;
        crate::devices::lapic_virt::mmio_access(gpa, true, val).ok_or(())?;
    } else {
        let v = crate::devices::lapic_virt::mmio_access(gpa, false, 0)
            .ok_or(())?
            .ok_or(())?;
        write_gpr(mov.reg, v as u64, gprs);
    }
    Ok(())
}

#[cfg(test)]
mod mmio_decode_test {
    use super::*;

    #[test]
    fn decode_mov_store_rax_disp8() {
        // mov [rcx+0x20], eax  → 89 41 20
        let m = decode_mov_mmio(&[0x89, 0x41, 0x20]).unwrap();
        assert!(m.is_write);
        assert_eq!(m.reg, 0); // eax
        assert_eq!(m.len, 3);
    }

    #[test]
    fn decode_mov_load_eax() {
        // mov eax, [rcx] → 8B 01
        let m = decode_mov_mmio(&[0x8B, 0x01]).unwrap();
        assert!(!m.is_write);
        assert_eq!(m.reg, 0);
        assert_eq!(m.len, 2);
    }
}
