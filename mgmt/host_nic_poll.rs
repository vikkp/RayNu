//! Bounded poll budget for the host-owned mgmt NIC (ADR-013 Phase C).
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! Poll-mode MVP: each scheduler quantum (or listen tick) may run at most
//! [`HOST_NIC_POLL_BUDGET`] PHY/TCP polls, then yield. Host tests prove the
//! cap without MMIO.

/// Max NIC/smoltcp poll iterations per yield (credit-scheduler adjacent).
pub const HOST_NIC_POLL_BUDGET: u32 = 32;

/// Outcome of one bounded poll slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedPollResult {
    pub iterations: u32,
    pub exhausted_budget: bool,
}

/// Run `poll` at most `budget` times (capped at [`HOST_NIC_POLL_BUDGET`]).
///
/// `poll` returns `true` when more work may be pending. Stops early on `false`.
///
/// INVARIANTS:
/// - `iterations <= budget.min(HOST_NIC_POLL_BUDGET)` (and 0 if budget is 0)
/// - `exhausted_budget` is true only when every slot ran and each returned true
pub fn bounded_poll<F: FnMut() -> bool>(budget: u32, mut poll: F) -> BoundedPollResult {
    let cap = if budget == 0 {
        0
    } else {
        budget.min(HOST_NIC_POLL_BUDGET)
    };
    let mut n = 0u32;
    while n < cap {
        n += 1;
        if !poll() {
            return BoundedPollResult {
                iterations: n,
                exhausted_budget: false,
            };
        }
    }
    BoundedPollResult {
        iterations: n,
        exhausted_budget: cap > 0 && n == cap,
    }
}

/// Package prop: budget is honored when work never yields.
pub fn prop_bounded_poll_respects_budget() -> bool {
    let r = bounded_poll(8, || true);
    r.iterations == 8
        && r.exhausted_budget
        && bounded_poll(0, || true).iterations == 0
        && bounded_poll(HOST_NIC_POLL_BUDGET.saturating_add(100), || true).iterations
            == HOST_NIC_POLL_BUDGET
}

#[cfg(test)]
#[path = "host_nic_poll_test.rs"]
mod host_nic_poll_test;
