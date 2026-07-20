# Post–M3.10 Plan — Harden Real Linux Guest

**Status:** M3.11–M3.22 **closed**; next track is **M4** — [m4_plan.md](m4_plan.md).  
**Parent:** [m3_plan.md](m3_plan.md) · lived gates: [progress.md](progress.md)

M3’s first-shell goal is closed. Post-shell harden delivered L3, refine, NOIRQ, tight EPT, Kani, and PE asset embed. Active: M4 platform work ([m4_plan.md](m4_plan.md)).

---

## Debt left by M3.10

| Crutch | Why it exists | Cost |
|--------|---------------|------|
| Host LAPIC one-shot → inject IRQ0 | `nolapic` + refined-jiffies need ticks | Host timer clobber; not guest APIC |
| `nolapic noapic` cmdline | Identity EPT aliases GPA `0xFEE00000` onto host APIC | No guest local APIC / IOAPIC |
| IRQ0/IRQ4 software inject | PIC path under `noapic` | Wrong long-term model |
| CPUID SHELL hypercall | ttyS0 TX IRQ-driven; stalls after 1 byte | Not a real userspace console path |
| 4 GiB identity EPT (1G/2M) | Fast bring-up | Blocks APIC hole + precise ownership |
| COM1-only I/O bitmaps | Avoid unconditional I/O storm | PIT/PIC go to L0; flaky calibrate |

---

## Subgates

Each = branch `cursor/m3-N-…-a623`, marker, Latitude (or host) gate, docs touch.

### M3.11 — Guest APIC timer — `RAYNU-V-M3-GTIMER3-OK`

**Status: closed on Latitude** (`Boot gate PASSED (M0 → M3.11)`).

**Shipped:**

1. **EPT hole** at GPA `0xFEE00000` + guest CR3 walk for MMIO insn fetch.
2. Virtual xAPIC MMIO / x2APIC MSRs (`devices/lapic_virt.rs`); CPUID shows APIC+x2APIC, hides TSC-deadline; `APIC_BASE` shadowed.
3. Internal TSC countdown latches `GTIMER3`; guest-visible `CUR_COUNT` stuck so calibrate fails closed (keeps IRQ0 path for SHELL).
4. Cmdline: **`nolapic` removed** (keep `noapic`).
5. **LVT inject deferred to M3.12** (bare inject panicked without IRR/ISR).

**Files:** `memory/ept_hw.rs`, `devices/lapic_virt.rs`, `vmx/guest_pt.rs`, `vmx/mmio_decode.rs`, `vmx/launch.rs`, `guest/linux_boot.rs`, `tools/qemu-boot-test.sh`.

### M3.12 — Faithful APIC inject + drop IRQ crutches — `RAYNU-V-M3-APIC-OK`

**Status: closed on Latitude** (`Boot gate PASSED (M0 → M3.12)`).

**Shipped:**

1. Virtual IRR/ISR + EOI; real `CUR_COUNT`; host one-shot → IRR → interrupt-window LVT inject.
2. MMIO decode fix for Linux `native_apic_mem_eoi` SIB abs disp32 (was panicking on EOI).
3. IRQ0 kept through SHELL for calibrate verification jiffies; IRQ4 COM1 TX retained; `noapic` stays.
4. Gate: `APIC-OK` + `SHELL` (+ retained `GTIMER3`).

**Files:** `devices/lapic_virt.rs`, `vmx/mmio_decode.rs`, `vmx/launch.rs`, `tools/qemu-boot-test.sh`.

### M3.13 — Precise EPT slice — `RAYNU-V-M3-EPT2-OK`

**Status: closed on Latitude** (`Boot gate PASSED (M0 → M3.13)`).

**Shipped:**

1. Precise identity EPT `[0, 1 GiB)` (QEMU `-m 1G`; covers e820/memmap RAM + UEFI CR3).
2. Local APIC `0xFEE00000` unmapped by omission (no hole punch).
3. ADR-004 range claim for the precise window (`claim_precise_identity_ranges`).
4. Gate: `EPT2-OK` + retained APIC/SHELL/GTIMER3 chain.

**Files:** `memory/ept_hw.rs`, `memory/ept.rs`, `src/main.rs`, `tools/qemu-boot-test.sh`.

### M3.14 — Verus L3 attempt (ADR-004) — `RAYNU-V-M3-L3-OK`

**Status: closed** — host `RAYNU-V-M3-L3-OK`; Latitude regression `Boot gate PASSED (M0 → M3.13)`.

