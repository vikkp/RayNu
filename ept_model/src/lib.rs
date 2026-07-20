//! Verus-verified ghost model for ADR-004 EPT exclusive ownership (M3.17–M3.18 / M4.6–M4.9).
//!
//! Host-only crate (`package.metadata.verus.verify = true`). Not linked into
//! the UEFI binary. Under the frozen Verus pin, exclusivity lemmas for 4K
//! map/unmap are discharged with **no `admit()`**. M3.18 adds ghost↔exec
//! refinement. M4.6 extends `MapUnmapStep` with an explicit `guest` field;
//! M4.7 claims ADR-006 L3 for N-guest 4K map/unmap exclusivity
//! (`theorem_n_guest_4k_map_unmap_exclusive` / `lemma_two_guests_map_distinct_frames_exclusive`).
//! M4.8 adds 2M/1G leaf sizes and span predicates to the ghost *spec*
//! (`GhostPageSize` / `large_map_enabled`). M5.7 discharges large-page span
//! map/unmap exclusivity (no `admit`) → `RAYNU-V-M5-LPAGE-VERIFY-OK`.
//! M4.9 extends concrete refine to N guests (`theorem_concrete_n_guest_4k_refine`).
//! M5.8 adds NUMA domains to the ghost *spec* (`GhostNumaTopology` / SRAT·SLIT
//! bring-up mock) → `RAYNU-V-M5-NUMA-OK` (full NUMA affinity L3 → M6).
//! M5.9 couples the ghost frame pool with concrete EPT refine (`alloc_ept_refines`)
//! and scoped precise-identity GPA==HPA correspondence → `RAYNU-V-M5-ALLOC-REFINE-OK`.
//! M6.0 discharges EPT-violation handling under exclusivity
//! (`theorem_ept_violation_preserves_exclusive`) → `RAYNU-V-M6-EPTVIO-OK`
//! (full HW PTE bit-decode → M6.1).
//!
//! Markers:
//! - M3.16 link: `RAYNU-V-M3-L3-LINK-OK`
//! - M3.17 true L3: `RAYNU-V-M3-L3-VERIFY-OK` (via tools/verus-verify-smoke.sh)
//! - M3.18 refine: `RAYNU-V-M3-L3-REFINE-OK` (via tools/verus-refine-smoke.sh)
//! - M4.6 N-guest spec: `RAYNU-V-M4-NGUEST-SPEC-OK` (via tools/verus-nguest-spec-smoke.sh)
//! - M4.7 N-guest L3: `RAYNU-V-M4-NGUEST-VERIFY-OK` (via tools/verus-nguest-verify-smoke.sh)
//! - M4.8 large-page spec: `RAYNU-V-M4-LPAGE-OK` (via tools/verus-lpage-spec-smoke.sh)
//! - M4.9 N-guest refine: `RAYNU-V-M4-REFINE-OK` (via tools/verus-nguest-refine-smoke.sh)
//! - M5.7 large-page L3: `RAYNU-V-M5-LPAGE-VERIFY-OK` (via tools/verus-lpage-verify-smoke.sh)
//! - M5.8 NUMA ghost spec: `RAYNU-V-M5-NUMA-OK` (via tools/verus-numa-smoke.sh)
//! - M5.9 alloc↔EPT refine: `RAYNU-V-M5-ALLOC-REFINE-OK` (via tools/verus-alloc-refine-smoke.sh)
//! - M6.0 EPT-violation: `RAYNU-V-M6-EPTVIO-OK` (via tools/verus-eptvio-smoke.sh)

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
// M4.8 — large-page (2M/1G) ghost *spec* (ADR-004)
// M5.7 — large-page L3: span map/unmap exclusivity (no admit)
//
// A large leaf is modeled as `frames_covered(ps)` contiguous 4K ghost maps so
// `exclusive_ownership` (owned.len == by_gpa.len) is preserved. MapUnmapStep
// remains the 4K step type; large ops are separate span folds.
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

/// Recursive: first GPA free, then the rest.
pub open spec fn span_gpas_free(m: GhostEptMap, guest: GuestId, gpa: Gpa, n: u64) -> bool
    decreases n,
{
    if n == 0 {
        true
    } else {
        &&& !m.by_gpa.dom().contains((guest, gpa))
        &&& span_gpas_free(m, guest, (gpa + PAGE_4K) as u64, (n - 1) as u64)
    }
}

/// Every synthetic 4K GPA in the large-page span is free for `guest`.
pub open spec fn large_gpa_span_free(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    ps: GhostPageSize,
) -> bool {
    span_gpas_free(m, guest, gpa, frames_covered(ps))
}

/// Enabled predicate for a large-page map (span of 4K ghost maps).
pub open spec fn large_map_enabled(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
) -> bool {
    &&& guest != 0
    &&& page_aligned(ps, gpa)
    &&& page_aligned_4k(gpa)
    &&& frame_base_aligned(ps, base)
    &&& large_span_free(m, base, ps)
    &&& large_gpa_span_free(m, guest, gpa, ps)
    &&& base <= 0xffff_ffff_ffff_ffff - frames_covered(ps)
    &&& gpa <= 0xffff_ffff_ffff_ffff - frames_covered(ps) * PAGE_4K
}

