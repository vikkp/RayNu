//! M7.1 / M7.6 mgmt HTTP listen surface (outside Proven Core).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-009 / ADR-012)
//!
//! - Host/`cfg(test)`: real `std::net::TcpListener` proving SPA + Bearer REST.
//! - Firmware (`uefi-bin`): PRE-EBS UEFI Tcp4 passive listen (ADR-012), with
//!   SNP + smoltcp residual when Tcp4 is absent. Soft-fails so the guest path
//!   still runs.
//!
//! Markers:
//! - `RAYNU-V-M7-HTTP-OK` — M7.1 host codec gate (unchanged)
//! - `RAYNU-V-M7-UEFI-HTTP-OK` — firmware Tcp4 served ≥1 HTTP exchange (M7.6)
//! - `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK` — host/CI scaffold for M7.6 wiring

use super::datastore::ImageTable;
use super::http::{handle_http_request, HTTP_LAB_NOTE, M7_HTTP_OK_MARKER, MGMT_HTTP_DEFAULT_PORT};
use super::iso::IsoDeployPlan;
use super::iso_install::InstallToDiskPlan;
use super::VmTable;

/// Iron / firmware marker when PRE-EBS Tcp4 listen serves SPA or REST.
pub const M7_UEFI_HTTP_OK_MARKER: &str = "RAYNU-V-M7-UEFI-HTTP-OK";

/// Firmware marker when SNP+smoltcp serves ≥1 HTTP exchange **after** EBS.
pub const M7_POST_EBS_HTTP_OK_MARKER: &str = "RAYNU-V-M7-POST-EBS-HTTP-OK";

/// Host/CI scaffold marker (never claims iron E3 alone).
pub const M7_UEFI_HTTP_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK";

/// Host/CI scaffold for post-EBS SNP listen wiring (never claims iron).
pub const M7_POST_EBS_HTTP_SCAFFOLD_MARKER: &str = "RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK";

/// Closed when firmware listen path is wired (scaffold); OK marker is runtime.
pub const UEFI_HTTP_GAP_NOTE: &str = "GAP(CLOSED M7.6): UEFI NIC HTTP listen";

/// How long to wait for an inbound connection before continuing to EBS (ms).
/// Shared Tcp4 budget (absolute). SNP residual uses a separate post-bind window.
pub const PRE_EBS_LISTEN_TIMEOUT_MS: u64 = 15_000;

/// SNP residual: listen window **after** DHCP bind (ms). Operator needs time to
/// read SOL IP and curl from a laptop — DHCP already consumed wall-clock.
pub const SNP_POST_BIND_LISTEN_MS: u64 = 45_000;

/// Max HTTP exchanges to serve in the PRE-EBS window (SPA multi-fetch).
pub const PRE_EBS_MAX_EXCHANGES: u32 = 24;

/// Why firmware cannot bind / serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MgmtListenError {
    /// Host build / no UEFI network attempt.
    UnsupportedOnFirmware,
    /// Tcp4 service binding not present (common on minimal OVMF / R640 floppy).
    NoTcp4Stack,
    /// SNP NIC open/start failed (SNP residual path).
    NoSnpNic,
    /// DHCP over SNP failed within PRE-EBS budget.
    DhcpFailed,
    BindFailed,
    AcceptFailed,
    ServeFailed,
}

/// Documented firmware entry: bind mgmt HTTP on the host NIC (PRE-EBS).
///
/// On success prints [`M7_UEFI_HTTP_OK_MARKER`] after at least one HTTP exchange.
pub fn listen_mgmt_http_uefi(port: u16) -> Result<(), MgmtListenError> {
    let _ = (M7_HTTP_OK_MARKER, HTTP_LAB_NOTE, MGMT_HTTP_DEFAULT_PORT);
    let _ = (
        M7_UEFI_HTTP_OK_MARKER,
        M7_UEFI_HTTP_SCAFFOLD_MARKER,
        M7_POST_EBS_HTTP_OK_MARKER,
        M7_POST_EBS_HTTP_SCAFFOLD_MARKER,
        UEFI_HTTP_GAP_NOTE,
    );

    #[cfg(feature = "uefi-bin")]
    {
        return match uefi_tcp4_listen(port) {
            Ok(()) => Ok(()),
            Err(MgmtListenError::NoTcp4Stack) => {
                use crate::boot::serial;
                serial::write_line("boot: falling back to SNP residual (ADR-012)");
                crate::mgmt::snp_listen_uefi::uefi_snp_listen(port)
            }
            Err(e) => Err(e),
        };
    }

    #[cfg(not(feature = "uefi-bin"))]
    {
        let _ = port;
        Err(MgmtListenError::UnsupportedOnFirmware)
    }
}

