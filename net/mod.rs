//! Virtual switch, packet filtering, SR-IOV awareness.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

/// L2 learning switch stub (M4).
pub struct VSwitch {
    ports: u16,
}

impl VSwitch {
    pub const fn new(ports: u16) -> Self {
        Self { ports }
    }

    pub fn port_count(&self) -> u16 {
        self.ports
    }
}

#[cfg(test)]
#[path = "net_test.rs"]
mod net_test;
