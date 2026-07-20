//! Verus-verified ghost model for ADR-004 EPT exclusive ownership (M3.17–M3.18 / M4.6–M4.9).
//!
//! Host-only crate (`package.metadata.verus.verify = true`). Not linked into
//! the UEFI binary. Under the frozen Verus pin, exclusivity lemmas for 4K
//! map/unmap are discharged with **no `admit()`**. M3.18 adds ghost↔exec
//! refinement. M4.6 extends `MapUnmapStep` with an explicit `guest` field;
//! M4.7 claims ADR-006 L3 for N-guest 4K map/unmap exclusivity
//! (`theorem_n_guest_4k_map_unmap_exclusive` / `lemma_two_guests_map_distinct_frames_exclusive`).
//! M4.8 adds 2M/1G leaf sizes and span predicates to the ghost *spec*
//! (`GhostPageSize` / `large_map_enabled`); large-page L3 discharge remains M5.
//! M4.9 extends concrete refine to N guests (`theorem_concrete_n_guest_4k_refine`).
//!
//! Markers:
//! - M3.16 link: `RAYNU-V-M3-L3-LINK-OK`
//! - M3.17 true L3: `RAYNU-V-M3-L3-VERIFY-OK` (via tools/verus-verify-smoke.sh)
//! - M3.18 refine: `RAYNU-V-M3-L3-REFINE-OK` (via tools/verus-refine-smoke.sh)
//! - M4.6 N-guest spec: `RAYNU-V-M4-NGUEST-SPEC-OK` (via tools/verus-nguest-spec-smoke.sh)
//! - M4.7 N-guest L3: `RAYNU-V-M4-NGUEST-VERIFY-OK` (via tools/verus-nguest-verify-smoke.sh)
//! - M4.8 large-page spec: `RAYNU-V-M4-LPAGE-OK` (via tools/verus-lpage-spec-smoke.sh)
//! - M4.9 N-guest refine: `RAYNU-V-M4-REFINE-OK` (via tools/verus-nguest-refine-smoke.sh)

use vstd::prelude::*;

