# RayNu-V Architecture Overview

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.

## Single Binary [Z]

Everything links into one `r640-hypervisor.efi` (PE/COFF). Non-critical assets are planned as lazy-decompressed PE sections (ADR-003). Target size 15 MB; hard limit 20 MB.

Boot path today (M3.13+ closed): UEFI entry → tiny bzImage + initrd → earlyprintk → guest APIC / GTIMER3 → real `/init` SHELL → VMXOFF. Precise EPT `[0,1 GiB)`. Verification: live EptMap **L2**; scoped ghost exclusivity **L3** (`ept_model`, M3.17, `RAYNU-V-M3-L3-VERIFY-OK`); Verus frozen at M3.15 (`verus-version.toml`).

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

Maturity levels L0→L3 are defined in [ADR-006](adr/ADR-006.md). Live EPT ownership + frame allocator are **L2**; scoped ghost exclusivity in `ept_model` is **L3** (M3.17, no `admit`). Lived checkpoint: [progress.md](progress.md).

## Conflict Resolution

When pillars conflict: safety ([V] architecture) > correctness ([A] audit trail) > simplicity ([Z]) > hardware depth ([D]).

## Next Milestone Gate

**M5.5 closed:** VMware inventory import → `RAYNU-V-M5-MIGRATE-OK`.  
**M6 closed** on Latitude (`RAYNU-V-M6-EXT-OK`; `80 verified, 0 errors`). Production-ready bar met.  
**Through M7.2 closed** (`RAYNU-V-M7-STORE-OK`). Next: **M7.3 ISO** — [m7_plan.md](m7_plan.md) · [ADR-009](adr/ADR-009.md) · [hda.md](hda.md) · [progress.md](progress.md).