/// Postcondition: every 4K frame in the span is owned by `guest`.
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

/// Recursive span mapped (head/tail — induction-friendly).
pub open spec fn span_mapped(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    n: u64,
) -> bool
    decreases n,
{
    if n == 0 {
        true
    } else {
        &&& m.by_gpa.dom().contains((guest, gpa))
        &&& m.by_gpa[(guest, gpa)] == base
        &&& span_mapped(
            m,
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        )
    }
}

/// Span is fully mapped as contiguous 4K `(guest, gpa+i*4K) → base+i`.
pub open spec fn large_span_mapped(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
) -> bool {
    span_mapped(m, guest, gpa, base, frames_covered(ps))
}

/// Enabled predicate for a large-page unmap.
pub open spec fn large_unmap_enabled(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
) -> bool {
    &&& guest != 0
    &&& page_aligned(ps, gpa)
    &&& page_aligned_4k(gpa)
    &&& frame_base_aligned(ps, base)
    &&& large_span_mapped(m, guest, gpa, base, ps)
    &&& base <= 0xffff_ffff_ffff_ffff - frames_covered(ps)
    &&& gpa <= 0xffff_ffff_ffff_ffff - frames_covered(ps) * PAGE_4K
}

pub open spec fn ghost_map_span(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    n: u64,
) -> GhostEptMap
    decreases n,
{
    if n == 0 {
        m
    } else {
        ghost_map_span(
            m.ghost_map(guest, gpa, base),
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        )
    }
}

pub open spec fn ghost_unmap_span(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    n: u64,
) -> GhostEptMap
    decreases n,
{
    if n == 0 {
        m
    } else {
        ghost_unmap_span(
            m.ghost_unmap(guest, gpa),
            guest,
            (gpa + PAGE_4K) as u64,
            (n - 1) as u64,
        )
    }
}

pub open spec fn ghost_large_map(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
) -> GhostEptMap {
    ghost_map_span(m, guest, gpa, base, frames_covered(ps))
}

pub open spec fn ghost_large_unmap(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    ps: GhostPageSize,
) -> GhostEptMap {
    ghost_unmap_span(m, guest, gpa, frames_covered(ps))
}

pub proof fn lemma_next_gpa_4k_aligned(gpa: Gpa)
    requires
        page_aligned_4k(gpa),
        gpa <= 0xffff_ffff_ffff_ffff - PAGE_4K,
    ensures
        page_aligned_4k((gpa + PAGE_4K) as u64),
{
}

/// Inserting `(guest,gpa_mapped)` preserves `span_gpas_free` for a disjoint GPA chain.
pub proof fn lemma_span_gpas_free_preserved_by_map(
    m: GhostEptMap,
    guest: GuestId,
    gpa_mapped: Gpa,
    frame: FrameId,
    gpa: Gpa,
    n: u64,
)
    requires
        span_gpas_free(m, guest, gpa, n),
        gpa_mapped + n * PAGE_4K <= 0xffff_ffff_ffff_ffff || n == 0,
        gpa <= 0xffff_ffff_ffff_ffff - n * PAGE_4K,
        forall|k: u64|
            #![trigger (gpa as int) + (k as int) * (PAGE_4K as int)]
            k < n ==> (gpa as int) + (k as int) * (PAGE_4K as int) != gpa_mapped as int,
    ensures
        span_gpas_free(m.ghost_map(guest, gpa_mapped, frame), guest, gpa, n),
    decreases n,
{
    if n == 0 {
    } else {
        let m2 = m.ghost_map(guest, gpa_mapped, frame);
        assert((gpa as int) + (0 as int) * (PAGE_4K as int) != gpa_mapped as int);
        assert(gpa as int != gpa_mapped as int);
        assert(!m.by_gpa.dom().contains((guest, gpa)));
        assert(!m2.by_gpa.dom().contains((guest, gpa)));
        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)
                != gpa_mapped as int
        by {
            let k2 = (1 + k) as u64;
            assert(k2 < n);
            assert((gpa as int) + (k2 as int) * (PAGE_4K as int) != gpa_mapped as int);
            assert(((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) == (gpa as int) + (
                k2 as int
            ) * (PAGE_4K as int));
        };
        lemma_span_gpas_free_preserved_by_map(
            m,
            guest,
            gpa_mapped,
            frame,
            (gpa + PAGE_4K) as u64,
            (n - 1) as u64,
        );
    }
}

