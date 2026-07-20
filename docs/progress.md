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
| M3.15 | `RAYNU-V-M3-VERUS-OK` | Frozen Verus `0.2026.07.12.0b42f4c` (tag + commit + sha256); CI + Latitude smoke |
| M3.16 | `RAYNU-V-M3-L3-LINK-OK` | Host-only `ept_model` `verus!` linked; CI + Latitude |
| M3.17 | `RAYNU-V-M3-L3-VERIFY-OK` | True L3: exclusivity lemmas discharged (no `admit`); CI + Latitude `13 verified, 0 errors` |
| M3.18 | `RAYNU-V-M3-L3-REFINE-OK` | Ghost↔exec refine; CI + Latitude `22 verified, 0 errors` |
| M3.19 | `RAYNU-V-M3-NOIRQ-OK` | Dropped IRQ4 inject; IRQ0 only until SHELL; no `console=ttyS0` (Latitude) |
| M3.20 | `RAYNU-V-M3-EPT3-OK` | Tight EPT `[0,512MiB)` @ 2M; QEMU `-m 512M` (Latitude) |
| M3.21 | `RAYNU-V-M3-KANI-OK` | Hard-fail Kani CI pin `0.67.0`; 2 harnesses (CI + Latitude) |
| M3.22 | `RAYNU-V-M3-ASSETS-OK` | PE `.askern`/`.asinit` embed; ESP fallback (Latitude) |
| M4.0 | `RAYNU-V-M4-2VM-OK` | G0 Linux SHELL + G1 SHELL under distinct EPT (dual VMCS; Latitude) |
| M4.1 | `RAYNU-V-M4-SCHED-OK` | Credit scheduler time-slices G0↔G1 (Latitude) |
| M4.2 | `RAYNU-V-M4-NVM-OK` | G0 Linux + G1–G3 SHELL (≥4 concurrent; Latitude) |
| M4.3 | `RAYNU-V-M4-BLK-OK` | Virtio-mmio BAR + probe guest; DRIVER_OK write/readback (Latitude) |

## Verification checkpoint (as of M4.3)

| Module | Maturity | Notes |
|--------|----------|-------|
| `memory/ept` ownership registry | **L2** runtime | Live registry + multi-hole precise ranges; L3 ghost (M3.18) for 4K |
| `memory/frame_allocator` | **L2** | Ghost allocated-set in `frame_allocator_spec.rs`; L1 runtime kept |
| `sched/interrupt` | L1 | Vector firewall + VM-entry pack; M3.9 GTIMER2 marker |
| `sched/msr_firewall` | L1-ish | CPUID filter + MSR classify; APIC_BASE shadow (M3.11) |
| `devices/serial_pio` | L0→L1-ish | COM1 OUT/IN + IO/EARLY/SHELL + LINUX-EARLY banner latch |
| `devices/lapic_virt` | L0→L1-ish | Virtual xAPIC/x2APIC; IRR/ISR + EOI; APIC-OK (M3.12) |
| `devices/virtio_blk` | L0→L1-ish | Virtio-mmio config/status; DRIVER_OK host write/readback (M4.3) |
| `guest/linux_boot` | L0→L1-ish | Relocatable bzImage; 2 MiB-aligned `init_size` workspace |
| `boot/esp_assets` | L0 | Pre-EBS ESP `\EFI\BOOT\BZIMAGE` stage |
| `arch/apic` | L0 | Host LAPIC one-shot + EOI + mask (outside Proven Core) |
| `memory/ept_hw` identity builder | L1-ish | Precise `[0,512MiB)` @ 2M (M3.20); APIC unmapped by omission |
| `vmx/*` | L0–L1 | 4 VMCS (G0 Linux + G1–G3 SHELL) + credit sched + blk probe (M4.3) |
| `memory/m4_2vm_gate` | L0 | Host artifact gate for dual-VMCS / dual-EPT path |
| `sched/scheduler` | L0→L1-ish | Credit quantum + fair pick; M4.1/M4.2 |
| `sched/m4_sched_gate` | L0 | Host artifact gate for dual-VMCS scheduling |
| `sched/m4_nvm_gate` | L0 | Host artifact gate for ≥4 concurrent guests |
| `devices/m4_blk_gate` | L0 | Host artifact gate for virtio-blk path |
| Verus proofs (`ept_model`) | **L3** (scoped) | Exclusivity (M3.17) + concrete refine (M3.18); no `admit` |
| Verus toolchain | Frozen pin | Exact tag+commit+sha256 in `verus-version.toml`; CI never uses `latest` |
| Kani in CI | Hard-fail (M3.21) | Pin `0.67.0`; `./tools/kani-smoke.sh` → `RAYNU-V-M3-KANI-OK` |

## Next (numbered)

M4 plan (platform spine → proof bolt-on): [m4_plan.md](m4_plan.md)  
Prior track: [m3_post_shell_plan.md](m3_post_shell_plan.md)

| Gate | Marker | Goal |
|------|--------|------|
| **M4.4** ← next | `RAYNU-V-M4-NET-OK` | Virtio-net + minimal vSwitch |
| M4.5 | `RAYNU-V-M4-SMP-OK` | SMP guest (2+ vCPUs); slip-ok vs blk/net |
| M4.6 | `RAYNU-V-M4-NGUEST-SPEC-OK` | N-guest exclusivity in ghost model |
| M4.7 | `RAYNU-V-M4-NGUEST-VERIFY-OK` | True L3 N-guest verify (M4 exit) |
| M4.8 | `RAYNU-V-M4-LPAGE-OK` | Large-page in ghost spec (proof → M5) |
| M4.9 | `RAYNU-V-M4-REFINE-OK` | N-guest ghost↔exec refine |
| M5 | — | Operationally viable (see CLAUDE.md) |
