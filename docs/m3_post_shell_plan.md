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

**Status: closed on Latitude** (`Boot gate PASSED (M0 → M3.12)`).

**Shipped:**

1. Virtual IRR/ISR + EOI; real `CUR_COUNT`; host one-shot → IRR → interrupt-window LVT inject.
2. MMIO decode fix for Linux `native_apic_mem_eoi` SIB abs disp32 (was panicking on EOI).
3. IRQ0 kept through SHELL for calibrate verification jiffies; IRQ4 COM1 TX retained; `noapic` stays.
4. Gate: `APIC-OK` + `SHELL` (+ retained `GTIMER3`).

**Files:** `devices/lapic_virt.rs`, `vmx/mmio_decode.rs`, `vmx/launch.rs`, `tools/qemu-boot-test.sh`.

### M3.13 — Precise EPT slice — `RAYNU-V-M3-EPT2-OK`

**In progress** on `cursor/m3-13-precise-ept-a623`.

- Replace full 4 GiB identity with precise `[0, 1 GiB)` (QEMU `-m 1G`; covers e820/memmap RAM + UEFI CR3)
- Local APIC `0xFEE00000` unmapped by omission (no hole punch)
- ADR-004 range claim for the precise window; still boots to SHELL + APIC-OK

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
M3.11 guest APIC timer  →  M3.12 APIC inject  →  M3.13 precise EPT
                              ↘
                         M3.14 Verus L3 (parallel)
```

**Now executing: M3.13.**

---

## M3.12 acceptance (met on Latitude)

```text
RAYNU-V-M3-GTIMER3-OK
RAYNU-V-M3-APIC-OK
RAYNU-V-M3-SHELL-OK
==> Boot gate PASSED (M0 → M3.12; qemu status=33)
```