pub proof fn lemma_map_span_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    n: u64,
)
    requires
        exclusive_ownership(m),
        guest != 0,
        page_aligned_4k(gpa),
        base <= 0xffff_ffff_ffff_ffff - n,
        gpa <= 0xffff_ffff_ffff_ffff - n * PAGE_4K,
        forall|f: FrameId|
            #![auto]
            base <= f && f < base + n ==> !m.owned.dom().contains(f),
        span_gpas_free(m, guest, gpa, n),
    ensures
        exclusive_ownership(ghost_map_span(m, guest, gpa, base, n)),
        span_mapped(ghost_map_span(m, guest, gpa, base, n), guest, gpa, base, n),
    decreases n,
{
    if n == 0 {
    } else {
        assert(base <= base && base < base + n);
        assert(!m.owned.dom().contains(base));
        assert(!m.by_gpa.dom().contains((guest, gpa)));
        lemma_map_ok_exclusive(m, guest, gpa, base);
        let m2 = m.ghost_map(guest, gpa, base);

        assert forall|f: FrameId|
            #![auto]
            (base + 1) as u64 <= f && f < (base + 1) as u64 + (n - 1) implies !m2.owned.dom().contains(
                f,
            )
        by {
            assert(f as int != base as int);
            assert(base <= f && f < base + n);
            assert(!m.owned.dom().contains(f));
        };

        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) != gpa
                as int
        by {
            assert(((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) == (gpa as int) + ((1
                + k) as int) * (PAGE_4K as int));
            assert((1 + k) as int * (PAGE_4K as int) > 0);
        };
        lemma_span_gpas_free_preserved_by_map(
            m,
            guest,
            gpa,
            base,
            (gpa + PAGE_4K) as u64,
            (n - 1) as u64,
        );
        lemma_next_gpa_4k_aligned(gpa);
        lemma_map_span_exclusive(
            m2,
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        );

        // Preserve head mapping through recursive maps
        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) != gpa
                as int
        by {
            assert(((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) == (gpa as int) + ((1
                + k) as int) * (PAGE_4K as int));
            assert((1 + k) as int * (PAGE_4K as int) > 0);
        };
        lemma_map_span_preserves_by_gpa(
            m2,
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
            guest,
            gpa,
        );
        let ms = ghost_map_span(m, guest, gpa, base, n);
        assert(ms.by_gpa.dom().contains((guest, gpa)));
        assert(ms.by_gpa[(guest, gpa)] == base);
    }
}

pub proof fn lemma_map_span_preserves_by_gpa(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    n: u64,
    g_old: GuestId,
    a_old: Gpa,
)
    requires
        m.by_gpa.dom().contains((g_old, a_old)),
        gpa <= 0xffff_ffff_ffff_ffff - n * PAGE_4K,
        forall|k: u64|
            #![trigger (gpa as int) + (k as int) * (PAGE_4K as int)]
            k < n ==> (gpa as int) + (k as int) * (PAGE_4K as int) != a_old as int || g_old
                != guest,
    ensures
        ghost_map_span(m, guest, gpa, base, n).by_gpa.dom().contains((g_old, a_old)),
        ghost_map_span(m, guest, gpa, base, n).by_gpa[(g_old, a_old)] == m.by_gpa[(g_old, a_old)],
    decreases n,
{
    if n == 0 {
    } else {
        assert((gpa as int) + (0 as int) * (PAGE_4K as int) != a_old as int || g_old != guest);
        assert((g_old, a_old) != (guest, gpa));
        let m2 = m.ghost_map(guest, gpa, base);
        assert(m2.by_gpa.dom().contains((g_old, a_old)));
        assert(m2.by_gpa[(g_old, a_old)] == m.by_gpa[(g_old, a_old)]);
        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) != a_old
                as int || g_old != guest
        by {
            let k2 = (1 + k) as u64;
            assert(k2 < n);
            assert((gpa as int) + (k2 as int) * (PAGE_4K as int) != a_old as int || g_old
                != guest);
        };
        lemma_map_span_preserves_by_gpa(
            m2,
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
            g_old,
            a_old,
        );
    }
}

/// Unmapping head preserves `span_mapped` for the tail.
pub proof fn lemma_span_mapped_preserved_by_unmap(
    m: GhostEptMap,
    guest: GuestId,
    gpa_unmapped: Gpa,
    gpa: Gpa,
    base: FrameId,
    n: u64,
)
    requires
        span_mapped(m, guest, gpa, base, n),
        gpa <= 0xffff_ffff_ffff_ffff - n * PAGE_4K,
        forall|k: u64|
            #![trigger (gpa as int) + (k as int) * (PAGE_4K as int)]
            k < n ==> (gpa as int) + (k as int) * (PAGE_4K as int) != gpa_unmapped as int,
    ensures
        span_mapped(m.ghost_unmap(guest, gpa_unmapped), guest, gpa, base, n),
    decreases n,
{
    if n == 0 {
    } else {
        let m2 = m.ghost_unmap(guest, gpa_unmapped);
        assert((gpa as int) + (0 as int) * (PAGE_4K as int) != gpa_unmapped as int);
        assert(gpa as int != gpa_unmapped as int);
        assert(m.by_gpa.dom().contains((guest, gpa)));
        assert(m2.by_gpa.dom().contains((guest, gpa)));
        assert(m2.by_gpa[(guest, gpa)] == m.by_gpa[(guest, gpa)]);
        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)
                != gpa_unmapped as int
        by {
            let k2 = (1 + k) as u64;
            assert(k2 < n);
            assert((gpa as int) + (k2 as int) * (PAGE_4K as int) != gpa_unmapped as int);
        };
        lemma_span_mapped_preserved_by_unmap(
            m,
            guest,
            gpa_unmapped,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        );
    }
}

