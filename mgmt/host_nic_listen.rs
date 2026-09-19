//! Post-EBS / post-BOOT-OK HTTP listen on the host-owned NIC (ADR-013).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Firmware SNP is never polled. QEMU e1000 uses static user-net addressing.
//! Iron BCM5720 (`14e4:165f`) reuses the PRE-EBS SNP lease when parked;
//! if SNP DHCP failed, native smoltcp DHCP binds the live LOM (`lease.mac`
//! is empty → pick by link). Function with live `BMSR_LSTATUS` first
//! (Dedicated iDRAC + host LOM); otherwise MAC match to that lease;
//! otherwise try func 0 (LOM1) then func 1.
//! Hardware bring-up runs **immediately after EBS**
//! so UNDI analog is not left idle through the guest path. Iron BCM5720 then
//! **arms Phase F coexist** as the standing SPA (HTTPS while RayNu-F / Alpine
//! run). The bounded `PRE_RAYNUF_HTTPS_MS` window is fallback if arm fails
//! (COM2 `1647a8d8`: `raynuf.txt` launched Alpine with no coexist listen).
//! Phase F coexist listens **while VMX is on** (`bounded_poll` on RayNu-F
//! vmexit, USB BOT waits, and the credit scheduler).
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
    coexist_millis_from_tsc, host_nic_lab_armed, http_accept_should_idle_abort, HOST_NIC_DHCP_MS,
    HOST_NIC_HTTP_IDLE_MS, HOST_NIC_LISTEN_MS, HOST_NIC_MAX_EXCHANGES, M7_HOST_NIC_QEMU_MARKER,
    PRE_RAYNUF_HTTPS_MS, QEMU_USERNET_GW, QEMU_USERNET_IPV4, QEMU_USERNET_PREFIX,
};
use crate::mgmt::host_nic_poll::{bounded_poll, HOST_NIC_POLL_BUDGET};
use crate::mgmt::http::handle_http_request;
use crate::mgmt::http::MGMT_HTTP_DEFAULT_PORT;
use crate::mgmt::mgmt_arena::{MgmtArena, MgmtFatal};
use crate::mgmt::mgmt_lease;
use crate::mgmt::pci_census;
use crate::mgmt::console::{maybe_print_iron_console_ok, take_spa_keys_injected};
use crate::mgmt::tls::maybe_print_iron_tls_ok;
use crate::mgmt::tls12::Tls12Listen;
use crate::mgmt::tls_coexist::{COEXIST_HTTP_OUT_N, COEXIST_RX_ACC_N};
use core::mem::MaybeUninit;
use smoltcp::iface::{Config, Interface, SocketHandle, SocketSet, SocketStorage};
use smoltcp::phy::Device;
use smoltcp::socket::{dhcpv4, tcp};
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
static mut COEXIST_LISTEN: Tls12Listen = Tls12Listen::empty();
static mut COEXIST_LISTEN_READY: bool = false;
static mut COEXIST_HTTP_OUT: [u8; HTTP_OUT_N] = [0; HTTP_OUT_N];
static mut COEXIST_WRAP_OUT: [u8; WRAP_OUT_N] = [0; WRAP_OUT_N];
static mut COEXIST_MILLIS: i64 = 0;
static mut COEXIST_TSC0: u64 = 0;
static mut COEXIST_ANNOUNCED: bool = false;
static mut COEXIST_ACCEPT_AT_MS: i64 = 0;
static mut COEXIST_LAST_DIAG: i64 = 0;
static mut COEXIST_LAST_RX_DROP: u32 = 0;
static mut COEXIST_PORT: u16 = MGMT_HTTP_DEFAULT_PORT;
/// Last TSC we polled the standing SPA (RayNu-F / USB waits).
static mut LAST_SPA_TICK_TSC: u64 = 0;

const TCP_RX_N: usize = 8192;
const TCP_TX_N: usize = COEXIST_HTTP_OUT_N;
const RX_ACC_N: usize = COEXIST_RX_ACC_N;
const HTTP_OUT_N: usize = COEXIST_HTTP_OUT_N;
const WRAP_OUT_N: usize = COEXIST_HTTP_OUT_N;
const SCRATCH_N: usize = TCP_RX_N + TCP_TX_N + HTTP_OUT_N + WRAP_OUT_N;
const _: [(); COEXIST_RX_ACC_N] = [(); RX_ACC_N];
const _: () = assert!(SCRATCH_N + 16 <= crate::mgmt::mgmt_arena::MGMT_ARENA_BYTES);

