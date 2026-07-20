//! Minimal guest instruction decode for APIC MMIO EPT violations (M3.11/M3.12).
//!
//! Supports 32-bit `mov r/m, r` / `mov r, r/m` (opcodes 0x89 / 0x8B) with
//! optional REX — enough for Linux `native_apic_mem_{read,write}`.
//!
//! Must parse SIB+disp32 correctly: Linux `native_apic_mem_eoi` is
//! `mov %eax, disp32` (`89 04 25 xx xx xx xx`). A short length advances
//! guest RIP into the displacement and panics (`Fatal exception in interrupt`).

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

    if mod_ == 3 {
        return None; // register-register — not MMIO
    }

    // SIB byte when rm == 4.
    let mut sib_base = None;
    if rm == 4 {
        if i >= insn.len() {
            return None;
        }
        let sib = insn[i];
        i += 1;
        sib_base = Some(sib & 7);
    }

    // Displacement (Intel Vol. 2: ModRM / SIB forms).
    let disp = match mod_ {
        0 => {
            if rm == 5 {
                // [RIP+disp32] (64-bit) / abs disp32 (32-bit)
                4
            } else if rm == 4 && sib_base == Some(5) {
                // SIB with base=5, mod=0 → [index*scale + disp32] (no base)
                4
            } else {
                0
            }
        }
        1 => 1,
        2 => 4,
        _ => return None,
    };
    if disp != 0 {
        if i + disp > insn.len() {
            return None;
        }
        i += disp;
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

/// Apply decoded MMIO mov into a virtio-mmio BAR (blk M4.3 or net M4.4).
pub fn apply_virtio_mov(
    mov: MovMmio,
    gpa: u64,
    gprs: &mut [u64; 16],
) -> Result<(), ()> {
    if mov.is_write {
        let val = read_gpr(mov.reg, gprs) as u32;
        if crate::devices::virtio_blk::bar_contains(gpa) {
            crate::devices::virtio_blk::mmio_access(gpa, true, val).ok_or(())?;
        } else if crate::devices::virtio_net::bar_contains(gpa) {
            crate::devices::virtio_net::mmio_access(gpa, true, val).ok_or(())?;
        } else {
            return Err(());
        }
    } else {
        let v = if crate::devices::virtio_blk::bar_contains(gpa) {
            crate::devices::virtio_blk::mmio_access(gpa, false, 0)
                .ok_or(())?
                .ok_or(())?
        } else if crate::devices::virtio_net::bar_contains(gpa) {
            crate::devices::virtio_net::mmio_access(gpa, false, 0)
                .ok_or(())?
                .ok_or(())?
        } else {
            return Err(());
        };
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

    #[test]
    fn decode_linux_apic_eoi_abs_store() {
        // native_apic_mem_eoi: xor %eax,%eax; mov %eax,0xff5fd0b0
        //   89 04 25 b0 d0 5f ff
        let m = decode_mov_mmio(&[0x89, 0x04, 0x25, 0xb0, 0xd0, 0x5f, 0xff]).unwrap();
        assert!(m.is_write);
        assert_eq!(m.reg, 0);
        assert_eq!(m.len, 7, "short len resumes inside disp32 and panics Linux");
    }

    #[test]
    fn decode_sib_base_no_disp() {
        // mov [rsp], eax → 89 04 24  (SIB base=rsp, index=none)
        let m = decode_mov_mmio(&[0x89, 0x04, 0x24]).unwrap();
        assert!(m.is_write);
        assert_eq!(m.len, 3);
    }
}
