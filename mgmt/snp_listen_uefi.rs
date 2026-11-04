//! PRE-EBS HTTP listen over SNP + smoltcp (ADR-012 residual).
//!
//! Used when firmware Tcp4/Ip4/Dhcp4 are absent but SNP NICs are up after
//! ConnectController. Size-audited via `tools/check-size.sh` (ADR-003).

#![cfg(feature = "uefi-bin")]

use crate::boot::serial;
use crate::mgmt::http_listen::MgmtListenError;
use crate::mgmt::snp_uefi::{open_first_snp, SnpDevice};
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpCidr, Ipv4Address, Ipv4Cidr};

const DHCP_BUDGET_MS: u64 = 12_000;

/// Bind `:port` on the first usable SNP NIC via DHCP + smoltcp TCP.
pub fn uefi_snp_listen(port: u16) -> Result<(), MgmtListenError> {
    let (_handle, snp, mac) = open_first_snp().map_err(|_| MgmtListenError::NoSnpNic)?;
    let mut device = SnpDevice::new(snp, mac);

    serial::write_str("boot: SNP residual MAC=");
    write_mac(mac);
    serial::write_byte(b'\n');

    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = mac_seed(mac);

    let mut millis: i64 = 0;
    let mut iface = Interface::new(config, &mut device, Instant::from_millis(millis));

    let dhcp_socket = dhcpv4::Socket::new();
    let mut sockets = SocketSet::new(alloc::vec::Vec::new());
    let dhcp_handle = sockets.add(dhcp_socket);

    serial::write_line("boot: SNP DHCP discover…");
    let mut leased: Option<Ipv4Cidr> = None;
    let mut leased_router: Option<Ipv4Address> = None;
    let dhcp_deadline = DHCP_BUDGET_MS as i64;

    while millis < dhcp_deadline {
        let ts = Instant::from_millis(millis);
        iface.poll(ts, &mut device, &mut sockets);

        match sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll() {
            Some(dhcpv4::Event::Configured(cfg)) => {
                leased = Some(cfg.address);
                leased_router = cfg.router;
                iface.update_ip_addrs(|addrs| {
                    addrs.clear();
                    let _ = addrs.push(IpCidr::Ipv4(cfg.address));
                });
                if let Some(router) = cfg.router {
                    let _ = iface.routes_mut().add_default_ipv4_route(router);
                }
                break;
            }
            Some(dhcpv4::Event::Deconfigured) => {
                iface.update_ip_addrs(|addrs| addrs.clear());
                iface.routes_mut().remove_default_ipv4_route();
            }
            None => {}
        }

        let _ = uefi::boot::stall(1_000);
        millis += 1;
    }

    let _ = sockets.remove(dhcp_handle);

    let Some(cidr) = leased else {
        serial::write_line("boot: WARN — SNP DHCP failed (no lease)");
        return Err(MgmtListenError::DhcpFailed);
    };

    let ip = cidr.address();
    serial::write_str("boot: SNP lease ");
    write_ipv4(ip);
    serial::write_byte(b'/');
    write_u16_dec(cidr.prefix_len() as u16);
    if let Some(r) = leased_router {
        serial::write_str(" router=");
        write_ipv4(r);
    } else {
        serial::write_str(" router=none");
    }
    serial::write_byte(b'\n');
    crate::mgmt::mgmt_lease::store(crate::mgmt::mgmt_lease::ParkedMgmtLease {
        ip: ip.octets(),
        prefix: cidr.prefix_len(),
        router: leased_router.map(|r| r.octets()).unwrap_or([0; 4]),
        has_router: leased_router.is_some(),
        mac,
        port,
    });
    crate::mgmt::bcm5720_mmio::peek_bcm5720_bmsr_pre_ebs();
    // ADR-021 E5: SNP keeps the DHCP lease. It does not bind TCP.
    // The first management packet is the post-EBS HTTPS listener.
    serial::write_line("boot: E5 plaintext PRE-EBS closed (no HTTP listen; HTTPS is post-EBS)");
    Ok(())
}