pub proof fn lemma_unmap_span_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    n: u64,
)
    requires
        exclusive_ownership(m),
        guest != 0,
        page_aligned_4k(gpa),
        base <= 0xffff_ffff_ffff_ffff - n,
        gpa <= 0xffff_ffff_ffff_ffff - n * PAGE_4K,
        span_mapped(m, guest, gpa, base, n),
    ensures
        exclusive_ownership(ghost_unmap_span(m, guest, gpa, n)),
    decreases n,
{
    if n == 0 {
    } else {
        assert(m.by_gpa.dom().contains((guest, gpa)));
        assert(m.by_gpa[(guest, gpa)] == base);
        lemma_unmap_ok_exclusive(m, guest, gpa);
        let m2 = m.ghost_unmap(guest, gpa);
        assert forall|k: u64|
            #![trigger ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int)]
            k < n - 1 implies ((gpa + PAGE_4K) as int) + (k as int) * (PAGE_4K as int) != gpa
                as int
        by {
            assert((1 + k) as int * (PAGE_4K as int) > 0);
        };
        lemma_span_mapped_preserved_by_unmap(
            m,
            guest,
            gpa,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        );
        lemma_next_gpa_4k_aligned(gpa);
        lemma_unmap_span_exclusive(
            m2,
            guest,
            (gpa + PAGE_4K) as u64,
            (base + 1) as u64,
            (n - 1) as u64,
        );
    }
}

/// M5.7: large-page map preserves exclusive ownership + span_mapped.
pub proof fn lemma_large_map_ok_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
)
    requires
        exclusive_ownership(m),
        large_map_enabled(m, guest, gpa, base, ps),
    ensures
        exclusive_ownership(ghost_large_map(m, guest, gpa, base, ps)),
        large_span_mapped(ghost_large_map(m, guest, gpa, base, ps), guest, gpa, base, ps),
{
    let n = frames_covered(ps);
    assert forall|f: FrameId|
        #![auto]
        base <= f && f < base + n implies !m.owned.dom().contains(f)
    by {
        assert(frame_in_large_span(base, ps, f));
    };
    lemma_map_span_exclusive(m, guest, gpa, base, n);
}

/// M5.7: large-page unmap preserves exclusive ownership.
pub proof fn lemma_large_unmap_ok_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
)
    requires
        exclusive_ownership(m),
        large_unmap_enabled(m, guest, gpa, base, ps),
    ensures
        exclusive_ownership(ghost_large_unmap(m, guest, gpa, ps)),
{
    lemma_unmap_span_exclusive(m, guest, gpa, base, frames_covered(ps));
}

/// M5.7: map then unmap a large page preserves exclusivity.
pub proof fn theorem_large_page_map_unmap_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
    ps: GhostPageSize,
)
    requires
        exclusive_ownership(m),
        large_map_enabled(m, guest, gpa, base, ps),
    ensures
        exclusive_ownership(
            ghost_large_unmap(ghost_large_map(m, guest, gpa, base, ps), guest, gpa, ps),
        ),
{
    lemma_large_map_ok_exclusive(m, guest, gpa, base, ps);
    let m2 = ghost_large_map(m, guest, gpa, base, ps);
    assert(large_unmap_enabled(m2, guest, gpa, base, ps));
    lemma_large_unmap_ok_exclusive(m2, guest, gpa, base, ps);
}

/// M5.7 ≥2-guest post via FourK large leaves (= ordinary 4K maps).
pub proof fn lemma_two_guests_large_map_distinct_spans_exclusive(
    m: GhostEptMap,
    g1: GuestId,
    gpa1: Gpa,
    b1: FrameId,
    g2: GuestId,
    gpa2: Gpa,
    b2: FrameId,
)
    requires
        exclusive_ownership(m),
        g1 != 0,
        g2 != 0,
        g1 != g2,
        large_map_enabled(m, g1, gpa1, b1, GhostPageSize::FourK),
        large_map_enabled(m, g2, gpa2, b2, GhostPageSize::FourK),
        b1 != b2,
    ensures
        exclusive_ownership(
            ghost_large_map(
                ghost_large_map(m, g1, gpa1, b1, GhostPageSize::FourK),
                g2,
                gpa2,
                b2,
                GhostPageSize::FourK,
            ),
        ),
{
    assert(frames_covered(GhostPageSize::FourK) == 1);
    assert(frame_in_large_span(b1, GhostPageSize::FourK, b1));
    assert(frame_in_large_span(b2, GhostPageSize::FourK, b2));
    assert(!m.owned.dom().contains(b1));
    assert(!m.owned.dom().contains(b2));
    assert(!m.by_gpa.dom().contains((g1, gpa1)));
    assert(!m.by_gpa.dom().contains((g2, gpa2)));
    lemma_two_guests_map_distinct_frames_exclusive(m, g1, gpa1, b1, g2, gpa2, b2);
    assert(ghost_map_span(m, g1, gpa1, b1, 1) == ghost_map_span(
        m.ghost_map(g1, gpa1, b1),
        g1,
        (gpa1 + PAGE_4K) as u64,
        (b1 + 1) as u64,
        0,
    ));
    assert(ghost_map_span(m.ghost_map(g1, gpa1, b1), g1, (gpa1 + PAGE_4K) as u64, (b1 + 1)
        as u64, 0) == m.ghost_map(g1, gpa1, b1));
    assert(ghost_map_span(m.ghost_map(g1, gpa1, b1), g2, gpa2, b2, 1) == ghost_map_span(
        m.ghost_map(g1, gpa1, b1).ghost_map(g2, gpa2, b2),
        g2,
        (gpa2 + PAGE_4K) as u64,
        (b2 + 1) as u64,
        0,
    ));
    assert(ghost_map_span(
        m.ghost_map(g1, gpa1, b1).ghost_map(g2, gpa2, b2),
        g2,
        (gpa2 + PAGE_4K) as u64,
        (b2 + 1) as u64,
        0,
    ) == m.ghost_map(g1, gpa1, b1).ghost_map(g2, gpa2, b2));
}

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