verus! {

broadcast use {
    vstd::map::group_map_lemmas,
    vstd::set::group_set_lemmas,
};

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

/// One map or unmap step for a non-zero guest (4K). M4.6: `guest` is explicit.
pub enum MapUnmapStep {
    Map { guest: GuestId, gpa: Gpa, frame: FrameId },
    Unmap { guest: GuestId, gpa: Gpa },
}

pub open spec fn step_guest_id(step: MapUnmapStep) -> GuestId {
    match step {
        MapUnmapStep::Map { guest, gpa: _, frame: _ } => guest,
        MapUnmapStep::Unmap { guest, gpa: _ } => guest,
    }
}

pub open spec fn step_guest_ok(step: MapUnmapStep) -> bool {
    match step {
        MapUnmapStep::Map { guest, gpa, frame: _ } => guest != 0 && page_aligned_4k(gpa),
        MapUnmapStep::Unmap { guest, gpa } => guest != 0 && page_aligned_4k(gpa),
    }
}

pub open spec fn step_enabled(m: GhostEptMap, step: MapUnmapStep) -> bool {
    match step {
        MapUnmapStep::Map { guest, gpa, frame } =>
            !m.owned.dom().contains(frame) && !m.by_gpa.dom().contains((guest, gpa)),
        MapUnmapStep::Unmap { guest, gpa } => m.by_gpa.dom().contains((guest, gpa)),
    }
}

pub open spec fn apply_step(m: GhostEptMap, step: MapUnmapStep) -> GhostEptMap {
    match step {
        MapUnmapStep::Map { guest, gpa, frame } => m.ghost_map(guest, gpa, frame),
        MapUnmapStep::Unmap { guest, gpa } => m.ghost_unmap(guest, gpa),
    }
}

pub open spec fn fold_steps(m: GhostEptMap, steps: Seq<MapUnmapStep>) -> GhostEptMap
    decreases steps.len(),
{
    if steps.len() == 0 {
        m
    } else {
        fold_steps(apply_step(m, steps[0]), steps.skip(1))
    }
}

pub open spec fn steps_ok(m: GhostEptMap, steps: Seq<MapUnmapStep>) -> bool
    decreases steps.len(),
{
    if steps.len() == 0 {
        true
    } else {
        &&& step_guest_ok(steps[0])
        &&& step_enabled(m, steps[0])
        &&& steps_ok(apply_step(m, steps[0]), steps.skip(1))
    }
}

/// Empty map is exclusive.
pub proof fn lemma_empty_exclusive()
    ensures
        exclusive_ownership(GhostEptMap::empty()),
{
}

/// Every by_gpa entry has a matching owned entry.
pub proof fn lemma_by_gpa_owned(m: GhostEptMap, g: GuestId, a: Gpa)
    requires
        exclusive_ownership(m),
        m.by_gpa.dom().contains((g, a)),
    ensures
        m.owned.dom().contains(m.by_gpa[(g, a)]),
        m.owned[m.by_gpa[(g, a)]] == g,
{
}

/// Owned frames are injective under by_gpa keys.
pub proof fn lemma_by_gpa_unique_frame(
    m: GhostEptMap,
    g1: GuestId,
    a1: Gpa,
    g2: GuestId,
    a2: Gpa,
)
    requires
        exclusive_ownership(m),
        m.by_gpa.dom().contains((g1, a1)),
        m.by_gpa.dom().contains((g2, a2)),
        m.by_gpa[(g1, a1)] == m.by_gpa[(g2, a2)],
    ensures
        g1 == g2,
        a1 == a2,
{
}

/// No by_gpa entry can point at a free frame.
pub proof fn lemma_free_frame_unmapped(m: GhostEptMap, frame: FrameId, g: GuestId, a: Gpa)
    requires
        exclusive_ownership(m),
        !m.owned.dom().contains(frame),
        m.by_gpa.dom().contains((g, a)),
    ensures
        m.by_gpa[(g, a)] != frame,
{
    lemma_by_gpa_owned(m, g, a);
}

/// Map Ok preserves exclusivity for any non-zero guest on a free 4K GPA/HPA (M4.6).
pub proof fn lemma_map_ok_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        guest != 0,
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

    assert(m2.owned.dom() == m.owned.dom().insert(frame));
    assert(m2.by_gpa.dom() == m.by_gpa.dom().insert((guest, gpa)));
    assert(m2.owned[frame] == guest);
    assert(m2.by_gpa[(guest, gpa)] == frame);
    assert(m2.owned.len() == m.owned.len() + 1);
    assert(m2.by_gpa.len() == m.by_gpa.len() + 1);

    assert forall|f: FrameId|
        m2.owned.dom().contains(f) implies exists|g: GuestId, a: Gpa|
            #![trigger m2.by_gpa[(g, a)]]
            m2.by_gpa.dom().contains((g, a)) && m2.by_gpa[(g, a)] == f && m2.owned[f] == g
    by {
        if f == frame {
            assert(m2.by_gpa.dom().contains((guest, gpa)));
            assert(m2.by_gpa[(guest, gpa)] == f);
            assert(m2.owned[f] == guest);
        } else {
            assert(m.owned.dom().contains(f));
            assert(exists|g0: GuestId, a0: Gpa|
                #![trigger m.by_gpa[(g0, a0)]]
                m.by_gpa.dom().contains((g0, a0)) && m.by_gpa[(g0, a0)] == f && m.owned[f]
                    == g0);
            let (g0, a0): (GuestId, Gpa) = choose|g0: GuestId, a0: Gpa|
                #![trigger m.by_gpa[(g0, a0)]]
                m.by_gpa.dom().contains((g0, a0)) && m.by_gpa[(g0, a0)] == f && m.owned[f]
                    == g0;
            assert(m.by_gpa.dom().contains((g0, a0)));
            assert(m.by_gpa[(g0, a0)] == f);
            assert(m.owned[f] == g0);
            lemma_free_frame_unmapped(m, frame, g0, a0);
            assert((g0, a0) != (guest, gpa));
            assert(m2.by_gpa.dom().contains((g0, a0)));
            assert(m2.by_gpa[(g0, a0)] == f);
            assert(m2.owned[f] == g0);
        }
    };

    assert forall|g: GuestId, a: Gpa|
        #![auto]
        m2.by_gpa.dom().contains((g, a)) implies m2.owned.dom().contains(m2.by_gpa[(g, a)])
            && m2.owned[m2.by_gpa[(g, a)]] == g
    by {
        if g == guest && a == gpa {
            assert(m2.by_gpa[(g, a)] == frame);
            assert(m2.owned.dom().contains(frame));
            assert(m2.owned[frame] == guest);
        } else {
            assert(m.by_gpa.dom().contains((g, a)));
            lemma_by_gpa_owned(m, g, a);
            let fr = m.by_gpa[(g, a)];
            lemma_free_frame_unmapped(m, frame, g, a);
            assert(fr != frame);
            assert(m2.by_gpa[(g, a)] == fr);
            assert(m2.owned.dom().contains(fr));
            assert(m2.owned[fr] == g);
        }
    };

    assert forall|g1: GuestId, a1: Gpa, g2: GuestId, a2: Gpa|
        #![auto]
        m2.by_gpa.dom().contains((g1, a1)) && m2.by_gpa.dom().contains((g2, a2))
            && m2.by_gpa[(g1, a1)] == m2.by_gpa[(g2, a2)] implies g1 == g2 && a1 == a2
    by {
        let f1 = m2.by_gpa[(g1, a1)];
        if f1 == frame {
            if (g1, a1) != (guest, gpa) {
                assert(m.by_gpa.dom().contains((g1, a1)));
                lemma_free_frame_unmapped(m, frame, g1, a1);
            }
            if (g2, a2) != (guest, gpa) {
                assert(m.by_gpa.dom().contains((g2, a2)));
                lemma_free_frame_unmapped(m, frame, g2, a2);
            }
            assert((g1, a1) == (guest, gpa));
            assert((g2, a2) == (guest, gpa));
        } else {
            assert((g1, a1) != (guest, gpa));
            assert((g2, a2) != (guest, gpa));
            assert(m.by_gpa.dom().contains((g1, a1)));
            assert(m.by_gpa.dom().contains((g2, a2)));
            assert(m.by_gpa[(g1, a1)] == m.by_gpa[(g2, a2)]);
            lemma_by_gpa_unique_frame(m, g1, a1, g2, a2);
        }
    };
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

/// Unmap Ok restores exclusivity for a mapped (guest, GPA) (any non-zero guest; M4.6).
pub proof fn lemma_unmap_ok_exclusive(m: GhostEptMap, guest: GuestId, gpa: Gpa)
    requires
        guest != 0,
        page_aligned_4k(gpa),
        exclusive_ownership(m),
        m.by_gpa.dom().contains((guest, gpa)),
    ensures
        exclusive_ownership(m.ghost_unmap(guest, gpa)),
        !m.ghost_unmap(guest, gpa).by_gpa.dom().contains((guest, gpa)),
{
    let frame = m.by_gpa[(guest, gpa)];
    let m2 = m.ghost_unmap(guest, gpa);

    lemma_by_gpa_owned(m, guest, gpa);
    assert(m.owned.dom().contains(frame));
    assert(m.owned[frame] == guest);
    assert(m2.owned.dom() == m.owned.dom().remove(frame));
    assert(m2.by_gpa.dom() == m.by_gpa.dom().remove((guest, gpa)));
    assert(!m2.by_gpa.dom().contains((guest, gpa)));
    assert(!m2.owned.dom().contains(frame));
    assert(m2.owned.len() + 1 == m.owned.len());
    assert(m2.by_gpa.len() + 1 == m.by_gpa.len());

    assert forall|f: FrameId|
        m2.owned.dom().contains(f) implies exists|g: GuestId, a: Gpa|
            #![trigger m2.by_gpa[(g, a)]]
            m2.by_gpa.dom().contains((g, a)) && m2.by_gpa[(g, a)] == f && m2.owned[f] == g
    by {
        assert(m.owned.dom().contains(f));
        assert(f != frame);
        assert(exists|g0: GuestId, a0: Gpa|
            #![trigger m.by_gpa[(g0, a0)]]
            m.by_gpa.dom().contains((g0, a0)) && m.by_gpa[(g0, a0)] == f && m.owned[f] == g0);
        let (g0, a0): (GuestId, Gpa) = choose|g0: GuestId, a0: Gpa|
            #![trigger m.by_gpa[(g0, a0)]]
            m.by_gpa.dom().contains((g0, a0)) && m.by_gpa[(g0, a0)] == f && m.owned[f] == g0;
        assert(m.by_gpa.dom().contains((g0, a0)));
        assert(m.by_gpa[(g0, a0)] == f);
        assert(m.owned[f] == g0);
        assert(m.by_gpa[(g0, a0)] != frame);
        if (g0, a0) == (guest, gpa) {
            assert(m.by_gpa[(g0, a0)] == frame);
        }
        assert((g0, a0) != (guest, gpa));
        assert(m2.by_gpa.dom().contains((g0, a0)));
        assert(m2.by_gpa[(g0, a0)] == f);
        assert(m2.owned[f] == g0);
    };

    assert forall|g: GuestId, a: Gpa|
        #![auto]
        m2.by_gpa.dom().contains((g, a)) implies m2.owned.dom().contains(m2.by_gpa[(g, a)])
            && m2.owned[m2.by_gpa[(g, a)]] == g
    by {
        assert(m.by_gpa.dom().contains((g, a)));
        assert((g, a) != (guest, gpa));
        lemma_by_gpa_owned(m, g, a);
        let fr = m.by_gpa[(g, a)];
        // Uniqueness: only (guest, gpa) maps to `frame`.
        if fr == frame {
            lemma_by_gpa_unique_frame(m, g, a, guest, gpa);
        }
        assert(fr != frame);
        assert(m2.by_gpa[(g, a)] == fr);
        assert(m2.owned.dom().contains(fr));
        assert(m2.owned[fr] == g);
    };

    assert forall|g1: GuestId, a1: Gpa, g2: GuestId, a2: Gpa|
        #![auto]
        m2.by_gpa.dom().contains((g1, a1)) && m2.by_gpa.dom().contains((g2, a2))
            && m2.by_gpa[(g1, a1)] == m2.by_gpa[(g2, a2)] implies g1 == g2 && a1 == a2
    by {
        assert(m.by_gpa.dom().contains((g1, a1)));
        assert(m.by_gpa.dom().contains((g2, a2)));
        assert(m.by_gpa[(g1, a1)] == m.by_gpa[(g2, a2)]);
        lemma_by_gpa_unique_frame(m, g1, a1, g2, a2);
    };
}

/// Applying one enabled step preserves exclusivity.
pub proof fn lemma_apply_step_exclusive(m: GhostEptMap, step: MapUnmapStep)
    requires
        exclusive_ownership(m),
        step_guest_ok(step),
        step_enabled(m, step),
    ensures
        exclusive_ownership(apply_step(m, step)),
{
    match step {
        MapUnmapStep::Map { guest, gpa, frame } => {
            lemma_map_ok_exclusive(m, guest, gpa, frame);
        },
        MapUnmapStep::Unmap { guest, gpa } => {
            lemma_unmap_ok_exclusive(m, guest, gpa);
        },
    }
}

/// Historical M3.17 name: any finite sequence of enabled 4K map/unmap steps
/// (each step's `guest != 0`) preserves exclusive ownership. Guest ids may
/// differ across steps — see `theorem_n_guest_4k_map_unmap_exclusive`.
pub proof fn theorem_single_guest_4k_map_unmap_exclusive(
    m: GhostEptMap,
    steps: Seq<MapUnmapStep>,
)
    requires
        exclusive_ownership(m),
        steps_ok(m, steps),
    ensures
        exclusive_ownership(fold_steps(m, steps)),
    decreases steps.len(),
{
    if steps.len() == 0 {
    } else {
        lemma_apply_step_exclusive(m, steps[0]);
        theorem_single_guest_4k_map_unmap_exclusive(
            apply_step(m, steps[0]),
            steps.skip(1),
        );
    }
}

/// M4.6/M4.7: N-guest 4K map/unmap exclusivity in the ghost model.
/// Same discharge as the historical single-guest theorem; named so host/CI
/// can assert the N-guest post. ADR-006 L3 claim → `RAYNU-V-M4-NGUEST-VERIFY-OK`.
pub proof fn theorem_n_guest_4k_map_unmap_exclusive(
    m: GhostEptMap,
    steps: Seq<MapUnmapStep>,
)
    requires
        exclusive_ownership(m),
        steps_ok(m, steps),
    ensures
        exclusive_ownership(fold_steps(m, steps)),
{
    theorem_single_guest_4k_map_unmap_exclusive(m, steps);
}

/// M4.7: two distinct non-zero guests mapping distinct free 4K frames
/// preserves exclusive ownership (concrete ≥2-guest post for ADR-006).
pub proof fn lemma_two_guests_map_distinct_frames_exclusive(
    m: GhostEptMap,
    g1: GuestId,
    gpa1: Gpa,
    f1: FrameId,
    g2: GuestId,
    gpa2: Gpa,
    f2: FrameId,
)
    requires
        exclusive_ownership(m),
        g1 != 0,
        g2 != 0,
        g1 != g2,
        page_aligned_4k(gpa1),
        page_aligned_4k(gpa2),
        f1 != f2,
        !m.owned.dom().contains(f1),
        !m.owned.dom().contains(f2),
        !m.by_gpa.dom().contains((g1, gpa1)),
        !m.by_gpa.dom().contains((g2, gpa2)),
    ensures
        exclusive_ownership(m.ghost_map(g1, gpa1, f1).ghost_map(g2, gpa2, f2)),
        m.ghost_map(g1, gpa1, f1).ghost_map(g2, gpa2, f2).owned[f1] == g1,
        m.ghost_map(g1, gpa1, f1).ghost_map(g2, gpa2, f2).owned[f2] == g2,
{
    lemma_map_ok_exclusive(m, g1, gpa1, f1);
    let m1 = m.ghost_map(g1, gpa1, f1);
    assert(!m1.owned.dom().contains(f2));
    assert(!m1.by_gpa.dom().contains((g2, gpa2)));
    lemma_map_ok_exclusive(m1, g2, gpa2, f2);
}

// ---------------------------------------------------------------------------
// M3.18 — ghost↔exec refinement
//
// `ConcreteEptMap` is the ownership content of live `memory::ept::EptMap`
// after collecting occupied slots (permissions / slot indices elided).
// `abs` projects to the verified ghost model; `refines` = exclusive ownership
// of that projection. Concrete map/unmap Ok steps commute with ghost steps.
// ---------------------------------------------------------------------------

/// Concrete ownership view of the live EPT registry (slot array abstracted).
pub struct ConcreteEptMap {
    pub owned: Map<FrameId, GuestId>,
    pub by_gpa: Map<(GuestId, Gpa), FrameId>,
}

impl ConcreteEptMap {
    pub open spec fn empty() -> Self {
        ConcreteEptMap { owned: Map::empty(), by_gpa: Map::empty() }
    }

    pub open spec fn concrete_map(self, guest: GuestId, gpa: Gpa, frame: FrameId) -> Self {
        ConcreteEptMap {
            owned: self.owned.insert(frame, guest),
            by_gpa: self.by_gpa.insert((guest, gpa), frame),
        }
    }

    pub open spec fn concrete_unmap(self, guest: GuestId, gpa: Gpa) -> Self {
        let frame = self.by_gpa[(guest, gpa)];
        ConcreteEptMap {
            owned: self.owned.remove(frame),
            by_gpa: self.by_gpa.remove((guest, gpa)),
        }
    }
}

/// Abstraction function: concrete ownership view → verified ghost model.
pub open spec fn abs(c: ConcreteEptMap) -> GhostEptMap {
    GhostEptMap { owned: c.owned, by_gpa: c.by_gpa }
}

/// Concrete state refines the L3 ghost exclusivity invariant.
pub open spec fn refines(c: ConcreteEptMap) -> bool {
    exclusive_ownership(abs(c))
}

pub open spec fn concrete_step_enabled(c: ConcreteEptMap, step: MapUnmapStep) -> bool {
    step_enabled(abs(c), step)
}

pub open spec fn apply_concrete_step(c: ConcreteEptMap, step: MapUnmapStep) -> ConcreteEptMap {
    match step {
        MapUnmapStep::Map { guest, gpa, frame } => c.concrete_map(guest, gpa, frame),
        MapUnmapStep::Unmap { guest, gpa } => c.concrete_unmap(guest, gpa),
    }
}

pub open spec fn fold_concrete_steps(c: ConcreteEptMap, steps: Seq<MapUnmapStep>) -> ConcreteEptMap
    decreases steps.len(),
{
    if steps.len() == 0 {
        c
    } else {
        fold_concrete_steps(apply_concrete_step(c, steps[0]), steps.skip(1))
    }
}

pub open spec fn concrete_steps_ok(c: ConcreteEptMap, steps: Seq<MapUnmapStep>) -> bool
    decreases steps.len(),
{
    if steps.len() == 0 {
        true
    } else {
        &&& step_guest_ok(steps[0])
        &&& concrete_step_enabled(c, steps[0])
        &&& concrete_steps_ok(apply_concrete_step(c, steps[0]), steps.skip(1))
    }
}

/// Empty concrete registry refines.
pub proof fn lemma_empty_refines()
    ensures
        refines(ConcreteEptMap::empty()),
        abs(ConcreteEptMap::empty()) == GhostEptMap::empty(),
{
    lemma_empty_exclusive();
}

/// Map Ok on concrete commutes with ghost_map under abs (any non-zero guest).
pub proof fn lemma_abs_map_commutes(c: ConcreteEptMap, guest: GuestId, gpa: Gpa, frame: FrameId)
    requires
        guest != 0,
        refines(c),
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Map { guest, gpa, frame }),
    ensures
        abs(c.concrete_map(guest, gpa, frame)) == abs(c).ghost_map(guest, gpa, frame),
{
}