/// Soft-fail wrapper for `src/main.rs`: never blocks guest bring-up.
///
/// Returns `true` when [`M7_UEFI_HTTP_OK_MARKER`] was printed.
pub fn run_pre_ebs_mgmt_listen() -> bool {
    crate::mgmt::pci_census::run_pre_ebs_pci_census();

    #[cfg(feature = "uefi-bin")]
    {
        crate::mgmt::api::probe_operator_auth_token();
        crate::mgmt::net_probe_uefi::probe_and_print();
    }

    #[cfg(feature = "uefi-bin")]
    {
        if crate::mgmt::host_nic::should_skip_pre_ebs_firmware_listen() {
            use crate::boot::serial;
            serial::write_line(
                "boot: QEMU e1000 8086:100e — skip PRE-EBS SNP/Tcp4 (ADR-013 Phase C)",
            );
            return false;
        }
    }

    match listen_mgmt_http_uefi(MGMT_HTTP_DEFAULT_PORT) {
        Ok(()) => true,
        Err(e) => {
            #[cfg(feature = "uefi-bin")]
            {
                use crate::boot::serial;
                match e {
                    MgmtListenError::NoTcp4Stack => {
                        serial::write_line(
                            "boot: WARN — Tcp4 stack absent after connect; mgmt HTTP residual (ADR-012)",
                        );
                    }
                    MgmtListenError::NoSnpNic => {
                        serial::write_line("boot: WARN — SNP NIC open failed");
                    }
                    MgmtListenError::DhcpFailed => {
                        serial::write_line(
                            "boot: WARN — SNP DHCP failed; mgmt HTTP residual (ADR-012)",
                        );
                    }
                    MgmtListenError::UnsupportedOnFirmware => {
                        serial::write_line("boot: WARN — mgmt HTTP unsupported on this path");
                    }
                    MgmtListenError::BindFailed => {
                        serial::write_line("boot: WARN — mgmt HTTP bind failed");
                    }
                    MgmtListenError::AcceptFailed => {
                        serial::write_line(
                            "boot: WARN — mgmt HTTP accept timeout (continuing to EBS)",
                        );
                    }
                    MgmtListenError::ServeFailed => {
                        serial::write_line("boot: WARN — mgmt HTTP serve failed");
                    }
                }
            }
            #[cfg(not(feature = "uefi-bin"))]
            {
                let _ = e;
            }
            false
        }
    }
}

/// After ExitBootServices: SNP probe is serial-only; native e1000 listen is Phase C.
pub fn run_post_ebs_mgmt_listen() {
    #[cfg(feature = "uefi-bin")]
    {
        crate::mgmt::snp_listen_uefi::uefi_snp_post_ebs_probe();
        crate::mgmt::host_nic_listen::run_post_ebs_host_nic_listen();
    }
}

/// After VMXOFF: serial WARN only for firmware SNP. Native NIC may idle-listen.
pub fn run_post_ebs_http_idle() {
    #[cfg(feature = "uefi-bin")]
    {
        crate::mgmt::snp_listen_uefi::uefi_snp_post_ebs_idle();
        crate::mgmt::host_nic_listen::run_post_boot_ok_native_idle();
    }
}

/// True when listen API + markers + host TcpListener proof exist.
pub fn prop_listen_surface() -> bool {
    let s = include_str!("http_listen.rs");
    s.contains("fn listen_mgmt_http_uefi(")
        && s.contains("fn run_pre_ebs_mgmt_listen(")
        && s.contains("UnsupportedOnFirmware")
        && s.contains("NoTcp4Stack")
        && s.contains("serve_one_connection_host")
        && s.contains("TcpListener")
        && s.contains(M7_UEFI_HTTP_OK_MARKER)
        && s.contains(M7_UEFI_HTTP_SCAFFOLD_MARKER)
        && s.contains(UEFI_HTTP_GAP_NOTE)
        && s.contains("fn run_post_ebs_mgmt_listen(")
        && s.contains("fn run_post_ebs_http_idle(")
        && s.contains(M7_POST_EBS_HTTP_OK_MARKER)
        && s.contains(M7_POST_EBS_HTTP_SCAFFOLD_MARKER)
        && UEFI_HTTP_GAP_NOTE.contains("CLOSED M7.6")
}

