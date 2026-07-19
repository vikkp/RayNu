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
| M3.3 | `RAYNU-V-M3-EARLY-OK` | 64-bit proto-kernel entry; Linux-style early serial |
| M3.4 | `RAYNU-V-M3-GTIMER-OK` | Post-proto guest timer → EOI → inject |
| M3.5 | `RAYNU-V-M3-SHELL-OK` | Proto-init shell marker; **synthetic M3 closed** |
| M3.6 | `RAYNU-V-M3-LOOP-OK` | Continuous HLT exit loop after shell; fuller GPR save |
| M3.7 | `RAYNU-V-M3-BZIMAGE-OK` | ESP/embedded bzImage parse+place; entry at PM+0x200 |
| M3.8 | `RAYNU-V-M3-LINUX-EARLY-OK` | Real tinyconfig Linux earlyprintk banner on COM1 |
| M3.9 | `RAYNU-V-M3-GTIMER2-OK` | MSR allow-list emulate + post-banner host LAPIC |
| M3.10 | `RAYNU-V-M3-SHELL-OK` | Real `/init` on initrd; CPUID SHELL hypercall (Latitude) |
| M3.11 | `RAYNU-V-M3-GTIMER3-OK` | Virtual APIC + EPT hole; `nolapic` dropped (Latitude) |
| M3.12 | `RAYNU-V-M3-APIC-OK` | IRR/ISR LVT inject + EOI decode; SHELL (Latitude) |
| M3.13 | `RAYNU-V-M3-EPT2-OK` | Precise `[0,1GiB)` EPT + range claims; SHELL (Latitude) |
| M3.14 | `RAYNU-V-M3-L3-OK` | Host Verus L3 *attempt* (4K single-guest lemmas + gaps); Latitude M0→M3.13 still green |
| M3.15 | `RAYNU-V-M3-VERUS-OK` | Pinned Verus `0.2026.07.12.0b42f4c`; CI + Latitude smoke (`cargo verus`) |

## Verification checkpoint (as of M3.15; true L3 track)

| Module | Maturity | Notes |
|--------|----------|-------|
| `memory/ept` ownership registry | **L2** | Ghost model + M3.13 range claims; L3-attempt lemmas in `ept_proof.rs` |
| `memory/frame_allocator` | **L2** | Ghost allocated-set in `frame_allocator_spec.rs`; L1 runtime kept |
| `sched/interrupt` | L1 | Vector firewall + VM-entry pack; M3.9 GTIMER2 marker |
| `sched/msr_firewall` | L1-ish | CPUID filter + MSR classify; APIC_BASE shadow (M3.11) |
| `devices/serial_pio` | L0→L1-ish | COM1 OUT/IN + IO/EARLY/SHELL + LINUX-EARLY banner latch |
| `devices/lapic_virt` | L0→L1-ish | Virtual xAPIC/x2APIC; IRR/ISR + EOI; APIC-OK (M3.12) |
| `guest/linux_boot` | L0→L1-ish | Relocatable bzImage; 2 MiB-aligned `init_size` workspace |
| `boot/esp_assets` | L0 | Pre-EBS ESP `\EFI\BOOT\BZIMAGE` stage |
| `arch/apic` | L0 | Host LAPIC one-shot + EOI + mask (outside Proven Core) |
| `memory/ept_hw` identity builder | L1-ish | Precise `[0,1GiB)` identity; APIC unmapped by omission (M3.13) |
| `vmx/*` | L0–L1 | Real Linux through EPT2 + APIC + SHELL (M3.13) |
| Verus proofs (`ept_proof.rs`) | L3-attempt | Lemmas + GAP list (M3.14); toolchain pinned (M3.15) — not ADR-006 L3 yet |
| Verus toolchain | Pinned | `verus-version.toml` + `tools/verus-smoke.sh` → `RAYNU-V-M3-VERUS-OK` |
| Kani in CI | Soft-fail best-effort | Harnesses: no HPA alias; alloc integrity |

## Next

Post-shell plan: [m3_post_shell_plan.md](m3_post_shell_plan.md)

1. **M3.16** — Verus-linkable EptMap → `RAYNU-V-M3-L3-LINK-OK`
2. **M3.17** — green `cargo verus --verify` on exclusivity theorem → `RAYNU-V-M3-L3-VERIFY-OK` (true L3)
3. Later: drop IRQ0/IRQ4 crutches; tighter-than-1GiB EPT windows if needed.
