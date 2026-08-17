//! PRE-EBS PCI / MSI-X / IOMMU census (ADR-013 Phase 0).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside**
//!
//! Evidence-only serial. Does **not** guess Broadcom vs Intel LOM. Phase D
//! may bind a driver only to a printed `vid:did` (lab: QEMU `8086:100e`).

use crate::mgmt::e1000_mmio::pci_id_is_qemu_e1000;

pub const PCI_CLASS_NETWORK: u8 = 0x02;
pub const PCI_CAP_MSIX: u8 = 0x11;
pub const CENSUS_NOTE: &str = "boot: PCI census";

/// One Ethernet-class function from a mocked or live config space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciNicRecord {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub bar0: u32,
    pub msix_entries: Option<u16>,
}

/// Network controller base class (PCI class code byte).
pub fn is_network_class(class: u8) -> bool {
    class == PCI_CLASS_NETWORK
}

/// MSI-X table size from Message Control (`N - 1` in bits 10:0).
pub fn msix_table_entries(msg_ctrl: u16) -> u16 {
    (msg_ctrl & 0x07FF) + 1
}

/// True when a 4-byte ACPI signature is Intel VT-d `DMAR`.
pub fn acpi_sig_is_dmar(sig: &[u8; 4]) -> bool {
    sig == b"DMAR"
}

/// RSDP signature ("RSD PTR ").
pub fn is_rsdp_signature(bytes: &[u8; 8]) -> bool {
    bytes == b"RSD PTR "
}

/// Pick the lab NIC only when QEMU e1000 is present. Never auto-picks X710/BCM.
pub fn pick_lab_or_none(nics: &[PciNicRecord]) -> Option<PciNicRecord> {
    nics.iter()
        .copied()
        .find(|n| pci_id_is_qemu_e1000(n.vendor, n.device))
}

/// Phase D: the only `smoltcp::phy::Device` in-tree is QEMU e1000. Iron waits.
pub fn census_nic_has_lab_driver(vendor: u16, device: u16) -> bool {
    pci_id_is_qemu_e1000(vendor, device)
}

/// Walk a mocked capability list (host tests / Kani). `read_dword(offset)`.
pub fn find_msix_entries(status: u16, cap_ptr: u8, mut read_dword: impl FnMut(u8) -> u32) -> Option<u16> {
    if status & (1 << 4) == 0 {
        return None;
    }
    let mut ptr = cap_ptr;
    for _ in 0..48 {
        if ptr == 0 || ptr == 0xff || ptr < 0x40 {
            break;
        }
        let d = read_dword(ptr);
        let id = d as u8;
        let next = (d >> 8) as u8;
        if id == PCI_CAP_MSIX {
            return Some(msix_table_entries((d >> 16) as u16));
        }
        if next == ptr {
            break;
        }
        ptr = next;
    }
    None
}

