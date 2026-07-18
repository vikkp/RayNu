//! Hypercall interface — sole intentional guest→host channel.
//!
//! Pillar: [V] · Proven Core · VERIFICATION: L0

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypercallError {
    UnknownNr,
    InvalidArg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HypercallNr {
    Nop = 0,
    // Future: balloon, debug, etc. — each validated explicitly.
}

/// INVARIANTS:
///   - Unknown numbers are rejected (no fall-through to host memory)
///   - Arguments validated before any host-side effect
///
/// VERIFICATION: L0 — see hypercall_spec.rs
pub fn dispatch(nr: u64, arg0: u64) -> Result<u64, HypercallError> {
    match nr {
        0 => {
            let _ = arg0;
            Ok(0)
        }
        _ => Err(HypercallError::UnknownNr),
    }
}

#[cfg(test)]
#[path = "hypercall_test.rs"]
mod hypercall_test;
