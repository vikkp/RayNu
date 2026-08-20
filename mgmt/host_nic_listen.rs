//! Post-EBS / post-BOOT-OK HTTP listen on the host-owned NIC (ADR-013).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Firmware SNP is never polled. QEMU e1000 uses static user-net addressing.
//! Iron BCM5720 (`14e4:165f`) reuses the PRE-EBS SNP lease and binds the
//! function with live `BMSR_LSTATUS` first (Dedicated iDRAC + host LOM);
//! otherwise MAC match to that lease; otherwise try func 0 (LOM1) then func 1.
//! Hardware bring-up runs **immediately after EBS**
//! so UNDI analog is not left idle through the guest path; HTTP idle is after
//! `BOOT-OK`. TCP/HTTP scratch comes from [`crate::mgmt::mgmt_arena::MgmtArena`] (Phase E).
//! Socket metadata is stack-local; the arena is `.bss` (no Boot Services heap).
//!
//! Iron HTTP-OK is printed from `pci_census`, never here.

#![cfg(feature = "uefi-bin")]

use crate::boot::serial;
use crate::mgmt::bcm5720::Bcm5720Device;
use crate::mgmt::e1000::E1000Device;
use crate::mgmt::host_nic::{
    host_nic_lab_armed, HOST_NIC_LISTEN_MS, HOST_NIC_MAX_EXCHANGES, M7_HOST_NIC_QEMU_MARKER,
    QEMU_USERNET_GW, QEMU_USERNET_IPV4, QEMU_USERNET_PREFIX,
};
use crate::mgmt::host_nic_poll::{bounded_poll, HOST_NIC_POLL_BUDGET};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::http::MGMT_HTTP_DEFAULT_PORT;
use crate::mgmt::mgmt_arena::{MgmtArena, MgmtFatal};
use crate::mgmt::mgmt_lease;
use crate::mgmt::pci_census;
use smoltcp::iface::{Config, Interface, SocketSet, SocketStorage};
use smoltcp::phy::Device;
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

/// JUSTIFICATION: dedicated mgmt heap, distinct from the Proven Core allocator
/// (ADR-013 Phase E). `.bss` so it survives ExitBootServices.
static mut MGMT_ARENA: MgmtArena = MgmtArena::new();

const TCP_RX_N: usize = 8192;
const TCP_TX_N: usize = 16384;
const RX_ACC_N: usize = 8192;
const HTTP_OUT_N: usize = 16384;
const SCRATCH_N: usize = TCP_RX_N + TCP_TX_N + RX_ACC_N + HTTP_OUT_N;

#[derive(Clone, Copy)]
enum ListenWhen {
    /// After ExitBootServices, before VMX. Lab may `qemu_exit` after GET /.
    AfterEbs,
    /// After `RAYNU-V-R640-BOOT-OK`. QEMU never reaches this (`qemu_exit` first).
    AfterBootOk,
}

/// After ExitBootServices: if QEMU e1000 is present, serve GET / then continue.
/// Iron BCM5720: arm analog/DMA **now** (before VMX/Linux) and listen after BOOT-OK.
/// Do not take the APE PHY (`ape-nophylock=yes`).
pub fn run_post_ebs_host_nic_listen() {
    run_listen(ListenWhen::AfterEbs);
}

/// After VMXOFF / BOOT-OK: native idle listen. Never prints the SNP-era
/// post-EBS curl prompt (firmware SNP is dead).
///
/// QEMU `8086:100e` may print [`M7_HOST_NIC_QEMU_MARKER`] again.
/// Iron HTTP-OK (see `pci_census::print_host_nic_exchange_ok_marker`) is only
/// emitted when the census NIC is BCM5720 **and** an exchange happened.
pub fn run_post_boot_ok_native_idle() {
    run_listen(ListenWhen::AfterBootOk);
}