pub proof fn lemma_2m_map_unmap_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
)
    requires
        exclusive_ownership(m),
        large_map_enabled(m, guest, gpa, base, GhostPageSize::TwoM),
    ensures
        exclusive_ownership(
            ghost_large_unmap(
                ghost_large_map(m, guest, gpa, base, GhostPageSize::TwoM),
                guest,
                gpa,
                GhostPageSize::TwoM,
            ),
        ),
{
    theorem_large_page_map_unmap_exclusive(m, guest, gpa, base, GhostPageSize::TwoM);
}

pub proof fn lemma_1g_map_unmap_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    base: FrameId,
)
    requires
        exclusive_ownership(m),
        large_map_enabled(m, guest, gpa, base, GhostPageSize::OneG),
    ensures
        exclusive_ownership(
            ghost_large_unmap(
                ghost_large_map(m, guest, gpa, base, GhostPageSize::OneG),
                guest,
                gpa,
                GhostPageSize::OneG,
            ),
        ),
{
    theorem_large_page_map_unmap_exclusive(m, guest, gpa, base, GhostPageSize::OneG);
}

// ---------------------------------------------------------------------------
// M5.8 — NUMA in ghost spec (SRAT / SLIT bring-up; affinity L3 → M6)
// ---------------------------------------------------------------------------

/// ACPI / SRAT-style NUMA node identifier.
pub type NumaNodeId = u8;

/// ACPI SLIT local distance (same node).
pub const SLIT_LOCAL: u8 = 10;

/// Ghost NUMA topology: node set, frame→node affinity, SLIT distances.
pub struct GhostNumaTopology {
    pub nodes: Set<NumaNodeId>,
    pub frame_node: Map<FrameId, NumaNodeId>,
    pub slit: Map<(NumaNodeId, NumaNodeId), u8>,
}

impl GhostNumaTopology {
    pub open spec fn empty() -> Self {
        GhostNumaTopology {
            nodes: Set::empty(),
            frame_node: Map::empty(),
            slit: Map::empty(),
        }
    }
}

/// Affinity + SLIT rows only reference known nodes; local diagonal is `SLIT_LOCAL`.
pub open spec fn numa_well_formed(t: GhostNumaTopology) -> bool {
    &&& (forall|f: FrameId|
        #![auto]
        t.frame_node.dom().contains(f) ==> t.nodes.contains(t.frame_node[f]))
    &&& (forall|a: NumaNodeId, b: NumaNodeId|
        #![auto]
        t.slit.dom().contains((a, b)) ==> t.nodes.contains(a) && t.nodes.contains(b))
    &&& (forall|n: NumaNodeId|
        #![auto]
        t.nodes.contains(n) ==> t.slit.dom().contains((n, n)) && t.slit[(n, n)] == SLIT_LOCAL)
}

/// SLIT is symmetric when both directions are present.
pub open spec fn slit_symmetric(t: GhostNumaTopology) -> bool {
    forall|a: NumaNodeId, b: NumaNodeId|
        #![auto]
        t.slit.dom().contains((a, b)) && t.slit.dom().contains((b, a)) ==> t.slit[(a, b)]
            == t.slit[(b, a)]
}

/// Every frame owned by `guest` lies on `node` (affinity policy post).
pub open spec fn guest_frames_on_node(
    m: GhostEptMap,
    t: GhostNumaTopology,
    guest: GuestId,
    node: NumaNodeId,
) -> bool {
    forall|f: FrameId|
        #![auto]
        m.owned.dom().contains(f) && m.owned[f] == guest ==> t.frame_node.dom().contains(f)
            && t.frame_node[f] == node
}

/// NUMA-aware 4K map: ordinary map enabled + frame affinity matches preferred node.
pub open spec fn numa_map_enabled(
    m: GhostEptMap,
    t: GhostNumaTopology,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
    node: NumaNodeId,
) -> bool {
    &&& guest != 0
    &&& page_aligned_4k(gpa)
    &&& step_enabled(m, MapUnmapStep::Map { guest, gpa, frame })
    &&& numa_well_formed(t)
    &&& t.nodes.contains(node)
    &&& t.frame_node.dom().contains(frame)
    &&& t.frame_node[frame] == node
}

/// Bring-up mock matching `assets/idrac/mock_topology.txt` (2 nodes, SLIT 10/21).
pub open spec fn mock_bringup_numa() -> GhostNumaTopology {
    GhostNumaTopology {
        nodes: Set::empty().insert(0).insert(1),
        frame_node: Map::empty().insert(0, 0).insert(1, 0).insert(100, 1).insert(101, 1),
        slit: Map::empty().insert((0, 0), 10).insert((1, 1), 10).insert((0, 1), 21).insert(
            (1, 0),
            21,
        ),
    }
}