**Shipped:**

1. `ept_proof.rs` L3 *attempt*: exclusivity lemmas for 4K single-guest map/unmap + explicit GAP list.
2. Host gate `memory/l3_gate.rs` (retains L2 floor; does not claim ADR-006 L3 — Verus unpinned).
3. Docs: ADR-004 / progress / `verus-version.toml` note the attempt vs machine-checked L3.
4. Kani stays soft-fail.
5. Latitude: M3.13 QEMU chain still green on this branch (EPT2 + GTIMER3 + APIC + SHELL).

**Files:** `memory/ept_proof.rs`, `memory/l3_gate.rs`, `memory/l3_gate_test.rs`, `memory/ept_spec.rs`, `memory/mod.rs`, `verus-version.toml`.

---

## True L3 track (ADR-004 / ADR-006 / ADR-008)

Host-only gates. Scope through M3.17: **4K, single guest, map/unmap exclusivity**. Out of band until M4–M6: N guests, large pages, EPT violation, migration, HW PTE correspondence.

```
M3.15 Verus pin  →  M3.16 Verus-linkable model  →  M3.17 green verify (true L3)
```

### M3.15 — Pin Verus toolchain — `RAYNU-V-M3-VERUS-OK`

**Status: closed** — host/CI + Latitude `./tools/verus-smoke.sh` → `RAYNU-V-M3-VERUS-OK`.

**Shipped:**

1. Frozen weekly Verus `0.2026.07.12.0b42f4c` in `verus-version.toml` — exact `tag` + 40-char `commit` + `sha256_linux` (never `latest` / rolling).
2. `tools/install-verus.sh` downloads that asset only, checks sha256 + `version.json` commit, installs Rust `1.96.0`.
3. `tools/verus-smoke.sh` runs `verus --version` + `cargo verus verify` (crate not opted into proofs yet — M3.16).
4. Host pin gate `memory/verus_gate.rs` + CI asserts pin fields then hard-passes `verus-smoke`.
5. Full Proven Core proof discharge deferred to M3.17 (now closed for scoped exclusivity).

**Files:** `verus-version.toml`, `tools/install-verus.sh`, `tools/verus-smoke.sh`, `memory/verus_gate.rs`, `memory/verus_gate_test.rs`, `.github/workflows/ci.yml`.

### M3.16 — Verus-linkable EptMap — `RAYNU-V-M3-L3-LINK-OK`

**Status: closed** — host/CI + Latitude `./tools/verus-link-smoke.sh` → `RAYNU-V-M3-L3-LINK-OK`.

**Shipped:**

1. Host-only `ept_model` crate with `package.metadata.verus.verify = true` (not linked into EFI).
2. `verus!` ghost model: `GhostEptMap`, `exclusive_ownership`, map/unmap lemmas + target theorem.
3. Incomplete inductive bodies used `admit()` until M3.17 discharged them.
4. `tools/verus-link-smoke.sh` + host gate `memory/l3_link_gate.rs` + CI `verus-link` job.

**Files:** `ept_model/`, `tools/verus-link-smoke.sh`, `memory/l3_link_gate.rs`, `Cargo.toml` (workspace), `.github/workflows/ci.yml`.

### M3.17 — True L3 verify — `RAYNU-V-M3-L3-VERIFY-OK`

**Status: closed** — host/CI + Latitude `./tools/verus-verify-smoke.sh` → `RAYNU-V-M3-L3-VERIFY-OK`.

**Shipped:**

1. Green `cargo verus verify -p ept_model` with **no `admit()`** on map/unmap exclusivity lemmas + `theorem_single_guest_4k_map_unmap_exclusive`.
2. `tools/verus-verify-smoke.sh` rejects `admit(` and requires a positive verified count.
3. Host gate `memory/l3_verify_gate.rs` + CI `verus-verify` hard-fail job.
4. **ADR-006 L3** for scoped ghost property (4K, single guest, map/unmap). Live `EptMap` stays L2 until M3.18 refinement.

**Files:** `ept_model/src/lib.rs`, `tools/verus-verify-smoke.sh`, `memory/l3_verify_gate.rs`, `.github/workflows/ci.yml`.

---

## Post-L3 track (M3.18–M3.22)

Numbered gates after true L3. Each = branch `cursor/m3-N-…-a623`, marker, Latitude and/or host gate, docs touch. Out of band until **M4**: N guests, large pages, migration proofs.

