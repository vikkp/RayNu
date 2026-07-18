# RayNu-V Architecture Overview

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.

## Single Binary [Z]

Everything links into one `r640-hypervisor.efi` (PE/COFF). Non-critical assets are planned as lazy-decompressed PE sections (ADR-003). Target size 15 MB; hard limit 20 MB.

Boot path today (M2.4 closed): UEFI entry → ExitBootServices → bump pool → Proven Core `FrameAllocator` → VMXON → EPT identity map → ADR-004 ownership (code/stack/IDT) → VMLAUNCH (store + loop + HLT) → inject IRQ + VMRESUME → guest ISR ack + HLT → verify → VMXOFF. Later: APIC timer path, Linux guest.

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

**M2.4 closed** (`RAYNU-V-M2-IRQ-OK` on Latitude). Next: APIC timer / EOI (remaining R03), or deepen ADR-004 / allocator toward L2 + Kani.