/// Unmap Ok on concrete commutes with ghost_unmap under abs (any non-zero guest).
pub proof fn lemma_abs_unmap_commutes(c: ConcreteEptMap, guest: GuestId, gpa: Gpa)
    requires
        guest != 0,
        refines(c),
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Unmap { guest, gpa }),
    ensures
        abs(c.concrete_unmap(guest, gpa)) == abs(c).ghost_unmap(guest, gpa),
{
}

/// Concrete map Ok preserves refinement (any non-zero guest; M4.6).
pub proof fn lemma_concrete_map_ok_refines(
    c: ConcreteEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        guest != 0,
        refines(c),
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Map { guest, gpa, frame }),
    ensures
        refines(c.concrete_map(guest, gpa, frame)),
{
    lemma_abs_map_commutes(c, guest, gpa, frame);
    lemma_map_ok_exclusive(abs(c), guest, gpa, frame);
}

/// Concrete unmap Ok preserves refinement (any non-zero guest; M4.6).
pub proof fn lemma_concrete_unmap_ok_refines(c: ConcreteEptMap, guest: GuestId, gpa: Gpa)
    requires
        guest != 0,
        refines(c),
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Unmap { guest, gpa }),
    ensures
        refines(c.concrete_unmap(guest, gpa)),
{
    lemma_abs_unmap_commutes(c, guest, gpa);
    lemma_unmap_ok_exclusive(abs(c), guest, gpa);
}

