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
| M4.4 | `RAYNU-V-M4-NET-OK` | Dual virtio-net BARs + L2 vSwitch port0→port1 exchange (Latitude) |
| M4.5 | `RAYNU-V-M4-SMP-OK` | Dual-vCPU BSP+AP shared EPT; host AP wake (Latitude) |
| M4.6 | `RAYNU-V-M4-NGUEST-SPEC-OK` | N-guest exclusivity in ghost model (host) |
| M4.7 | `RAYNU-V-M4-NGUEST-VERIFY-OK` | True L3 N-guest verify; ADR-006 claim (CI + Latitude; M4 exit) |
| M4.8 | `RAYNU-V-M4-LPAGE-OK` | Large-page (2M/1G) ghost *spec* (CI + Latitude; L3 → M5) |
| M4.9 | `RAYNU-V-M4-REFINE-OK` | N-guest ghost↔exec refine (CI + Latitude) |
| M5.0 | `RAYNU-V-M5-LIFE-OK` | VM lifecycle API (CI + Latitude) |
| M5.1 | `RAYNU-V-M5-API-OK` | CLI + REST control plane (CI + Latitude) |
| M5.2 | `RAYNU-V-M5-WEBUI-OK` | Embedded Web UI SPA (CI + Latitude) |
| M5.3 | `RAYNU-V-M5-AUDIT-OK` | Audit ring + hash chain (CI + Latitude) |
| M5.4 | `RAYNU-V-M5-REPORT-OK` | SOX / ISO-style reports (CI + Latitude) |
| M5.5 | `RAYNU-V-M5-MIGRATE-OK` | VMware inventory import (CI + Latitude; ADR-007) |
| M5.6 | `RAYNU-V-M5-IDRAC-OK` | Dell Tier‑1 mock Redfish + topology (CI + Latitude) |
| M5.7 | `RAYNU-V-M5-LPAGE-VERIFY-OK` | Large-page L3 verify; `47 verified, 0 errors` (CI + Latitude) |
| M5.8 | `RAYNU-V-M5-NUMA-OK` | NUMA ghost *spec* (SRAT/SLIT); `51 verified, 0 errors` (CI + Latitude) |
| M5.9 | `RAYNU-V-M5-ALLOC-REFINE-OK` | Allocator↔EPT refine + identity abs; `61 verified, 0 errors` (CI + Latitude) |
| M6.0 | `RAYNU-V-M6-EPTVIO-OK` | EPT-violation exclusivity; `65 verified, 0 errors` (CI + Latitude) |
| M6.1 | `RAYNU-V-M6-HWPTE-OK` | HW PTE bit-decode; `72 verified, 0 errors` (CI + Latitude) |
| M6.2 | `RAYNU-V-M6-NUMA-L3-OK` | NUMA affinity L3; `77 verified, 0 errors` (CI + Latitude) |
| M6.3 | `RAYNU-V-M6-MIGRATE-XFER-OK` | Migrate page transfer; `80 verified, 0 errors` (CI + Latitude) |
| M6.4 | `RAYNU-V-M6-AUTH-OK` | REST auth (CI + Latitude) |
| M6.5 | `RAYNU-V-M6-PDF-OK` | PDF audit reports (CI + Latitude) |
| M6.6 | `RAYNU-V-M6-HA-OK` | HA failover + harden (CI + Latitude) |
| M6.7 | `RAYNU-V-M6-FAULT-OK` | Fault injection suite (CI + Latitude) |
| M6.8 | `RAYNU-V-M6-SOAK-OK` | 72-hr soak thresholds (CI + Latitude) |
| M6.9 | `RAYNU-V-M6-EXT-OK` | External audit + R09 review; `80 verified, 0 errors` (CI + Latitude) |
| M7.0 | `RAYNU-V-M7-SHIP-OK` | EFI release kit + SHA256 + USB/iDRAC runbook (CI + Latitude) |
| M7.1 | `RAYNU-V-M7-HTTP-OK` | Network HTTP codec + host TCP SPA/REST (CI + Latitude) |
| M7.2 | `RAYNU-V-M7-STORE-OK` | Datastore / image library + ESP catalog host path (CI + Latitude; UEFI persist stub) |
| M7.3 | `RAYNU-V-M7-ISO-OK` | ISO extract-boot plan + virtio install size (CI + Latitude host smoke; El Torito/CD-ROM stub) |
| M7.4 | `RAYNU-V-M7-UI-OK` | Create-VM fields + media SPA (CI + Latitude host smoke; console/TLS/NIC residual) |
| M7.5 | `RAYNU-V-R640-BOOT-OK` | Real PowerEdge R640 COM2: M0→SHELL→M4 BLK/NET/SMP (`v0.1.0-xsavesfix`, 2026-08-15); scaffold `RAYNU-V-M7-R640-SCAFFOLD-OK` |
| M7.6 | `RAYNU-V-M7-UEFI-HTTP-OK` | Real R640 PRE-EBS SNP+smoltcp HTTP (`10.99.99.127:8443`, 2026-08-16); scaffold `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK` |
| M7.7 | `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` | E5 iron stamp persist closed 2026-08-16 (Cruzer two-boot); scaffold `RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`; documented equiv. of `ISO-INSTALL-OK` |
| M7.8 | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | Real R640 post-`BOOT-OK` native BCM5720 HTTP (`10.99.99.144:8443`, 2026-08-20); SPA + `AuthAllowed`; keep APE PHY |
| E4 | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` | Real R640 SPA start → private-EPT SHELL `VMLAUNCH` + clear-state re-entry (`10.99.99.126:8443`, EFI `2b795a0`, 2026-08-21). Not TLS/distro. |
| E5 Stage 0 | `RAYNU-V-M7-E5-BOOT-SPEC-OK` | Host boot spec on REST/SPA + El Torito catalog parse (2026-08-22). Not attach. Not guest UEFI. |
| E5 Stage 1 | `RAYNU-V-M7-E5-CDROM-ATTACH-OK` | Host El Torito CD-ROM attach + REST/SPA (2026-08-22). Not firmware CD. Not VMLAUNCH. |
| E5 Stage 2 | `RAYNU-V-M7-E5-CDROM-FIRMWARE-OK` | Host firmware-facing CD arm + sector validate (2026-08-22). Not OVMF. Not VMLAUNCH. |
| E5 Stage 3 | `RAYNU-V-M7-E5-GUEST-FW-OK` | Host guest UEFI firmware envelope boxed under ADR-003 (2026-08-22). Not OVMF. Not VMLAUNCH. |
| E5 Stage 4 | `RAYNU-V-M7-E5-GUEST-FW-LOAD-OK` | Host identity-lazy stub payload load (2026-08-22). Not OVMF. Not VMLAUNCH. |
| E5 Stage 5 | `RAYNU-V-M7-E5-OVMF-PROBE-OK` | Host UEFI `_FVH` probe + ESP split-mode path (2026-08-22). Not embedded EDK2. Not VMLAUNCH. |
| E5 Stage 6 | `RAYNU-V-M7-E5-OVMF-ESP-OK` | Host ESP fixture load after probe (2026-08-22). Not embedded EDK2. Not VMLAUNCH. |
| E5 Stage 7 | `RAYNU-V-M7-E5-OVMF-SLOT-OK` | Host firmware slot 1 arm after ESP load (2026-08-22). Not VMLAUNCH. |
| E5 Stage 8 | `RAYNU-V-M7-E5-FW-BIND-OK` | Host firmware-to-guest bind after slot arm (2026-08-22). Not VMLAUNCH. |
| E5 Stage 9 | `RAYNU-V-M7-E5-FW-PREP-OK` | Host firmware launch-prepare after bind; mock VMLAUNCH refused (2026-08-22). |
| E5 Stage 10 | `RAYNU-V-M7-E5-FW-FLOOR-OK` | Host 4 KiB size-floor FV after prepare; not EDK2; VMLAUNCH refused (2026-08-22). |
| E5 Stage 11 | `RAYNU-V-M7-E5-FW-EDK2-OK` | Host 1 MiB EDK2-sized FV after floor; not a shipped OVMF.fd; VMLAUNCH not wired (2026-08-22). |
| E5 Stage 12 | `RAYNU-V-M7-E5-ESP-LAUNCH-OK` | Host ESP-path VMLAUNCH wired in launch.rs after EDK2; no live OVMF.fd; fixture refused (2026-08-22). |
| E5 Stage 13 | `RAYNU-V-M7-E5-ESP-MAP-OK` | Host live-sized ESP OVMF map (2 MiB+) after ESP launch; not a shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 14 | `RAYNU-V-M7-E5-RESET-VEC-OK` | Host reset-vector VMCS contract after live map; synthetic 0xEA stub not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 15 | `RAYNU-V-M7-E5-FW-ALIAS-OK` | Host firmware-alias EPT contract after reset-vector; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 16 | `RAYNU-V-M7-E5-ALIAS-EPT-OK` | Host alias-EPT program contract after firmware-alias; live EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 17 | `RAYNU-V-M7-E5-EPT-INSTALL-OK` | Host private alias-EPT install after program; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 18 | `RAYNU-V-M7-E5-REAL-ESP-OK` | Host real-ESP VMLAUNCH-ready contract after install; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 19 | `RAYNU-V-M7-E5-REAL-LAUNCH-OK` | Host guest-UEFI VMLAUNCH insn-path arm after qualify; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 20 | `RAYNU-V-M7-E5-LIVE-EXEC-OK` | Host live-ESP VMLAUNCH execute gate after insn arm; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 21 | `RAYNU-V-M7-E5-PRIV-VMCS-OK` | Host private guest-UEFI VMCS arm after live-ESP require; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 22 | `RAYNU-V-M7-E5-LIVE-ISSUE-OK` | Host live-ESP VMLAUNCH issue path after private VMCS; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 23 | `RAYNU-V-M7-E5-LIVE-BYTES-OK` | Host live-ESP bytes probe after live-issue; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 24 | `RAYNU-V-M7-E5-LIVE-FD-OK` | Host live-ESP FD require after live-bytes probe; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 25 | `RAYNU-V-M7-E5-LIVE-PRESENT-OK` | Host live-ESP present-attempt after live-FD require; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 26 | `RAYNU-V-M7-E5-LIVE-ADMIT-OK` | Host live-ESP admit-attempt after live-present; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 27 | `RAYNU-V-M7-E5-LIVE-READ-OK` | Host live-ESP read-attempt after live-admit; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 28 | `RAYNU-V-M7-E5-LIVE-COPY-OK` | Host live-ESP copy-attempt after live-read; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 29 | `RAYNU-V-M7-E5-LIVE-PLACE-OK` | Host live-ESP place-attempt after live-copy; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 30 | `RAYNU-V-M7-E5-LIVE-APPLY-OK` | Host live-ESP apply-attempt after live-place; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 31 | `RAYNU-V-M7-E5-LIVE-COMMIT-OK` | Host live-ESP commit-attempt after live-apply; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 32 | `RAYNU-V-M7-E5-LIVE-LATCH-OK` | Host live-ESP latch-attempt after live-commit; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 33 | `RAYNU-V-M7-E5-LIVE-SEAL-OK` | Host live-ESP seal-attempt after live-latch; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 34 | `RAYNU-V-M7-E5-LIVE-LOCK-OK` | Host live-ESP lock-attempt after live-seal; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 35 | `RAYNU-V-M7-E5-LIVE-HOLD-OK` | Host live-ESP hold-attempt after live-lock; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 36 | `RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK` | Real ESP OVMF.fd retained pre-EBS; presence rule closed; QEMU proof via system OVMF.fd; private VMCS not allocated; VMLAUNCH insn not issued (2026-08-22). |
| E5 Stage 37 | `RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK` | Private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP OVMF.fd; not E4 SHELL; first entry only; not installer (2026-08-22). |
| E5 Stage 38 | `RAYNU-V-M7-E5-OVMF-ALIVE-OK` | Past first triple-fault: CR4.VMXE host-owned; short resume loop; not full OVMF boot; not installer (2026-08-22). |
| E5 Stage 39 | `RAYNU-V-M7-E5-OVMF-PAST-SEC-OK` | Left SEC tail (last 64 KiB) + PEI PCI / firmware COM / HLT; COM1/COM2 forwarded; not full DXE; not installer (2026-08-22). |
| E5 Stage 40 | `RAYNU-V-M7-E5-OVMF-CDROM-OK` | `attach_cdrom_uefi` → GuestVisible; PCI IDE/ATAPI on the private VMCS; not full DXE; not installer (2026-08-22). |
| E5 Stage 41 | `RAYNU-V-M7-E5-OVMF-DXE-OK` | CMOS/fw_cfg/i440FX platform + EPT sink-resume; past-PEI/DXE or CD boot attempt; not installer (2026-08-22). |

## Verification checkpoint (as of M7.5 iron closed)

| Module | Maturity | Notes |
|--------|----------|-------|
| `memory/ept` ownership registry | **L2** runtime | Live registry + multi-hole precise ranges; L3 ghost (M3.18) for 4K |
| `memory/frame_allocator` | **L2** | Ghost allocated-set in `frame_allocator_spec.rs`; L1 runtime kept |
| `sched/interrupt` | L1 | Vector firewall + VM-entry pack; M3.9 GTIMER2 marker |
| `sched/msr_firewall` | L1-ish | CPUID filter + MSR classify; APIC_BASE shadow (M3.11) |
| `devices/serial_pio` | L0→L1-ish | COM1 OUT/IN + IO/EARLY/SHELL + LINUX-EARLY banner latch |
| `devices/lapic_virt` | L0→L1-ish | Virtual xAPIC/x2APIC; IRR/ISR + EOI; APIC-OK (M3.12) |
| `devices/virtio_blk` | L0→L1-ish | Virtio-mmio config/status; DRIVER_OK host write/readback (M4.3) |
| `devices/virtio_net` | L0→L1-ish | Dual virtio-mmio net BARs; DRIVER_OK → vSwitch exchange (M4.4) |
| `net::VSwitch` | L0→L1-ish | L2 MAC learning + unicast forward (M4.4) |
| `sched/smp_probe` | L0→L1-ish | Dual-vCPU BSP+AP ready flags; host AP wake (M4.5) |
| `guest/linux_boot` | L0→L1-ish | Relocatable bzImage; 2 MiB-aligned `init_size` workspace |
| `boot/esp_assets` | L0 | Pre-EBS ESP `\EFI\BOOT\BZIMAGE` stage |
| `arch/apic` | L0 | Host LAPIC one-shot + EOI + mask (outside Proven Core) |
| `memory/ept_hw` identity builder | L1-ish | Precise `[0,512MiB)` @ 2M (M3.20); APIC unmapped by omission |
| `vmx/*` | L0–L1 | Multi-VMCS + credit sched + blk/net/SMP probes (M4.5) |
| `memory/m4_2vm_gate` | L0 | Host artifact gate for dual-VMCS / dual-EPT path |
| `sched/scheduler` | L0→L1-ish | Credit quantum + fair pick; M4.1/M4.2 |
| `sched/m4_sched_gate` | L0 | Host artifact gate for dual-VMCS scheduling |
| `sched/m4_nvm_gate` | L0 | Host artifact gate for ≥4 concurrent guests |
| `devices/m4_blk_gate` | L0 | Host artifact gate for virtio-blk path |
| `devices/m4_net_gate` | L0 | Host artifact gate for virtio-net + vSwitch path |
| `sched/m4_smp_gate` | L0 | Host artifact gate for dual-vCPU SMP probe |
| Verus proofs (`ept_model`) | **L3** (scoped) | Through migrate-xfer (M6.3); `80 verified, 0 errors` at M6.9 auditor path |
| `memory/m4_nguest_spec_gate` | L0 | Host artifact gate for N-guest ghost exclusivity (M4.6) |
| `memory/m4_nguest_verify_gate` | L0 | Host artifact gate for N-guest ADR-006 L3 (M4.7) |
| `memory/m4_lpage_gate` | L0 | Host artifact gate for large-page ghost *spec* (M4.8) |
| `memory/m4_nguest_refine_gate` | L0 | Host artifact gate for N-guest concrete refine (M4.9) |
| `memory/m5_lpage_verify_gate` | L0 | Host artifact gate for large-page L3 (M5.7) |
| `memory/numa` / `m5_numa_gate` | L0 | Host NUMA view + artifact gate (M5.8); affinity L3 closed M6.2 |
| `memory/m5_alloc_refine_gate` | L0 | Host artifact gate for allocator↔EPT refine (M5.9) |
| `memory/m6_eptvio_gate` | L0 | Host artifact gate for EPT-violation exclusivity (M6.0) |
| `memory/m6_hwpte_gate` | L0 | Host artifact gate for HW PTE bit-decode (M6.1) |
| `memory/m6_numa_gate` | L0 | Host artifact gate for NUMA affinity L3 (M6.2) |
| `memory/m6_migrate_gate` | L0 | Host artifact gate for migrate page transfer (M6.3) |
| `mgmt/m6_auth_gate` | L0 | Host artifact gate for REST auth (M6.4) |
| `audit/m6_pdf_gate` | L0 | Host artifact gate for PDF reports (M6.5) |
| `mgmt/ha` / `m6_ha_gate` | L0 | Mock HA failover + harden checklist; HA-OK (M6.6) |
| `mgmt/fault` / `m6_fault_gate` | L0 | Fault injection suite; FAULT-OK (M6.7) |
| `mgmt/soak` / `m6_soak_gate` | L0 | 72-hr soak thresholds; SOAK-OK (M6.8) |
| `mgmt/ext` / `m6_ext_gate` | L0 | External audit + spec review; EXT-OK (M6.9) |
| `mgmt/ship` / `m7_ship_gate` | L0 | EFI release kit + SHA256 tarball; SHIP-OK (M7.0) |
| `mgmt/http` / `http_listen` / `m7_http_gate` | L0 | HTTP/1.1 codec + host TCP; HTTP-OK (M7.1) |
| `mgmt/tcp4_uefi` / `snp_*` / `m7_uefi_http_gate` | L0 | PRE-EBS Tcp4 + SNP residual; iron `RAYNU-V-M7-UEFI-HTTP-OK` (M7.6) |
| `mgmt/bcm5720*` / `host_nic_listen` / `m7_host_nic_gate` | L0 | Native BCM5720 Device; iron `RAYNU-V-M7-HOST-NIC-HTTP-OK` (M7.8 / E3b) |
| `mgmt/datastore` / `m7_store_gate` | L0 | Image library + ESP catalog host path; STORE-OK (M7.2); UEFI persist stub |
| `mgmt/iso` / `m7_iso_gate` | L0 | ISO extract-boot + virtio install plan; ISO-OK (M7.3 host smoke); CD-ROM stub |
| `mgmt/m7_ui_gate` / `webui` | L0 | Create-VM SPA fields + media; UI-OK (M7.4 host smoke); console residual |
| `mgmt/m7_r640_gate` | L0 | R640 scaffold + **iron closed** (`GAP(CLOSED M7.5)`; evidence `STATUS=closed`) |
| Verus toolchain | Frozen pin | Exact tag+commit+sha256 in `verus-version.toml`; CI never uses `latest` |
| `audit/integrity` | L0→L1-ish | Append-only ring + hash chain + tamper detect; AUDIT-OK (M5.3) |
| `audit/report` | L0 | SOX/ISO JSON/CSV/PDF from ring snapshot; REPORT-OK (M5.4); PDF-OK (M6.5) |
| `migrate/` | L0 | One-command OVF/VMDK inventory → VmTable; MIGRATE-OK (M5.5); live vCenter → polish |
| `idrac/` | L0 | Mock Redfish Tier‑1 + SMBIOS/ACPI topology; IDRAC-OK (M5.6) |
| Kani in CI | Hard-fail (M3.21) | Pin `0.67.0`; `./tools/kani-smoke.sh` → `RAYNU-V-M3-KANI-OK` |

## Next (numbered)

**M7.4 closed** on Latitude (`RAYNU-V-M7-UI-OK` — host package smoke; console/TLS residual).  
**M7.5 closed on iron:** `RAYNU-V-R640-BOOT-OK` — real R640 COM2 through SHELL + M4 (`v0.1.0-xsavesfix`, 2026-08-15).  
**M7.6 closed on iron:** `RAYNU-V-M7-UEFI-HTTP-OK` — SNP residual PRE-EBS HTTP on R640 (`10.99.99.127:8443`, 2026-08-16).  
**M7.7 closed on iron:** `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` — Cruzer Micro persist-detect + prefix-copy (2026-08-16). LBA stamps, not a distro installer.  
**M7.8 closed on iron:** `RAYNU-V-M7-HOST-NIC-HTTP-OK` — native BCM5720 after `BOOT-OK` on R640 (`10.99.99.144:8443`, 2026-08-20). SPA + Bearer `AuthAllowed`.  
**ADR-013 Phase F closed on iron:** coexist HTTP while VMX on (`10.99.99.149:8443`, EFI `0d06297b`, 2026-08-20). G0 scheduled; G1–G3 parked. Hold COM2: 25× `HOST-NIC-HTTP-OK`.  
**E4 SPA VMLAUNCH closed on iron:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK` — spec **201** + start **200** on `10.99.99.126:8443` (EFI `2b795a0`, 2026-08-21). First SPA `VMLAUNCH` + G0↔SPA clear-state re-entry via 98-field VMCS shadow. No error 7/11. Guest is SHELL CPUID, not a distro installer.  
**ADR-013 Phase G closed** 2026-08-21 as accepted-risk (shared LOM `:38` with virtio-net; Appendix B). Not VLAN / second NIC. Stage 1 is 0–G.  
**P0-15 / E5 Stage 0 closed (host):** typed boot spec on REST/SPA + El Torito catalog parse (`RAYNU-V-M7-E5-BOOT-SPEC-OK`, #172).  
**P0-16 / E5 Stage 1 closed (host):** host El Torito CD-ROM attach (`RAYNU-V-M7-E5-CDROM-ATTACH-OK`, #173).  
**P0-17 / E5 Stage 2 closed (host):** firmware-facing CD arm (`RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`). Not OVMF, not VMLAUNCH, not Everest E5.  
**P0-18 / E5 Stage 3 closed (host):** guest UEFI firmware envelope boxed (`RAYNU-V-M7-E5-GUEST-FW-OK`). Not OVMF, not VMLAUNCH, not Everest E5.  
**P0-19 / E5 Stage 4 closed (host):** guest firmware stub payload load (`RAYNU-V-M7-E5-GUEST-FW-LOAD-OK`). Not OVMF, not VMLAUNCH, not Everest E5.  
**P0-20 / E5 Stage 5 closed (host):** OVMF FV probe (`RAYNU-V-M7-E5-OVMF-PROBE-OK`). Not embedded EDK2, not VMLAUNCH, not Everest E5.  
**P0-21 / E5 Stage 6 closed (host):** ESP OVMF load (`RAYNU-V-M7-E5-OVMF-ESP-OK`). Not embedded EDK2, not VMLAUNCH, not Everest E5.  
**P0-22 / E5 Stage 7 closed (host):** firmware slot arm (`RAYNU-V-M7-E5-OVMF-SLOT-OK`). Not VMLAUNCH, not Everest E5.  
**P0-23 / E5 Stage 8 closed (host):** firmware-to-guest bind (`RAYNU-V-M7-E5-FW-BIND-OK`). Not VMLAUNCH, not Everest E5.  
**P0-24 / E5 Stage 9 closed (host):** firmware launch-prepare (`RAYNU-V-M7-E5-FW-PREP-OK`). Mock VMLAUNCH refused. Not Everest E5.  
**P0-25 / E5 Stage 10 closed (host):** firmware size-floor (`RAYNU-V-M7-E5-FW-FLOOR-OK`). 4 KiB fixture; not EDK2; VMLAUNCH refused. Not Everest E5.  
**P0-26 / E5 Stage 11 closed (host):** firmware EDK2-sized stage (`RAYNU-V-M7-E5-FW-EDK2-OK`). 1 MiB size-qualified candidate; not a shipped `OVMF.fd`; VMLAUNCH not wired. Not Everest E5.  
**P0-27 / E5 Stage 12 closed (host):** ESP-path guest UEFI VMLAUNCH (`RAYNU-V-M7-E5-ESP-LAUNCH-OK`). `try_vmlaunch_guest_uefi_ovmf` wired; no live `OVMF.fd`; fixture refused. Not Everest E5.  
**P0-28 / E5 Stage 13 closed (host):** live ESP OVMF map (`RAYNU-V-M7-E5-ESP-MAP-OK`). 2 MiB+ live-sized map recorded; not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-29 / E5 Stage 14 closed (host):** reset-vector VMCS contract (`RAYNU-V-M7-E5-RESET-VEC-OK`). SDM 9.1.4 CS=`0xF000` / RIP=`0xFFF0`; synthetic `0xEA` stub is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-30 / E5 Stage 15 closed (host):** firmware-alias EPT contract (`RAYNU-V-M7-E5-FW-ALIAS-OK`). Unrestricted-guest bit + 4 GiB alias recorded; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-31 / E5 Stage 16 closed (host):** alias-EPT program contract (`RAYNU-V-M7-E5-ALIAS-EPT-OK`). 4 GiB window recorded; live EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-32 / E5 Stage 17 closed (host):** private alias-EPT install (`RAYNU-V-M7-E5-EPT-INSTALL-OK`). Private window recorded; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-33 / E5 Stage 18 closed (host):** real-ESP VMLAUNCH-ready contract (`RAYNU-V-M7-E5-REAL-ESP-OK`). Real-ESP qualify recorded; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-34 / E5 Stage 19 closed (host):** guest-UEFI VMLAUNCH insn-path arm (`RAYNU-V-M7-E5-REAL-LAUNCH-OK`). Insn selected for real ESP only; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-35 / E5 Stage 20 closed (host):** live-ESP VMLAUNCH execute gate (`RAYNU-V-M7-E5-LIVE-EXEC-OK`). Live ESP bytes required before insn path; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-36 / E5 Stage 21 closed (host):** private guest-UEFI VMCS arm (`RAYNU-V-M7-E5-PRIV-VMCS-OK`). Private VMCS selected (not E4 SHELL); live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-37 / E5 Stage 22 closed (host):** live-ESP VMLAUNCH issue path (`RAYNU-V-M7-E5-LIVE-ISSUE-OK`). Issue path armed; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-38 / E5 Stage 23 closed (host):** live-ESP bytes probe (`RAYNU-V-M7-E5-LIVE-BYTES-OK`). Bytes probed; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-39 / E5 Stage 24 closed (host):** live-ESP FD require (`RAYNU-V-M7-E5-LIVE-FD-OK`). Real ESP `OVMF.fd` required; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-40 / E5 Stage 25 closed (host):** live-ESP present-attempt (`RAYNU-V-M7-E5-LIVE-PRESENT-OK`). Real ESP `OVMF.fd` bytes presented; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-41 / E5 Stage 26 closed (host):** live-ESP admit-attempt (`RAYNU-V-M7-E5-LIVE-ADMIT-OK`). Real ESP `OVMF.fd` bytes admitted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-42 / E5 Stage 27 closed (host):** live-ESP read-attempt (`RAYNU-V-M7-E5-LIVE-READ-OK`). Real ESP `OVMF.fd` bytes read-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-43 / E5 Stage 28 closed (host):** live-ESP copy-attempt (`RAYNU-V-M7-E5-LIVE-COPY-OK`). Real ESP `OVMF.fd` bytes copy-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-44 / E5 Stage 29 closed (host):** live-ESP place-attempt (`RAYNU-V-M7-E5-LIVE-PLACE-OK`). Real ESP `OVMF.fd` bytes place-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-45 / E5 Stage 30 closed (host):** live-ESP apply-attempt (`RAYNU-V-M7-E5-LIVE-APPLY-OK`). Real ESP `OVMF.fd` bytes apply-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-46 / E5 Stage 31 closed (host):** live-ESP commit-attempt (`RAYNU-V-M7-E5-LIVE-COMMIT-OK`). Real ESP `OVMF.fd` bytes commit-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-47 / E5 Stage 32 closed (host):** live-ESP latch-attempt (`RAYNU-V-M7-E5-LIVE-LATCH-OK`). Real ESP `OVMF.fd` bytes latch-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-48 / E5 Stage 33 closed (host):** live-ESP seal-attempt (`RAYNU-V-M7-E5-LIVE-SEAL-OK`). Real ESP `OVMF.fd` bytes seal-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-49 / E5 Stage 34 closed (host):** live-ESP lock-attempt (`RAYNU-V-M7-E5-LIVE-LOCK-OK`). Real ESP `OVMF.fd` bytes lock-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-50 / E5 Stage 35 closed (host):** live-ESP hold-attempt (`RAYNU-V-M7-E5-LIVE-HOLD-OK`). Real ESP `OVMF.fd` bytes hold-attempted; live ESP bytes still absent; live E4 SHELL EPT not written; 4 MiB fixture is not a shipped `OVMF.fd`; VMLAUNCH insn not issued. Not Everest E5.  
**P0-51 / E5 Stage 36 closed (host + QEMU):** real ESP `OVMF.fd` retain (`RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK`). Presence is true only for accepted retained bytes. QEMU stages a system `OVMF.fd`. Private VMCS is not allocated. VMLAUNCH insn not issued. No further `*Absent` bookkeeping stages. Not Everest E5.  
**P0-52 / E5 Stage 37 closed (host + QEMU):** private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP `OVMF.fd` (`RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`). Not the E4 SHELL VMCS/EPT. First entry only. Host `cargo test` still does not execute the instruction. Not installer. Not Everest E5.  
**P0-53 / E5 Stage 38 closed (host + QEMU):** OVMF past first triple-fault (`RAYNU-V-M7-E5-OVMF-ALIVE-OK`). Root cause: SEC `mov cr4, 0x640` cleared VMXE → `#GP` → TF. Host-owns CR4.VMXE (same as E4 Linux). 32 MiB low RAM + short resume. Not full OVMF boot. Not installer. Not Everest E5.  
**P0-54 / E5 Stage 39 closed (host + QEMU):** OVMF past SEC (`RAYNU-V-M7-E5-OVMF-PAST-SEC-OK`). Linear left last 64 KiB + PEI PCI config / firmware COM / HLT. COM1/COM2 forwarded. Not full DXE. Not installer. Not Everest E5.  
**P0-55 / E5 Stage 40 closed (host + QEMU):** guest-UEFI CD visible (`RAYNU-V-M7-E5-OVMF-CDROM-OK`). `attach_cdrom_uefi` after FirmwareArmed is GuestVisible. PCI IDE/ATAPI on the private VMCS. Unarmed path stays `UnsupportedOnFirmware`. Not full DXE. Not installer. Not Everest E5.  
**P0-56 / E5 Stage 41 (host + QEMU):** PEI/DXE platform or CD boot attempt (`RAYNU-V-M7-E5-OVMF-DXE-OK`). Honest CMOS/fw_cfg RAM size + i440FX host bridge + EPT sink-resume for the Stage 40 `0xFCF8_F000` stall. Not a completed firmware CD boot. Not installer. Not Everest E5.  
Plan: [m7_plan.md](m7_plan.md) · HDA: [hda.md](hda.md) · ADR-013: [adr/ADR-013.md](adr/ADR-013.md) · ADR-014: [adr/ADR-014.md](adr/ADR-014.md) · evidence: [evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md](evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md)

| Gate | Marker | Goal |
|------|--------|------|
| E3b Durable HTTP | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | **CLOSED** 2026-08-20 on BCM5720 `:38` after `BOOT-OK`. SNP/Tcp4 do not count. |
| Post-EBS SNP | `RAYNU-V-M7-POST-EBS-HTTP-OK` | **Rejected.** Hang + curl timeout + RSOD 2026-08-17. |
| ADR-013 Phase F | coexist HTTP while VMX on | **CLOSED** 2026-08-20 on BCM5720 `:38` / `10.99.99.149:8443`. G0 scheduled; G1–G3 parked. Hold COM2: 25× HTTP-OK. |
| E4 SPA VMLAUNCH | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` | **CLOSED** 2026-08-21 on BCM5720 `:38` / `10.99.99.126:8443`. Private 2M EPT SHELL + shadow restore re-entry. |
| ADR-013 Phase G | shared LOM vs virtio-net | **CLOSED** 2026-08-21 as accepted-risk note ([ADR-013](adr/ADR-013.md) Appendix B). Host HTTP + guest virtio share `:38`. Not 802.1Q / dedicated NIC. |
| P0-15 / E5 Stage 0 | `RAYNU-V-M7-E5-BOOT-SPEC-OK` | **CLOSED (host, #172).** Boot spec on the wire + El Torito catalog parse. Not attach. |
| P0-16 / E5 Stage 1 | `RAYNU-V-M7-E5-CDROM-ATTACH-OK` | **CLOSED (host, #173).** Host El Torito CD-ROM attach. Not guest UEFI VMLAUNCH. |
| P0-17 / E5 Stage 2 | `RAYNU-V-M7-E5-CDROM-FIRMWARE-OK` | **CLOSED (host).** Firmware-facing CD arm + sector validate. Not OVMF, not VMLAUNCH, not Everest E5. |
| P0-18 / E5 Stage 3 | `RAYNU-V-M7-E5-GUEST-FW-OK` | **CLOSED (host).** Guest UEFI firmware envelope boxed under ADR-003. Not OVMF, not VMLAUNCH, not Everest E5. |
| P0-19 / E5 Stage 4 | `RAYNU-V-M7-E5-GUEST-FW-LOAD-OK` | **CLOSED (host).** Identity-lazy stub payload load. Not OVMF, not VMLAUNCH, not Everest E5. |
| P0-20 / E5 Stage 5 | `RAYNU-V-M7-E5-OVMF-PROBE-OK` | **CLOSED (host).** UEFI `_FVH` probe + ESP split-mode path. Not embedded EDK2, not VMLAUNCH, not Everest E5. |
| P0-21 / E5 Stage 6 | `RAYNU-V-M7-E5-OVMF-ESP-OK` | **CLOSED (host).** ESP fixture load after probe. Not embedded EDK2, not VMLAUNCH, not Everest E5. |
| P0-22 / E5 Stage 7 | `RAYNU-V-M7-E5-OVMF-SLOT-OK` | **CLOSED (host).** Firmware slot 1 arm after ESP load. Not VMLAUNCH, not Everest E5. |
| P0-23 / E5 Stage 8 | `RAYNU-V-M7-E5-FW-BIND-OK` | **CLOSED (host).** Firmware-to-guest bind after slot arm. Not VMLAUNCH, not Everest E5. |
| P0-24 / E5 Stage 9 | `RAYNU-V-M7-E5-FW-PREP-OK` | **CLOSED (host).** Firmware launch-prepare after bind. Mock VMLAUNCH refused. Not Everest E5. |
| P0-25 / E5 Stage 10 | `RAYNU-V-M7-E5-FW-FLOOR-OK` | **CLOSED (host).** 4 KiB size-floor FV after prepare. Not EDK2. VMLAUNCH refused. Not Everest E5. |
| P0-26 / E5 Stage 11 | `RAYNU-V-M7-E5-FW-EDK2-OK` | **CLOSED (host).** 1 MiB EDK2-sized FV after floor. Not a shipped OVMF.fd. VMLAUNCH not wired. Not Everest E5. |
| P0-27 / E5 Stage 12 | `RAYNU-V-M7-E5-ESP-LAUNCH-OK` | **CLOSED (host).** ESP-path VMLAUNCH wired in launch.rs. No live OVMF.fd. Fixture refused. Not Everest E5. |
| P0-28 / E5 Stage 13 | `RAYNU-V-M7-E5-ESP-MAP-OK` | **CLOSED (host).** Live-sized ESP OVMF map (2 MiB+). Not a shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-29 / E5 Stage 14 | `RAYNU-V-M7-E5-RESET-VEC-OK` | **CLOSED (host).** Reset-vector VMCS contract. Synthetic 0xEA stub not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-30 / E5 Stage 15 | `RAYNU-V-M7-E5-FW-ALIAS-OK` | **CLOSED (host).** Firmware-alias EPT contract. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-31 / E5 Stage 16 | `RAYNU-V-M7-E5-ALIAS-EPT-OK` | **CLOSED (host).** Alias-EPT program contract. Live EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-32 / E5 Stage 17 | `RAYNU-V-M7-E5-EPT-INSTALL-OK` | **CLOSED (host).** Private alias-EPT install. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-33 / E5 Stage 18 | `RAYNU-V-M7-E5-REAL-ESP-OK` | **CLOSED (host).** Real-ESP VMLAUNCH-ready contract. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-34 / E5 Stage 19 | `RAYNU-V-M7-E5-REAL-LAUNCH-OK` | **CLOSED (host).** Guest-UEFI VMLAUNCH insn-path arm. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-35 / E5 Stage 20 | `RAYNU-V-M7-E5-LIVE-EXEC-OK` | **CLOSED (host).** Live-ESP VMLAUNCH execute gate. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-36 / E5 Stage 21 | `RAYNU-V-M7-E5-PRIV-VMCS-OK` | **CLOSED (host).** Private guest-UEFI VMCS arm. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-37 / E5 Stage 22 | `RAYNU-V-M7-E5-LIVE-ISSUE-OK` | **CLOSED (host).** Live-ESP VMLAUNCH issue path. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-38 / E5 Stage 23 | `RAYNU-V-M7-E5-LIVE-BYTES-OK` | **CLOSED (host).** Live-ESP bytes probe. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-39 / E5 Stage 24 | `RAYNU-V-M7-E5-LIVE-FD-OK` | **CLOSED (host).** Live-ESP FD require. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-40 / E5 Stage 25 | `RAYNU-V-M7-E5-LIVE-PRESENT-OK` | **CLOSED (host).** Live-ESP present-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-41 / E5 Stage 26 | `RAYNU-V-M7-E5-LIVE-ADMIT-OK` | **CLOSED (host).** Live-ESP admit-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-42 / E5 Stage 27 | `RAYNU-V-M7-E5-LIVE-READ-OK` | **CLOSED (host).** Live-ESP read-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-43 / E5 Stage 28 | `RAYNU-V-M7-E5-LIVE-COPY-OK` | **CLOSED (host).** Live-ESP copy-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-44 / E5 Stage 29 | `RAYNU-V-M7-E5-LIVE-PLACE-OK` | **CLOSED (host).** Live-ESP place-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-45 / E5 Stage 30 | `RAYNU-V-M7-E5-LIVE-APPLY-OK` | **CLOSED (host).** Live-ESP apply-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-46 / E5 Stage 31 | `RAYNU-V-M7-E5-LIVE-COMMIT-OK` | **CLOSED (host).** Live-ESP commit-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-47 / E5 Stage 32 | `RAYNU-V-M7-E5-LIVE-LATCH-OK` | **CLOSED (host).** Live-ESP latch-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-48 / E5 Stage 33 | `RAYNU-V-M7-E5-LIVE-SEAL-OK` | **CLOSED (host).** Live-ESP seal-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-49 / E5 Stage 34 | `RAYNU-V-M7-E5-LIVE-LOCK-OK` | **CLOSED (host).** Live-ESP lock-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-50 / E5 Stage 35 | `RAYNU-V-M7-E5-LIVE-HOLD-OK` | **CLOSED (host).** Live-ESP hold-attempt. Live E4 SHELL EPT not written. 4 MiB fixture not shipped OVMF.fd. VMLAUNCH insn not issued. Not Everest E5. |
| P0-51 / E5 Stage 36 | `RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK` | **CLOSED (host + QEMU).** Real ESP OVMF.fd retained. Presence rule closed. Private VMCS not allocated. VMLAUNCH insn not issued. No further *Absent bookkeeping. Not Everest E5. |
| P0-52 / E5 Stage 37 | `RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK` | **CLOSED (host + QEMU).** Private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP OVMF.fd. Not E4 SHELL. First entry only. Not installer. Not Everest E5. |
| P0-53 / E5 Stage 38 | `RAYNU-V-M7-E5-OVMF-ALIVE-OK` | **CLOSED (host + QEMU).** Past first triple-fault. CR4.VMXE host-owned. Not full OVMF boot. Not installer. Not Everest E5. |
| P0-54 / E5 Stage 39 | `RAYNU-V-M7-E5-OVMF-PAST-SEC-OK` | **CLOSED (host + QEMU).** Left SEC tail + PEI PCI / firmware COM / HLT. COM1/COM2 forwarded. Not full DXE. Not installer. Not Everest E5. |
| P0-55 / E5 Stage 40 | `RAYNU-V-M7-E5-OVMF-CDROM-OK` | **CLOSED (host + QEMU).** `attach_cdrom_uefi` → GuestVisible. PCI IDE/ATAPI on the private VMCS. Not full DXE. Not installer. Not Everest E5. |
| P0-56 / E5 Stage 41 | `RAYNU-V-M7-E5-OVMF-DXE-OK` | **Host + QEMU.** CMOS/fw_cfg/i440FX + EPT sink-resume. Past-PEI/DXE or CD boot attempt. Not installer. Not Everest E5. |
| Everest residual | virtio-blk + boot order CD→disk + TLS/console + distro installer | After P0-56. Product ISO: [ADR-014](adr/ADR-014.md). Firmware can attempt this CD; virtio disk is next. |
| M8 (sketch) | — | vMotion-like · DRS-like · hot-add (after M7) |
| Optional | Dell Tier‑2 / pin upgrades | Slip-ok — see [m6_plan.md](m6_plan.md) / ADR-005 |