/// M5.8: concrete facts about the SRAT/SLIT bring-up mock.
pub proof fn lemma_mock_bringup_numa_facts()
    ensures
        mock_bringup_numa().nodes.contains(0),
        mock_bringup_numa().nodes.contains(1),
        mock_bringup_numa().frame_node[0] == 0,
        mock_bringup_numa().frame_node[1] == 0,
        mock_bringup_numa().frame_node[100] == 1,
        mock_bringup_numa().frame_node[101] == 1,
        mock_bringup_numa().slit[(0, 0)] == SLIT_LOCAL,
        mock_bringup_numa().slit[(1, 1)] == SLIT_LOCAL,
        mock_bringup_numa().slit[(0, 1)] == 21,
        mock_bringup_numa().slit[(1, 0)] == 21,
{
}

/// M5.8: local SLIT constant matches ACPI convention.
pub proof fn lemma_slit_local_is_10()
    ensures
        SLIT_LOCAL == 10,
{
}

/// M5.8: NUMA-aware map still preserves exclusive ownership (uses 4K map lemma).
pub proof fn lemma_numa_map_ok_exclusive(
    m: GhostEptMap,
    t: GhostNumaTopology,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
    node: NumaNodeId,
)
    requires
        exclusive_ownership(m),
        numa_map_enabled(m, t, guest, gpa, frame, node),
    ensures
        exclusive_ownership(m.ghost_map(guest, gpa, frame)),
        m.ghost_map(guest, gpa, frame).owned[frame] == guest,
        m.ghost_map(guest, gpa, frame).by_gpa[(guest, gpa)] == frame,
{
    lemma_map_ok_exclusive(m, guest, gpa, frame);
}

// ---------------------------------------------------------------------------
// M5.9 — Frame-allocator ↔ EPT refine + scoped precise-identity correspondence
// ---------------------------------------------------------------------------

/// Ghost frame pool (allocated-set view of `FrameAllocator`).
pub struct GhostFramePool {
    pub base: FrameId,
    pub capacity: u64,
    pub allocated: Set<FrameId>,
}

impl GhostFramePool {
    pub open spec fn empty(base: FrameId, capacity: u64) -> Self {
        GhostFramePool { base, capacity, allocated: Set::empty() }
    }
}

/// Frame lies in the pool's contiguous HPA range.
pub open spec fn pool_contains(p: GhostFramePool, f: FrameId) -> bool {
    p.base <= f && f < p.base + p.capacity
}

pub open spec fn pool_well_formed(p: GhostFramePool) -> bool {
    &&& p.capacity > 0
    &&& (forall|f: FrameId|
        #![auto]
        p.allocated.contains(f) ==> pool_contains(p, f))
}

pub open spec fn ghost_allocate(p: GhostFramePool, f: FrameId) -> GhostFramePool {
    GhostFramePool {
        base: p.base,
        capacity: p.capacity,
        allocated: p.allocated.insert(f),
    }
}

pub open spec fn ghost_free(p: GhostFramePool, f: FrameId) -> GhostFramePool {
    GhostFramePool {
        base: p.base,
        capacity: p.capacity,
        allocated: p.allocated.remove(f),
    }
}

pub open spec fn alloc_enabled(p: GhostFramePool, f: FrameId) -> bool {
    &&& pool_well_formed(p)
    &&& pool_contains(p, f)
    &&& !p.allocated.contains(f)
}

pub open spec fn free_enabled(p: GhostFramePool, f: FrameId) -> bool {
    &&& pool_well_formed(p)
    &&& p.allocated.contains(f)
}

/// Coupled refine: concrete EPT exclusivity + every owned frame is allocated.
pub open spec fn alloc_ept_refines(c: ConcreteEptMap, p: GhostFramePool) -> bool {
    &&& refines(c)
    &&& pool_well_formed(p)
    &&& (forall|f: FrameId|
        #![auto]
        abs(c).owned.dom().contains(f) ==> p.allocated.contains(f))
}

/// Map is enabled under allocator coupling (frame must already be allocated).
pub open spec fn alloc_map_enabled(
    c: ConcreteEptMap,
    p: GhostFramePool,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
) -> bool {
    &&& alloc_ept_refines(c, p)
    &&& guest != 0
    &&& page_aligned_4k(gpa)
    &&& concrete_step_enabled(c, MapUnmapStep::Map { guest, gpa, frame })
    &&& p.allocated.contains(frame)
}

/// Precise identity window in 4K frames (matches `ept_hw::PRECISE_BYTES` = 512 MiB).
pub const PRECISE_IDENTITY_FRAMES: u64 = 131072;

pub open spec fn identity_frame_for_gpa(gpa: Gpa) -> FrameId {
    (gpa / PAGE_4K) as u64
}

pub open spec fn in_precise_identity(gpa: Gpa) -> bool {
    page_aligned_4k(gpa) && gpa < PRECISE_IDENTITY_FRAMES * PAGE_4K
}

/// Abstract identity leaf: GPA maps to HPA frame `gpa/4096` inside the precise window.
pub open spec fn identity_leaf_ok(gpa: Gpa, frame: FrameId) -> bool {
    in_precise_identity(gpa) && frame == identity_frame_for_gpa(gpa)
}

