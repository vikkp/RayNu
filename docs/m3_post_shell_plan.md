# Post–M3.10 Plan — Harden Real Linux Guest

**Status:** M3.11–M3.15 closed; **true L3 track** next is M3.16 → M3.17.  
**Parent:** [m3_plan.md](m3_plan.md) · lived gates: [progress.md](progress.md)

M3’s first-shell goal is closed. Post-shell harden replaced APIC/EPT crutches, drafted the ADR-004 L3 attempt, and froze the Verus toolchain (exact tag + commit + sha256). Active track: link lemmas and discharge true L3. Parallel: drop IRQ0/IRQ4 when ready.

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
5. Full Proven Core proof discharge still deferred to M3.17.

**Files:** `verus-version.toml`, `tools/install-verus.sh`, `tools/verus-smoke.sh`, `memory/verus_gate.rs`, `memory/verus_gate_test.rs`, `.github/workflows/ci.yml`.

### M3.16 — Verus-linkable EptMap — `RAYNU-V-M3-L3-LINK-OK`

**Status: planned.**

**Goal:** Lemmas / ghost model are Verus-checkable (not prose-only): `verus!` path for map/unmap exclusivity; crate opts into verification. Marker when the linked model typechecks under the pinned Verus (proofs may still be `assume` / incomplete).

### M3.17 — True L3 verify — `RAYNU-V-M3-L3-VERIFY-OK`

**Status: planned.**

**Goal:** Green `cargo verus --verify` on `theorem_single_guest_4k_map_unmap_exclusive` → **ADR-006 L3** for that scope; CI hard-fails on verify; promote `EptMap` maturity from L2 / L3-attempt to L3.

### Parallel (any time)

- PE `.assets.*` embed (ADR-003) when size budget allows
- Harden Kani CI
- Drop IRQ0/IRQ4 crutches when lapic/serial fully own those paths
- Tighter-than-1 GiB EPT windows if needed

---

## Execution order

```
M3.11 → M3.12 → M3.13 → M3.14 (closed)
M3.15 Verus pin → M3.16 L3-link → M3.17 L3-verify   ← now
```

**M3.15 closed. Now executing: M3.16.**

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