#[cfg(feature = "uefi-bin")]
pub fn run_pre_ebs_pci_census() {
    use crate::boot::serial;
    use crate::mgmt::e1000_mmio::pci_read32;

    let mut nics = [PciNicRecord {
        bus: 0,
        dev: 0,
        func: 0,
        vendor: 0,
        device: 0,
        class: 0,
        bar0: 0,
        msix_entries: None,
    }; 16];
    let mut n = 0usize;

    // R640 / QEMU: a handful of buses is enough; do not scan 256.
    for bus in 0u8..=15 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let id = pci_read32(bus, dev, func, 0);
                if id == 0xFFFF_FFFF {
                    if func == 0 {
                        break;
                    }
                    continue;
                }
                let vendor = id as u16;
                let device = (id >> 16) as u16;
                let class_dw = pci_read32(bus, dev, func, 0x08);
                let class = (class_dw >> 24) as u8;
                if is_network_class(class) && n < nics.len() {
                    let bar0 = pci_read32(bus, dev, func, 0x10) & !0xF;
                    let status = (pci_read32(bus, dev, func, 0x04) >> 16) as u16;
                    let cap_ptr = pci_read32(bus, dev, func, 0x34) as u8;
                    let msix = find_msix_entries(status, cap_ptr, |off| pci_read32(bus, dev, func, off));
                    nics[n] = PciNicRecord {
                        bus,
                        dev,
                        func,
                        vendor,
                        device,
                        class,
                        bar0,
                        msix_entries: msix,
                    };
                    n += 1;
                }
                if func == 0 {
                    let ht = (pci_read32(bus, dev, func, 0x0C) >> 16) as u8;
                    if ht & 0x80 == 0 {
                        break;
                    }
                }
            }
        }
    }

    serial::write_str(CENSUS_NOTE);
    serial::write_str(" nics=");
    write_u8_dec(n as u8);
    serial::write_byte(b'\n');
    for nic in nics.iter().take(n) {
        serial::write_str("boot: PCI ");
        write_hex_u8(nic.bus);
        serial::write_byte(b':');
        write_hex_u8(nic.dev);
        serial::write_byte(b'.');
        write_hex_u8(nic.func);
        serial::write_str(" vid:did=");
        write_hex_u16(nic.vendor);
        serial::write_byte(b':');
        write_hex_u16(nic.device);
        serial::write_str(" bar0=0x");
        write_hex_u32(nic.bar0);
        serial::write_str(" msix=");
        match nic.msix_entries {
            Some(e) => write_u16_dec(e),
            None => serial::write_str("none"),
        }
        serial::write_byte(b'\n');
    }

    // SAFETY: read-only BIOS/ACPI windows (EBDA + 0xE0000). No writes.
    // KANI-TARGET: host tests mock DMAR/RSDP signatures, not this scan.
    let dmar = unsafe { acpi_dmar_present() };
    serial::write_str("boot: IOMMU ACPI DMAR=");
    serial::write_line(if dmar { "yes" } else { "no" });

    if let Some(pick) = pick_lab_or_none(&nics[..n]) {
        serial::write_str("boot: HOST-NIC census pick vid:did=");
        write_hex_u16(pick.vendor);
        serial::write_byte(b':');
        write_hex_u16(pick.device);
        serial::write_line(" (QEMU e1000 lab)");
        store_pick(pick.vendor, pick.device);
    } else if n > 0 {
        serial::write_line(
            "boot: HOST-NIC census: no lab e1000; do not guess LOM (Phase D waits on this list)",
        );
        store_pick(nics[0].vendor, nics[0].device);
    } else {
        serial::write_line("boot: HOST-NIC census: no Ethernet-class PCI functions");
    }
}

#[cfg(not(feature = "uefi-bin"))]
pub fn run_pre_ebs_pci_census() {}

// JUSTIFICATION: single PRE-EBS writer; post-BOOT-OK idle reads on the same hart.
static mut PICK_VID: u16 = 0;
static mut PICK_DID: u16 = 0;

fn store_pick(vendor: u16, device: u16) {
    // SAFETY: BSP-only; written once before EBS, read later on the same hart.
    // KANI-TARGET: host tests call iron_marker_allowed with mocked ids.
    unsafe {
        PICK_VID = vendor;
        PICK_DID = device;
    }
}

/// Census pick for Phase D (0,0 = none).
pub fn census_pick() -> Option<(u16, u16)> {
    // SAFETY: BSP-only; PRE-EBS store happens-before idle reads.
    let (v, d) = unsafe { (PICK_VID, PICK_DID) };
    if v == 0 && d == 0 {
        None
    } else {
        Some((v, d))
    }
}

/// Iron `HOST-NIC-HTTP-OK` only for a non-QEMU census NIC after BOOT-OK.
pub fn iron_marker_allowed(vendor: u16, device: u16) -> bool {
    !pci_id_is_qemu_e1000(vendor, device) && vendor != 0
}

/// After a native HTTP exchange: QEMU-OK for `8086:100e`, iron HTTP-OK otherwise.
///
/// The iron marker string lives here so `host_nic_listen.rs` never contains it.
#[cfg(feature = "uefi-bin")]
pub fn print_host_nic_exchange_ok_marker() {
    use crate::boot::serial;
    use crate::mgmt::host_nic::{M7_HOST_NIC_HTTP_OK_MARKER, M7_HOST_NIC_QEMU_MARKER};
    match census_pick() {
        Some((v, d)) if iron_marker_allowed(v, d) => {
            serial::write_line(M7_HOST_NIC_HTTP_OK_MARKER);
        }
        _ => serial::write_line(M7_HOST_NIC_QEMU_MARKER),
    }
}