/// Empty concrete registry + empty pool refine under allocator coupling.
pub proof fn lemma_empty_alloc_ept_refines()
    ensures
        alloc_ept_refines(ConcreteEptMap::empty(), GhostFramePool::empty(0, 8)),
{
    lemma_empty_refines();
}

/// Allocate preserves pool well-formedness.
pub proof fn lemma_allocate_preserves_pool(p: GhostFramePool, f: FrameId)
    requires
        alloc_enabled(p, f),
    ensures
        pool_well_formed(ghost_allocate(p, f)),
        ghost_allocate(p, f).allocated.contains(f),
{
    let p2 = ghost_allocate(p, f);
    assert forall|g: FrameId|
        #![auto]
        p2.allocated.contains(g) implies pool_contains(p2, g)
    by {
        if g == f {
            assert(pool_contains(p, f));
        } else {
            assert(p.allocated.contains(g));
            assert(pool_contains(p, g));
        }
    };
}

/// Allocate on an empty-owned concrete registry preserves `alloc_ept_refines`.
pub proof fn lemma_allocate_preserves_alloc_ept_refines(
    c: ConcreteEptMap,
    p: GhostFramePool,
    f: FrameId,
)
    requires
        alloc_ept_refines(c, p),
        alloc_enabled(p, f),
        abs(c).owned.dom() == Set::<FrameId>::empty(),
    ensures
        alloc_ept_refines(c, ghost_allocate(p, f)),
{
    lemma_allocate_preserves_pool(p, f);
}

/// Map of an allocated frame preserves coupled refine.
pub proof fn lemma_alloc_map_ok_refines(
    c: ConcreteEptMap,
    p: GhostFramePool,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        alloc_map_enabled(c, p, guest, gpa, frame),
    ensures
        alloc_ept_refines(c.concrete_map(guest, gpa, frame), p),
        abs(c.concrete_map(guest, gpa, frame)).owned[frame] == guest,
{
    lemma_concrete_map_ok_refines(c, guest, gpa, frame);
    let c2 = c.concrete_map(guest, gpa, frame);
    assert forall|f: FrameId|
        #![auto]
        abs(c2).owned.dom().contains(f) implies p.allocated.contains(f)
    by {
        if f == frame {
            assert(p.allocated.contains(frame));
        } else {
            assert(abs(c).owned.dom().contains(f));
        }
    };
}

/// Unmap preserves coupled refine (owned shrinks; allocated may keep the frame).
pub proof fn lemma_alloc_unmap_ok_refines(c: ConcreteEptMap, p: GhostFramePool, guest: GuestId, gpa: Gpa)
    requires
        alloc_ept_refines(c, p),
        guest != 0,
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Unmap { guest, gpa }),
    ensures
        alloc_ept_refines(c.concrete_unmap(guest, gpa), p),
{
    lemma_concrete_unmap_ok_refines(c, guest, gpa);
    let c2 = c.concrete_unmap(guest, gpa);
    assert forall|f: FrameId|
        #![auto]
        abs(c2).owned.dom().contains(f) implies p.allocated.contains(f)
    by {
        assert(abs(c).owned.dom().contains(f));
    };
}

