//! PRE-EBS UEFI network protocol presence probe (ADR-012 diagnostics).
//!
//! Pillar: [Z] · Proven Core: **outside**
//! Prints handle counts for SNP / MNP / Ip4 / Dhcp4 / Tcp4 so iron COM2
//! shows *why* Tcp4 listen soft-failed (stack never loaded vs bind fail).

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
/// EFI_TCP4_SERVICE_BINDING_PROTOCOL_GUID (same as tcp4_uefi)
const TCP4_SB_GUID: Guid = uefi::guid!("00720665-67eb-4a99-baf7-d3c33a1c7ce9");

fn count_protocol(guid: &Guid) -> u32 {
    match boot::locate_handle_buffer(SearchType::ByProtocol(guid)) {
        Ok(handles) => handles.len() as u32,
        Err(_) => 0,
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

/// Emit one COM line: `boot: uefi-net probe snp=N mnp=N ip4=N dhcp4=N tcp4=N`
pub fn probe_and_print() {
    use crate::boot::serial;

    let snp = count_protocol(&SNP_GUID);
    let mnp = count_protocol(&MNP_SB_GUID);
    let ip4 = count_protocol(&IP4_SB_GUID);
    let dhcp4 = count_protocol(&DHCP4_SB_GUID);
    let tcp4 = count_protocol(&TCP4_SB_GUID);

    serial::write_str("boot: uefi-net probe snp=");
    write_u32(snp);
    serial::write_str(" mnp=");
    write_u32(mnp);
    serial::write_str(" ip4=");
    write_u32(ip4);
    serial::write_str(" dhcp4=");
    write_u32(dhcp4);
    serial::write_str(" tcp4=");
    write_u32(tcp4);
    serial::write_byte(b'\n');
}