/// Read-only ACPI DMAR presence (Intel VT-d). Evidence serial only.
///
/// SAFETY: caller must invoke only on the BSP with identity-mapped BIOS/ACPI
/// windows. This function never writes.
/// KANI-TARGET: host tests use `acpi_sig_is_dmar` / `is_rsdp_signature`.
#[cfg(feature = "uefi-bin")]
unsafe fn acpi_dmar_present() -> bool {
    if let Some(rsdp) = find_rsdp() {
        return xsdt_has_dmar(rsdp);
    }
    false
}

/// SAFETY: 0x40E is the real-mode EBDA segment pointer (ACPI RSDP search).
/// KANI-TARGET: host tests mock RSDP bytes, not this path.
#[cfg(feature = "uefi-bin")]
unsafe fn find_rsdp() -> Option<u64> {
    let ebda_seg = core::ptr::read_volatile(0x40E as *const u16) as u64;
    let ebda = ebda_seg << 4;
    if ebda != 0 {
        if let Some(a) = scan_rsdp(ebda, ebda.saturating_add(1024)) {
            return Some(a);
        }
    }
    scan_rsdp(0xE_0000, 0x10_0000)
}

/// SAFETY: `start..end` is a BIOS ROM / EBDA window; 16-byte aligned reads.
/// KANI-TARGET: host tests mock RSDP bytes, not this path.
#[cfg(feature = "uefi-bin")]
unsafe fn scan_rsdp(start: u64, end: u64) -> Option<u64> {
    let mut addr = start & !0xF;
    while addr < end {
        let p = addr as *const u8;
        let mut sig = [0u8; 8];
        for i in 0..8 {
            sig[i] = core::ptr::read_volatile(p.add(i));
        }
        if is_rsdp_signature(&sig) {
            return Some(addr);
        }
        addr = addr.saturating_add(16);
    }
    None
}

/// SAFETY: `rsdp` came from [`find_rsdp`]; XSDT entries are packed LE phys addrs.
/// KANI-TARGET: host tests parse mocked signatures, not this walk.
#[cfg(feature = "uefi-bin")]
unsafe fn xsdt_has_dmar(rsdp: u64) -> bool {
    let rev = core::ptr::read_volatile((rsdp as *const u8).add(15));
    if rev < 2 {
        return false;
    }
    let xsdt = core::ptr::read_volatile((rsdp + 24) as *const u64);
    if xsdt < 0x1000 {
        return false;
    }
    let len = core::ptr::read_volatile((xsdt + 4) as *const u32) as u64;
    if len < 36 || len > 4096 {
        return false;
    }
    let entries = (len - 36) / 8;
    for i in 0..entries {
        let tbl = core::ptr::read_volatile((xsdt + 36 + i * 8) as *const u64);
        if tbl < 0x1000 {
            continue;
        }
        let mut sig = [0u8; 4];
        for b in 0..4 {
            sig[b] = core::ptr::read_volatile((tbl as *const u8).add(b));
        }
        if acpi_sig_is_dmar(&sig) {
            return true;
        }
    }
    false
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u8(b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    use crate::boot::serial;
    serial::write_byte(HEX[(b >> 4) as usize]);
    serial::write_byte(HEX[(b & 0xf) as usize]);
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u16(n: u16) {
    write_hex_u8((n >> 8) as u8);
    write_hex_u8(n as u8);
}

#[cfg(feature = "uefi-bin")]
fn write_hex_u32(n: u32) {
    use crate::boot::serial;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in (0..8).rev() {
        serial::write_byte(HEX[((n >> (i * 4)) & 0xf) as usize]);
    }
}

#[cfg(feature = "uefi-bin")]
fn write_u8_dec(n: u8) {
    write_u16_dec(n as u16);
}

#[cfg(feature = "uefi-bin")]
fn write_u16_dec(mut n: u16) {
    use crate::boot::serial;
    let mut buf = [0u8; 5];
    let mut i = 5;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

#[cfg(test)]
#[path = "pci_census_test.rs"]
mod pci_census_test;