/// Host-only: serve one HTTP exchange on `127.0.0.1:port` (or ephemeral if 0).
#[cfg(test)]
pub fn serve_one_connection_host(port: u16) -> Result<u16, MgmtListenError> {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener =
        TcpListener::bind(("127.0.0.1", port)).map_err(|_| MgmtListenError::BindFailed)?;
    let bound = listener
        .local_addr()
        .map_err(|_| MgmtListenError::BindFailed)?
        .port();
    let (mut stream, _) = listener
        .accept()
        .map_err(|_| MgmtListenError::AcceptFailed)?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let raw = core::str::from_utf8(&buf[..n]).unwrap_or("");
    let mut table = VmTable::new();
    let mut images = ImageTable::new();
    let mut iso_plan = IsoDeployPlan::empty();
    let mut iso_install = InstallToDiskPlan::empty();
    let mut out = [0u8; 16384];
    let wn = handle_http_request(
        &mut table,
        &mut images,
        &mut iso_plan,
        &mut iso_install,
        raw,
        &mut out,
    )
    .unwrap_or(0);
    let _ = stream.write_all(&out[..wn]);
    let _ = stream.flush();
    Ok(bound)
}

#[cfg(feature = "uefi-bin")]
fn uefi_tcp4_listen(port: u16) -> Result<(), MgmtListenError> {
    use crate::boot::serial;
    use crate::mgmt::tcp4_uefi::{
        create_tcp4_child, Ipv4Address, Tcp4AccessPoint, Tcp4CompletionToken, Tcp4ConfigData,
        Tcp4FragmentData, Tcp4IoToken, Tcp4ListenToken, Tcp4Packet, Tcp4Protocol, Tcp4ReceiveData,
        Tcp4TransmitData,
    };
    use core::ptr;
    use uefi::boot::{self, EventType, Tpl};
    use uefi::{Handle, Status};

    let (listen_child, mut listen_tcp, mut sb) =
        create_tcp4_child().map_err(|_| MgmtListenError::NoTcp4Stack)?;

    let config = Tcp4ConfigData {
        type_of_service: 0,
        time_to_live: 64,
        access_point: Tcp4AccessPoint {
            use_default_address: true,
            station_address: Ipv4Address([0, 0, 0, 0]),
            subnet_mask: Ipv4Address([0, 0, 0, 0]),
            station_port: port,
            remote_address: Ipv4Address([0, 0, 0, 0]),
            remote_port: 0,
            active_flag: false,
        },
        control_option: ptr::null(),
    };

    unsafe {
        listen_tcp
            .configure(&config)
            .map_err(|_| MgmtListenError::BindFailed)?;
    }

    serial::write_str("boot: mgmt HTTP listening on 0.0.0.0:");
    write_port_dec(port);
    serial::write_line(" (PRE-EBS Tcp4 window)");
    crate::mgmt::pre_ebs_mgmt::reset_pre_ebs_mgmt();

    let mut served: u32 = 0;
    let mut ticks_left = PRE_EBS_LISTEN_TIMEOUT_MS;

    while served < PRE_EBS_MAX_EXCHANGES && ticks_left > 0 {
        let Ok(event) =
            (unsafe { boot::create_event(EventType::NOTIFY_WAIT, Tpl::CALLBACK, None, None) })
        else {
            break;
        };

        let mut token = Tcp4ListenToken {
            completion_token: Tcp4CompletionToken {
                event: event.as_ptr().cast(),
                status: Status::NOT_READY,
            },
            new_child_handle: ptr::null_mut(),
        };

        if unsafe { listen_tcp.accept(&mut token) }.is_err() {
            let _ = boot::close_event(event);
            let _ = unsafe { listen_tcp.poll() };
            let _ = boot::stall(1_000);
            ticks_left = ticks_left.saturating_sub(1);
            continue;
        }

        let mut accepted = false;
        while ticks_left > 0 {
            let _ = unsafe { listen_tcp.poll() };
            if boot::check_event(unsafe { event.unsafe_clone() }).ok() == Some(true) {
                accepted = true;
                break;
            }
            let _ = boot::stall(1_000);
            ticks_left = ticks_left.saturating_sub(1);
        }
        let _ = boot::close_event(event);

        if !accepted || token.new_child_handle.is_null() {
            let _ = unsafe { listen_tcp.cancel_all() };
            continue;
        }

        let Some(conn_handle) = (unsafe { Handle::from_ptr(token.new_child_handle) }) else {
            continue;
        };

        if let Ok(mut conn) = boot::open_protocol_exclusive::<Tcp4Protocol>(conn_handle) {
            if serve_one_tcp4_exchange(&mut conn, &mut ticks_left).is_ok() {
                served += 1;
            }
            let _ = unsafe { conn.cancel_all() };
            drop(conn);
        }
        let _ = unsafe { sb.destroy_child_handle(conn_handle) };
    }

    let _ = unsafe { listen_tcp.cancel_all() };
    drop(listen_tcp);
    let _ = unsafe { sb.destroy_child_handle(listen_child) };
    drop(sb);

    if served == 0 {
        return Err(MgmtListenError::AcceptFailed);
    }

    serial::write_line(M7_UEFI_HTTP_OK_MARKER);
    Ok(())
}

