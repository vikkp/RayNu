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
| M2.4 | `RAYNU-V-M2-IRQ-OK` | Inject vector 0x21 → guest ISR ack + HLT (pending Latitude) |

## Verification checkpoint (as of M2.4)

| Module | Maturity | Notes |
|--------|----------|-------|
| `memory/ept` ownership registry | L1 | Runtime self-test + audit `EptMapped` |
| `memory/frame_allocator` | L1 | Alloc / free / double-free / reuse self-test |
| `sched/interrupt` | L1 | Vector firewall + VM-entry pack for inject |
| `memory/ept_hw` identity builder | L0→L1-ish | Bring-up scaffold; precise per-GPA maps later |
| `vmx/*` | L0–L1 | Lifecycle + launch + VMRESUME inject path |
| Verus / Kani in CI | Soft-fail scaffold | ADR-001 / ADR-008 |

## Next

1. Latitude gate for M2.4 (`RAYNU-V-M2-IRQ-OK`).
2. Deeper IRQ (APIC timer / EOI) or ADR-004 + allocator toward L2 / Kani.
3. M3: unmodified Linux guest.