```
M3.18 refine  →  M3.19 drop IRQ crutches  →  M3.20 tighter EPT
                 M3.21 Kani harden (parallel)
                 M3.22 PE assets (parallel)
```

### M3.18 — Ghost↔exec refinement — `RAYNU-V-M3-L3-REFINE-OK`

**Status: closed** — host/CI + Latitude `./tools/verus-refine-smoke.sh` → `RAYNU-V-M3-L3-REFINE-OK`.

**Shipped:**

1. `ConcreteEptMap` + `abs` / `refines` in `ept_model`; map/unmap commute with ghost under abs.
2. `theorem_concrete_single_guest_4k_refine` discharged (no `admit`).
3. Host gate `memory/l3_refine_gate.rs` (live `EptMap` correspondence) + CI `verus-refine` job.
4. Closed `GAP(CLOSED M3.18)` in `ept_proof.rs`.

**Out of scope (still open):** N guests, large pages, HW PTE identity builder, frame-allocator coupling (M4+).

**Files:** `ept_model/src/lib.rs`, `tools/verus-refine-smoke.sh`, `memory/l3_refine_gate.rs`, `.github/workflows/ci.yml`.

### M3.19 — No IRQ4 + earlyprintk-only console — `RAYNU-V-M3-NOIRQ-OK`

**Status: closed** — Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M3.19)`.

Dropping **both** ISA software injects stalled Linux (APIC calibrate needs IRQ0
jiffies; `console=ttyS0` needs IRQ4 TX). Shipped policy:

1. **Dropped IRQ4** COM1 TX software inject (`try_inject_linux_com1_tx` gone).
2. SHELL latches via CPUID (`note_shell_cpuid`); cmdline omits `console=ttyS0`
   (earlyprintk only).
3. **IRQ0 only until SHELL** — APIC calibrate jiffies; stops at `guest_shell_ok()`.
4. Marker `RAYNU-V-M3-NOIRQ-OK`; `qemu-boot-test.sh` pass line M0→M3.19.
5. **`noapic` retained** — IOAPIC still stubbed (future work).

**Files:** `vmx/launch.rs`, `vmx/noirq_gate.rs`, `devices/serial_pio.rs`,
`guest/linux_boot.rs`, `tools/qemu-boot-test.sh`.

### M3.20 — Tighter EPT windows — `RAYNU-V-M3-EPT3-OK`

**Status: closed** — Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M3.20)`.

**Shipped:**

1. Precise identity shrinks to `[0, 512 MiB)` via `build_identity_2m_bytes` (2M leaves).
2. QEMU `-m 512M` so machine RAM matches the EPT window (guest e820 stays 256 MiB).
3. ADR-004 `claim_precise_identity_ranges` uses `PRECISE_BYTES`; asserts window `< 1 GiB`.
4. Emit `RAYNU-V-M3-EPT2-OK` + `RAYNU-V-M3-EPT3-OK`; APIC remains unmapped-by-omission.
5. Host gate `memory/ept3_gate.rs`; `qemu-boot-test.sh` pass line M0→M3.20.

**Files:** `memory/ept_hw.rs`, `memory/ept.rs`, `memory/ept3_gate.rs`, `src/main.rs`,
`tools/run-qemu.sh`, `tools/qemu-boot-test.sh`.

### M3.21 — Harden Kani CI — `RAYNU-V-M3-KANI-OK`

**Status: closed** — CI `kani (M3.21 hard-fail)` green + Latitude `./tools/kani-smoke.sh`.

**Shipped:**

1. Pin `kani-verifier` **0.67.0** in `kani-version.toml`.
2. CI job hard-fails via `./tools/kani-smoke.sh` (`cargo kani --lib --tests`).
3. Harnesses bounded: `#[kani::unwind(16)]`; under `cfg(kani)` `MAP_CAP=8`.
4. Skip UEFI `[[bin]]` (needs `uefi-bin`) — was the soft-fail root cause.
5. Host gate `memory/kani_gate.rs` → marker `RAYNU-V-M3-KANI-OK`.

**Files:** `tools/kani-smoke.sh`, `kani-version.toml`, `.github/workflows/ci.yml`,
`memory/ept.rs`, `memory/ept_test.rs`, `memory/frame_allocator_test.rs`,
`memory/kani_gate.rs`, `src/lib.rs`.

### M3.22 — PE `.assets.*` embed — `RAYNU-V-M3-ASSETS-OK`

**Status: closed** — Latitude `./tools/qemu-boot-test.sh` → `Boot gate PASSED (M0 → M3.22)`.

**Shipped:**