/// Complete HTTP request → codec → TLS wrap. Not iron TLS-OK by itself.
fn wrap_session_try_exchange(
    session: &mut Tls12Listen,
    sock: &mut tcp::Socket,
    out: &mut [u8],
    wrap: &mut [u8],
) -> bool {
    let Some(raw_bytes) = session.take_http() else {
        return false;
    };
    if !sock.can_send() {
        return false;
    }
    let raw = core::str::from_utf8(raw_bytes).unwrap_or("");
    // SAFETY: BSP-only coexist; PRE-EBS tables leaked for the HTTP codec.
    // KANI-TARGET: host HTTP tests cover the codec; this wrap is firmware-only.
    let wn = unsafe {
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
    if wn == 0 {
        return false;
    }
    let n = session.wrap_http(&out[..wn], wrap);
    if n == 0 {
        return false;
    }
    let mut off = 0;
    while off < n && sock.can_send() {
        match sock.send_slice(&wrap[off..n]) {
            Ok(0) => break,
            Ok(k) => off += k,
            Err(_) => break,
        }
    }
    off == n
}

fn tls_session() -> &'static mut Tls12Listen {
    // SAFETY: BSP-only listen slot; initialized once before ticks.
    // Session lives in .bss (`Tls12Listen::empty`) — `new()` is ~40 KiB and
    // blows the UEFI BSP stack (QEMU serial died after e1000 MAC=).
    unsafe {
        if !COEXIST_LISTEN_READY {
            COEXIST_LISTEN.load_lab_material();
            COEXIST_LISTEN_READY = true;
        }
        &mut COEXIST_LISTEN
    }
}

fn drain_tls(session: &mut Tls12Listen, sock: &mut tcp::Socket, wrap: &mut [u8]) {
    if !sock.can_send() {
        return;
    }
    let n = session.drain_tcp(wrap);
    if n > 0 {
        let _ = sock.send_slice(&wrap[..n]);
    }
}