fn run_listen(when: ListenWhen) {
    if crate::mgmt::e1000_mmio::qemu_e1000_present() {
        listen_with_retries(when, NicKind::E1000);
        return;
    }
    if crate::mgmt::bcm5720_mmio::bcm5720_present() {
        match when {
            ListenWhen::AfterEbs => bringup_bcm5720_post_ebs(),
            ListenWhen::AfterBootOk => listen_with_retries(when, NicKind::Bcm5720),
        }
        return;
    }
    if matches!(when, ListenWhen::AfterBootOk) {
        if let Some((v, d)) = pci_census::census_pick() {
            serial::write_str("boot: HOST-NIC idle: no native Device for census vid:did=");
            write_hex_u16(v);
            serial::write_byte(b':');
            write_hex_u16(d);
            serial::write_line(" (Phase D waits on this id; do not guess LOM)");
        }
    }
}

/// Steal BCM5720 analog immediately after EBS. Do **not** HTTP-listen here
/// (guest path first). Iron 2026-08-19 complete COM2 (`1404f055`): both
/// funcs `cand bmsr=7949` then `CORECLK_RESET` without BMCR still
/// `link=timeout`. AfterBootOk listen is skipped without `LSTATUS`.
fn bringup_bcm5720_post_ebs() {
    let Some(lease) = mgmt_lease::load().filter(mgmt_lease::lease_is_usable) else {
        serial::write_line(
            "boot: WARN — HOST-NIC BCM5720 post-EBS bring-up skipped (no parked SNP lease)",
        );
        return;
    };
    serial::write_line("boot: HOST-NIC BCM5720 post-EBS bring-up (keep analog before guest path)");
    match Bcm5720Device::init(lease.mac) {
        Ok(dev) => {
            serial::write_str("boot: HOST-NIC BCM5720 post-EBS Device MAC=");
            write_mac(dev.mac());
            serial::write_line(" (listen after BOOT-OK)");
        }
        Err(_) => {
            serial::write_line(
                "boot: WARN — HOST-NIC BCM5720 post-EBS bring-up failed (BOOT-OK will retry)",
            );
        }
    }
}

#[derive(Clone, Copy)]
enum NicKind {
    E1000,
    Bcm5720,
}

fn listen_with_retries(when: ListenWhen, kind: NicKind) {
    // SAFETY: BSP-only; arena is not the NIC DMA region.
    // KANI-TARGET: host tests cover MgmtArena reset, not this listen loop.
    let arena = unsafe { &mut *core::ptr::addr_of_mut!(MGMT_ARENA) };
    for attempt in 0u32..3 {
        arena.reset();
        let result = match kind {
            NicKind::E1000 => listen_e1000(MGMT_HTTP_DEFAULT_PORT, when, arena),
            NicKind::Bcm5720 => listen_bcm5720(MGMT_HTTP_DEFAULT_PORT, when, arena),
        };
        match result {
            Ok(()) => return,
            Err(e) => {
                let kind_u8 = fatal_kind(e);
                crate::audit_log!(crate::audit::AuditEvent::MgmtRestarted {
                    generation: arena.generation(),
                    kind: kind_u8,
                });
                serial::write_str("boot: WARN — HOST-NIC MgmtFatal (");
                serial::write_str(fatal_name(e));
                serial::write_str(") attempt=");
                write_u16_dec(attempt as u16);
                serial::write_line("; arena reset, retry");
                arena.reset();
            }
        }
    }
    serial::write_line("boot: WARN — HOST-NIC listen gave up after MgmtFatal retries");
}

fn fatal_name(e: MgmtFatal) -> &'static str {
    match e {
        MgmtFatal::Device => "device",
        MgmtFatal::Bind => "tcp bind",
        MgmtFatal::ArenaExhausted => "arena exhausted",
        MgmtFatal::Induced => "induced",
    }
}

fn fatal_kind(e: MgmtFatal) -> u8 {
    match e {
        MgmtFatal::Device => 0,
        MgmtFatal::Bind => 1,
        MgmtFatal::ArenaExhausted => 2,
        MgmtFatal::Induced => 3,
    }
}