/// One enabled concrete step preserves refinement (N-guest; M4.6).
pub proof fn lemma_apply_concrete_step_refines(c: ConcreteEptMap, step: MapUnmapStep)
    requires
        refines(c),
        step_guest_ok(step),
        concrete_step_enabled(c, step),
    ensures
        refines(apply_concrete_step(c, step)),
        abs(apply_concrete_step(c, step)) == apply_step(abs(c), step),
{
    match step {
        MapUnmapStep::Map { guest, gpa, frame } => {
            lemma_abs_map_commutes(c, guest, gpa, frame);
            lemma_concrete_map_ok_refines(c, guest, gpa, frame);
        },
        MapUnmapStep::Unmap { guest, gpa } => {
            lemma_abs_unmap_commutes(c, guest, gpa);
            lemma_concrete_unmap_ok_refines(c, guest, gpa);
        },
    }
}

/// Target theorem (M3.18): concrete 4K bring-up-guest steps preserve refinement
/// into the verified ghost exclusivity model.
pub proof fn theorem_concrete_single_guest_4k_refine(
    c: ConcreteEptMap,
    steps: Seq<MapUnmapStep>,
)
    requires
        refines(c),
        concrete_steps_ok(c, steps),
        forall|i: int| #![auto] 0 <= i < steps.len() ==> step_guest_id(steps[i]) == BRINGUP_GUEST,
    ensures
        refines(fold_concrete_steps(c, steps)),
        abs(fold_concrete_steps(c, steps)) == fold_steps(abs(c), steps),
    decreases steps.len(),
{
    if steps.len() == 0 {
    } else {
        lemma_apply_concrete_step_refines(c, steps[0]);
        theorem_concrete_single_guest_4k_refine(
            apply_concrete_step(c, steps[0]),
            steps.skip(1),
        );
    }
}

