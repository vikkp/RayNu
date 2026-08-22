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
//! so UNDI analog is not left idle through the guest path. Phase F coexist
//! listens **while VMX is on** (`bounded_poll` on a scheduler quantum).
//! Phase D fallback (post-`VMXOFF` idle) remains if coexist cannot arm.
//! TCP/HTTP scratch comes from [`crate::mgmt::mgmt_arena::MgmtArena`] (Phase E)
//! or coexist `.bss` buffers (still not the Proven Core allocator).
//!
//! Iron HTTP-OK is printed from `pci_census`, never here.

#![cfg(feature = "uefi-bin")]

use crate::boot::serial;
use crate::mgmt::bcm5720::Bcm5720Device;
use crate::mgmt::e1000::E1000Device;
use crate::mgmt::host_nic::{
    host_nic_lab_armed, http_accept_should_idle_abort, HOST_NIC_HTTP_IDLE_MS, HOST_NIC_LISTEN_MS,
    HOST_NIC_MAX_EXCHANGES, M7_HOST_NIC_QEMU_MARKER, QEMU_USERNET_GW, QEMU_USERNET_IPV4,
    QEMU_USERNET_PREFIX,
};
use crate::mgmt::host_nic_poll::{bounded_poll, HOST_NIC_POLL_BUDGET};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::http::MGMT_HTTP_DEFAULT_PORT;
use crate::mgmt::mgmt_arena::{MgmtArena, MgmtFatal};
use crate::mgmt::mgmt_lease;
use crate::mgmt::pci_census;
use core::mem::MaybeUninit;
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
use smoltcp::phy::Device;
use smoltcp::socket::tcp;
use smoltcp::time::Instant;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

/// JUSTIFICATION: dedicated mgmt heap, distinct from the Proven Core allocator
/// (ADR-013 Phase E). `.bss` so it survives ExitBootServices.
static mut MGMT_ARENA: MgmtArena = MgmtArena::new();

