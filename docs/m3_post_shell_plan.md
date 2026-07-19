# Post–M3.10 Plan — Harden Real Linux Guest

**Status:** active after Latitude `RAYNU-V-M3-SHELL-OK` (M0→M3.10).  
**Parent:** [m3_plan.md](m3_plan.md) · lived gates: [progress.md](progress.md)

M3’s first-shell goal is closed. This plan replaces bring-up crutches with guest-owned mechanisms and starts ADR-004 L3 / precise EPT work.

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

- Virtual IRR/ISR + EOI so guest LVT timer inject does not panic (`Fatal exception in interrupt` on Latitude)
- Emulate enough 8259 *or* move serial to polling/virt path so ttyS0 TX works without IRQ4 inject
- Remove CPL0-only IRQ0 inject; remove interrupt-window TX hack if APIC owns it
- Cmdline: drop `noapic` when IOAPIC stub exists **or** document PIC-only stay

Marker: durable Linux run with APIC timer inject + SHELL without host→IRQ0.

### M3.13 — Precise EPT slice — `RAYNU-V-M3-EPT2-OK`

- Replace full 4 GiB identity with: identity for RAM windows from e820 + explicit MMIO holes
- Ownership registry claims for every mapped GPA (ADR-004)
- Host tests + Latitude: still boots to SHELL

### M3.14 — Verus L3 attempt (ADR-004) — host marker / doc gate

- `ept_proof.rs` L3 attempt for 4K single-guest map/unmap exclusivity
- Document gaps; Kani stays soft-fail until green
- No Latitude requirement if proofs are host-only

### Parallel (any time)

- PE `.assets.*` embed (ADR-003) when size budget allows
- Harden Kani CI
- Site copy: “unmodified Linux to init”

---

## Execution order

```
M3.11 guest APIC timer  →  M3.12 drop IRQ crutches  →  M3.13 precise EPT
                              ↘
                         M3.14 Verus L3 (parallel after M3.11 lands)
```

**Now executing: M3.12.**

---

## M3.11 acceptance (met on Latitude)

```text
RAYNU-V-M3-GTIMER3-OK
RAYNU-V-M3-SHELL-OK
==> Boot gate PASSED (M0 → M3.11; qemu status=33)
```