/// M4.9 target: concrete 4K map/unmap steps for any non-zero guest(s) preserve
/// refinement into the verified ghost exclusivity model (no `admit`).
pub proof fn theorem_concrete_n_guest_4k_refine(
    c: ConcreteEptMap,
    steps: Seq<MapUnmapStep>,
)
    requires
        refines(c),
        concrete_steps_ok(c, steps),
    ensures
        refines(fold_concrete_steps(c, steps)),
        abs(fold_concrete_steps(c, steps)) == fold_steps(abs(c), steps),
    decreases steps.len(),
{
    if steps.len() == 0 {
    } else {
        lemma_apply_concrete_step_refines(c, steps[0]);
        theorem_concrete_n_guest_4k_refine(
            apply_concrete_step(c, steps[0]),
            steps.skip(1),
        );
    }
}

/// M4.9: two distinct guests mapping distinct free frames on concrete preserves
/// refinement (exec-side ≥2-guest post under `abs` / `refines`).
pub proof fn lemma_concrete_two_guests_map_refines(
    c: ConcreteEptMap,
    g1: GuestId,
    gpa1: Gpa,
    f1: FrameId,
    g2: GuestId,
    gpa2: Gpa,
    f2: FrameId,
)
    requires
        refines(c),
        g1 != 0,
        g2 != 0,
        g1 != g2,
        page_aligned_4k(gpa1),
        page_aligned_4k(gpa2),
        f1 != f2,
        concrete_step_enabled(c, MapUnmapStep::Map { guest: g1, gpa: gpa1, frame: f1 }),
        concrete_step_enabled(
            c.concrete_map(g1, gpa1, f1),
            MapUnmapStep::Map { guest: g2, gpa: gpa2, frame: f2 },
        ),
    ensures
        refines(c.concrete_map(g1, gpa1, f1).concrete_map(g2, gpa2, f2)),
        abs(c.concrete_map(g1, gpa1, f1).concrete_map(g2, gpa2, f2)) == abs(c).ghost_map(
            g1,
            gpa1,
            f1,
        ).ghost_map(g2, gpa2, f2),
{
    lemma_concrete_map_ok_refines(c, g1, gpa1, f1);
    let c1 = c.concrete_map(g1, gpa1, f1);
    lemma_abs_map_commutes(c, g1, gpa1, f1);
    lemma_concrete_map_ok_refines(c1, g2, gpa2, f2);
    lemma_abs_map_commutes(c1, g2, gpa2, f2);
}

