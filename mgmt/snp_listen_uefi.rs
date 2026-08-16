//! PRE-EBS HTTP listen over SNP + smoltcp (ADR-012 residual).
//!
//! Used when firmware Tcp4/Ip4/Dhcp4 are absent but SNP NICs are up after
//! ConnectController. Size-audited via `tools/check-size.sh` (ADR-003).

#![cfg(feature = "uefi-bin")]

use crate::boot::serial;
use crate::mgmt::http::handle_http_request;
use crate::mgmt::http_listen::{
    MgmtListenError, PRE_EBS_MAX_EXCHANGES, SNP_POST_BIND_LISTEN_MS, M7_UEFI_HTTP_OK_MARKER,
};
use crate::mgmt::snp_uefi::{open_first_snp, SnpDevice};
use smoltcp::iface::{Config, Interface, SocketSet};
use smoltcp::socket::{dhcpv4, tcp};
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpCidr, Ipv4Address, Ipv4Cidr};

const DHCP_BUDGET_MS: u64 = 8_000;

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
    serial::write_str("boot: mgmt HTTP listening on ");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line(" (PRE-EBS SNP window)");
    serial::write_str("boot: CURL NOW → http://");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line("/");
    serial::write_line("boot: PING NOW (same LAN as HOST NIC, not iDRAC) before curl");
    serial::write_str("boot: SNP listen window_ms=");
    write_u32_dec(SNP_POST_BIND_LISTEN_MS as u32);
    serial::write_byte(b'\n');
    serial::write_line(
        "boot: HINT — Mac must be on lease subnet; curl PRE-EBS window, then again after BOOT-OK (post-EBS SNP)",
    );
    crate::mgmt::pre_ebs_mgmt::reset_pre_ebs_mgmt();

    let tcp_rx = tcp::SocketBuffer::new(alloc::vec![0u8; 8192]);
    let tcp_tx = tcp::SocketBuffer::new(alloc::vec![0u8; 16384]);
    let tcp_handle = sockets.add(tcp::Socket::new(tcp_rx, tcp_tx));

    {
        let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
        sock.listen(port).map_err(|_| MgmtListenError::BindFailed)?;
    }

    let mut served: u32 = 0;
    let mut announced_accept = false;
    let mut rx_acc = [0u8; 8192];
    let mut rx_len: usize = 0;
    // Fresh post-bind budget — do not share remaining DHCP wall-clock.
    let listen_start = millis;
    let listen_deadline = listen_start + SNP_POST_BIND_LISTEN_MS as i64;
    let mut last_remind = listen_start;

    while served < PRE_EBS_MAX_EXCHANGES && millis < listen_deadline {
        let ts = Instant::from_millis(millis);
        iface.poll(ts, &mut device, &mut sockets);

        let mut do_close = false;
        let mut did_exchange = false;

        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                rx_len = 0;
                announced_accept = false;
                let _ = sock.listen(port);
            } else if !sock.is_active() && !sock.is_listening() {
                let _ = sock.listen(port);
            }

            if sock.is_active() && !announced_accept {
                serial::write_line("boot: SNP TCP accept — client connected");
                announced_accept = true;
                rx_len = 0;
            }

            if sock.can_recv() {
                let mut chunk = [0u8; 2048];
                if let Ok(n) = sock.recv_slice(&mut chunk) {
                    if n > 0 {
                        let copy = n.min(rx_acc.len().saturating_sub(rx_len));
                        rx_acc[rx_len..rx_len + copy].copy_from_slice(&chunk[..copy]);
                        rx_len += copy;
                    }
                }
            }

            let headers_done = rx_len >= 4
                && rx_acc[..rx_len].windows(4).any(|w| w == b"\r\n\r\n");

            if headers_done && sock.can_send() {
                let raw = core::str::from_utf8(&rx_acc[..rx_len]).unwrap_or("");
                let mut out = [0u8; 16384];
                let wn = unsafe {
                    crate::mgmt::pre_ebs_mgmt::with_pre_ebs_mgmt(|m| {
                        handle_http_request(
                            &mut m.vms,
                            &mut m.images,
                            &mut m.iso_plan,
                            &mut m.iso_install,
                            raw,
                            &mut out,
                        )
                    })
                }
                .unwrap_or(0);
                if wn > 0 {
                    let _ = sock.send_slice(&out[..wn]);
                    did_exchange = true;
                    do_close = true;
                }
            }
        }

        if did_exchange {
            served = served.saturating_add(1);
            serial::write_line("boot: SNP HTTP exchange ok");
            // Flush TX before close.
            for _ in 0..50 {
                millis += 1;
                let ts = Instant::from_millis(millis);
                iface.poll(ts, &mut device, &mut sockets);
                let _ = uefi::boot::stall(1_000);
            }
        }

        if do_close {
            sockets.get_mut::<tcp::Socket>(tcp_handle).close();
            rx_len = 0;
            announced_accept = false;
        }

        if millis - last_remind >= 5_000 {
            last_remind = millis;
            let left = (listen_deadline - millis).max(0) as u32;
            serial::write_str("boot: waiting curl http://");
            write_ipv4(ip);
            serial::write_byte(b':');
            write_u16_dec(port);
            serial::write_str("/  remaining_ms=");
            write_u32_dec(left);
            serial::write_byte(b'\n');
        }

        // Second poll flushes TX from socket handling.
        let ts2 = Instant::from_millis(millis);
        iface.poll(ts2, &mut device, &mut sockets);

        let _ = uefi::boot::stall(1_000);
        millis += 1;
    }

    // Keep the TCP socket listening so a parked session can accept after EBS.
    {
        let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
        if !sock.is_open() || (!sock.is_active() && !sock.is_listening()) {
            let _ = sock.listen(port);
        }
    }

    park_snp_http(ParkedSnpHttp {
        device,
        iface,
        sockets,
        tcp_handle,
        ip,
        port,
        millis,
    });

    if served == 0 {
        return Err(MgmtListenError::AcceptFailed);
    }

    serial::write_line(M7_UEFI_HTTP_OK_MARKER);
    Ok(())
}

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