/// Phase F coexist session (BSP-only; not the Proven Core allocator).
static mut COEXIST_ARMED: bool = false;
static mut COEXIST_DEVICE: MaybeUninit<Bcm5720Device> = MaybeUninit::uninit();
static mut COEXIST_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
static mut COEXIST_SOCK_STORAGE: [SocketStorage<'static>; 1] = [SocketStorage::EMPTY; 1];
static mut COEXIST_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();
static mut COEXIST_TCP_HANDLE: MaybeUninit<SocketHandle> = MaybeUninit::uninit();
static mut COEXIST_TCP_RX: [u8; TCP_RX_N] = [0; TCP_RX_N];
static mut COEXIST_TCP_TX: [u8; TCP_TX_N] = [0; TCP_TX_N];
static mut COEXIST_RX_ACC: [u8; RX_ACC_N] = [0; RX_ACC_N];
static mut COEXIST_HTTP_OUT: [u8; HTTP_OUT_N] = [0; HTTP_OUT_N];
static mut COEXIST_MILLIS: i64 = 0;
static mut COEXIST_ANNOUNCED: bool = false;
static mut COEXIST_ACCEPT_AT_MS: i64 = 0;
static mut COEXIST_RX_LEN: usize = 0;
static mut COEXIST_LAST_DIAG: i64 = 0;
static mut COEXIST_LAST_RX_DROP: u32 = 0;
static mut COEXIST_PORT: u16 = MGMT_HTTP_DEFAULT_PORT;

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
///
/// Phase F prefers [`arm_bcm5720_coexist`] + scheduler ticks instead of this
/// blocking loop. This remains the fallback if coexist cannot arm.
pub fn run_post_boot_ok_native_idle() {
    run_listen(ListenWhen::AfterBootOk);
}

/// Arm native HTTP beside VMX (ADR-013 Phase F). Returns false on skip/fail.
///
/// INVARIANTS:
/// - Does not call `VMXOFF`
/// - Does not spin; caller resumes guests
/// - Scratch is `.bss`, not `FrameAllocator`
pub fn arm_bcm5720_coexist() -> bool {
    if unsafe { COEXIST_ARMED } {
        return true;
    }
    if crate::mgmt::e1000_mmio::qemu_e1000_present() {
        return false;
    }
    if !crate::mgmt::bcm5720_mmio::bcm5720_present() {
        return false;
    }
    let Some(lease) = mgmt_lease::load().filter(mgmt_lease::lease_is_usable) else {
        serial::write_line(
            "boot: WARN — HOST-NIC coexist skip (no parked SNP lease)",
        );
        return false;
    };
    let mut device = match Bcm5720Device::init(lease.mac) {
        Ok(d) => d,
        Err(_) => {
            serial::write_line("boot: WARN — HOST-NIC coexist skip (device init)");
            return false;
        }
    };
    if crate::mgmt::bcm5720_mmio::skip_http_listen_without_lstatus()
        && !crate::mgmt::bcm5720_mmio::bcm5720_phy_link_up()
    {
        serial::write_line(
            "boot: WARN — HOST-NIC BCM5720 skip listen (no LSTATUS; do not curl)",
        );
        return false;
    }
    let mac = device.mac();
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
    let port = MGMT_HTTP_DEFAULT_PORT;
    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = mac_seed(mac);
    let mut iface = Interface::new(config, &mut device, Instant::from_millis(0));
    iface.update_ip_addrs(|addrs| {
        addrs.clear();
        let _ = addrs.push(IpCidr::new(IpAddress::Ipv4(ip), lease.prefix));
    });
    if let Some(gw) = gw {
        let _ = iface.routes_mut().add_default_ipv4_route(gw);
    }

    // SAFETY: BSP-only coexist session; buffers are .bss.
    // KANI-TARGET: host gate checks wiring; this arm is firmware-only.
    unsafe {
        let tcp_rx = tcp::SocketBuffer::new(
            (&mut *core::ptr::addr_of_mut!(COEXIST_TCP_RX)).as_mut_slice(),
        );
        let tcp_tx = tcp::SocketBuffer::new(
            (&mut *core::ptr::addr_of_mut!(COEXIST_TCP_TX)).as_mut_slice(),
        );
        let sockets = SocketSet::new(
            (&mut *core::ptr::addr_of_mut!(COEXIST_SOCK_STORAGE)).as_mut_slice(),
        );
        let mut sockets = sockets;
        let tcp_handle = sockets.add(tcp::Socket::new(tcp_rx, tcp_tx));
        if sockets
            .get_mut::<tcp::Socket>(tcp_handle)
            .listen(port)
            .is_err()
        {
            serial::write_line("boot: WARN — HOST-NIC coexist skip (tcp bind)");
            return false;
        }
        COEXIST_DEVICE.write(device);
        COEXIST_IFACE.write(iface);
        COEXIST_SOCKETS.write(sockets);
        COEXIST_TCP_HANDLE.write(tcp_handle);
        COEXIST_MILLIS = 0;
        COEXIST_ANNOUNCED = false;
        COEXIST_ACCEPT_AT_MS = 0;
        COEXIST_RX_LEN = 0;
        COEXIST_LAST_DIAG = 0;
        COEXIST_LAST_RX_DROP = 0;
        COEXIST_PORT = port;
        COEXIST_ARMED = true;
    }

    crate::mgmt::pre_ebs_mgmt::reset_pre_ebs_mgmt();
    serial::write_str("boot: HOST-NIC coexist listening on ");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line(" (VMX on; ADR-013 Phase F)");
    serial::write_str("boot: CURL NOW → http://");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line("/  (native BCM5720; G0 still scheduled; SNP is dead)");
    serial::write_line("boot: HINT — COM2 idle after this snapshot (TCP accept / HTTP only)");
    print_bcm5720_poll_diag();
    true
}

/// One scheduler-quantum NIC/HTTP step. No-op if not armed. Does not spin.
pub fn tick_bcm5720_coexist() {
    if !unsafe { COEXIST_ARMED } {
        return;
    }
    // SAFETY: BSP-only; armed once before ticks; guests are VMEXITed here.
    // HTTP codec uses the leaked PRE-EBS tables (reset at arm). Not NIC MMIO.
    // KANI-TARGET: host tests cover bounded_poll, not this firmware session.
    unsafe {
        let device = COEXIST_DEVICE.assume_init_mut();
        let iface = COEXIST_IFACE.assume_init_mut();
        let sockets = COEXIST_SOCKETS.assume_init_mut();
        let tcp_handle = *COEXIST_TCP_HANDLE.assume_init_ref();
        let rx_acc = &mut *core::ptr::addr_of_mut!(COEXIST_RX_ACC);
        let out = &mut *core::ptr::addr_of_mut!(COEXIST_HTTP_OUT);
        COEXIST_MILLIS = COEXIST_MILLIS.saturating_add(10);
        let millis = COEXIST_MILLIS;
        let ts = Instant::from_millis(millis);
        let _ = bounded_poll(HOST_NIC_POLL_BUDGET, || {
            matches!(
                iface.poll(ts, device, sockets),
                smoltcp::iface::PollResult::SocketStateChanged
            )
        });

        let mut do_close = false;
        let mut did_exchange = false;
        let mut did_idle_abort = false;
        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                COEXIST_RX_LEN = 0;
                COEXIST_ANNOUNCED = false;
                COEXIST_ACCEPT_AT_MS = 0;
                let _ = sock.listen(COEXIST_PORT);
            } else if !sock.is_active() && !sock.is_listening() {
                let _ = sock.listen(COEXIST_PORT);
            }
            if sock.is_active() && !COEXIST_ANNOUNCED {
                serial::write_line("boot: HOST-NIC TCP accept — client connected");
                COEXIST_ANNOUNCED = true;
                COEXIST_ACCEPT_AT_MS = millis;
                COEXIST_RX_LEN = 0;
            }
            if sock.can_recv() {
                let mut chunk = [0u8; 2048];
                if let Ok(n) = sock.recv_slice(&mut chunk) {
                    if n > 0 {
                        let copy = n.min(rx_acc.len().saturating_sub(COEXIST_RX_LEN));
                        rx_acc[COEXIST_RX_LEN..COEXIST_RX_LEN + copy]
                            .copy_from_slice(&chunk[..copy]);
                        COEXIST_RX_LEN += copy;
                    }
                }
            }
            let rx_len = COEXIST_RX_LEN;
            let headers_done = rx_len >= 4 && rx_acc[..rx_len].windows(4).any(|w| w == b"\r\n\r\n");
            if headers_done && sock.can_send() {
                let raw = core::str::from_utf8(&rx_acc[..rx_len]).unwrap_or("");
                let wn = crate::mgmt::pre_ebs_mgmt::with_pre_ebs_mgmt(|m| {
                    handle_http_request(
                        &mut m.vms,
                        &mut m.images,
                        &mut m.iso_plan,
                        &mut m.iso_install,
                        raw,
                        out,
                    )
                })
                .unwrap_or(0);
                if wn > 0 {
                    let _ = sock.send_slice(&out[..wn]);
                    did_exchange = true;
                    do_close = true;
                }
            } else if http_accept_should_idle_abort(
                COEXIST_ANNOUNCED,
                headers_done,
                millis.saturating_sub(COEXIST_ACCEPT_AT_MS),
                HOST_NIC_HTTP_IDLE_MS,
            ) {
                sock.abort();
                COEXIST_RX_LEN = 0;
                COEXIST_ANNOUNCED = false;
                COEXIST_ACCEPT_AT_MS = 0;
                did_idle_abort = true;
            }
        }
        if did_exchange {
            serial::write_line("boot: HOST-NIC HTTP exchange ok");
            let _ = iface.poll(Instant::from_millis(millis + 1), device, sockets);
            pci_census::print_host_nic_exchange_ok_marker();
        }
        if do_close {
            // abort() not close(): FIN_WAIT held the only listen slot so the
            // next curl (spec→start, 31ms) got RST (`curl: (7)`). Iron 2026-08-21.
            sockets.get_mut::<tcp::Socket>(tcp_handle).abort();
            COEXIST_RX_LEN = 0;
            COEXIST_ANNOUNCED = false;
            COEXIST_ACCEPT_AT_MS = 0;
            let _ = iface.poll(Instant::from_millis(millis + 2), device, sockets);
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                let _ = sock.listen(COEXIST_PORT);
            }
            serial::write_line("boot: HOST-NIC TCP re-listen after HTTP");
        }
        if did_idle_abort {
            serial::write_line("boot: WARN — HOST-NIC TCP idle abort; re-listen");
            let _ = iface.poll(Instant::from_millis(millis + 1), device, sockets);
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                let _ = sock.listen(COEXIST_PORT);
            }
        }
        if millis - COEXIST_LAST_DIAG >= 5000 {
            COEXIST_LAST_DIAG = millis;
            if let Some(d) = crate::mgmt::bcm5720_mmio::bcm5720_poll_diag() {
                if d.rx_drop > COEXIST_LAST_RX_DROP {
                    serial::write_line("boot: WARN — HOST-NIC BCM5720 rx_drop rose");
                    print_bcm5720_poll_diag();
                }
                COEXIST_LAST_RX_DROP = d.rx_drop;
            }
        }
    }
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
                serial::write_line(
                    "boot: HINT — COM2 idle after this snapshot (TCP accept / HTTP only)",
                );
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
    let mut accept_at: i64 = 0;
    let mut rx_len: usize = 0;
    let mut last_diag: i64 = 0;
    let mut last_rx_drop: u32 = 0;
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
        let mut did_idle_abort = false;
        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                rx_len = 0;
                announced = false;
                accept_at = 0;
                let _ = sock.listen(port);
            } else if !sock.is_active() && !sock.is_listening() {
                let _ = sock.listen(port);
            }
            if sock.is_active() && !announced {
                serial::write_line("boot: HOST-NIC TCP accept — client connected");
                announced = true;
                accept_at = millis;
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
            } else if http_accept_should_idle_abort(
                announced,
                headers_done,
                millis.saturating_sub(accept_at),
                HOST_NIC_HTTP_IDLE_MS,
            ) {
                sock.abort();
                rx_len = 0;
                announced = false;
                accept_at = 0;
                did_idle_abort = true;
            }
        }

        if did_exchange {
            served = served.saturating_add(1);
            serial::write_line("boot: HOST-NIC HTTP exchange ok");
            // Drain TX before qemu_exit. SPA HTML + headers are ~15 KiB;
            // a fixed 80-poll window dropped the tail on TCG (curl GET / failed).
            flush_tcp_tx(&mut iface, device, &mut sockets, tcp_handle, &mut millis);
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
            accept_at = 0;
        }
        if did_idle_abort {
            serial::write_line("boot: WARN — HOST-NIC TCP idle abort; re-listen");
            millis += 1;
            iface.poll(Instant::from_millis(millis), device, &mut sockets);
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                let _ = sock.listen(port);
            }
        }

        millis += 1;
        tsc_spin_ms(1);
        // E3b closed: do not spam `poll rx_prod=` every 5s. Sample drops only.
        if nic_tag == "BCM5720" && millis - last_diag >= 5000 {
            last_diag = millis;
            if let Some(d) = crate::mgmt::bcm5720_mmio::bcm5720_poll_diag() {
                if d.rx_drop > last_rx_drop {
                    serial::write_line("boot: WARN — HOST-NIC BCM5720 rx_drop rose");
                    print_bcm5720_poll_diag();
                }
                last_rx_drop = d.rx_drop;
            }
        }
    }

    if served == 0 {
        serial::write_line(
            "boot: WARN — HOST-NIC accept timeout (continuing; native listen did not serve)",
        );
    }
    Ok(())
}