// ---------------------------------------------------------------------------
// M4.8 — large-page (2M/1G) ghost *spec* (ADR-004; may stay L2)
//
// Leaf sizes and span predicates live in the model so exclusivity posts can
// talk about contiguous 4K-frame ranges. MapUnmapStep remains 4K-only;
// large-page L3 discharge is M5 (`GAP: Large-page L3 discharge`).
// ---------------------------------------------------------------------------

/// 2 MiB page size (matches `EptPageSize::TwoMib` leaves).
pub const PAGE_2M: u64 = 0x20_0000;

/// 1 GiB page size (matches `EptPageSize::OneGib` leaves).
pub const PAGE_1G: u64 = 0x4000_0000;

/// Ghost EPT leaf size (4K / 2M / 1G).
pub enum GhostPageSize {
    FourK,
    TwoM,
    OneG,
}

pub open spec fn page_size_bytes(ps: GhostPageSize) -> u64 {
    match ps {
        GhostPageSize::FourK => PAGE_4K,
        GhostPageSize::TwoM => PAGE_2M,
        GhostPageSize::OneG => PAGE_1G,
    }
}

/// Number of 4K frames covered by one leaf (`FrameId` = HPA / 4096).
pub open spec fn frames_covered(ps: GhostPageSize) -> u64 {
    match ps {
        GhostPageSize::FourK => 1,
        GhostPageSize::TwoM => 512,
        GhostPageSize::OneG => 262144,
    }
}

