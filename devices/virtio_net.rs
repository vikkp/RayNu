//! Minimal virtio-mmio network device + dual-port exchange (M4.4).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Two virtio-net MMIO BARs attached to a host [`crate::net::VSwitch`].
//! When both ports reach `DRIVER_OK`, the host injects an Ethernet frame
//! port0→port1 through the learning switch into host-owned RX buffers and
//! latches [`M4_NET_OK_MARKER`]. Packet buffers stay in the FrameAllocator
//! pool (not guest-exclusive precise ranges).

use crate::net::{self, VSwitch, ETH_HDR_LEN};

/// COM1 / gate marker when dual-port vSwitch exchange succeeds.
pub const M4_NET_OK_MARKER: &str = "RAYNU-V-M4-NET-OK";

/// Virtio-mmio magic ("virt").
pub const VIRTIO_MMIO_MAGIC: u32 = 0x7472_6976;
pub const VIRTIO_MMIO_VERSION: u32 = 2;
/// Virtio network device id.
pub const VIRTIO_ID_NET: u32 = 1;
pub const VIRTIO_VENDOR: u32 = 0x5241_594E; // "RAYN"

pub const STATUS_ACKNOWLEDGE: u32 = 1;
pub const STATUS_DRIVER: u32 = 2;
pub const STATUS_DRIVER_OK: u32 = 4;
pub const STATUS_FEATURES_OK: u32 = 8;

pub const OFF_MAGIC: u64 = 0x00;
pub const OFF_VERSION: u64 = 0x04;
pub const OFF_DEVICE_ID: u64 = 0x08;
pub const OFF_VENDOR_ID: u64 = 0x0c;
pub const OFF_STATUS: u64 = 0x70;

/// Custom ethertype for the M4.4 probe payload.
pub const PROBE_ETHERTYPE: u16 = 0x88B5;
/// Probe payload pattern.
pub(crate) const PROBE_PAYLOAD: &[u8] = b"RAYNU-V-M4-NET";

const MAC0: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
const MAC1: [u8; 6] = [0x52, 0x54, 0x00, 0x12, 0x34, 0x57];

static mut BAR0: u64 = 0;
static mut BAR1: u64 = 0;
static mut BUF0: u64 = 0;
static mut BUF1: u64 = 0;
static mut STATUS0: u32 = 0;
static mut STATUS1: u32 = 0;
static mut READY0: bool = false;
static mut READY1: bool = false;
static mut NET_OK: bool = false;
static mut NET_MARKED: bool = false;
static mut SWITCH: VSwitch = VSwitch::new(2);

/// True after a successful port0→port1 exchange.
pub fn net_ok() -> bool {
    unsafe { NET_OK }
}

/// Take-once COM1 emit helper.
pub fn take_net_ok_latch() -> bool {
    unsafe {
        if NET_OK && !NET_MARKED {
            NET_MARKED = true;
            true
        } else {
            false
        }
    }
}

pub fn bar0() -> u64 {
    unsafe { BAR0 }
}

pub fn bar1() -> u64 {
    unsafe { BAR1 }
}

pub fn bar_contains(gpa: u64) -> bool {
    let (b0, b1) = unsafe { (BAR0, BAR1) };
    (b0 != 0 && (b0..b0 + 0x1000).contains(&gpa))
        || (b1 != 0 && (b1..b1 + 0x1000).contains(&gpa))
}

/// Install two MMIO BARs + host-owned RX buffers; attach ports to the vSwitch.
///
/// `bar0`/`bar1` must be EPT-unmapped. `buf0`/`buf1` are 4 KiB identity-mapped
/// host frames for RX verification (allocator-owned).
///
/// SAFETY: buffers writable for 4096 bytes each.
pub unsafe fn init(bar0: u64, bar1: u64, buf0: u64, buf1: u64) {
    BAR0 = bar0;
    BAR1 = bar1;
    BUF0 = buf0;
    BUF1 = buf1;
    STATUS0 = 0;
    STATUS1 = 0;
    READY0 = false;
    READY1 = false;
    NET_OK = false;
    NET_MARKED = false;
    core::ptr::write_bytes(buf0 as *mut u8, 0, 4096);
    core::ptr::write_bytes(buf1 as *mut u8, 0, 4096);
    SWITCH = VSwitch::new(2);
    // SAFETY: single-threaded boot / VMX root device model.
    let sw = core::ptr::addr_of_mut!(SWITCH);
    let _ = (*sw).attach(0, MAC0);
    let _ = (*sw).attach(1, MAC1);
}