/// M5.9: allocate → map → unmap preserves allocator↔EPT coupled refine.
pub proof fn theorem_alloc_map_unmap_refines(
    c: ConcreteEptMap,
    p: GhostFramePool,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        alloc_ept_refines(c, p),
        abs(c).owned.dom() == Set::<FrameId>::empty(),
        alloc_enabled(p, frame),
        guest != 0,
        page_aligned_4k(gpa),
        concrete_step_enabled(c, MapUnmapStep::Map { guest, gpa, frame }),
    ensures
        alloc_ept_refines(
            c.concrete_map(guest, gpa, frame).concrete_unmap(guest, gpa),
            ghost_allocate(p, frame),
        ),
{
    lemma_allocate_preserves_alloc_ept_refines(c, p, frame);
    let p2 = ghost_allocate(p, frame);
    assert(alloc_map_enabled(c, p2, guest, gpa, frame));
    lemma_alloc_map_ok_refines(c, p2, guest, gpa, frame);
    let c2 = c.concrete_map(guest, gpa, frame);
    assert(concrete_step_enabled(c2, MapUnmapStep::Unmap { guest, gpa }));
    lemma_alloc_unmap_ok_refines(c2, p2, guest, gpa);
}

/// M5.9: precise identity window size matches `ept_hw::PRECISE_BYTES`.
pub proof fn lemma_precise_identity_frames()
    ensures
        PRECISE_IDENTITY_FRAMES == 131072,
        PRECISE_IDENTITY_FRAMES * PAGE_4K == 0x2000_0000,
{
}

/// M5.8/M5.9: identity leaf at aligned GPA uses matching HPA frame.
pub proof fn lemma_identity_leaf_gpa_eq_hpa(gpa: Gpa)
    requires
        in_precise_identity(gpa),
    ensures
        identity_leaf_ok(gpa, identity_frame_for_gpa(gpa)),
        identity_frame_for_gpa(gpa) == gpa / PAGE_4K,
{
}

/// Concrete 0-GPA identity leaf (bring-up).
pub proof fn lemma_identity_leaf_zero()
    ensures
        identity_leaf_ok(0, 0),
        in_precise_identity(0),
{
    lemma_identity_leaf_gpa_eq_hpa(0);
}

// ---------------------------------------------------------------------------
// M6.0 — EPT-violation handling preserves exclusive ownership (ADR-004)
// ---------------------------------------------------------------------------

/// Disposition taken by the EPT-violation / miss handler.
pub enum EptViolationDisposition {
    /// Emulate access without installing a mapping (APIC / virtio MMIO).
    EmulateNoMap,
    /// Fail-closed reject; ownership unchanged.
    Reject,
    /// Demand-fill: install a 4K mapping for the faulting GPA.
    ClaimMap { guest: GuestId, gpa: Gpa, frame: FrameId },
}

/// Enabled when ClaimMap would be a legal map step; emulate/reject always OK.
pub open spec fn violation_enabled(m: GhostEptMap, d: EptViolationDisposition) -> bool {
    match d {
        EptViolationDisposition::EmulateNoMap => true,
        EptViolationDisposition::Reject => true,
        EptViolationDisposition::ClaimMap { guest, gpa, frame } =>
            guest != 0 && page_aligned_4k(gpa) && step_enabled(
                m,
                MapUnmapStep::Map { guest, gpa, frame },
            ),
    }
}

pub open spec fn apply_violation(m: GhostEptMap, d: EptViolationDisposition) -> GhostEptMap {
    match d {
        EptViolationDisposition::EmulateNoMap => m,
        EptViolationDisposition::Reject => m,
        EptViolationDisposition::ClaimMap { guest, gpa, frame } => m.ghost_map(guest, gpa, frame),
    }
}

/// Emulate / reject leave the ownership registry unchanged.
pub proof fn lemma_violation_noop_preserves_exclusive(m: GhostEptMap, d: EptViolationDisposition)
    requires
        exclusive_ownership(m),
        d is EmulateNoMap || d is Reject,
    ensures
        exclusive_ownership(apply_violation(m, d)),
        apply_violation(m, d) == m,
{
}

/// Demand-fill ClaimMap preserves exclusivity (uses 4K map lemma).
pub proof fn lemma_violation_claim_preserves_exclusive(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        exclusive_ownership(m),
        violation_enabled(m, EptViolationDisposition::ClaimMap { guest, gpa, frame }),
    ensures
        exclusive_ownership(
            apply_violation(m, EptViolationDisposition::ClaimMap { guest, gpa, frame }),
        ),
        apply_violation(m, EptViolationDisposition::ClaimMap { guest, gpa, frame }).owned[frame]
            == guest,
{
    lemma_map_ok_exclusive(m, guest, gpa, frame);
}

/// M6.0: every enabled EPT-violation disposition preserves exclusive ownership.
pub proof fn theorem_ept_violation_preserves_exclusive(m: GhostEptMap, d: EptViolationDisposition)
    requires
        exclusive_ownership(m),
        violation_enabled(m, d),
    ensures
        exclusive_ownership(apply_violation(m, d)),
{
    match d {
        EptViolationDisposition::EmulateNoMap => {
            lemma_violation_noop_preserves_exclusive(m, d);
        },
        EptViolationDisposition::Reject => {
            lemma_violation_noop_preserves_exclusive(m, d);
        },
        EptViolationDisposition::ClaimMap { guest, gpa, frame } => {
            lemma_violation_claim_preserves_exclusive(m, guest, gpa, frame);
        },
    }
}

/// Concrete post: emulate then claim still exclusive.
pub proof fn lemma_ept_violation_emulate_then_claim(
    m: GhostEptMap,
    guest: GuestId,
    gpa: Gpa,
    frame: FrameId,
)
    requires
        exclusive_ownership(m),
        guest != 0,
        page_aligned_4k(gpa),
        step_enabled(m, MapUnmapStep::Map { guest, gpa, frame }),
    ensures
        exclusive_ownership(
            apply_violation(
                apply_violation(m, EptViolationDisposition::EmulateNoMap),
                EptViolationDisposition::ClaimMap { guest, gpa, frame },
            ),
        ),
{
    lemma_violation_noop_preserves_exclusive(m, EptViolationDisposition::EmulateNoMap);
    let m2 = apply_violation(m, EptViolationDisposition::EmulateNoMap);
    assert(violation_enabled(m2, EptViolationDisposition::ClaimMap { guest, gpa, frame }));
    theorem_ept_violation_preserves_exclusive(
        m2,
        EptViolationDisposition::ClaimMap { guest, gpa, frame },
    );
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
/// M5.7: large-page (2M/1G) L3 map/unmap exclusivity (no `admit`).
pub const M5_LPAGE_VERIFY_OK_MARKER: &str = "RAYNU-V-M5-LPAGE-VERIFY-OK";
/// M5.8: NUMA domains in ghost *spec* (SRAT/SLIT; affinity L3 → M6).
pub const M5_NUMA_OK_MARKER: &str = "RAYNU-V-M5-NUMA-OK";
/// M5.9: allocator↔EPT coupled refine + scoped identity correspondence.
pub const M5_ALLOC_REFINE_OK_MARKER: &str = "RAYNU-V-M5-ALLOC-REFINE-OK";
/// M6.0: EPT-violation handling preserves exclusive ownership (no `admit`).
pub const M6_EPTVIO_OK_MARKER: &str = "RAYNU-V-M6-EPTVIO-OK";
