//! PRE-EBS UEFI network protocol presence probe + driver connect (ADR-012).
//!
//! Pillar: [Z] · Proven Core: **outside**
//!
//! R640 Virtual Floppy boots often show `snp=0 … tcp4=0` because UNDI/SNP
//! drivers are present in firmware but not yet **started**. This module:
//! 1. Probes handle counts (SNP/MNP/Ip4/Dhcp4/Tcp4)
//! 2. `ConnectController` on PCI I/O (+ optional all-handles pass)
//! 3. Re-probes so COM2 shows before/after

#![cfg(feature = "uefi-bin")]

use uefi::boot::{self, SearchType};
use uefi::Guid;

/// EFI_SIMPLE_NETWORK_PROTOCOL_GUID
const SNP_GUID: Guid = uefi::guid!("a19832b9-ac25-11d3-9a2d-0090273fc14d");
/// EFI_MANAGED_NETWORK_SERVICE_BINDING_PROTOCOL_GUID
const MNP_SB_GUID: Guid = uefi::guid!("f36ff770-a7e1-4cf9-9cba-e34b511d67b6");
/// EFI_IP4_SERVICE_BINDING_PROTOCOL_GUID
const IP4_SB_GUID: Guid = uefi::guid!("c51711eb-a9cf-46df-8e9e-23f19aa49611");
/// EFI_DHCP4_SERVICE_BINDING_PROTOCOL_GUID
const DHCP4_SB_GUID: Guid = uefi::guid!("9d9a39d8-bd06-45c5-aa0b-918bd6483b45");
/// EFI_TCP4_SERVICE_BINDING_PROTOCOL_GUID
const TCP4_SB_GUID: Guid = uefi::guid!("00720665-67eb-4a99-baf7-d3c33a1c7ce9");
/// EFI_PCI_IO_PROTOCOL_GUID — NIC UNDI usually hangs off PCI controllers
const PCI_IO_GUID: Guid = uefi::guid!("4cf5b200-68b8-4ca5-9eec-b23e3f50029a");
/// EFI_DEVICE_PATH_PROTOCOL_GUID
const DEVICE_PATH_GUID: Guid = uefi::guid!("09576e91-6d3f-11d2-8e39-00a0c969723b");

#[derive(Clone, Copy, Default)]
struct NetCounts {
    snp: u32,
    mnp: u32,
    ip4: u32,
    dhcp4: u32,
    tcp4: u32,
    pci: u32,
}

fn count_protocol(guid: &Guid) -> u32 {
    match boot::locate_handle_buffer(SearchType::ByProtocol(guid)) {
        Ok(handles) => handles.len() as u32,
        Err(_) => 0,
    }
}

fn snapshot() -> NetCounts {
    NetCounts {
        snp: count_protocol(&SNP_GUID),
        mnp: count_protocol(&MNP_SB_GUID),
        ip4: count_protocol(&IP4_SB_GUID),
        dhcp4: count_protocol(&DHCP4_SB_GUID),
        tcp4: count_protocol(&TCP4_SB_GUID),
        pci: count_protocol(&PCI_IO_GUID),
    }
}

fn write_u32(n: u32) {
    use crate::boot::serial;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = 10;
    let mut x = n;
    while x > 0 {
        i -= 1;
        buf[i] = b'0' + (x % 10) as u8;
        x /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}

fn print_counts(tag: &str, c: &NetCounts) {
    use crate::boot::serial;
    serial::write_str("boot: uefi-net ");
    serial::write_str(tag);
    serial::write_str(" snp=");
    write_u32(c.snp);
    serial::write_str(" mnp=");
    write_u32(c.mnp);
    serial::write_str(" ip4=");
    write_u32(c.ip4);
    serial::write_str(" dhcp4=");
    write_u32(c.dhcp4);
    serial::write_str(" tcp4=");
    write_u32(c.tcp4);
    serial::write_str(" pci=");
    write_u32(c.pci);
    serial::write_byte(b'\n');
}

fn connect_handles_by_protocol(guid: &Guid) -> u32 {
    let Ok(handles) = boot::locate_handle_buffer(SearchType::ByProtocol(guid)) else {
        return 0;
    };
    let mut n = 0u32;
    for h in handles.iter() {
        // recursive=true: start child drivers (UNDI → SNP → Ip4 → Tcp4)
        if boot::connect_controller(*h, None, None, true).is_ok() {
            n = n.saturating_add(1);
        }
    }
    n
}

fn connect_all_handles_shallow() -> u32 {
    let Ok(handles) = boot::locate_handle_buffer(SearchType::AllHandles) else {
        return 0;
    };
    let mut n = 0u32;
    // Cap work: Virtual Floppy PRE-EBS window is short; connect first 256.
    for h in handles.iter().take(256) {
        if boot::connect_controller(*h, None, None, true).is_ok() {
            n = n.saturating_add(1);
        }
    }
    n
}

/// Probe → connect PCI/device-path → re-probe. Call before Tcp4 listen.
pub fn probe_and_print() {
    use crate::boot::serial;

    let before = snapshot();
    print_counts("probe", &before);

    if before.tcp4 > 0 {
        return;
    }

    serial::write_line("boot: uefi-net connect — starting PCI/UNDI drivers");
    let pci_ok = connect_handles_by_protocol(&PCI_IO_GUID);
    let dp_ok = connect_handles_by_protocol(&DEVICE_PATH_GUID);
    serial::write_str("boot: uefi-net connect pci_ok=");
    write_u32(pci_ok);
    serial::write_str(" device_path_ok=");
    write_u32(dp_ok);
    serial::write_byte(b'\n');

    let mid = snapshot();
    print_counts("after-pci", &mid);

    if mid.tcp4 > 0 {
        return;
    }

    if mid.snp > 0 {
        // Start drivers hanging off SNP (MNP/Ip4/Tcp4 if firmware has them).
        serial::write_line("boot: uefi-net connect — SNP child drivers");
        let snp_ok = connect_handles_by_protocol(&SNP_GUID);
        serial::write_str("boot: uefi-net connect snp_ok=");
        write_u32(snp_ok);
        serial::write_byte(b'\n');
        let after_snp = snapshot();
        print_counts("after-snp", &after_snp);
        if after_snp.tcp4 > 0 {
            return;
        }
        serial::write_line("boot: HINT — SNP present, Tcp4 still 0; SNP residual next (ADR-012)");
        return;
    }

    // Last resort: connect a bounded set of all handles (slow but honest).
    serial::write_line("boot: uefi-net connect — all-handles pass (bounded)");
    let all_ok = connect_all_handles_shallow();
    serial::write_str("boot: uefi-net connect all_ok=");
    write_u32(all_ok);
    serial::write_byte(b'\n');

    let after = snapshot();
    print_counts("after-all", &after);

    if after.snp == 0 && after.tcp4 == 0 && after.pci == 0 {
        serial::write_line(
            "boot: HINT — no UEFI net/PCI protocols; post-EBS PCI NIC residual (not SNP)",
        );
    } else if after.snp > 0 && after.tcp4 == 0 {
        serial::write_line("boot: HINT — SNP present, Tcp4 still 0; SNP residual next (ADR-012)");
    }
}