pub open spec fn page_aligned(ps: GhostPageSize, addr: u64) -> bool {
    addr % page_size_bytes(ps) == 0
}

/// Base `FrameId` is aligned for a large leaf (HPA = base × 4096).
pub open spec fn frame_base_aligned(ps: GhostPageSize, base: FrameId) -> bool {
    match ps {
        GhostPageSize::FourK => true,
        GhostPageSize::TwoM => base % 512 == 0,
        GhostPageSize::OneG => base % 262144 == 0,
    }
}

pub open spec fn frame_in_large_span(base: FrameId, ps: GhostPageSize, f: FrameId) -> bool {
    base <= f && f < base + frames_covered(ps)
}

/// Every 4K frame in the large-page span is free.
pub open spec fn large_span_free(m: GhostEptMap, base: FrameId, ps: GhostPageSize) -> bool {
    forall|f: FrameId|
        #![auto]
        frame_in_large_span(base, ps, f) ==> !m.owned.dom().contains(f)
}

/// Enabled predicate for a large-page map (spec only; not folded into `MapUnmapStep`).
pub open spec fn large_map_enabled(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
) -> bool {
    &&& guest != 0
    &&& page_aligned(ps, gpa)
    &&& frame_base_aligned(ps, base)
    &&& large_span_free(m, base, ps)
    &&& !m.by_gpa.dom().contains((guest, gpa))
}