fn listen_bcm5720(port: u16, when: ListenWhen, arena: &mut MgmtArena) -> Result<(), MgmtFatal> {
    let Some(lease) = mgmt_lease::load().filter(mgmt_lease::lease_is_usable) else {
        serial::write_line(
            "boot: WARN — HOST-NIC BCM5720: no parked SNP lease (cannot bind; skip MMIO)",
        );
        return Err(MgmtFatal::Bind);
    };
    let mut device = Bcm5720Device::init(lease.mac).map_err(|_| MgmtFatal::Device)?;
    let mac = device.mac();
    if crate::mgmt::bcm5720_mmio::skip_http_listen_without_lstatus()
        && !crate::mgmt::bcm5720_mmio::bcm5720_phy_link_up()
    {
        serial::write_line("boot: WARN — HOST-NIC BCM5720 skip listen (no LSTATUS; do not curl)");
        return Ok(());
    }
    let ip = Ipv4Address::new(lease.ip[0], lease.ip[1], lease.ip[2], lease.ip[3]);
    let gw = if lease.has_router {
        Some(Ipv4Address::new(
            lease.router[0],
            lease.router[1],
            lease.router[2],
            lease.router[3],
        ))
    } else {
        None
    };
    serial::write_str("boot: HOST-NIC BCM5720 Device MAC=");
    write_mac(mac);
    serial::write_byte(b'\n');
    listen_loop(
        &mut device,
        mac,
        ip,
        gw,
        lease.prefix,
        port,
        when,
        arena,
        "BCM5720",
    )
}

fn listen_e1000(port: u16, when: ListenWhen, arena: &mut MgmtArena) -> Result<(), MgmtFatal> {
    let mut device = E1000Device::init().map_err(|_| MgmtFatal::Device)?;
    let mac = device.mac();
    serial::write_str("boot: HOST-NIC e1000 MAC=");
    write_mac(mac);
    serial::write_byte(b'\n');
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
    listen_loop(
        &mut device,
        mac,
        ip,
        Some(gw),
        QEMU_USERNET_PREFIX,
        port,
        when,
        arena,
        "e1000",
    )
}