/// Leaked so Drop never CloseProtocol after EBS. Fields other than `ip`/`port`
/// are unread on purpose: firmware SNP must not be polled after EBS, but the
/// session must stay leaked (iron hang + RSOD when polled).
#[allow(dead_code)]
struct ParkedSnpHttp {
    device: SnpDevice,
    iface: Interface,
    sockets: SocketSet<'static>,
    tcp_handle: smoltcp::iface::SocketHandle,
    ip: Ipv4Address,
    port: u16,
    millis: i64,
}

static mut PARKED_HTTP: Option<&'static mut ParkedSnpHttp> = None;

#[allow(dead_code)]
fn park_snp_http(session: ParkedSnpHttp) {
    let leaked: &'static mut ParkedSnpHttp =
        alloc::boxed::Box::leak(alloc::boxed::Box::new(session));
    // SAFETY: single-threaded boot; leaked so Drop never CloseProtocol after EBS.
    unsafe {
        PARKED_HTTP = Some(leaked);
    }
    serial::write_line("boot: SNP parked for post-EBS (protocol not closed)");
}

fn parked_http() -> Option<&'static mut ParkedSnpHttp> {
    unsafe { PARKED_HTTP.as_deref_mut() }
}

/// ~1 ms spin without Boot Services (`stall` is invalid after EBS).
/// R640 Gold 6130 ~2.1 GHz; extra spin is harmless.
///
/// Kept for a future **host-owned** NIC poll loop. Do **not** use this to
/// retry firmware SNP after EBS (iron hung, then RSOD — 2026-08-17).
#[allow(dead_code)]
fn tsc_delay_ms(ms: u32) {
    let ticks = (ms as u64).saturating_mul(2_100_000);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

/// After EBS: **do not** call SNP. Iron 2026-08-16 hung here — first
/// `iface.poll` after ExitBootServices never returned (`smoke frame` last line).
/// Guest path must continue. Parked lease is printed only. After VMXOFF the
/// idle path is WARN-only (`uefi_snp_post_ebs_idle`) — firmware SNP is dead.
pub fn uefi_snp_post_ebs_probe() {
    let Some(s) = parked_http() else {
        serial::write_line(
            "boot: WARN — no parked SNP (PRE-EBS fallback only; post-EBS HTTP skipped)",
        );
        return;
    };
    serial::write_str("boot: post-EBS SNP parked lease=");
    write_ipv4(s.ip);
    serial::write_byte(b':');
    write_u16_dec(s.port);
    serial::write_line(" (not polling SNP yet — iron hung on immediate post-EBS poll)");
}

/// After VMXOFF: **do not** poll firmware SNP. Iron 2026-08-17: idle banner
/// printed, Mac curl to the parked lease timed out, then Dell UEFI RSOD
/// (invalid opcode, RIP=`0x17`). Immediate post-EBS poll already hung.
/// PRE-EBS remains the mgmt UI. See `docs/evidence/r640/2026-08-17-post-ebs-snp-dead.md`.
pub fn uefi_snp_post_ebs_idle() {
    let Some(s) = parked_http() else {
        return;
    };
    serial::write_str("boot: WARN — POST-EBS SNP idle skipped; firmware SNP dead after EBS lease=");
    write_ipv4(s.ip);
    serial::write_byte(b':');
    write_u16_dec(s.port);
    serial::write_line(" (PRE-EBS was the mgmt window; do not chase SNP/Tcp4)");
}

fn mac_seed(mac: [u8; 6]) -> u64 {
    let mut s = 0u64;
    for (i, b) in mac.iter().enumerate() {
        s ^= (*b as u64) << ((i % 8) * 8);
    }
    s | 1
}

fn write_mac(mac: [u8; 6]) {
    for (i, b) in mac.iter().enumerate() {
        if i > 0 {
            serial::write_byte(b':');
        }
        write_hex_byte(*b);
    }
}

fn write_hex_byte(b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    serial::write_byte(HEX[(b >> 4) as usize]);
    serial::write_byte(HEX[(b & 0xf) as usize]);
}

fn write_ipv4(ip: Ipv4Address) {
    let o = ip.octets();
    for (i, b) in o.iter().enumerate() {
        if i > 0 {
            serial::write_byte(b'.');
        }
        write_u16_dec(*b as u16);
    }
}

fn write_u16_dec(mut n: u16) {
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
