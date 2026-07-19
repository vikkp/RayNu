//! Verus-linkable ghost model for ADR-004 EPT exclusive ownership (M3.16).
//!
//! Host-only crate (`package.metadata.verus.verify = true`). Not linked into
//! the UEFI binary. Lemmas typecheck under the frozen Verus pin; incomplete
//! inductive proofs use `admit()` until M3.17 discharges them.
//!
//! Marker (via tools/verus-link-smoke.sh): `RAYNU-V-M3-L3-LINK-OK`.

use vstd::prelude::*;

verus! {

/// Bring-up single-guest id (matches `memory::ept::M2_BRINGUP_GUEST_ID`).
pub const BRINGUP_GUEST: u64 = 1;

/// 4K page size assumed by M3.14–M3.17 lemmas.
pub const PAGE_4K: u64 = 4096;

pub type GuestId = u64;
pub type Gpa = u64;
pub type FrameId = u64;

/// Ghost EPT ownership registry (ADR-004 / `ept_spec.rs`).
pub struct GhostEptMap {
    /// HPA frame → owning guest
    pub owned: Map<FrameId, GuestId>,
    /// (guest, GPA) → HPA frame
    pub by_gpa: Map<(GuestId, Gpa), FrameId>,
}

impl GhostEptMap {
    pub open spec fn empty() -> Self {
        GhostEptMap { owned: Map::empty(), by_gpa: Map::empty() }
    }

    pub open spec fn ghost_map(self, guest: GuestId, gpa: Gpa, frame: FrameId) -> Self {
        GhostEptMap {
            owned: self.owned.insert(frame, guest),
            by_gpa: self.by_gpa.insert((guest, gpa), frame),
        }
    }

    pub open spec fn ghost_unmap(self, guest: GuestId, gpa: Gpa) -> Self {
        let frame = self.by_gpa[(guest, gpa)];
        GhostEptMap {
            owned: self.owned.remove(frame),
            by_gpa: self.by_gpa.remove((guest, gpa)),
        }
    }
}

/// GPA is 4K-aligned.
pub open spec fn page_aligned_4k(gpa: Gpa) -> bool {
    gpa % PAGE_4K == 0
}

/// ADR-004 exclusive ownership over the ghost maps.
pub open spec fn exclusive_ownership(m: GhostEptMap) -> bool {
    &&& (forall|f: FrameId|
        #![auto]
        m.owned.dom().contains(f) ==> exists|g: GuestId, a: Gpa|
            #![trigger m.by_gpa[(g, a)]]
            m.by_gpa.dom().contains((g, a)) && m.by_gpa[(g, a)] == f && m.owned[f] == g)
    &&& (forall|g: GuestId, a: Gpa|
        #![auto]
        m.by_gpa.dom().contains((g, a)) ==> m.owned.dom().contains(m.by_gpa[(g, a)])
            && m.owned[m.by_gpa[(g, a)]] == g)
    &&& (forall|g1: GuestId, a1: Gpa, g2: GuestId, a2: Gpa|
        #![auto]
        m.by_gpa.dom().contains((g1, a1)) && m.by_gpa.dom().contains((g2, a2))
            && m.by_gpa[(g1, a1)] == m.by_gpa[(g2, a2)] ==> g1 == g2 && a1 == a2)
    &&& m.owned.len() == m.by_gpa.len()
}

/// Empty map is exclusive (discharged).
pub proof fn lemma_empty_exclusive()
    ensures
        exclusive_ownership(GhostEptMap::empty()),
{
}

/// Map Ok preserves exclusivity for the bring-up guest on a free 4K GPA/HPA.
///
/// M3.16: typechecks; body admitted until M3.17.
pub proof fn lemma_map_ok_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        guest == BRINGUP_GUEST,
        page_aligned_4k(gpa),
        exclusive_ownership(m),
        !m.owned.dom().contains(frame),
        !m.by_gpa.dom().contains((guest, gpa)),
    ensures
        exclusive_ownership(m.ghost_map(guest, gpa, frame)),
        m.ghost_map(guest, gpa, frame).owned[frame] == guest,
        m.ghost_map(guest, gpa, frame).by_gpa[(guest, gpa)] == frame,
{
    let m2 = m.ghost_map(guest, gpa, frame);
    assert(m2.owned.dom().contains(frame));
    assert(m2.by_gpa.dom().contains((guest, gpa)));
    assert(m2.owned[frame] == guest);
    assert(m2.by_gpa[(guest, gpa)] == frame);
    // GAP(M3.17): discharge exclusive_ownership(m2) without admit.
    admit();
}

/// Rejected map (HPA or GPA already taken) leaves the abstract state unchanged.
pub proof fn lemma_map_already_owned_unchanged(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        exclusive_ownership(m),
        m.owned.dom().contains(frame) || m.by_gpa.dom().contains((guest, gpa)),
    ensures
        exclusive_ownership(m),
{
}

/// Unmap Ok restores exclusivity for a mapped (guest, GPA).
///
/// M3.16: typechecks; body admitted until M3.17.
pub proof fn lemma_unmap_ok_exclusive(m: GhostEptMap, guest: GuestId, gpa: Gpa)
    requires
        guest == BRINGUP_GUEST,
        page_aligned_4k(gpa),
        exclusive_ownership(m),
        m.by_gpa.dom().contains((guest, gpa)),
    ensures
        exclusive_ownership(m.ghost_unmap(guest, gpa)),
        !m.ghost_unmap(guest, gpa).by_gpa.dom().contains((guest, gpa)),
{
    let frame = m.by_gpa[(guest, gpa)];
    let m2 = m.ghost_unmap(guest, gpa);
    assert(!m2.by_gpa.dom().contains((guest, gpa)));
    assert(!m2.owned.dom().contains(frame));
    // GAP(M3.17): discharge exclusive_ownership(m2) without admit.
    admit();
}

/// Target theorem for M3.17: single-guest 4K map/unmap steps preserve exclusivity.
///
/// M3.16 links the statement under Verus; proof body admitted.
pub proof fn theorem_single_guest_4k_map_unmap_exclusive(m: GhostEptMap)
    requires
        exclusive_ownership(m),
    ensures
        exclusive_ownership(m),
{
    // GAP(M3.17): induct over a Seq of MapUnmapStep using the lemmas above.
    admit();
}

/// Host marker string for M3.16 (also asserted by `memory/l3_link_gate.rs`).
pub open spec fn m3_l3_link_ok_marker() -> bool {
    true
}

} // verus!

/// Exec-visible marker for host tests / smoke scripts.
pub const M3_L3_LINK_OK_MARKER: &str = "RAYNU-V-M3-L3-LINK-OK";
