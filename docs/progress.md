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
| E5 Stage 41 | `RAYNU-V-M7-E5-OVMF-DXE-OK` | **CLOSED** nested VT-x: CMOS/fw_cfg + i440FX at `00:08.0` + IDE at `00:00.0` (PEI DID `0x7010`) + EPT sink-resume; `OVMF-CDROM-OK` pci_ide=1 sectors=0; post-DXE tail then E4; not installer (2026-08-23). |
| E5 Stage 42 | `RAYNU-V-M7-E5-OVMF-VIRTIO-OK` | **CLOSED** nested VT-x: PEI DID `00:00.0` `val=0x1042`; `OVMF-VIRTIO-OK` pci=1; CD GuestVisible; `pci_ide=0` sectors=0; stop n=115 virtio=1; not installer (2026-08-23). |
| E5 Stage 43 | `RAYNU-V-M7-E5-OVMF-BOTH-OK` | **CLOSED** nested VT-x `1b07692`: `pci select 00:00.01` `val=0x70108086`; `OVMF-BOTH-OK`; stop n=1111 `pci_ide=1 virtio=1` `sectors=0` `spin=1`; E4 #DF fail-soft; not installer (2026-08-23). |
| E5 Stage 44 | `RAYNU-V-M7-E5-OVMF-ATAPI-OK` | **CLOSED** iron COM2 `bf696ca`: `sectors=1` `packet=9` `scsi=0x28` stop n=30769 `pci_ide=1 virtio=1`. Not El Torito. Not installer (2026-08-27). |
| E5 Stage 45 | `RAYNU-V-M7-E5-OVMF-ELTORITO-OK` | **CLOSED** iron COM2 `0be7283`: `RN-ELT` + `OVMF-ELTORITO-OK` n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8 com=6. Not installer (2026-08-27). |

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
**P0-56 / E5 Stage 41 closed (host + QEMU nested VT-x):** PEI/DXE platform or CD boot attempt (`RAYNU-V-M7-E5-OVMF-DXE-OK`). raynuvsrv1: `pci cfg=0x80000002 val=0x7010`, `OVMF-CDROM-OK` pci_ide=1 sectors=0, `OVMF-DXE-OK` plat=1 ram_rip=1, stop n=115, E4 SHELL fail-soft. Not a completed firmware CD boot. Not installer. Not Everest E5.  
**P0-57 / E5 Stage 42 closed (host + QEMU nested VT-x):** empty virtio-blk + boot order CD then disk (`RAYNU-V-M7-E5-OVMF-VIRTIO-OK`). raynuvsrv1: `pci cfg=0x80000002 val=0x1042`, `OVMF-VIRTIO-OK` pci=1 boot=CD,disk, CD GuestVisible, `OVMF-DXE-OK` plat=1 ram_rip=1, stop n=115 `pci_ide=0 virtio=1` sectors=0, E4 SHELL fail-soft. Boot gate PASSED. Not a completed firmware CD boot. Not installer. Not Everest E5.  
**P0-58 / E5 Stage 43 closed (host + QEMU nested VT-x):** firmware-simultaneous PCI enum (`RAYNU-V-M7-E5-OVMF-BOTH-OK`). Nested VT-x `1b07692`: `pci select 00:00.01`, `cfg=0x80000100 val=0x70108086`, Header Type `0x8000000c val=0x800000`, `OVMF-BOTH-OK`, stop n=1111 `pci_ide=1 virtio=1` `sectors=0` `spin=1` `bdfs=3`. CpuDeadLoop `eb f3` skip let PciBus walk `00:00.1`. E4 Linux #DF fail-soft (3-attempt). Simultaneous enum ≠ firmware CD boot ≠ installer. Not Everest E5.  
**P0-59 / E5 Stage 44 closed (iron COM2 `bf696ca`):** firmware ATAPI READ (`RAYNU-V-M7-E5-OVMF-ATAPI-OK`). `sectors=1` `packet=9` `scsi=0x28` `ata=0xa0` `ataio=982` stop n=30769 `pci_ide=1 virtio=1`; BOTH-OK n=12411 virtio `00:02.0` + IDE `00:00.1`; `00:00.0` stays i440FX `0x1237` (no AcpiTimerLib ASSERT). E4 SHELL then M4.2 G1 EPT `GPA=0x10403000` fail-soft is not Stage 44. Not El Torito boot. Not installer. Not Everest E5.  
**P0-61 / E5 Stage 45 closed (iron COM2 `0be7283`):** firmware El Torito CD EFI (`RAYNU-V-M7-E5-OVMF-ELTORITO-OK`). `firmware-serial begin` then `RN-ELT`; n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8 com=6 insn=`ee31c0c3`. ATAPI-OK n=30769 then catalog=1 ~n=33280 then bootimg=1 ~n=38656. 2048-byte FAT12 ESP + ISO9660 `\EFI\BOOT\BOOTX64.EFI`; PE SectionAlignment `0x1000`; 262144-exit cap. E4 LINUX-EARLY then M4.2 G1 EPT fail-soft is not Stage 45. Not installer. Not Everest E5.  
**P0-60 closed (iron COM2 after `5147222`, not an E5 stage):** G0 SHELL then G1 `RAYNU-V-M4-SHELL-G1` / `RAYNU-V-M4-2VM-OK` (`EPTP=0x1040601e` CPUID `rip=0x1040000a`); G2/G3 latched; no `GPA=0x10403000`. Stage 45 El Torito held (`RN-ELT` n=197992). **G0 VMCS relocate closed** (`E4 G0 VMCS relocated HPA=0x10a00000`; `M4-NVM-OK`). **M4.3 host-slab closed** on iron COM2 after `22e28d0`: `M4-BLK-OK` `guest_code=0x10c00000`; then `M4-NET-OK` / `M4-SMP-OK` / `R640-BOOT-OK` / Phase F coexist. `ISO-BOOTED-FROM-DISK` is persist-detect, not a distro installer. Not Stage 46 close. Not Everest E5. **Stage 46 OPEN:** PRE-EBS ESP product ISO retain + virtio-pci queues + product PIC/IOAPIC inject + 16550/ttyS0 + `squashfs,virtio_blk console=ttyS0` ISO patch (keeps squashfs in `modules=`) + product ISO xAPIC 4K trap/`lapic_virt` + `alpine_dev=vdb` when present + PIT IRQ 0 on HLT/preempt (i8253 16-bit) + GRUB `set timeout=1` / efi_gop / all_video / `terminal_output console` serial + MMIO XCHG/MOVSX/moffs + i8253 unlatched lo/hi + MMIO AH/CH/DH/BH + group-1 ADD/SUB + register-form ALU (mem and dest-reg) + ALU/TEST/CMP RFLAGS + INC/DEC/NOT/NEG + BT/BTS/BTR/BTC + CMPXCHG/XADD + guest-UEFI CR8-load/store exiting (Linux TPR → lapic_virt) + MMIO ADC/SBB (RFLAGS.CF) + MMIO group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR + MMIO CMOV/SETCC + MMIO PREFETCH/NOP/CLFLUSH hints + BSF/BSR + MMIO IMUL + MMIO MUL/IMUL DX:AX + MMIO DIV/IDIV + MOVNTI + SHLD/SHRD + CMPXCHG8B + TZCNT/LZCNT/POPCNT + PUSH/POP r/m + MOVS/STOS/LODS + CALL/JMP r/m + CMPS/SCAS + IOAPIC→LAPIC IRR/ISR (remote IRR/level) + 31-sector ATAPI PIO + chained virtio OUT + read-only ISO virtio-blk at `00:03.0` + 4 KiB GPA copies + lazy report-RAM virtqueue GPA + virtio IOAPIC pin 11 gated on the window + hold (not E4) when armed; armed product ISO resume cap is 16 777 216 on nested too; lab 72 KiB stub still fail-softs to E4. Product ISO fw_cfg ACPI MADT (iso=0 named files stay 3); linux PIC before LAPIC; linux PIC IRQ0; MADT IRQ0 ISO GSI 2; PIT skips IOAPIC pin 0; linux GSI 2 before PIC; fw_cfg IoReadFifo8 fills RAM (skip HV identity PML4 dest); PIIX4 PM1 SCI_EN; PM1 SCI_EN at reset; DSDT PCI0 _PRT; DSDT PCI0 _CRS; linux hides duplicate slot0 IDE; linux hides PIIX IDE; linux high-half hides PIIX; linux-line alpine_dev=vdb; linux-line virtio_pci; linux ATA floating bus; fw_cfg skip dest n=; fw_cfg identity overlay; HV identity PML4 0x400000. PEI dest holds ACPI tables. fw_cfg dest_ok fill dest=. dest_ok fill log cap 8. ACPI tables ZONE_FSEG. FSEG dest holds ACPI tables. linux-line ata_piix blacklist. linux-line piix_init blacklist. FADT FACS. flashcruzer reject 2d6b109 dest skip. auto-answer / # without login. product ISO POST_DXE_TAIL skip. emergency mount+exit. linux-line usbdelay. io string (rep insb); 0xAF00 PM timer; 0xB000 dword timer firmware PIC before GSI 2; HLT stall quiet tick print-only; firmware HLT ignores TPR; firmware HLT stall waits for IRQ; iron COM2 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d; do not F11 9ce65ae. firmware PIC before GSI 2; HLT stall quiet tick print-only; firmware HLT ignores TPR; firmware HLT stall waits for IRQ; iron COM2 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d; do not F11 9ce65ae; firmware virtual-wire PIC; firmware virtual-wire AEOI; firmware virtual-wire GSI 2; firmware HLT force IF; firmware HLT skip after inject; firmware HLT activity active; firmware LAPIC timer expiry; IOAPIC I/O over PIT; firmware virtual-wire GSI 14; flash 5c0f7a2; do not F11 2ae4544; product ISO fw_cfg bootorder virtio-iso scsi@3 first; product ISO fw_cfg bootorder El Torito ide@ first + product ISO hides PIIX IDE; flash ea30da1; do not F11 b824789; flash b824789; do not F11 d61dc7e; skip-after-inject uses pci_ready; flash d61dc7e; do not F11 5c0f7a2; iron COM2 eac424b IRET-to-HLT; iron COM2 eac424b pic=1 sparse inject; iron COM2 beb1576 HLT if=1 tpr=0x0 pic=0 gsi2=0; do not F11 eac424b; do not F11 8e81c2e; do not F11 daf3195; do not F11 b26c86a; flash 2ae4544.. iron COM2 2d6b109 pde0=0x20b027. flash 56f31d3; firmware HLT skip without inject; product ISO HLT stall before n=16384; do not F11 ea30da1; do not F11 a2acfc8; firmware HLT skip after ataio; do not F11 90da03d.
Plan: [m7_plan.md](m7_plan.md) · HDA: [hda.md](hda.md) · ADR-013: [adr/ADR-013.md](adr/ADR-013.md) · ADR-014: [adr/ADR-014.md](adr/ADR-014.md) · evidence: [evidence/r640/2026-08-28-m4-blk-host-slab.md](evidence/r640/2026-08-28-m4-blk-host-slab.md)

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
| P0-56 / E5 Stage 41 | `RAYNU-V-M7-E5-OVMF-DXE-OK` | **CLOSED (host + QEMU nested VT-x).** CMOS/fw_cfg + i440FX at `00:08.0` + IDE at `00:00.0` (PEI DID `0x7010`). `OVMF-CDROM-OK` pci_ide=1 sectors=0. Post-DXE tail then E4. Not a completed firmware CD boot. Not installer. Not Everest E5. |
| P0-57 / E5 Stage 42 | `RAYNU-V-M7-E5-OVMF-VIRTIO-OK` | **CLOSED (host + QEMU nested VT-x).** Empty PCI virtio-blk at `00:00.0` (PEI DID `0x1042`). `OVMF-VIRTIO-OK` pci=1. CD GuestVisible. `pci_ide=0` sectors=0. Stop n=115 virtio=1. Not a completed firmware CD boot. Not installer. Not Everest E5. |
| P0-58 / E5 Stage 43 | `RAYNU-V-M7-E5-OVMF-BOTH-OK` | **CLOSED (host + QEMU nested VT-x).** Firmware-simultaneous virtio `00:00.0` + IDE `00:00.1`. Nested VT-x `1b07692`: `val=0x70108086` `OVMF-BOTH-OK` stop n=1111 `pci_ide=1 virtio=1` `sectors=0`. Not a completed firmware CD boot. Not installer. Not Everest E5. |
| P0-59 / E5 Stage 44 | `RAYNU-V-M7-E5-OVMF-ATAPI-OK` | **CLOSED (iron COM2 `bf696ca`).** `sectors=1` `packet=9` `scsi=0x28` stop n=30769 `pci_ide=1 virtio=1`. Not El Torito boot. Not installer. Not Everest E5. |
| P0-61 / E5 Stage 45 | `RAYNU-V-M7-E5-OVMF-ELTORITO-OK` | **CLOSED (iron COM2 `0be7283`).** `RN-ELT` n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1 packet=533 scsi=0x28 port=0x3f8. Not installer. Not Everest E5. Work order: 45 → P0-60 → 46. |
| P0-60 | M4.2 G1 shell EPT / fail-soft | **CLOSED (iron COM2 after `5147222`, not an E5 stage).** G1 `RAYNU-V-M4-SHELL-G1` / `RAYNU-V-M4-2VM-OK`. G0 relocate **CLOSED** (`M4-NVM-OK`). M4.3 host-slab **CLOSED** on iron after `22e28d0` (`M4-BLK-OK` `0x10c00000`; `M4-NET-OK`; `M4-SMP-OK`; `R640-BOOT-OK`). Not Stage 46. |
| P0-62 / E5 Stage 46 | `ISO-INSTALL-OK` | **OPEN.** Cruzer ESP prunes leftover/partial `*.iso` then df-checks before alpine-virt `linux.iso`. PRE-EBS ESP product ISO + virtio-pci queues + product PIC/IOAPIC inject + product 16550/`ttyS0` + host SOL RX→guest COM1 + Alpine serial auto-answer (`BOOTLOADER=grub` `USE_EFI=1` `setup-disk` + mkdir `/media/cdrom` + mount `-t iso9660` `/dev/vdb`||`/dev/sr0` + virtio_pci/mdev wait /dev/vda/sr_mod/isofs + `setup-disk -s 0` + `bootloader?` + `Which disk` + `No disks available`→n + `How would you like`→sys + apk repos overwrite + `[y/N]`) + `squashfs,virtio_blk console=ttyS0` ISO patch (keeps squashfs in `modules=`; product ISO xAPIC 4K trap so CUR_COUNT/EOI are live) + GRUB linux-line NUL-pad grow + ISO9660/Joliet `grub.cfg` Data Length 143→294 (a 143-byte read truncated at `tsc=` and dropped to rescue `grub>` on iron COM2 after El Torito `bootimg=1`; grown linux line now includes `alpine_dev=vdb` and `virtio_pci` in `modules=`) + Linux `hypervisor_cpuid_base` callee-saved GPR bump to `0x4000FF00` (iron COM2 `90c85d5`: `Booting Linux virt`, `Loaded initrd`, `#PF linux deliver`, then `n=256 leaf=0x4000bd00`) + alpine-virt `native_cpuid` `push %rbx` RSP slot (`base` in EBX, not R12; iron `6e5c84a` `gpr=0 stack=1` then `Linux version 6.12.13-0-virt` / PAT; COM2 ended at PAT with UART `in al,dx` `hpet=40191`) + HPET TSC-delta on UART COM I/O cap 4us (not 1ms/byte; not PCI/ATA; iron `115e5ee` UART log + `hpet` climbs ~1/UART-exit through PAT, not 1ms/byte) + Linux printk ticks every 4096 after `#PF` deliver (iron `115e5ee` every-256 UART ticks split `Linux version` / PAT) + guest UART nowait (do not clear `COM2_LIVE` on THR timeout; iron `115e5ee` PAT freeze at `n=441600`) + Linux CPUID GenuineIntel + NX + guest UART TX ring drain 4/exit (iron `202312f` readable `Linux version 6.12.13-0-virt` / cmdline / e820 start; paste cut mid-`usabl` after blocking hypervisor-scan bump; nested QEMU `be0f1cd` `/init` SIGSEGV 3/3) + linux earlycon share TX ring + linux earlycon quiet ticks + linux earlycon hush HV + linux earlycon share product ISO + cpu_flush on tick cadence even when share + linux earlycon share first CPUID + linux earlycon skip #PF dump + linux earlycon skip exc deliver + linux earlycon share first high-half + poll ISO-INSTALL-OK every resume + ISO-INSTALL-OK on GPT not 16KiB + setup-disk before apk update + 256MiB disk leftover report-RAM (nested `e0019a3`/`4f875d6` `/init` SIGSEGV after quiet ticks skipped cpu_flush on iso=0 high-half RIP; iron `9a3cbfa` `linux cpuid n=2`/`n=3` shredded `Linux version`; `write_byte` drops during share; `ISO-INSTALL-OK` uses `write_line_nowait`) + linux earlycon share first bootimg (iron `b983ef8` 256MiB `Loaded initrd` then readable `Linux version 6.12.13-0-virt` / e820 `0x7eb3efff`; identity-map earlycon is not bit 63) + guest UART TX drain COM2 independent (iron `b983ef8` COM2 froze after two e820 lines; drain waited on COM1+COM2 THRE) + linux earlycon pace LSR THRE + report-RAM EPT pre-map + cpu_flush skip leftover pre-map + cpu_flush leftover per walk (iron `029ac8f`/`3dc7d11` hush-on-bootimg still cut at `[` after two e820 lines; guest LSR always THRE) + guest UART TX ring drain (iron `45aec97` `GenuineIntEl` / `NX missing` then PAT; EFER NXE after high-half) + `alpine_dev=cdrom` → `alpine_dev=vdb` when present + PIT IRQ 0 on HLT/preempt (UART/virtio beat the timer; i8253 channel 0 is 16-bit lo/hi + latch) + GRUB `set timeout=1` → `set timeout=0` / `efi_gop` / `all_video` / `terminal_output console` serial + MMIO XCHG/MOVSX/moffs + group-1 AND/OR/XOR/ADD/SUB + register-form ALU (mem and dest-reg `02`/`03`…`32`/`33`) + TEST/CMP RFLAGS + INC/DEC/NOT/NEG + BT/BTS/BTR/BTC + CMPXCHG/XADD + guest-UEFI CR8-load/store exiting (Linux TPR → lapic_virt; E4 VMCS does not request CR8) + MMIO ADC/SBB (RFLAGS.CF) + MMIO group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR + MMIO CMOV/SETCC + MMIO PREFETCH/NOP/CLFLUSH hints + BSF/BSR + MMIO IMUL + MMIO MUL/IMUL DX:AX + MMIO DIV/IDIV + MOVNTI + SHLD/SHRD + CMPXCHG8B + TZCNT/LZCNT/POPCNT + PUSH/POP r/m + MOVS/STOS/LODS + CALL/JMP r/m + CMPS/SCAS + IOAPIC→LAPIC IRR/ISR (remote IRR/level EOI) + AH/CH/DH/BH + 31-sector ATAPI PIO + chained virtio OUT + read-only ISO virtio-blk at `00:03.0` (`/dev/vdb`) + 4 KiB GPA copies + lazy report-RAM virtqueue GPA + virtio IOAPIC pin 11 (PCI line) + hold (not E4) when armed. Armed product ISO resume cap is 16 777 216 on nested too (lab stub nested stays 65536). Lab stub still E4. INVLPG `0F 01 /7` skip-decode (empty fetch does not guess; do not clear INVLPG-exiting). + linux unhandled nowait stop (iron `1a2544d` `Freeing initrd` then restore host xcr0; share hushes `stop n=`) + virtio MMIO eax fallback + linux NMI inject + iso=0 decode fail still stops + linux MMIO decode retry + linux EAX fallback skip 3 + IOAPIC decode fail nowait + linux MOV DR skip + virtio BAR trap over scratch + PIIX3 ISA BAR RAZ + packed virtio common cfg + virtio MMIO raises PIT + virtio MMIO off= + virtio MMIO eax fallback size. Packed virtio common cfg write + virtio MMIO polls lapic. linux I/O does not raise PIT (iron MADT stop); linux xAPIC EPT insn_len 0 + linux preempt deadloop noskip + linux PIT prefer once; linux PIT prefer until DRIVER_OK; UART reassert RX not THRE; virtio drain every resume; linux virtio DRIVER_OK; product ISO fw_cfg ACPI MADT (iso=0 named files stay 3); linux PIC before LAPIC; linux PIC IRQ0; MADT IRQ0 ISO GSI 2; PIT skips IOAPIC pin 0; linux GSI 2 before PIC; fw_cfg IoReadFifo8 fills RAM (skip HV identity PML4 dest); PIIX4 PM1 SCI_EN; PM1 SCI_EN at reset; DSDT PCI0 _PRT; DSDT PCI0 _CRS; linux hides duplicate slot0 IDE; linux hides PIIX IDE; linux high-half hides PIIX. linux-line alpine_dev=vdb (iron COM2 cmdline had no alpine_dev; alpine-virt grub.cfg alpine_dev=cdrom swap is 0 hits). linux-line virtio_pci. linux ATA floating bus. fw_cfg skip dest n=. fw_cfg identity overlay. HV identity PML4 0x400000. PEI dest holds ACPI tables. fw_cfg dest_ok fill dest=. dest_ok fill log cap 8. ACPI tables ZONE_FSEG. FSEG dest holds ACPI tables. linux-line ata_piix blacklist. linux-line piix_init blacklist (initcall_blacklist=piix_init; Linux 6.12 ata_piix.c is module_init(piix_init), not ata_piix_init). FADT FACS. flashcruzer reject 2d6b109 dest skip. auto-answer / # without login. product ISO POST_DXE_TAIL skip. emergency mount+exit. linux-line usbdelay. io string (rep insb); 0xAF00 PM timer; 0xB000 dword timer firmware PIC before GSI 2; HLT stall quiet tick print-only; firmware HLT ignores TPR; firmware HLT stall waits for IRQ; iron COM2 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d; do not F11 9ce65ae; firmware virtual-wire PIC; firmware virtual-wire AEOI; firmware virtual-wire GSI 2; firmware HLT force IF; firmware HLT skip after inject; firmware HLT activity active; firmware LAPIC timer expiry; IOAPIC I/O over PIT; firmware virtual-wire GSI 14; flash 5c0f7a2; do not F11 2ae4544; product ISO fw_cfg bootorder virtio-iso scsi@3 first; product ISO fw_cfg bootorder El Torito ide@ first + product ISO hides PIIX IDE; flash ea30da1; do not F11 b824789; flash b824789; do not F11 d61dc7e; skip-after-inject uses pci_ready; flash d61dc7e; do not F11 5c0f7a2; iron COM2 eac424b IRET-to-HLT; iron COM2 eac424b pic=1 sparse inject; iron COM2 beb1576 HLT if=1 tpr=0x0 pic=0 gsi2=0; do not F11 eac424b; do not F11 8e81c2e; do not F11 daf3195; do not F11 b26c86a; flash 2ae4544.. iron COM2 2d6b109 pde0=0x20b027. flash 56f31d3; firmware HLT skip without inject; product ISO HLT stall before n=16384; do not F11 ea30da1; do not F11 a2acfc8; firmware HLT skip after ataio; do not F11 90da03d; firmware skip PIT inject; do not F11 e70a295; flash 77f5866; firmware force IF for inject; do not F11 77f5866; retrigger 9df52c5 CI after nested-KVM SHELL flake (33402411199); flash 5227ad9 pin 33404368817; firmware arm ATA GSI 14; flash 489d938 pin 33408594472; do not F11 5227ad9; firmware prefer ATA IRR. Not closed. `ISO-BOOTED-FROM-DISK` is persist-detect, not this gate. Host/CI never prints the iron OK. |
| Everest residual | TLS/console + Windows later | After Stage 46. Product ISO: [ADR-014](adr/ADR-014.md). |
| M8 (sketch) | — | vMotion-like · DRS-like · hot-add (after M7) |
| Optional | Dell Tier‑2 / pin upgrades | Slip-ok — see [m6_plan.md](m6_plan.md) / ADR-005 |