/// Handle a 32-bit MMIO access at `gpa`.
pub fn mmio_access(gpa: u64, is_write: bool, write_val: u32) -> Option<Option<u32>> {
    if !bar_contains(gpa) {
        return None;
    }
    let (b0, b1) = unsafe { (BAR0, BAR1) };
    let (base, port) = if (b0..b0 + 0x1000).contains(&gpa) {
        (b0, 0u8)
    } else {
        (b1, 1u8)
    };
    let off = gpa - base;
    if is_write {
        if off == OFF_STATUS {
            unsafe {
                if port == 0 {
                    STATUS0 = write_val;
                    if write_val & STATUS_DRIVER_OK != 0 {
                        READY0 = true;
                    }
                } else {
                    STATUS1 = write_val;
                    if write_val & STATUS_DRIVER_OK != 0 {
                        READY1 = true;
                    }
                }
                if READY0 && READY1 {
                    run_port_exchange();
                }
            }
        }
        Some(None)
    } else {
        let status = unsafe {
            if port == 0 {
                STATUS0
            } else {
                STATUS1
            }
        };
        let v = match off {
            OFF_MAGIC => VIRTIO_MMIO_MAGIC,
            OFF_VERSION => VIRTIO_MMIO_VERSION,
            OFF_DEVICE_ID => VIRTIO_ID_NET,
            OFF_VENDOR_ID => VIRTIO_VENDOR,
            OFF_STATUS => status,
            _ => 0,
        };
        Some(Some(v))
    }
}

unsafe fn run_port_exchange() {
    if BUF0 == 0 || BUF1 == 0 || NET_OK {
        return;
    }
    let mut frame = [0u8; 64];
    let n = match net::build_eth_frame(&mut frame, &MAC1, &MAC0, PROBE_ETHERTYPE, PROBE_PAYLOAD) {
        Ok(n) => n,
        Err(()) => return,
    };
    let dst = match (*core::ptr::addr_of_mut!(SWITCH)).forward(0, &frame[..n]) {
        Ok(Some(1)) => 1u16,
        _ => return,
    };
    debug_assert_eq!(dst, 1);
    // Deliver into port1 RX buffer (host-owned).
    let rx = BUF1 as *mut u8;
    core::ptr::write_bytes(rx, 0, 4096);
    core::ptr::copy_nonoverlapping(frame.as_ptr(), rx, n);
    // Verify Ethernet header + payload.
    let mut ok = true;
    for i in 0..6 {
        if core::ptr::read_volatile(rx.add(i)) != MAC1[i] {
            ok = false;
        }
        if core::ptr::read_volatile(rx.add(6 + i)) != MAC0[i] {
            ok = false;
        }
    }
    if core::ptr::read_volatile(rx.add(12)) != (PROBE_ETHERTYPE >> 8) as u8
        || core::ptr::read_volatile(rx.add(13)) != PROBE_ETHERTYPE as u8
    {
        ok = false;
    }
    for (i, &b) in PROBE_PAYLOAD.iter().enumerate() {
        if core::ptr::read_volatile(rx.add(ETH_HDR_LEN + i)) != b {
            ok = false;
            break;
        }
    }
    if ok {
        NET_OK = true;
    }
}

/// Host-only selftest (no guest).
#[cfg(test)]
pub unsafe fn host_selftest(buf0: u64, buf1: u64) -> bool {
    init(0x1000_0000, 0x1000_1000, buf0, buf1);
    STATUS0 = STATUS_DRIVER_OK;
    READY0 = true;
    STATUS1 = STATUS_DRIVER_OK;
    READY1 = true;
    run_port_exchange();
    NET_OK
}

#[cfg(test)]
#[path = "virtio_net_test.rs"]
mod virtio_net_test;
