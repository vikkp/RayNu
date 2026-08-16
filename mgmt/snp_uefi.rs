//! EFI Simple Network Protocol helpers for M7.6 SNP residual (ADR-012).
//!
//! Pillar: [Z] · Proven Core: **outside**
//! Boot Services only — must not be used after ExitBootServices.

#![cfg(feature = "uefi-bin")]

use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::proto::network::snp::{NetworkState, ReceiveFlags, SimpleNetwork};
use uefi::{Handle, Identify, Status};

/// Open the first usable SNP NIC (prefer `media_present`), start + initialize.
pub fn open_first_snp() -> Result<(Handle, ScopedProtocol<SimpleNetwork>, [u8; 6]), Status> {
    let handles = boot::locate_handle_buffer(SearchType::ByProtocol(&SimpleNetwork::GUID))
        .map_err(|e| e.status())?;
    if handles.is_empty() {
        return Err(Status::NOT_FOUND);
    }

    // Pass 1: media_present; pass 2: any that starts.
    for require_media in [true, false] {
        for &handle in handles.iter() {
            if let Ok(result) = try_open_snp(handle, require_media) {
                return Ok(result);
            }
        }
    }
    Err(Status::NOT_FOUND)
}

fn try_open_snp(
    handle: Handle,
    require_media: bool,
) -> Result<(Handle, ScopedProtocol<SimpleNetwork>, [u8; 6]), Status> {
    let snp = unsafe {
        boot::open_protocol::<SimpleNetwork>(
            OpenProtocolParams {
                handle,
                agent: boot::image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
        .map_err(|e| e.status())?
    };

    let mode = snp.mode();
    if require_media && mode.media_present_supported && !mode.media_present {
        return Err(Status::NO_MEDIA);
    }

    match mode.state {
        NetworkState::STOPPED => {
            snp.start().map_err(|e| e.status())?;
            snp.initialize(0, 0).map_err(|e| e.status())?;
        }
        NetworkState::STARTED => {
            snp.initialize(0, 0).map_err(|e| e.status())?;
        }
        NetworkState::INITIALIZED => {}
        _ => {
            snp.start().map_err(|e| e.status())?;
            snp.initialize(0, 0).map_err(|e| e.status())?;
        }
    }

    let _ = snp.receive_filters(
        ReceiveFlags::UNICAST | ReceiveFlags::BROADCAST | ReceiveFlags::MULTICAST,
        ReceiveFlags::empty(),
        false,
        None,
    );

    let mut mac = [0u8; 6];
    mac.copy_from_slice(&snp.mode().current_address.0[..6]);
    // Reject all-zero MACs (placeholder handles).
    if mac.iter().all(|&b| b == 0) {
        return Err(Status::DEVICE_ERROR);
    }

    Ok((handle, snp, mac))
}

/// smoltcp `Device` backed by UEFI SNP (full Ethernet frames).
pub struct SnpDevice {
    snp: ScopedProtocol<SimpleNetwork>,
    mac: [u8; 6],
}

impl SnpDevice {
    pub fn new(snp: ScopedProtocol<SimpleNetwork>, mac: [u8; 6]) -> Self {
        Self { snp, mac }
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }

    fn recycle_tx(&mut self) {
        // Drain recycled TX buffers so SNP does not stall.
        for _ in 0..16 {
            match self.snp.get_recycled_transmit_buffer_status() {
                Ok(Some(_)) => continue,
                Ok(None) => break,
                Err(_) => break,
            }
        }
    }
}

pub struct SnpRxToken {
    buf: [u8; 1536],
    len: usize,
}

pub struct SnpTxToken<'a> {
    device: &'a mut SnpDevice,
}

impl Device for SnpDevice {
    type RxToken<'a>
        = SnpRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = SnpTxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        self.recycle_tx();
        let mut tmp = [0u8; 1536];
        match self.snp.receive(&mut tmp, None, None, None, None) {
            Ok(n) if n > 0 => {
                let mut owned = [0u8; 1536];
                let n = n.min(1536);
                owned[..n].copy_from_slice(&tmp[..n]);
                Some((SnpRxToken { buf: owned, len: n }, SnpTxToken { device: self }))
            }
            _ => None,
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        self.recycle_tx();
        Some(SnpTxToken { device: self })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        let mtu = self.snp.mode().max_packet_size as usize;
        // max_packet_size is usually payload; Ethernet MTU ~1500 + header.
        caps.max_transmission_unit = if mtu >= 1514 { mtu.min(1536) } else { 1514 };
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}

impl RxToken for SnpRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buf[..self.len])
    }
}

impl TxToken for SnpTxToken<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut scratch = [0u8; 1536];
        let len = len.min(scratch.len());
        let result = f(&mut scratch[..len]);
        // header_size=0 → buffer is a complete media frame.
        let _ = self.device.snp.transmit(0, &scratch[..len], None, None, None);
        self.device.recycle_tx();
        result
    }
}