1. Embed `assets/bzImage` + `assets/initrd` as PE sections `.askern` / `.asinit`
   (8-char COFF aliases for ADR-003 `.assets.kernel` / `.assets.initrd`).
2. Boot prefers PE → ESP → runtime minimal → synthetic.
3. Emit `RAYNU-V-M3-ASSETS-OK` when PE embed is present.
4. `tools/check-pe-assets.sh` + `build.sh` verify sections; `check-size.sh` still enforces 15/20 MB.
5. Host gate `boot/assets_gate.rs`; qemu pass line M0→M3.22.

**Deferred:** zstd, webui/schemas/vmconfigs (still under budget without them).

**Files:** `boot/pe_assets.rs`, `boot/assets_gate.rs`, `src/main.rs`, `tools/build.sh`,
`tools/check-pe-assets.sh`, `tools/qemu-boot-test.sh`, `docs/adr/ADR-003.md`.

---

## Execution order

```
M3.11 → … → M3.21 Kani (closed) → M3.22 assets (closed)
→ M4 (usable VM platform) — see m4_plan.md
```

**M3.22 closed on Latitude. M4.0–M4.1 closed. Next: [M4.2](m4_plan.md) (`RAYNU-V-M4-NVM-OK`).**

---

## M3.13 acceptance (met on Latitude)

```text
RAYNU-V-M3-EPT2-OK
RAYNU-V-M3-GTIMER3-OK
RAYNU-V-M3-APIC-OK
RAYNU-V-M3-SHELL-OK
==> Boot gate PASSED (M0 → M3.13; qemu status=33)
```

## M3.14 acceptance (met)

```text
RAYNU-V-M3-L3-OK
==> Host L3-attempt gate PASSED (cargo test; Verus still unpinned)

# Latitude no-regression (same branch):
==> Boot gate PASSED (M0 → M3.13; qemu status=33)
```

## M3.15 acceptance (met)

```text
RAYNU-V-M3-VERUS-OK
==> Verus pin smoke PASSED (M3.15)
# host CI + Latitude ~/raynu
```

## M3.16 acceptance (met)

```text
RAYNU-V-M3-L3-LINK-OK
==> Verus L3-link smoke PASSED (M3.16)
# cargo verus verify -p ept_model → 7 verified, 0 errors (admit gaps OK)
# host CI + Latitude ~/raynu
```

## M3.17 acceptance (met on Latitude)

```text
RAYNU-V-M3-L3-VERIFY-OK
==> Verus L3-verify smoke PASSED (M3.17)
# cargo verus verify -p ept_model → 13 verified, 0 errors (no admit)
# host CI + Latitude ~/raynu
```

## M3.18 acceptance (met on Latitude)

```text
RAYNU-V-M3-L3-REFINE-OK
==> Verus L3-refine smoke PASSED (M3.18)
# cargo verus verify -p ept_model → 22 verified, 0 errors (no admit)
# host CI + Latitude ~/raynu
```

## M3.19 acceptance (met on Latitude)

```text
RAYNU-V-M3-NOIRQ-OK
RAYNU-V-M3-APIC-OK
RAYNU-V-M3-SHELL-OK
==> M3.19 NOIRQ marker found (no IRQ4; IRQ0 until SHELL)
==> Boot gate PASSED (M0 → M3.19; qemu status=33)
# host CI + Latitude ~/raynu
```

## M3.20 acceptance (met on Latitude)

```text
RAYNU-V-M3-EPT3-OK
RAYNU-V-M3-NOIRQ-OK
RAYNU-V-M3-APIC-OK
==> M3.19 NOIRQ marker found (no IRQ4; IRQ0 until SHELL)
==> Boot gate PASSED (M0 → M3.20; qemu status=33)
# host CI + Latitude ~/raynu
```

## M3.21 acceptance (met on CI + Latitude)

```text
Complete - 2 successfully verified harnesses, 0 failures, 2 total.
RAYNU-V-M3-KANI-OK
==> Kani smoke PASSED (M3.21)
# CI job kani (M3.21 hard-fail) + Latitude ~/raynu
```

## M3.22 acceptance (met on Latitude)

```text
RAYNU-V-M3-ASSETS-OK
RAYNU-V-M3-NOIRQ-OK
RAYNU-V-M3-APIC-OK
==> M3.19 NOIRQ marker found (no IRQ4; IRQ0 until SHELL)
==> Boot gate PASSED (M0 → M3.22; qemu status=33)
# host CI + Latitude ~/raynu
```