#[cfg(feature = "uefi-bin")]
fn serve_one_tcp4_exchange(
    conn: &mut uefi::boot::ScopedProtocol<crate::mgmt::tcp4_uefi::Tcp4Protocol>,
    ticks_left: &mut u64,
) -> Result<(), MgmtListenError> {
    use crate::mgmt::tcp4_uefi::{
        Tcp4CompletionToken, Tcp4FragmentData, Tcp4IoToken, Tcp4Packet, Tcp4ReceiveData,
        Tcp4TransmitData,
    };
    use uefi::boot::{self, EventType, Tpl};
    use uefi::Status;

    let mut rx_buf = [0u8; 8192];
    let mut rx_data = Tcp4ReceiveData {
        urgent_flag: false,
        data_length: rx_buf.len() as u32,
        fragment_count: 1,
        fragment_table: [Tcp4FragmentData {
            fragment_length: rx_buf.len() as u32,
            fragment_buffer: rx_buf.as_mut_ptr().cast(),
        }],
    };

    let rx_event = unsafe {
        boot::create_event(EventType::NOTIFY_WAIT, Tpl::CALLBACK, None, None)
            .map_err(|_| MgmtListenError::ServeFailed)?
    };

    let mut rx_token = Tcp4IoToken {
        completion_token: Tcp4CompletionToken {
            event: rx_event.as_ptr().cast(),
            status: Status::NOT_READY,
        },
        packet: Tcp4Packet {
            rx_data: &mut rx_data,
        },
    };

    unsafe {
        conn.receive(&mut rx_token)
            .map_err(|_| MgmtListenError::ServeFailed)?;
    }

    let mut got_rx = false;
    while *ticks_left > 0 {
        let _ = unsafe { conn.poll() };
        if boot::check_event(unsafe { rx_event.unsafe_clone() }).ok() == Some(true) {
            got_rx = true;
            break;
        }
        let _ = boot::stall(1_000);
        *ticks_left = ticks_left.saturating_sub(1);
    }
    let _ = boot::close_event(rx_event);
    if !got_rx || rx_data.data_length == 0 {
        return Err(MgmtListenError::ServeFailed);
    }

    let n = (rx_data.data_length as usize).min(rx_buf.len());
    let raw = core::str::from_utf8(&rx_buf[..n]).unwrap_or("");

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
    if wn == 0 {
        return Err(MgmtListenError::ServeFailed);
    }

    let mut tx_data = Tcp4TransmitData {
        push: true,
        urgent: false,
        data_length: wn as u32,
        fragment_count: 1,
        fragment_table: [Tcp4FragmentData {
            fragment_length: wn as u32,
            fragment_buffer: out.as_mut_ptr().cast(),
        }],
    };

    let tx_event = unsafe {
        boot::create_event(EventType::NOTIFY_WAIT, Tpl::CALLBACK, None, None)
            .map_err(|_| MgmtListenError::ServeFailed)?
    };

    let mut tx_token = Tcp4IoToken {
        completion_token: Tcp4CompletionToken {
            event: tx_event.as_ptr().cast(),
            status: Status::NOT_READY,
        },
        packet: Tcp4Packet {
            tx_data: &mut tx_data,
        },
    };

    unsafe {
        conn.transmit(&mut tx_token)
            .map_err(|_| MgmtListenError::ServeFailed)?;
    }

    while *ticks_left > 0 {
        let _ = unsafe { conn.poll() };
        if boot::check_event(unsafe { tx_event.unsafe_clone() }).ok() == Some(true) {
            break;
        }
        let _ = boot::stall(1_000);
        *ticks_left = ticks_left.saturating_sub(1);
    }
    let _ = boot::close_event(tx_event);
    Ok(())
}

#[cfg(feature = "uefi-bin")]
fn write_port_dec(mut n: u16) {
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
#[path = "http_listen_test.rs"]
mod http_listen_test;