fn listen_loop<D: Device>(
    device: &mut D,
    mac: [u8; 6],
    ip: Ipv4Address,
    gw: Option<Ipv4Address>,
    prefix: u8,
    port: u16,
    when: ListenWhen,
    arena: &mut MgmtArena,
    nic_tag: &str,
) -> Result<(), MgmtFatal> {
    let scratch = arena
        .alloc_bytes(SCRATCH_N, 16)
        .map_err(|_| MgmtFatal::ArenaExhausted)?;
    let (tcp_rx_mem, rest) = scratch.split_at_mut(TCP_RX_N);
    let (tcp_tx_mem, rest) = rest.split_at_mut(TCP_TX_N);
    let (rx_acc, out) = rest.split_at_mut(RX_ACC_N);

    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = mac_seed(mac);

    let mut millis: i64 = 0;
    let mut iface = Interface::new(config, device, Instant::from_millis(millis));
    iface.update_ip_addrs(|addrs| {
        addrs.clear();
        let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(ip), prefix));
    });
    if let Some(gw) = gw {
        let _ = iface.routes_mut().add_default_ipv4_route(gw);
    }

    match when {
        ListenWhen::AfterEbs => {
            serial::write_str("boot: HOST-NIC listening on ");
            write_ipv4(ip);
            serial::write_byte(b':');
            write_u16_dec(port);
            serial::write_str(" (post-EBS ");
            serial::write_str(nic_tag);
            serial::write_line(")");
        }
        ListenWhen::AfterBootOk => {
            serial::write_str("boot: HOST-NIC idle listening on ");
            write_ipv4(ip);
            serial::write_byte(b':');
            write_u16_dec(port);
            serial::write_str(" (after BOOT-OK ");
            serial::write_str(nic_tag);
            serial::write_line(")");
            if nic_tag == "BCM5720" {
                serial::write_str("boot: CURL NOW → http://");
                write_ipv4(ip);
                serial::write_byte(b':');
                write_u16_dec(port);
                serial::write_line("/  (native BCM5720; SNP is dead)");
                print_bcm5720_poll_diag();
            }
        }
    }

    crate::mgmt::pre_ebs_mgmt::reset_pre_ebs_mgmt();

    let mut sockets_storage = [SocketStorage::EMPTY; 1];
    let mut sockets = SocketSet::new(&mut sockets_storage[..]);
    let tcp_rx = tcp::SocketBuffer::new(tcp_rx_mem);
    let tcp_tx = tcp::SocketBuffer::new(tcp_tx_mem);
    let tcp_handle = sockets.add(tcp::Socket::new(tcp_rx, tcp_tx));
    sockets
        .get_mut::<tcp::Socket>(tcp_handle)
        .listen(port)
        .map_err(|_| MgmtFatal::Bind)?;

    let mut served: u32 = 0;
    let mut announced = false;
    let mut rx_len: usize = 0;
    let mut last_diag: i64 = 0;
    let deadline = match when {
        ListenWhen::AfterEbs => HOST_NIC_LISTEN_MS as i64,
        ListenWhen::AfterBootOk => i64::MAX / 4,
    };
    let max_ex = match when {
        ListenWhen::AfterEbs => HOST_NIC_MAX_EXCHANGES,
        ListenWhen::AfterBootOk => u32::MAX,
    };

    while served < max_ex && millis < deadline {
        let ts = Instant::from_millis(millis);
        let _ = bounded_poll(HOST_NIC_POLL_BUDGET, || {
            matches!(
                iface.poll(ts, device, &mut sockets),
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
            let headers_done = rx_len >= 4 && rx_acc[..rx_len].windows(4).any(|w| w == b"\r\n\r\n");
            if headers_done && sock.can_send() {
                let raw = core::str::from_utf8(&rx_acc[..rx_len]).unwrap_or("");
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
                            out,
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
                iface.poll(ts, device, &mut sockets);
                tsc_spin_ms(1);
            }
            match when {
                ListenWhen::AfterEbs => {
                    serial::write_line(M7_HOST_NIC_QEMU_MARKER);
                    if host_nic_lab_armed() {
                        serial::qemu_exit_success();
                    }
                }
                ListenWhen::AfterBootOk => {
                    pci_census::print_host_nic_exchange_ok_marker();
                }
            }
        }
        if do_close {
            sockets.get_mut::<tcp::Socket>(tcp_handle).close();
            rx_len = 0;
            announced = false;
        }

        millis += 1;
        tsc_spin_ms(1);
        if nic_tag == "BCM5720" && millis - last_diag >= 5000 {
            last_diag = millis;
            print_bcm5720_poll_diag();
        }
    }

    if served == 0 {
        serial::write_line(
            "boot: WARN — HOST-NIC accept timeout (continuing; native listen did not serve)",
        );
    }
    Ok(())
}

fn print_bcm5720_poll_diag() {
    let Some(d) = crate::mgmt::bcm5720_mmio::bcm5720_poll_diag() else {
        return;
    };
    serial::write_str("boot: HOST-NIC BCM5720 poll rx_prod=");
    write_u16_dec(d.rx_prod);
    serial::write_str(" rx_cons=");
    write_u16_dec(d.rx_cons);
    serial::write_str(" tx_cons=");
    write_u16_dec(d.tx_cons);
    serial::write_str(" rx_ok=");
    write_u32_dec(d.rx_ok);
    serial::write_str(" rx_drop=");
    write_u32_dec(d.rx_drop);
    serial::write_byte(b'\n');
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

fn write_hex_u16(n: u16) {
    write_hex_byte((n >> 8) as u8);
    write_hex_byte(n as u8);
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
