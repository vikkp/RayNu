//! Health of the function-0 management listener while function 1 is up.
//!
//! Pillar: [Z] [D] [A]
//! Proven Core: **outside** (ADR-002).
//! VERIFICATION: L1 host tests. Host/CI never print an iron marker.
//!
//! The standing listener (`:38`) and the M8.9 guest egress (`:39`) are the
//! two PCI functions of one BCM5720. Function 1's arm is a full port
//! bring-up. This module decides whether function 0 is still serving from
//! two snapshots of its own MAC counters and rings. It does not touch MMIO.
//!
//! The coexist tick samples once a second. Three consecutive misses while
//! function 1 is armed are [`ListenerHealth::Stalled`]. The tick then
//! rebuilds function 0's rings and re-listens. A quiet port stays
//! [`ListenerHealth::Idle`] so a recover does not fire with nobody talking.

/// Misses required before [`ListenerHealth::Stalled`].
pub const LISTENER_STALL_STREAK: u8 = 3;
/// How often the coexist tick samples function 0.
pub const LISTENER_SAMPLE_MS: i64 = 1_000;
/// Quiet time after a rebuild before the next sample counts.
pub const LISTENER_RECOVER_HOLDOFF_MS: i64 = 8_000;
/// Rebuilds in one boot. A predicate that never clears stops here.
pub const LISTENER_RECOVER_CAP: u8 = 4;

/// Function 1's arm is a core reset of the shared BCM5720. Function 0's
/// rings were programmed before that reset. A quiet port is not a stall,
/// so the sampler does not rebuild. Rearm once, and not over a live session.
///
/// INVARIANTS:
/// - False once `already` is set
/// - False while a TCP session is active
/// - False until function 1 is armed
/// - Never panics
pub fn spa_rearm_after_egress(egress_armed: bool, already: bool, session_active: bool) -> bool {
    egress_armed && !already && !session_active
}

/// One read of function 0. Counters are cumulative; ring indices wrap at 32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListenerSample {
    pub mac_rx: u32,
    pub mac_tx: u32,
    pub rx_prod: u16,
    pub rx_cons: u16,
    pub tx_prod: u16,
    pub tx_cons: u16,
    pub rx_ok: u32,
}

/// What the last sample said about function 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerHealth {
    /// No baseline yet, the port is quiet, or a miss has not repeated.
    Idle,
    /// The rings moved with the MAC.
    Serving,
    /// Function 1 is armed and function 0 has missed for [`LISTENER_STALL_STREAK`] samples.
    Stalled,
}

/// Previous sample and the current miss streak.
#[derive(Clone, Copy, Debug)]
pub struct ListenerTracker {
    prev: Option<ListenerSample>,
    streak: u8,
}

impl ListenerTracker {
    pub const fn new() -> Self {
        Self {
            prev: None,
            streak: 0,
        }
    }

    /// Drop the baseline. Call after a rebuild so the next sample is quiet.
    pub fn reset(&mut self) {
        self.prev = None;
        self.streak = 0;
    }

    /// One observation.
    ///
    /// INVARIANTS:
    /// - The first sample is [`ListenerHealth::Idle`]
    /// - [`ListenerHealth::Stalled`] only when `egress_armed`
    /// - [`ListenerHealth::Idle`] and [`ListenerHealth::Serving`] clear the streak
    /// - [`ListenerHealth::Stalled`] only at [`LISTENER_STALL_STREAK`] consecutive misses
    /// - A sample with no MAC delta and no ring delta is [`ListenerHealth::Idle`]
    /// - Never panics
    pub fn observe(&mut self, sample: ListenerSample, egress_armed: bool) -> ListenerHealth {
        let Some(prev) = self.prev else {
            self.prev = Some(sample);
            self.streak = 0;
            return ListenerHealth::Idle;
        };
        self.prev = Some(sample);
        if !egress_armed {
            self.streak = 0;
            return if progressed(&prev, &sample) {
                ListenerHealth::Serving
            } else {
                ListenerHealth::Idle
            };
        }
        if progressed(&prev, &sample) {
            self.streak = 0;
            return ListenerHealth::Serving;
        }
        if quiet(&prev, &sample) {
            self.streak = 0;
            return ListenerHealth::Idle;
        }
        if missed(&prev, &sample) {
            self.streak = self.streak.saturating_add(1);
            if self.streak >= LISTENER_STALL_STREAK {
                self.streak = LISTENER_STALL_STREAK;
                return ListenerHealth::Stalled;
            }
            return ListenerHealth::Idle;
        }
        self.streak = 0;
        ListenerHealth::Idle
    }
}

fn progressed(prev: &ListenerSample, now: &ListenerSample) -> bool {
    now.rx_ok.wrapping_sub(prev.rx_ok) > 0
        || now.rx_prod != prev.rx_prod
        || now.rx_cons != prev.rx_cons
        || now.tx_cons != prev.tx_cons
        || now.mac_tx.wrapping_sub(prev.mac_tx) > 0
}