fn park_snp_http(session: ParkedSnpHttp) {
    let leaked: &'static mut ParkedSnpHttp = alloc::boxed::Box::leak(alloc::boxed::Box::new(session));
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
fn tsc_delay_ms(ms: u32) {
    let ticks = (ms as u64).saturating_mul(2_100_000);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
}

/// After EBS: **do not** call SNP. Iron 2026-08-16 hung here — first
/// `iface.poll` after ExitBootServices never returned (`smoke frame` last line).
/// Guest path must continue. Parked lease is printed only; listen waits until
/// VMXOFF (`uefi_snp_post_ebs_idle`).
pub fn uefi_snp_post_ebs_probe() {
    let Some(s) = parked_http() else {
        serial::write_line("boot: WARN — no parked SNP (PRE-EBS fallback only; post-EBS HTTP skipped)");
        return;
    };
    serial::write_str("boot: post-EBS SNP parked lease=");
    write_ipv4(s.ip);
    serial::write_byte(b':');
    write_u16_dec(s.port);
    serial::write_line(" (not polling SNP yet — iron hung on immediate post-EBS poll)");
}

/// Durable SNP+smoltcp HTTP after VMXOFF (iron idle). Does not allocate.
///
/// Prints [`M7_POST_EBS_HTTP_OK_MARKER`] after the first post-EBS exchange.
/// Soft-returns if SNP was not parked.
pub fn uefi_snp_post_ebs_idle() {
    use crate::mgmt::http_listen::M7_POST_EBS_HTTP_OK_MARKER;

    let Some(s) = parked_http() else {
        return;
    };

    serial::write_str("boot: mgmt HTTP listening on ");
    write_ipv4(s.ip);
    serial::write_byte(b':');
    write_u16_dec(s.port);
    serial::write_line(" (POST-EBS SNP idle)");
    serial::write_str("boot: CURL NOW (post-EBS) → http://");
    write_ipv4(s.ip);
    serial::write_byte(b':');
    write_u16_dec(s.port);
    serial::write_line("/");

    {
        let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
        if !sock.is_open() || (!sock.is_active() && !sock.is_listening()) {
            let _ = sock.listen(s.port);
        }
    }

    let mut served: u32 = 0;
    let mut announced_accept = false;
    let mut rx_acc = [0u8; 8192];
    let mut rx_len: usize = 0;
    let mut last_remind = s.millis;
    let mut printed_ok = false;

    loop {
        s.millis = s.millis.saturating_add(1);
        let ts = Instant::from_millis(s.millis);
        let _ = s.iface.poll(ts, &mut s.device, &mut s.sockets);

        let mut do_close = false;
        let mut did_exchange = false;

        {
            let sock = s.sockets.get_mut::<tcp::Socket>(s.tcp_handle);
            if !sock.is_open() {
                rx_len = 0;
                announced_accept = false;
                let _ = sock.listen(s.port);
            } else if !sock.is_active() && !sock.is_listening() {
                let _ = sock.listen(s.port);
            }

            if sock.is_active() && !announced_accept {
                serial::write_line("boot: SNP TCP accept — client connected (post-EBS)");
                announced_accept = true;
                rx_len = 0;
            }

            if sock.can_recv() {
                let mut chunk = [0u8; 2048];
                if let Ok(n) = sock.recv_slice(&mut chunk) {
                    if n > 0 {
                        let copy = n.min(rx_acc.len().saturating_sub(rx_len));
                        rx_acc[rx_len..rx_len + copy].copy_from_slice(&chunk[..copy]);
                        rx_len += copy;
                    }
                }
            }

            let headers_done = rx_len >= 4 && rx_acc[..rx_len].windows(4).any(|w| w == b"\r\n\r\n");

            if headers_done && sock.can_send() {
                let raw = core::str::from_utf8(&rx_acc[..rx_len]).unwrap_or("");
                let mut out = [0u8; 16384];
                let wn = unsafe {
                    crate::mgmt::pre_ebs_mgmt::with_pre_ebs_mgmt(|m| {
                        handle_http_request(
                            &mut m.vms,
                            &mut m.images,
                            &mut m.iso_plan,
                            &mut m.iso_install,
                            raw,
                            &mut out,
                        )
                    })
                }
                .unwrap_or(0);
                if wn > 0 {
                    let _ = sock.send_slice(&out[..wn]);
                    did_exchange = true;
                    do_close = true;
                }
            }
        }

        if did_exchange {
            served = served.saturating_add(1);
            serial::write_line("boot: SNP HTTP exchange ok (post-EBS)");
            if !printed_ok {
                serial::write_line(M7_POST_EBS_HTTP_OK_MARKER);
                printed_ok = true;
            }
            for _ in 0..50 {
                s.millis = s.millis.saturating_add(1);
                let ts = Instant::from_millis(s.millis);
                let _ = s.iface.poll(ts, &mut s.device, &mut s.sockets);
                tsc_delay_ms(1);
            }
        }

        if do_close {
            s.sockets.get_mut::<tcp::Socket>(s.tcp_handle).close();
            rx_len = 0;
            announced_accept = false;
        }

        if s.millis.wrapping_sub(last_remind) >= 15_000 {
            last_remind = s.millis;
            serial::write_str("boot: post-EBS waiting curl http://");
            write_ipv4(s.ip);
            serial::write_byte(b':');
            write_u16_dec(s.port);
            serial::write_str("/  served=");
            write_u32_dec(served);
            serial::write_byte(b'\n');
        }

        let ts2 = Instant::from_millis(s.millis);
        let _ = s.iface.poll(ts2, &mut s.device, &mut s.sockets);
        tsc_delay_ms(1);
    }
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

fn write_u32_dec(mut n: u32) {
    let mut buf = [0u8; 10];
    let mut i = 10;
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