/// Postcondition sketch: every 4K frame in the span is owned by `guest`.
pub open spec fn large_map_post_owned(
    m: GhostEptMap,
    guest: GuestId,
    base: FrameId,
    ps: GhostPageSize,
) -> bool {
    forall|f: FrameId|
        #![auto]
        frame_in_large_span(base, ps, f) ==> m.owned.dom().contains(f) && m.owned[f] == guest
}

/// Trivial size facts for 2M / 1G leaves (spec scaffolding; exclusivity L3 → M5).
pub proof fn lemma_2m_covers_512_frames()
    ensures
        page_size_bytes(GhostPageSize::TwoM) == PAGE_2M,
        frames_covered(GhostPageSize::TwoM) == 512,
        PAGE_2M == 512 * PAGE_4K,
{
}

pub proof fn lemma_1g_covers_262144_frames()
    ensures
        page_size_bytes(GhostPageSize::OneG) == PAGE_1G,
        frames_covered(GhostPageSize::OneG) == 262144,
        PAGE_1G == 262144 * PAGE_4K,
{
}

pub proof fn lemma_4k_is_unit_large_page()
    ensures
        frames_covered(GhostPageSize::FourK) == 1,
        page_size_bytes(GhostPageSize::FourK) == PAGE_4K,
{
}

} // verus!

/// Exec-visible markers for host tests / smoke scripts.
pub const M3_L3_LINK_OK_MARKER: &str = "RAYNU-V-M3-L3-LINK-OK";
pub const M3_L3_VERIFY_OK_MARKER: &str = "RAYNU-V-M3-L3-VERIFY-OK";
pub const M3_L3_REFINE_OK_MARKER: &str = "RAYNU-V-M3-L3-REFINE-OK";
/// M4.6: N-guest ghost map/unmap is in the model (spec OK).
pub const M4_NGUEST_SPEC_OK_MARKER: &str = "RAYNU-V-M4-NGUEST-SPEC-OK";
/// M4.7: ADR-006 L3 for N-guest 4K map/unmap exclusivity (no `admit`).
pub const M4_NGUEST_VERIFY_OK_MARKER: &str = "RAYNU-V-M4-NGUEST-VERIFY-OK";
/// M4.8: large-page (2M/1G) ghost *spec* (L3 discharge → M5).
pub const M4_LPAGE_OK_MARKER: &str = "RAYNU-V-M4-LPAGE-OK";
/// M4.9: N-guest ghost↔exec refine (no `admit`).
pub const M4_REFINE_OK_MARKER: &str = "RAYNU-V-M4-REFINE-OK";
