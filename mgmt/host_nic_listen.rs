//! Post-EBS HTTP listen on the host-owned e1000 (ADR-013 Phase C).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Firmware SNP is never polled. Static QEMU user-net addressing (no DHCP).
//! Socket buffers are `.bss` — UEFI Boot Services alloc is invalid after EBS.

#![cfg(feature = "uefi-bin")]

use crate::boot::serial;
use crate::mgmt::e1000::E1000Device;
use crate::mgmt::host_nic::{
    host_nic_lab_armed, HOST_NIC_LISTEN_MS, HOST_NIC_MAX_EXCHANGES, M7_HOST_NIC_QEMU_MARKER,
    QEMU_USERNET_GW, QEMU_USERNET_IPV4, QEMU_USERNET_PREFIX,
};
use crate::mgmt::host_nic_poll::{bounded_poll, HOST_NIC_POLL_BUDGET};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::http::MGMT_HTTP_DEFAULT_PORT;
use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

/// JUSTIFICATION: post-EBS has no Boot Services heap; one listen session.
static mut SOCKET_STORE: [SocketStorage<'static>; 1] = [SocketStorage::EMPTY; 1];
static mut TCP_RX: [u8; 8192] = [0; 8192];
static mut TCP_TX: [u8; 16384] = [0; 16384];

/// After ExitBootServices: if QEMU e1000 is present, serve GET / then continue.
pub fn run_post_ebs_host_nic_listen() {
    if !crate::mgmt::e1000_mmio::qemu_e1000_present() {
        return;
    }
    match listen_e1000(MGMT_HTTP_DEFAULT_PORT) {
        Ok(()) => {}
        Err(e) => {
            serial::write_str("boot: WARN — HOST-NIC listen failed (");
            serial::write_str(err_name(e));
            serial::write_line("); guest path continues");
        }
    }
}

#[derive(Clone, Copy)]
enum HostNicErr {
    Init,
    Bind,
}

fn err_name(e: HostNicErr) -> &'static str {
    match e {
        HostNicErr::Init => "e1000 init",
        HostNicErr::Bind => "tcp bind",
    }
}

fn listen_e1000(port: u16) -> Result<(), HostNicErr> {
    let mut device = E1000Device::init().map_err(|_| HostNicErr::Init)?;
    let mac = device.mac();
    serial::write_str("boot: HOST-NIC e1000 MAC=");
    write_mac(mac);
    serial::write_byte(b'\n');

    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = mac_seed(mac);

    let mut millis: i64 = 0;
    let mut iface = Interface::new(config, &mut device, Instant::from_millis(millis));
    let ip = Ipv4Address::new(
        QEMU_USERNET_IPV4[0],
        QEMU_USERNET_IPV4[1],
        QEMU_USERNET_IPV4[2],
        QEMU_USERNET_IPV4[3],
    );
    let gw = Ipv4Address::new(
        QEMU_USERNET_GW[0],
        QEMU_USERNET_GW[1],
        QEMU_USERNET_GW[2],
        QEMU_USERNET_GW[3],
    );
    iface.update_ip_addrs(|addrs| {
        addrs.clear();
        let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(ip), QEMU_USERNET_PREFIX));
    });
    let _ = iface.routes_mut().add_default_ipv4_route(gw);

    serial::write_str("boot: HOST-NIC listening on ");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line(" (post-EBS e1000)");

    crate::mgmt::pre_ebs_mgmt::reset_pre_ebs_mgmt();

    // SAFETY: BSP-only post-EBS listen; these statics are not the NIC DMA arena
    // (that `unsafe` lives in `e1000_mmio`). No Boot Services heap after EBS.
    // KANI-TARGET: SocketSet construction is not MMIO; host tests cover poll budget.
    let sockets_storage = unsafe { &mut *core::ptr::addr_of_mut!(SOCKET_STORE) };
    let mut sockets = SocketSet::new(&mut sockets_storage[..]);
    let tcp_rx = tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(TCP_RX) }.as_mut_slice());
    let tcp_tx = tcp::SocketBuffer::new(unsafe { &mut *core::ptr::addr_of_mut!(TCP_TX) }.as_mut_slice());
    let tcp_handle = sockets.add(tcp::Socket::new(tcp_rx, tcp_tx));
    sockets
        .get_mut::<tcp::Socket>(tcp_handle)
        .listen(port)
        .map_err(|_| HostNicErr::Bind)?;

    let mut served: u32 = 0;
    let mut announced = false;
    let mut rx_acc = [0u8; 8192];
    let mut rx_len: usize = 0;
    let deadline = HOST_NIC_LISTEN_MS as i64;

    while served < HOST_NIC_MAX_EXCHANGES && millis < deadline {
        let ts = Instant::from_millis(millis);
        let _ = bounded_poll(HOST_NIC_POLL_BUDGET, || {
            matches!(
                iface.poll(ts, &mut device, &mut sockets),
                smoltcp::iface::PollResult::SocketStateChanged
            )
        });

        let mut do_close = false;
        let mut did_exchange = false;
        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                rx_len = 0;
                announced = false;
                let _ = sock.listen(port);
            } else if !sock.is_active() && !sock.is_listening() {
                let _ = sock.listen(port);
            }
            if sock.is_active() && !announced {
                serial::write_line("boot: HOST-NIC TCP accept — client connected");
                announced = true;
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
            let headers_done =
                rx_len >= 4 && rx_acc[..rx_len].windows(4).any(|w| w == b"\r\n\r\n");
            if headers_done && sock.can_send() {
                let raw = core::str::from_utf8(&rx_acc[..rx_len]).unwrap_or("");
                let mut out = [0u8; 16384];
                let wn = unsafe {
                    // SAFETY: BSP-only HTTP codec; tables are the leaked PRE-EBS
                    // session (reset at listen start). Not NIC MMIO.
                    // KANI-TARGET: handle_http_request is covered by host HTTP tests.
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
            serial::write_line("boot: HOST-NIC HTTP exchange ok");
            for _ in 0..80 {
                millis += 1;
                let ts = Instant::from_millis(millis);
                iface.poll(ts, &mut device, &mut sockets);
                tsc_spin_ms(1);
            }
            serial::write_line(M7_HOST_NIC_QEMU_MARKER);
            if host_nic_lab_armed() {
                serial::qemu_exit_success();
            }
        }
        if do_close {
            sockets.get_mut::<tcp::Socket>(tcp_handle).close();
            rx_len = 0;
            announced = false;
        }

        millis += 1;
        tsc_spin_ms(1);
    }

    if served == 0 {
        serial::write_line(
            "boot: WARN — HOST-NIC accept timeout (continuing; PRE-EBS SNP was skipped)",
        );
    }
    Ok(())
}

fn mac_seed(mac: [u8; 6]) -> u64 {
    let mut s = 0u64;
    for (i, b) in mac.iter().enumerate() {
        s ^= (*b as u64) << ((i % 8) * 8);
    }
    s | 1
}

fn tsc_spin_ms(ms: u32) {
    let ticks = (ms as u64).saturating_mul(2_100_000);
    let start = crate::arch::cpu::rdtsc();
    while crate::arch::cpu::rdtsc().wrapping_sub(start) < ticks {
        core::hint::spin_loop();
    }
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
