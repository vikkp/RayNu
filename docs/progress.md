# RayNu-V Progress

Lived status for closed gates. Roadmap weeks stay in [CLAUDE.md](../CLAUDE.md); this file tracks what has actually shipped.

## Closed gates (Latitude + QEMU)

| Gate | Marker | Notes |
|------|--------|-------|
| M0 | `RAYNU-V-M0-BOOT-OK` | UEFI EFI, COM1 banner |
| M1.0 | `RAYNU-V-M1-EBS-OK` | ExitBootServices + bump pool |
| M1.1 | `RAYNU-V-M1-VMXON-OK` | Real VMXON / VMXOFF |
| M1.2 | `RAYNU-V-M1-VMEXIT-OK` | VMLAUNCH → HLT VMEXIT |
| M2.0 | `RAYNU-V-M2-EPT-OK` | 4 GiB EPT identity (1G/2M) |
| M2.1 | `RAYNU-V-M2-GUEST-OK` | Guest store + loop + HLT; host verify |
| M2.2 | `RAYNU-V-M2-OWN-OK` | ADR-004 exclusive-ownership self-test |
| M2.3 | `RAYNU-V-M2-ALLOC-OK` | Proven Core bitmap `FrameAllocator` |
| M2.4 | `RAYNU-V-M2-IRQ-OK` | Inject vector 0x21 → guest ISR ack + HLT |
| M2.5 | `RAYNU-V-M2-TIMER-OK` | LAPIC one-shot → ext-IRQ VMEXIT → EOI → re-inject |
| M2.6 | `RAYNU-V-M2-L2-OK` | Host L2 specs + Kani harnesses for EptMap / FrameAllocator |
| M3.0 | `RAYNU-V-M3-IO-OK` | Guest COM1 `out dx,al` → I/O VMEXIT → host UART |
| M3.1 | `RAYNU-V-M3-CPUID-OK` | CPUID exiting; leaf 1 hides VMX from guest |
| M3.2 | `RAYNU-V-M3-LOAD-OK` | Synthetic kernel/initrd + packed `boot_params` (HdrS) |

## Verification checkpoint (as of M3.2)

| Module | Maturity | Notes |
|--------|----------|-------|
| `memory/ept` ownership registry | **L2** | Ghost model in `ept_spec.rs`; L1 runtime kept |
| `memory/frame_allocator` | **L2** | Ghost allocated-set in `frame_allocator_spec.rs`; L1 runtime kept |
| `sched/interrupt` | L1 | Vector firewall + VM-entry pack for inject |
| `sched/msr_firewall` | L0→L1-ish | CPUID filter (hide VMX); MSR stub allow-list |
| `devices/serial_pio` | L0→L1-ish | COM1 OUT passthrough + magic latch (outside Proven Core) |
| `guest/linux_boot` | L0 | boot_params packing + synthetic load (outside Proven Core) |
| `arch/apic` | L0 | Host LAPIC one-shot + EOI (outside Proven Core) |
| `memory/ept_hw` identity builder | L0→L1-ish | Bring-up scaffold; precise per-GPA maps later |
| `vmx/*` | L0–L1 | Lifecycle + launch + VMRESUME inject / timer / I/O / CPUID |
| Verus proofs (`*_proof.rs`) | L0 | L3 deferred M3+ |
| Kani in CI | Soft-fail best-effort | Harnesses: no HPA alias; alloc integrity |

## Next

1. **M3.3** earlyprintk (`RAYNU-V-M3-EARLY-OK`) — see [m3_plan.md](m3_plan.md).
2. M3.4 guest timer → M3.5 shell.
3. Verus L3 proofs for EPT / allocator (parallel).
