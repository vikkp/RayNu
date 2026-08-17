//! smoltcp `Device` for the QEMU e1000 (ADR-013 Phase C).
//!
//! Pillar: [Z]
//! Proven Core: **outside**
//!
//! Thin safe wrapper: all MMIO/DMA is in [`crate::mgmt::e1000_mmio`].

#![cfg(feature = "uefi-bin")]

use crate::mgmt::e1000_mmio::{self, FRAME_MAX};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

/// smoltcp PHY wrapping the host-owned e1000.
pub struct E1000Device {
    mac: [u8; 6],
}

impl E1000Device {
    pub fn init() -> Result<Self, e1000_mmio::E1000Error> {
        let mac = e1000_mmio::init_e1000()?;
        Ok(Self { mac })
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

pub struct E1000RxToken {
    buf: [u8; FRAME_MAX],
    len: usize,
}

pub struct E1000TxToken;

impl Device for E1000Device {
    type RxToken<'a>
        = E1000RxToken
    where
        Self: 'a;
    type TxToken<'a>
        = E1000TxToken
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut buf = [0u8; FRAME_MAX];
        let len = e1000_mmio::receive_frame(&mut buf)?;
        Some((E1000RxToken { buf, len }, E1000TxToken))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(E1000TxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = FRAME_MAX;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}

impl RxToken for E1000RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buf[..self.len])
    }
}

impl TxToken for E1000TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut scratch = [0u8; FRAME_MAX];
        let len = len.min(scratch.len());
        let result = f(&mut scratch[..len]);
        let _ = e1000_mmio::transmit_frame(&scratch[..len]);
        result
    }
}
