//! smoltcp `Device` for BCM5720 `14e4:165f` (ADR-013 Phase D).
//!
//! Pillar: [Z] [D]
//! Proven Core: **outside**
//!
//! Thin safe wrapper. All MMIO/DMA is in [`crate::mgmt::bcm5720_mmio`].
//! Same PHY trait as QEMU e1000 — do not fork HTTP.

#![cfg(feature = "uefi-bin")]

use crate::mgmt::bcm5720_mmio::{self, FRAME_MAX};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;

pub struct Bcm5720Device {
    mac: [u8; 6],
}

impl Bcm5720Device {
    pub fn init(prefer_mac: [u8; 6]) -> Result<Self, bcm5720_mmio::Bcm5720Error> {
        let mac = bcm5720_mmio::init_bcm5720(prefer_mac)?;
        Ok(Self { mac })
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

pub struct Bcm5720RxToken {
    buf: [u8; FRAME_MAX],
    len: usize,
}

pub struct Bcm5720TxToken;

impl Device for Bcm5720Device {
    type RxToken<'a>
        = Bcm5720RxToken
    where
        Self: 'a;
    type TxToken<'a>
        = Bcm5720TxToken
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut buf = [0u8; FRAME_MAX];
        let len = bcm5720_mmio::receive_frame(&mut buf)?;
        Some((Bcm5720RxToken { buf, len }, Bcm5720TxToken))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(Bcm5720TxToken)
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = FRAME_MAX;
        caps.max_burst_size = Some(1);
        caps.medium = Medium::Ethernet;
        caps
    }
}

impl RxToken for Bcm5720RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buf[..self.len])
    }
}

impl TxToken for Bcm5720TxToken {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut scratch = [0u8; FRAME_MAX];
        let len = len.min(scratch.len());
        let result = f(&mut scratch[..len]);
        let _ = bcm5720_mmio::transmit_frame(&scratch[..len]);
        result
    }
}
