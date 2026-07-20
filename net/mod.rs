//! Virtual switch and packet path (M4.4).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A
//!
//! Minimal L2 learning switch: MAC→port FDB, unicast forward / flood.
//! Packet buffers are host-owned (allocator frames), never guest-exclusive.

/// Max ports attached to the bring-up switch.
pub const MAX_PORTS: usize = 8;
/// FDB capacity (simple linear table).
const FDB_CAP: usize = 16;
/// Minimum Ethernet header size.
pub const ETH_HDR_LEN: usize = 14;

/// L2 learning switch for virtio-net ports.
pub struct VSwitch {
    /// Per-port MAC (None = unattached).
    port_mac: [Option<[u8; 6]>; MAX_PORTS],
    /// Learned (mac, port) entries; port = `u16::MAX` means empty.
    fdb_mac: [[u8; 6]; FDB_CAP],
    fdb_port: [u16; FDB_CAP],
    fdb_len: usize,
    ports: u16,
}

impl VSwitch {
    pub const fn new(ports: u16) -> Self {
        Self {
            port_mac: [None; MAX_PORTS],
            fdb_mac: [[0; 6]; FDB_CAP],
            fdb_port: [u16::MAX; FDB_CAP],
            fdb_len: 0,
            ports,
        }
    }

    pub fn port_count(&self) -> u16 {
        self.ports
    }

    /// Attach `port` with a station MAC (overwrites prior attachment).
    pub fn attach(&mut self, port: u16, mac: [u8; 6]) -> Result<(), ()> {
        if port as usize >= MAX_PORTS || port >= self.ports {
            return Err(());
        }
        self.port_mac[port as usize] = Some(mac);
        self.learn(mac, port);
        Ok(())
    }

    /// Learn / refresh `mac` → `port`.
    pub fn learn(&mut self, mac: [u8; 6], port: u16) {
        for i in 0..self.fdb_len {
            if self.fdb_mac[i] == mac {
                self.fdb_port[i] = port;
                return;
            }
        }
        if self.fdb_len < FDB_CAP {
            self.fdb_mac[self.fdb_len] = mac;
            self.fdb_port[self.fdb_len] = port;
            self.fdb_len += 1;
        }
    }

    /// Lookup destination port for `mac` (None → flood).
    pub fn lookup(&self, mac: &[u8; 6]) -> Option<u16> {
        for i in 0..self.fdb_len {
            if &self.fdb_mac[i] == mac {
                let p = self.fdb_port[i];
                if p != u16::MAX {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Forward `frame` from `src_port`. Returns destination port on unicast hit,
    /// or `None` if flooded / dropped. Caller delivers bytes to the dst buffer.
    ///
    /// Learns the source MAC from the Ethernet header.
    pub fn forward(&mut self, src_port: u16, frame: &[u8]) -> Result<Option<u16>, ()> {
        if frame.len() < ETH_HDR_LEN {
            return Err(());
        }
        if src_port as usize >= MAX_PORTS || self.port_mac[src_port as usize].is_none() {
            return Err(());
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&frame[0..6]);
        src.copy_from_slice(&frame[6..12]);
        self.learn(src, src_port);

        // Broadcast / multicast → flood (return None; caller may skip for MV).
        if dst[0] & 1 != 0 {
            return Ok(None);
        }
        match self.lookup(&dst) {
            Some(p) if p != src_port => Ok(Some(p)),
            Some(_) => Ok(None), // hairpin
            None => Ok(None),    // unknown → flood (MV: no deliver)
        }
    }
}

/// Build a minimal Ethernet frame: dst|src|ethertype|payload.
pub fn build_eth_frame(
    out: &mut [u8],
    dst: &[u8; 6],
    src: &[u8; 6],
    ethertype: u16,
    payload: &[u8],
) -> Result<usize, ()> {
    let need = ETH_HDR_LEN.checked_add(payload.len()).ok_or(())?;
    if out.len() < need {
        return Err(());
    }
    out[0..6].copy_from_slice(dst);
    out[6..12].copy_from_slice(src);
    out[12] = (ethertype >> 8) as u8;
    out[13] = ethertype as u8;
    out[ETH_HDR_LEN..need].copy_from_slice(payload);
    Ok(need)
}

#[cfg(test)]
#[path = "net_test.rs"]
mod net_test;
