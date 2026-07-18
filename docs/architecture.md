# RayNu-V Architecture Overview

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.

## Single Binary [Z]

Everything links into one `r640-hypervisor.efi` (PE/COFF). Non-critical assets are planned as lazy-decompressed PE sections (ADR-003). Target size 15 MB; hard limit 20 MB.

Boot path today (M2.5 closed): UEFI entry → ExitBootServices → bump pool → `FrameAllocator` → VMXON → EPT → ownership (code/stack/IDT) → VMLAUNCH → software inject ISR → arm LAPIC one-shot → guest HLT wait → external-IRQ VMEXIT → EOI → re-inject ISR → `RAYNU-V-M2-TIMER-OK` → VMXOFF. Later: Linux guest.

Lived gate history: [docs/progress.md](progress.md).

## Subsystems

| Directory | Role | Proven Core? |
|-----------|------|--------------|
| `boot/` | Firmware handoff, CPU bring-up, serial | Outside |
| `vmx/` | VMXON/OFF, VMCS, entry/exit | **Inside** |
| `memory/` | Physical frames, page tables, EPT | **Inside** |
| `sched/` | Runstate / credit scheduler; hosts vCPU + IPI stubs | Mixed† |
| `audit/` | Ring buffer, hash chain (integrity) | **Inside** (integrity) |
| `devices/` | Serial, RTC, virtio, passthrough | Outside |
| `net/` | vSwitch, VLAN, SR-IOV awareness | Outside |
| `mgmt/` | CLI / REST / Web UI / lifecycle | Outside |
| `migrate/` | vCenter / VMDK / OVF (ADR-007) | Outside |
| `idrac/` | Redfish Tier 1/2 (ADR-005) | Outside |
| `arch/` | x86 / R640 helpers | Outside |

† Scheduler algorithms are outside the Proven Core. vCPU state save/restore and IPI confinement live under `sched/` as Proven Core modules (four-file convention). See [ADR-002](adr/ADR-002.md).

## Proven Core Boundary [V]

Only security-critical modules receive Verus specs and proofs. Default is **outside**. Promotion/demotion requires an ADR and LOC budget update (hard limit 15,000 LOC including scaffolding).

Headline theorem (ADR-004): every valid EPT GPA→HPA mapping is **exclusively owned** by one guest and belongs to neither the hypervisor nor any other guest.

Maturity levels L0→L3 are defined in [ADR-006](adr/ADR-006.md). Scaffolding ships at **L0** (documented invariants only).

## Conflict Resolution

When pillars conflict: safety ([V] architecture) > correctness ([A] audit trail) > simplicity ([Z]) > hardware depth ([D]).

## Next Milestone Gate (M2 continue)

**M2.6 gate:** `RAYNU-V-M2-L2-OK` (host) — L2 ghost specs for `EptMap` + `FrameAllocator`, Kani harnesses, `memory::run_l2_gate`. No new serial boot marker. Next after close: M3 Linux.
