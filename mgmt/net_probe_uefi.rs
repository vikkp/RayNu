//! PRE-EBS UEFI network protocol presence probe + driver connect (ADR-012).
//!
//! Pillar: [Z] · Proven Core: **outside**
//!
//! R640 Virtual Floppy boots often show `snp=0 … tcp4=0` because UNDI/SNP
//! drivers are present in firmware but not yet **started**. This module:
//! 1. Probes handle counts (SNP/MNP/Ip4/Dhcp4/Tcp4 + extra NetworkPkg census)
//! 2. `ConnectController` on PCI I/O, Device Path, SNP, then stack SB + all-handles
//! 3. Re-probes so COM2 shows before/after
//!
//! Does **not** change the SNP + smoltcp listen residual. Tcp4 listen still
//! prefers firmware `Tcp4ServiceBinding` when this probe makes it appear.
//!
//! Root-cause note: [`docs/evidence/r640/2026-08-16-uefi-tcp4-absent-root-cause.md`]

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
/// EFI_NETWORK_INTERFACE_IDENTIFIER_PROTOCOL_GUID (UNDI 3.1 / NII)
const NII_GUID: Guid = uefi::guid!("e18541cd-f755-4f73-928d-643c8a79b229");
/// EFI_PXE_BASE_CODE_PROTOCOL_GUID
const PXE_BC_GUID: Guid = uefi::guid!("03c4e603-ac28-11d3-9a2d-0090273fc14d");
/// EFI_HTTP_SERVICE_BINDING_PROTOCOL_GUID
const HTTP_SB_GUID: Guid = uefi::guid!("bdc8e6af-d9bc-4379-a72a-e0c4e75dae1c");
/// EFI_IP4_CONFIG2_PROTOCOL_GUID
const IP4_CONFIG2_GUID: Guid = uefi::guid!("5b446ed1-e30b-4faa-871a-3654eca36080");
/// EFI_DRIVER_BINDING_PROTOCOL_GUID — any dispatched UEFI driver
const DRIVER_BINDING_GUID: Guid = uefi::guid!("18a031ab-b443-4d1a-a5c0-0c09261e9f71");

#[derive(Clone, Copy, Default)]
struct NetCounts {
    snp: u32,
    mnp: u32,
    ip4: u32,
    dhcp4: u32,
    tcp4: u32,
    pci: u32,
}

#[derive(Clone, Copy, Default)]
struct ExtraCounts {
    nii: u32,
    pxe: u32,
    http: u32,
    ip4cfg: u32,
    drv: u32,
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

fn snapshot_extra() -> ExtraCounts {
    ExtraCounts {
        nii: count_protocol(&NII_GUID),
        pxe: count_protocol(&PXE_BC_GUID),
        http: count_protocol(&HTTP_SB_GUID),
        ip4cfg: count_protocol(&IP4_CONFIG2_GUID),
        drv: count_protocol(&DRIVER_BINDING_GUID),
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

fn print_extra(tag: &str, e: &ExtraCounts) {
    use crate::boot::serial;
    serial::write_str("boot: uefi-net ");
    serial::write_str(tag);
    serial::write_str(" nii=");
    write_u32(e.nii);
    serial::write_str(" pxe=");
    write_u32(e.pxe);
    serial::write_str(" http=");
    write_u32(e.http);
    serial::write_str(" ip4cfg=");
    write_u32(e.ip4cfg);
    serial::write_str(" drv=");
    write_u32(e.drv);
    serial::write_byte(b'\n');
}

fn connect_handles_by_protocol(guid: &Guid) -> u32 {
    let Ok(handles) = boot::locate_handle_buffer(SearchType::ByProtocol(guid)) else {
        return 0;
    };
    let mut n = 0u32;
    for h in handles.iter() {
        // recursive=true: start child drivers (UNDI → SNP → MNP → Ip4 → Tcp4)
        if boot::connect_controller(*h, None, None, true).is_ok() {
            n = n.saturating_add(1);
        }
    }
    n
}

/// Connect any already-present NetworkPkg service-binding handles.
/// No-op when counts are 0 (R640 Virtual Floppy lived case).
fn connect_network_stack_bindings() -> u32 {
    let mut n = 0u32;
    n = n.saturating_add(connect_handles_by_protocol(&MNP_SB_GUID));
    n = n.saturating_add(connect_handles_by_protocol(&IP4_SB_GUID));
    n = n.saturating_add(connect_handles_by_protocol(&DHCP4_SB_GUID));
    n = n.saturating_add(connect_handles_by_protocol(&TCP4_SB_GUID));
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

fn print_diagnosis(c: &NetCounts, e: &ExtraCounts) {
    use crate::boot::serial;
    if c.tcp4 > 0 {
        serial::write_line("boot: HINT — Tcp4ServiceBinding present; firmware Tcp4 path next");
        return;
    }
    if c.snp == 0 && c.pci == 0 {
        serial::write_line(
            "boot: HINT — no UEFI net/PCI protocols; post-EBS PCI NIC residual (not SNP)",
        );
        return;
    }
    if c.snp > 0 && c.mnp == 0 && c.ip4 == 0 && c.dhcp4 == 0 && c.tcp4 == 0 {
        if e.pxe == 0 && e.http == 0 && e.ip4cfg == 0 {
            serial::write_line(
                "boot: HINT — UNDI/SNP up, NetworkPkg DXEs not dispatched (no MNP/Ip4/Tcp4/PXE/HTTP SB)",
            );
            serial::write_line(
                "boot: HINT — Virtual Floppy BDS did not start firmware Tcp4; SNP residual next (ADR-012)",
            );
            return;
        }
        serial::write_line(
            "boot: HINT — PXE/HTTP/Ip4Config2 published, Tcp4ServiceBinding still 0 (vendor stack)",
        );
        serial::write_line(
            "boot: HINT — firmware Tcp4 listen impossible on this path; SNP residual next (ADR-012)",
        );
        return;
    }
    serial::write_line("boot: HINT — SNP present, Tcp4 still 0; SNP residual next (ADR-012)");
}

/// Probe → connect PCI/device-path/SNP/stack/all-handles → re-probe.
/// Call before Tcp4 listen. Does not open SNP exclusively (listen path does GetProtocol).
pub fn probe_and_print() {
    use crate::boot::serial;

    let before = snapshot();
    print_counts("probe", &before);
    print_extra("extra", &snapshot_extra());

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
    }

    // Even when SNP is already up: try SB handles (usually 0) then all-handles.
    // Prior tip skipped all-handles once snp>0 — that hid whether a late DXE exists.
    serial::write_line("boot: uefi-net connect — NetworkPkg SB + all-handles (bounded)");
    let stack_ok = connect_network_stack_bindings();
    let all_ok = connect_all_handles_shallow();
    serial::write_str("boot: uefi-net connect stack_ok=");
    write_u32(stack_ok);
    serial::write_str(" all_ok=");
    write_u32(all_ok);
    serial::write_byte(b'\n');

    let after = snapshot();
    print_counts("after-all", &after);
    let extra = snapshot_extra();
    print_extra("extra-after", &extra);
    print_diagnosis(&after, &extra);
}