/// Poll until the TCP TX queue is empty (or a bounded cap). QEMU lab
/// `qemu_exit`s immediately after GET /; leaving octets queued makes curl fail.
fn flush_tcp_tx<D: Device>(
    iface: &mut Interface,
    device: &mut D,
    sockets: &mut SocketSet,
    tcp_handle: SocketHandle,
    millis: &mut i64,
) {
    for _ in 0..240 {
        *millis += 1;
        iface.poll(Instant::from_millis(*millis), device, sockets);
        tsc_spin_ms(1);
        if sockets.get::<tcp::Socket>(tcp_handle).send_queue() == 0 {
            for _ in 0..16 {
                *millis += 1;
                iface.poll(Instant::from_millis(*millis), device, sockets);
                tsc_spin_ms(1);
            }
            return;
        }
    }
}

fn print_bcm5720_poll_diag() {
    let Some(d) = crate::mgmt::bcm5720_mmio::bcm5720_poll_diag() else {
        return;
    };
    serial::write_str("boot: HOST-NIC BCM5720 poll rx_prod=");
    write_u16_dec(d.rx_prod);
    serial::write_str(" rx_cons=");
    write_u16_dec(d.rx_cons);
    serial::write_str(" tx_prod=");
    write_u16_dec(d.tx_prod);
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