fn print_tls_wrap_plaintext_note() {
    serial::write_line("boot: HOST-NIC TLS wrap=tls12 (ECDHE-RSA-AES128-GCM; not iron TLS-OK)");
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListenWhen {
    /// After ExitBootServices, before VMX. Lab may `qemu_exit` after GET /.
    AfterEbs,
    /// After `RAYNU-V-R640-BOOT-OK`. QEMU never reaches this (`qemu_exit` first).
    AfterBootOk,
}

/// After ExitBootServices: if QEMU e1000 is present, serve GET / then continue.
/// Iron BCM5720: arm analog/DMA **now**, then the standing SPA coexist
/// (ticks during RayNu-F). Bounded `PRE_RAYNUF_HTTPS_MS` is fallback if arm
/// fails. Phase F / BOOT-OK idle remain.
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
/// - DHCP may spin up to [`HOST_NIC_DHCP_MS`] when SNP parked no lease
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
    let mut device = match Bcm5720Device::init(mgmt_lease::prefer_mac()) {
        Ok(d) => d,
        Err(_) => {
            serial::write_line("boot: WARN — HOST-NIC coexist skip (device init)");
            return false;
        }
    };
    let Some(lease) = mgmt_lease::load_usable().or_else(|| dhcp_bcm5720(&mut device)) else {
        serial::write_line(
            "boot: WARN — HOST-NIC coexist skip (no parked SNP lease; native DHCP failed)",
        );
        return false;
    };
    if crate::mgmt::bcm5720_mmio::skip_http_listen_without_lstatus()
        && !crate::mgmt::bcm5720_mmio::bcm5720_phy_link_up()
    {
        serial::write_line("boot: WARN — HOST-NIC BCM5720 skip listen (no LSTATUS; do not curl)");
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
        let tcp_rx =
            tcp::SocketBuffer::new((&mut *core::ptr::addr_of_mut!(COEXIST_TCP_RX)).as_mut_slice());
        let tcp_tx =
            tcp::SocketBuffer::new((&mut *core::ptr::addr_of_mut!(COEXIST_TCP_TX)).as_mut_slice());
        let sockets =
            SocketSet::new((&mut *core::ptr::addr_of_mut!(COEXIST_SOCK_STORAGE)).as_mut_slice());
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
        COEXIST_TSC0 = crate::arch::cpu::rdtsc();
        COEXIST_ANNOUNCED = false;
        COEXIST_ACCEPT_AT_MS = 0;
        tls_session().reset();
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
    serial::write_str("boot: CURL NOW → https://");
    write_ipv4(ip);
    serial::write_byte(b':');
    write_u16_dec(port);
    serial::write_line("/  (native BCM5720; standing SPA during RayNu-F; SNP is dead)");
    print_tls_wrap_plaintext_note();
    serial::write_line("boot: HINT — COM2 idle after this snapshot (TCP accept / HTTP only)");
    crate::mgmt::bcm5720_mmio::reset_host_nic_frame_dumps();
    print_bcm5720_poll_diag();
    true
}

/// Rate-limited coexist poll for RayNu-F vmexit / USB BOT waits.
///
/// INVARIANTS:
/// - No-op unless [`arm_bcm5720_coexist`] succeeded
/// - At most one [`tick_bcm5720_coexist`] per ~2 ms (TSC)
pub fn maybe_tick_bcm5720_standing_spa() {
    // SAFETY: BSP-only coexist latch; same session as [`tick_bcm5720_coexist`].
    // KANI-TARGET: host gate checks the call sites, not this flag.
    if !unsafe { COEXIST_ARMED } {
        return;
    }
    let now = crate::arch::cpu::rdtsc();
    let hz = crate::boot::raynu_f_flag::tsc_hz();
    let hz = if hz < 1_000 {
        crate::mgmt::host_nic::COEXIST_TSC_HZ_FALLBACK
    } else {
        hz
    };
    // SAFETY: BSP-only coexist; same session as [`tick_bcm5720_coexist`].
    // KANI-TARGET: host gate checks the call sites, not this TSC latch.
    unsafe {
        let last = LAST_SPA_TICK_TSC;
        if last != 0 && now.wrapping_sub(last) < hz / 500 {
            return;
        }
        LAST_SPA_TICK_TSC = now;
    }
    tick_bcm5720_coexist();
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
        let session = tls_session();
        let out = &mut *core::ptr::addr_of_mut!(COEXIST_HTTP_OUT);
        let wrap = &mut *core::ptr::addr_of_mut!(COEXIST_WRAP_OUT);
        COEXIST_MILLIS = coexist_millis_from_tsc(
            COEXIST_TSC0,
            crate::arch::cpu::rdtsc(),
            crate::boot::raynu_f_flag::tsc_hz(),
        );
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
        let mut did_tls_fail = false;
        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                session.reset();
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
                session.reset();
            }
            if sock.can_recv() {
                let mut chunk = [0u8; 2048];
                if let Ok(n) = sock.recv_slice(&mut chunk) {
                    if n > 0 {
                        let _ = session.feed_tcp(&chunk[..n]);
                    }
                }
            }
            drain_tls(session, sock, wrap);
            let headers_done = session.take_http().is_some();
            if headers_done && sock.can_send() {
                if wrap_session_try_exchange(session, sock, out, wrap) {
                    did_exchange = true;
                    do_close = true;
                }
            } else if session.handshake_failed() {
                sock.abort();
                session.reset();
                COEXIST_ANNOUNCED = false;
                COEXIST_ACCEPT_AT_MS = 0;
                did_tls_fail = true;
            } else if http_accept_should_idle_abort(
                COEXIST_ANNOUNCED,
                headers_done,
                millis.saturating_sub(COEXIST_ACCEPT_AT_MS),
                HOST_NIC_HTTP_IDLE_MS,
            ) {
                sock.abort();
                session.reset();
                COEXIST_ANNOUNCED = false;
                COEXIST_ACCEPT_AT_MS = 0;
                did_idle_abort = true;
            }
        }
        if did_exchange {
            serial::write_line("boot: HOST-NIC HTTP exchange ok");
            let _ = maybe_print_iron_tls_ok(true, true);
            if take_spa_keys_injected() {
                let _ = maybe_print_iron_console_ok(true, true);
            }
            let _ = iface.poll(Instant::from_millis(millis + 1), device, sockets);
            pci_census::print_host_nic_exchange_ok_marker();
        }
        if do_close {
            // drain TX before reclaim: abort() without drain RSTs Firefox
            // nssFailure2 "authenticity of the received data could not be
            // verified". curl tolerated the RST; browsers do not.
            // close() first so Connection: close gets FIN; abort() only if
            // FIN_WAIT still holds the one listen slot (iron 2026-08-21).
            for _ in 0..64 {
                let _ = iface.poll(Instant::from_millis(millis + 1), device, sockets);
                if sockets.get::<tcp::Socket>(tcp_handle).send_queue() == 0 {
                    break;
                }
                tsc_spin_ms(1);
            }
            tsc_spin_ms(8);
            sockets.get_mut::<tcp::Socket>(tcp_handle).close();
            for _ in 0..24 {
                let _ = iface.poll(Instant::from_millis(millis + 1), device, sockets);
                tsc_spin_ms(1);
                if !sockets.get::<tcp::Socket>(tcp_handle).is_open() {
                    break;
                }
            }
            if sockets.get::<tcp::Socket>(tcp_handle).is_open() {
                sockets.get_mut::<tcp::Socket>(tcp_handle).abort();
            }
            session.reset();
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
        if did_tls_fail {
            serial::write_line("boot: WARN — HOST-NIC TLS handshake fail; re-listen");
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
            ListenWhen::AfterEbs => {
                bringup_bcm5720_post_ebs();
                if arm_bcm5720_coexist() {
                    serial::write_line(
                        "boot: HOST-NIC standing SPA armed (ticks during RayNu-F; SNP is dead)",
                    );
                } else {
                    serial::write_line(
                        "boot: WARN — HOST-NIC standing SPA arm failed; bounded window fallback",
                    );
                    listen_with_retries(when, NicKind::Bcm5720);
                }
            }
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

/// Steal BCM5720 analog immediately after EBS, then HTTPS-listen before
/// RayNu-F (guest USB I/O must not run first). Iron 2026-08-19 complete COM2
/// (`1404f055`): both funcs `cand bmsr=7949` then `CORECLK_RESET` without
/// BMCR still `link=timeout`. AfterBootOk listen is skipped without `LSTATUS`.
fn bringup_bcm5720_post_ebs() {
    let prefer = mgmt_lease::prefer_mac();
    serial::write_line("boot: HOST-NIC BCM5720 post-EBS bring-up (keep analog before guest path)");
    match Bcm5720Device::init(prefer) {
        Ok(dev) => {
            serial::write_str("boot: HOST-NIC BCM5720 post-EBS Device MAC=");
            write_mac(dev.mac());
            serial::write_line(" (HTTPS window before RayNu-F)");
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
    let mut device =
        Bcm5720Device::init(mgmt_lease::prefer_mac()).map_err(|_| MgmtFatal::Device)?;
    let Some(lease) = mgmt_lease::load_usable().or_else(|| dhcp_bcm5720(&mut device)) else {
        serial::write_line(
            "boot: WARN — HOST-NIC BCM5720: no parked SNP lease (native DHCP failed; skip MMIO)",
        );
        return Err(MgmtFatal::Bind);
    };
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

/// Native DHCP when firmware SNP did not park a lease (ADR-013).
/// Iron `6ba076cc`: SNP DHCP failed → analog/coexist skipped → Phase B idle.
fn dhcp_bcm5720(device: &mut Bcm5720Device) -> Option<mgmt_lease::ParkedMgmtLease> {
    let mac = device.mac();
    serial::write_line("boot: HOST-NIC BCM5720 DHCP discover…");
    let mut config = Config::new(EthernetAddress(mac).into());
    config.random_seed = mac_seed(mac);
    let tsc0 = crate::arch::cpu::rdtsc();
    let hz = crate::boot::raynu_f_flag::tsc_hz();
    let mut iface = Interface::new(config, device, Instant::from_millis(0));
    let mut storage = [SocketStorage::EMPTY; 1];
    let mut sockets = SocketSet::new(&mut storage[..]);
    let dhcp_handle = sockets.add(dhcpv4::Socket::new());
    let mut leased: Option<smoltcp::wire::Ipv4Cidr> = None;
    let mut leased_router: Option<Ipv4Address> = None;
    let mut millis: i64 = 0;
    while millis <= HOST_NIC_DHCP_MS {
        let ts = Instant::from_millis(millis);
        let _ = bounded_poll(HOST_NIC_POLL_BUDGET, || {
            matches!(
                iface.poll(ts, device, &mut sockets),
                smoltcp::iface::PollResult::SocketStateChanged
            )
        });
        match sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll() {
            Some(dhcpv4::Event::Configured(cfg)) => {
                leased = Some(cfg.address);
                leased_router = cfg.router;
                break;
            }
            Some(dhcpv4::Event::Deconfigured) => {
                iface.update_ip_addrs(|addrs| addrs.clear());
                iface.routes_mut().remove_default_ipv4_route();
            }
            None => {}
        }
        millis = coexist_millis_from_tsc(tsc0, crate::arch::cpu::rdtsc(), hz);
        core::hint::spin_loop();
    }
    let Some(cidr) = leased else {
        serial::write_line("boot: WARN — HOST-NIC BCM5720 DHCP failed (no lease)");
        return None;
    };
    let ip = cidr.address();
    serial::write_str("boot: HOST-NIC BCM5720 DHCP lease ");
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
    let lease = mgmt_lease::ParkedMgmtLease {
        ip: ip.octets(),
        prefix: cidr.prefix_len(),
        router: leased_router.map(|r| r.octets()).unwrap_or([0; 4]),
        has_router: leased_router.is_some(),
        mac,
        port: MGMT_HTTP_DEFAULT_PORT,
    };
    mgmt_lease::store(lease);
    Some(lease)
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
    let (out, wrap) = rest.split_at_mut(HTTP_OUT_N);
    let session = tls_session();
    session.reset();

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
            if nic_tag == "BCM5720" {
                serial::write_str("boot: CURL NOW → https://");
                write_ipv4(ip);
                serial::write_byte(b':');
                write_u16_dec(port);
                serial::write_line("/  (native BCM5720; before RayNu-F; SNP is dead)");
                print_tls_wrap_plaintext_note();
                serial::write_str("boot: native HTTPS window_ms=");
                write_u32_dec(PRE_RAYNUF_HTTPS_MS as u32);
                serial::write_byte(b'\n');
                serial::write_line(
                    "boot: HINT — Mac must be on lease subnet; curl --cacert now; RayNu-F starts after this window",
                );
                print_bcm5720_poll_diag();
            }
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
                serial::write_str("boot: CURL NOW → https://");
                write_ipv4(ip);
                serial::write_byte(b':');
                write_u16_dec(port);
                serial::write_line("/  (native BCM5720; SNP is dead)");
                print_tls_wrap_plaintext_note();
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
    let mut last_diag: i64 = 0;
    let mut last_rx_drop: u32 = 0;
    let mut last_remind: i64 = 0;
    let deadline = match (when, nic_tag) {
        (ListenWhen::AfterEbs, "BCM5720") => PRE_RAYNUF_HTTPS_MS as i64,
        (ListenWhen::AfterEbs, _) => HOST_NIC_LISTEN_MS as i64,
        (ListenWhen::AfterBootOk, _) => i64::MAX / 4,
    };
    let max_ex = match (when, nic_tag) {
        (ListenWhen::AfterEbs, "BCM5720") => 1,
        (ListenWhen::AfterEbs, _) => HOST_NIC_MAX_EXCHANGES,
        (ListenWhen::AfterBootOk, _) => u32::MAX,
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
        let mut did_tls_fail = false;
        {
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                session.reset();
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
                session.reset();
            }
            if sock.can_recv() {
                let mut chunk = [0u8; 2048];
                if let Ok(n) = sock.recv_slice(&mut chunk) {
                    if n > 0 {
                        let _ = session.feed_tcp(&chunk[..n]);
                    }
                }
            }
            drain_tls(session, sock, wrap);
            let headers_done = session.take_http().is_some();
            if headers_done && sock.can_send() {
                if wrap_session_try_exchange(session, sock, out, wrap) {
                    did_exchange = true;
                    do_close = true;
                }
            } else if session.handshake_failed() {
                sock.abort();
                session.reset();
                announced = false;
                accept_at = 0;
                did_tls_fail = true;
            } else if http_accept_should_idle_abort(
                announced,
                headers_done,
                millis.saturating_sub(accept_at),
                HOST_NIC_HTTP_IDLE_MS,
            ) {
                sock.abort();
                session.reset();
                announced = false;
                accept_at = 0;
                did_idle_abort = true;
            }
        }

        if did_exchange {
            served = served.saturating_add(1);
            serial::write_line("boot: HOST-NIC HTTP exchange ok");
            let _ = maybe_print_iron_tls_ok(true, nic_tag == "BCM5720");
            if take_spa_keys_injected() {
                let _ = maybe_print_iron_console_ok(true, nic_tag == "BCM5720");
            }
        }
        if do_close {
            sockets.get_mut::<tcp::Socket>(tcp_handle).close();
            // FIN + drain before qemu_exit. Exiting with an open socket RSTs
            // curl (SPA ~15 KiB; TCG user-net).
            flush_tcp_tx(&mut iface, device, &mut sockets, tcp_handle, &mut millis);
            session.reset();
            announced = false;
            accept_at = 0;
        }
        if did_exchange {
            match when {
                ListenWhen::AfterEbs => {
                    if nic_tag != "BCM5720" {
                        serial::write_line(M7_HOST_NIC_QEMU_MARKER);
                        if host_nic_lab_armed() {
                            serial::qemu_exit_success();
                        }
                    }
                }
                ListenWhen::AfterBootOk => {
                    pci_census::print_host_nic_exchange_ok_marker();
                }
            }
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
        if did_tls_fail {
            serial::write_line("boot: WARN — HOST-NIC TLS handshake fail; re-listen");
            millis += 1;
            iface.poll(Instant::from_millis(millis), device, &mut sockets);
            let sock = sockets.get_mut::<tcp::Socket>(tcp_handle);
            if !sock.is_open() {
                let _ = sock.listen(port);
            }
        }

        millis += 1;
        tsc_spin_ms(1);
        if when == ListenWhen::AfterEbs && nic_tag == "BCM5720" && millis - last_remind >= 5000 {
            last_remind = millis;
            let rem = deadline.saturating_sub(millis).max(0) as u32;
            serial::write_str("boot: waiting curl https://");
            write_ipv4(ip);
            serial::write_byte(b':');
            write_u16_dec(port);
            serial::write_str("/  remaining_ms=");
            write_u32_dec(rem);
            serial::write_line(" (before RayNu-F; SNP is dead)");
        }
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
        if nic_tag == "BCM5720" && when == ListenWhen::AfterEbs {
            serial::write_line(
                "boot: WARN — HOST-NIC accept timeout (continuing to RayNu-F; native listen did not serve)",
            );
        } else {
            serial::write_line(
                "boot: WARN — HOST-NIC accept timeout (continuing; native listen did not serve)",
            );
        }
    }
    Ok(())
}

/// Poll until TX is empty (and the socket is no longer active after `close`).
/// QEMU lab `qemu_exit`s after GET /; a RST or queued tail makes curl fail.
fn flush_tcp_tx<D: Device>(
    iface: &mut Interface,
    device: &mut D,
    sockets: &mut SocketSet,
    tcp_handle: SocketHandle,
    millis: &mut i64,
) {
    for _ in 0..400 {
        *millis += 1;
        iface.poll(Instant::from_millis(*millis), device, sockets);
        tsc_spin_ms(1);
        let sock = sockets.get::<tcp::Socket>(tcp_handle);
        if sock.send_queue() == 0 && !sock.is_active() {
            break;
        }
    }
    for _ in 0..64 {
        *millis += 1;
        iface.poll(Instant::from_millis(*millis), device, sockets);
        tsc_spin_ms(1);
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