fn quiet(prev: &ListenerSample, now: &ListenerSample) -> bool {
    now.tx_prod == now.tx_cons
        && now.mac_rx == prev.mac_rx
        && now.mac_tx == prev.mac_tx
        && now.rx_prod == prev.rx_prod
        && now.rx_cons == prev.rx_cons
        && now.tx_prod == prev.tx_prod
        && now.tx_cons == prev.tx_cons
        && now.rx_ok == prev.rx_ok
}

/// MAC receive counted frames the return ring did not take, or a transmit
/// descriptor sat uncompleted with the MAC transmit counter flat.
fn missed(prev: &ListenerSample, now: &ListenerSample) -> bool {
    let rx_blind = now.mac_rx.wrapping_sub(prev.mac_rx) > 0
        && now.rx_ok == prev.rx_ok
        && now.rx_prod == prev.rx_prod;
    let tx_stuck = now.tx_prod != now.tx_cons
        && now.tx_prod == prev.tx_prod
        && now.tx_cons == prev.tx_cons
        && now.mac_tx == prev.mac_tx;
    rx_blind || tx_stuck
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mac_rx: u32, rx_prod: u16, rx_ok: u32) -> ListenerSample {
        ListenerSample {
            mac_rx,
            mac_tx: 0,
            rx_prod,
            rx_cons: 0,
            tx_prod: 0,
            tx_cons: 0,
            rx_ok,
        }
    }

    #[test]
    fn egress_reset_rearms_a_quiet_function_0_once() {
        assert!(!spa_rearm_after_egress(false, false, false));
        assert!(spa_rearm_after_egress(true, false, false));
        assert!(!spa_rearm_after_egress(true, true, false));
        assert!(!spa_rearm_after_egress(true, false, true));
    }

    #[test]
    fn first_sample_is_idle() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(0, 0, 0), true), ListenerHealth::Idle);
    }

    #[test]
    fn quiet_port_stays_idle() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(4, 1, 1), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(4, 1, 1), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(4, 1, 1), true), ListenerHealth::Idle);
    }

    #[test]
    fn rings_keeping_up_are_serving() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(10, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(11, 3, 3), true), ListenerHealth::Serving);
    }

    #[test]
    fn frozen_return_ring_while_the_mac_counts_stalls() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(10, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(15, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(20, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(25, 2, 2), true), ListenerHealth::Stalled);
    }

    #[test]
    fn the_same_miss_before_egress_is_not_a_stall() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(10, 2, 2), false), ListenerHealth::Idle);
        assert_eq!(t.observe(at(15, 2, 2), false), ListenerHealth::Idle);
        assert_eq!(t.observe(at(20, 2, 2), false), ListenerHealth::Idle);
        assert_eq!(t.observe(at(25, 2, 2), false), ListenerHealth::Idle);
    }

    #[test]
    fn transmit_consumer_stuck_behind_the_producer_stalls() {
        let mut t = ListenerTracker::new();
        let stuck = ListenerSample {
            mac_rx: 1,
            mac_tx: 4,
            rx_prod: 1,
            rx_cons: 1,
            tx_prod: 5,
            tx_cons: 2,
            rx_ok: 1,
        };
        assert_eq!(t.observe(stuck, true), ListenerHealth::Idle);
        assert_eq!(t.observe(stuck, true), ListenerHealth::Idle);
        assert_eq!(t.observe(stuck, true), ListenerHealth::Idle);
        assert_eq!(t.observe(stuck, true), ListenerHealth::Stalled);
    }

    #[test]
    fn a_served_frame_clears_the_streak() {
        let mut t = ListenerTracker::new();
        assert_eq!(t.observe(at(10, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(15, 2, 2), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(16, 3, 3), true), ListenerHealth::Serving);
        assert_eq!(t.observe(at(21, 3, 3), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(26, 3, 3), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(31, 3, 3), true), ListenerHealth::Stalled);
    }

    #[test]
    fn mac_counter_wrap_still_counts_as_a_miss() {
        let mut t = ListenerTracker::new();
        assert_eq!(
            t.observe(at(u32::MAX - 1, 4, 4), true),
            ListenerHealth::Idle
        );
        assert_eq!(t.observe(at(1, 4, 4), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(3, 4, 4), true), ListenerHealth::Idle);
        assert_eq!(t.observe(at(5, 4, 4), true), ListenerHealth::Stalled);
    }

    #[test]
    fn rebuild_touches_function_0_rings_only() {
        let src = include_str!("bcm5720_mmio.rs");
        let body = src
            .split_once("unsafe fn rebuild_spa_rings_uefi")
            .expect("rebuild")
            .1;
        let body = body
            .split_once("unsafe fn halt_mac_dma")
            .expect("end of rebuild")
            .0;
        assert!(body.contains("halt_mac_dma"));
        assert!(body.contains("init_rings"));
        assert!(body.contains("program_rings"));
        assert!(body.contains("enable_dma_engines"));
        assert!(!body.contains("chip_reset"));
        assert!(!body.contains("phy_write"));
        assert!(!body.contains("phy_read"));
        assert!(!body.contains("arm_egress"));
        assert!(!body.contains("CORECLK"));
        let listen = include_str!("host_nic_listen.rs");
        assert!(listen.contains("HOST-NIC :38 recover"));
        assert!(listen.contains("rebuild_spa_rings()"));
    }
}
