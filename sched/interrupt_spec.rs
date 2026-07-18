//! Verus specifications for interrupt injection.
//!
//! VERIFICATION: L1 posts (M2.4) — L2/L3 still TODO.
//!
//! # `validate_vector`
//! requires
//!   - (none)
//! ensures
//!   - on `Ok(v)`: `v == vector as u8` and `vector <= 255`
//!   - on `Err(InvalidVector)`: `vector > 255`
//!
//! # `prepare_external_inject`
//! ensures
//!   - on `Ok(info)`: vector bits match, type = external, valid bit set
//!   - routes through `validate_vector` (no inject of out-of-range vectors)
//!
//! TODO(M2 L2): ghost pending-event queue; Kani on pack/unpack.

#![allow(dead_code)]
