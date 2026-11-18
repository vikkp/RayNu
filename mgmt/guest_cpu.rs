//! Saved vCPU count for a Guests row (ADR-023 B7).
//!
//! Pillar: [Z] · Outside the Proven Core.
//!
//! The sector, the route and the audit event land with the Guests page.
//! Until then a Start uses one vCPU.

/// Count the next Start of the current slot should boot. 1 until a row
/// saves a count.
pub fn armed() -> u8 {
    1
}
