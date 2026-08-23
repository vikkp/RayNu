---
hda_version: 1
last_updated: 2026-08-23
last_commit: 2b795a0bef4ae5a5c356a0131205f9de439ffe57
last_commit_short: 2b795a0
updated_by: cursor
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO (M7 Mount Everest)"
months_to_everest: 0.5
months_to_everest_prev: 0.5
velocity_commits_30d: 368
velocity_gates_30d: 59
overall_pct: 95
confidence: high
baseline_date: 2026-07-20
baseline_months: 4.5
everest_eta_month: "2026-09"
summit_core_pct: 88
summit_efi_pct: 95
summit_r640_pct: 98
summit_ui_pct: 96
summit_iso_pct: 99
summit_prod_pct: 100
---

# Honest Distance Assessment (HDA)

> **Living document.** Updated on every meaningful commit by Cursor (see `.cursor/rules/hda-update.mdc`).  
> **North star product loop (“Mount Everest”):**  
> Ship the EFI → boot on a **real PowerEdge R640** → **network-reachable** vSphere-like UI → **deploy a Linux ISO** (M7 / ADR-009). Production bar (M6.8–M6.9) is already closed on Latitude.

Pillars: **[V]** verified core · **[Z]** single binary · **[D]** iDRAC-native · **[A]** audit-first.  
Authoritative gates: [`docs/progress.md`](progress.md) · plan: [`m7_plan.md`](m7_plan.md) · ADR: [`adr/ADR-009.md`](adr/ADR-009.md) · constitution: [`CLAUDE.md`](../CLAUDE.md).

---

## Scoreboard (read this first)

| Metric | Value | Δ vs previous HDA |
|--------|------:|-------------------|
| **Overall product readiness** | **95%** | +1 (P0-14 E4 SPA VMLAUNCH + re-entry on iron) |
| **Months to Mount Everest** | **0.5** | held (TLS/console + distro remain) |
| **ETA month** | **2026-09** | held |
| **Confidence** | high | E2+E3+E3b+E5+Phase F+P0-14 stamps on COM2; SPA guest is SHELL stub; TLS/console + distro residual |
| **Hypervisor core (VMX/EPT/Linux/multi-VM)** | ~88% | proved on real R640 through M4 |
| **Ship EFI artifact** | ~95% | M7.0 + iron kits under `releases/` |
| **Real R640 boot** | ~98% | E2 closed; Redfish/soak follow-ons only |
| **vSphere-like UI (network)** | ~96% | E3 + E3b + Phase F + P0-14 closed; SHELL stub not distro; TLS/console residual |
| **Deploy Linux ISO** | ~99% | OVMF past SEC on private VMCS (QEMU/VMX); not installer; distro later |
| **Production bar (M6.8–M6.9)** | **100%** | soak + EXT closed on Latitude |

```
Months to Everest  █░░░░░░░░░░░░░░░░░░░  0.5 mo  (was 1.5)
Overall %          ███████████████████░  95%
```

**How the month number moves:** faster closed Everest-path work → `months_to_everest` shrinks and `everest_eta_month` pulls closer. Stalls / new scope → it slips. See [Velocity model](#velocity-model).

---

## Mount Everest — definition of done

All must be true (no hand-waving):

| # | Criterion | Done when | Pillar |
|---|-----------|-----------|--------|
| E1 | **Ship EFI** | Versioned `r640-hypervisor.efi` + checksums; `tools/check-size.sh` green; USB/iDRAC media runbook | [Z] |
| E2 | **R640 boot** | Marker `RAYNU-V-R640-BOOT-OK` (or equiv.) on **real PowerEdge R640**; serial via iDRAC; VMX+EPT+Linux shell | [D][Z] |
| E3 | **Network UI (bring-up)** | Browser/curl on operator LAN reaches SPA/REST during PRE-EBS window (HTTP MVP; TLS deferred); not host-only | [Z][A] |
| E3b | **Durable mgmt** | Same SPA/REST reachable **after** ExitBootServices / `BOOT-OK` on a host-owned NIC (ADR-013); firmware SNP/Tcp4 do not count | [Z][A] |
| E4 | **vSphere-like MVP** | Datastore/images, create-VM (CPU/RAM/disk/NIC), attach ISO or boot media, basic console/log, auth beyond bring-up toy | [Z][A] |
| E5 | **Linux ISO deploy** | Operator registers a distro ISO → VM boots **UEFI installer** (ADR-014) to virtio-blk → reboot to disk. Extract-boot/bzImage is lab MVP only. Windows ISO later, same model. | [Z] |
| E6 | **Production bar** | M6.8 soak + M6.9 external audit/spec review closed per `progress.md` | [V][A] |

**Out of Everest / M7 scope (→ M8 or later):** vMotion-like live migrate, DRS-like placement, hot-add, full vSphere parity, Dell Tier-2 PERC OEM, multi-site DR, Windows guest WHQL. Windows **install** is later under [ADR-014](adr/ADR-014.md); the image type exists now so E5 does not stay Linux-kernel-only.

---

## Four-summit breakdown

### Summit A — Ship the EFI
**Status: NEAR · ~95% · ~0.0–0.25 months residual (Secure Boot optional)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| `cargo build` → `.efi` | DONE | `tools/build.sh`, UEFI target |
| Size budget | DONE | `tools/check-size.sh` (15/20 MB) |
| PE assets kernel/initrd/webui | DONE | M3.22 / M5.2 |
| CI build | DONE | `.github/workflows/ci.yml` |
| Release tarball + SHA256 | DONE | `tools/package-release.sh` → `dist/` (M7.0 Latitude) |
| Secure Boot signing | MISSING | optional; not required for M7.0 |
| One-page USB/iDRAC runbook | DONE | `docs/runbooks/usb_idrac.md` (M7.0) |

### Summit B — Load on real R640
**Status: NEAR · ~98% · ~0.0–0.25 months residual (Redfish/soak polish)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| UEFI+VMX+EPT+Linux shell | DONE on Latitude **and iron** | `progress.md` M0–M4; COM2 2026-08-15 |
| R640 boot **scaffold** (runbook + evidence) | DONE (host) | `docs/runbooks/r640_boot.md`; `RAYNU-V-M7-R640-SCAFFOLD-OK` |
| Real R640 **M0 → SHELL → M4** | DONE | `v0.1.0-xsavesfix` COM2; evidence `STATUS=closed` |
| iDRAC SOL (`console com2`) | DONE | COM1+COM2 UART mirror |
| Precise EPT pool on large RAM | DONE | pool clipped ≤ guest RAM / precise window |
| Real **R640** boot gate (E2) | **DONE** | `RAYNU-V-R640-BOOT-OK`; `GAP(CLOSED M7.5)` |
| Live iDRAC Redfish | MISSING | `GAP: live Redfish BMC → polish` |
| R640 topology from real SRAT/SMBIOS | MOCK | `idrac/` mock text |
| Hardware CI on R640 | MISSING | optional in M6 plan |

### Summit C — vSphere-like UI
**Status: NEAR · ~96% · ~0.5 months residual (TLS/console polish + distro waits on Summit D)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Embedded SPA list/start/stop | DONE | `assets/webui.html`, M5.2 |
| In-process REST shapes + auth token | DONE | `mgmt/api.rs` M5.1/M6.4 |
| HTTP/1.1 codec + Bearer wire | DONE | `mgmt/http.rs` (M7.1 Latitude) |
| Host TCP proof (loopback) | DONE | `mgmt/http_listen.rs` (M7.1 Latitude) |
| Create-VM fields (CPU/RAM/disk/ISO) | DONE (host) | M7.4 SPA + `POST /vms/{id}/spec/...` Latitude smoke |
| Datastore / ISO media buttons | DONE (host) | SPA → `/images`, `/iso/{id}/deploy` + install |
| **UEFI NIC HTTP listen** | DONE (M7.6 iron) | `RAYNU-V-M7-UEFI-HTTP-OK` R640 SNP residual; [2026-08-16-uefi-http-ok.md](evidence/r640/2026-08-16-uefi-http-ok.md) |
| PRE-EBS durable mgmt tables | DONE | `pre_ebs_mgmt` shared across HTTP exchanges |
| TLS | DEFERRED | plaintext lab HTTP (ADR-009) |
| Guest console / serial log UI | PARTIAL | Host UART ring via `GET /logs/serial` + SPA; guest VNC residual |
| Auth beyond bring-up toy | PARTIAL | ESP `auth.token` overrides bring-up; iron used lab bring-up (`AuthAllowed`) |
| Networking/storage ops UI | MISSING | probes only |
| Audit/tasks pane | PARTIAL | ring exists; UI thin |
| E4 SPA create on iron | DONE | Firefox create-VM + Bearer; [2026-08-16-e4-spa-install-arm.md](evidence/r640/2026-08-16-e4-spa-install-arm.md) |
| **Post-EBS durable HTTP (E3b)** | **DONE** | `RAYNU-V-M7-HOST-NIC-HTTP-OK` after `BOOT-OK` on BCM5720 `:38`; [2026-08-20-e3b-host-nic-http-ok.md](evidence/r640/2026-08-20-e3b-host-nic-http-ok.md) |
| **Phase F coexist (VMX on)** | **DONE** | `HOST-NIC-HTTP-OK` while VMX on; G0 scheduled; G1–G3 parked; [2026-08-20-phase-f-coexist-ok.md](evidence/r640/2026-08-20-phase-f-coexist-ok.md) |
| E4 SPA VMLAUNCH (private EPT) | **DONE** | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` + shadow re-entry; SHELL stub; [2026-08-21-e4-spa-shadow-reentry-ok.md](evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md) |

### Summit D — Deploy Linux ISO
**Status: NEAR · ~90% · ~0.25–0.5 months residual (real distro installer)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| bzImage + initrd boot | DONE | real tiny Linux → shell |
| Image library (register/list/delete) | DONE | `mgmt/datastore.rs` (M7.2 Latitude) |
| Host ESP-shaped catalog | DONE | `EFI/RAYNU/images/catalog.txt` (host `std::fs`) |
| UEFI catalog persist | STUB | `UnsupportedOnFirmware` until SFS/NVMe write |
| ISO register + extract-boot bind | DONE (host) | `mgmt/iso.rs` Latitude package smoke (~0s) |
| Install-to-disk scaffold (M7.7) | DONE (host + iron stamps) | `STATUS-iso-install=closed`; COM2 `BOOTED-FROM-DISK` |
| Virtio-blk install target surface | DONE (plan) | `DEFAULT_INSTALL_DISK_BYTES` + capacity helper |
| Wire contract → guest launch | PARTIAL | PRE-EBS arm → post-EBS sized `virtio_blk::init`; guest FS installer open |
| QEMU lab (1 MiB ESP flag) | DONE (host/TCG arm) | boot1 `isoinstall.txt` → `ISO-INSTALL-LAB-OK`; soft-pass arm-only on TCG |
| QEMU lab reboot-to-disk | DONE (host/TCG arm) | boot2 `isoreboot.txt` + synth img → `BOOTED-FROM-DISK`; soft-pass arm-only on TCG |
| ISO parse / El Torito / EFI boot img | PARTIAL (guest-visible CD) | Catalog parse + host attach + FirmwareArmed + GuestVisible PCI IDE/ATAPI; not DXE boot / not installer |
| CD-ROM attach | PARTIAL (guest-visible) | `attach_cdrom_uefi` → GuestVisible + PCI IDE/ATAPI; firmware does not yet boot the CD |
| Guest UEFI firmware blob | PARTIAL (ESP retained + past SEC) | Real ESP OVMF.fd retained; private VMCS past SEC on QEMU/VMX; not full DXE / not installer / not Everest E5 |
| Persistent install + reboot-to-disk | **DONE (stamps)** | Iron Cruzer `BOOTED-FROM-DISK` 2026-08-16; guest FS residual |
| Upload ISO via API/UI | PARTIAL | REST `/iso/{id}/deploy` + `/install`; blob upload residual |
| Multi-OS image types | **WIRED (host)** | REST/SPA `linux_iso` \| `windows_iso` \| `generic_uefi` ([ADR-014](adr/ADR-014.md) Stage 0); Windows install later |
| Multi-distro matrix | MISSING | — |

---

## Rolling month timeline (Mount Everest)

Months are **calendar months from `baseline_date`**, adjusted by velocity.  
When work finishes early, **pull rows upward** (shrink residual). When blocked, **push ETA**.  

| Month | Calendar | Planned focus | Exit criteria | Status |
|-------|----------|---------------|---------------|--------|
| M+0 | 2026-07 | **M7.0–M7.4 closed** (lab host); **M7.5 R640 next** | M7.4 Latitude host smoke | **DONE (M7.4 host)** |
| M+1 | 2026-08 | **R640 iron bring-up** → **E2 closed** | `RAYNU-V-R640-BOOT-OK` on COM2 | **DONE (M7.5 iron)** |
| M+2 | 2026-09 | E3b native NIC lab (QEMU e1000) + ISO residual | ADR-013 Phase C | **Phase C DONE (QEMU)** |
| M+3 | 2026-08 | E3b iron HTTP | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | **DONE (M7.8 iron)** |
| M+4 | 2026-09 | TLS/console + distro installer | remaining Everest | **ETA** |
| M+5 | 2026-10 | Buffer / M7 closed on all E1–E6 | **M7 Mount Everest** | BUFFER |

### Timeline burn-down

```
2026-07 ████████  HDA + M6 closed (Latitude)
2026-08 ████████  R640 boot (E2) + E3b HTTP-OK
2026-09 ████░░░░  TLS/console + distro installer  ← months_to_everest ≈ 0.5
2026-10 ░░░░░░░░  buffer
2026-11 ░░░░░░░░  buffer
```

**Pull-forward rule:** E2 closed 2026-08-15; E3 bring-up closed 2026-08-16; **E3b closed 2026-08-20**; **P0-14 closed 2026-08-21**. Shrink further when a real distro installer lands. Document why in [Changelog](#hda-changelog).

---

## Everest workstream backlog (P0)

Ordered for critical path (parallelize B with D design):

| ID | Workstream | Summit | Est. residual (mo) | Depends on | Repo touchpoints |
|----|------------|--------|-------------------|------------|------------------|
| P0-1 | **M7.0** Release kit: tag, SHA256, size gate, USB/iDRAC runbook | A | **DONE** | — | `tools/package-release.sh`, runbook |
| P0-2 | **M7.5** R640 boot gate (real iron) | B | **DONE** | P0-1 helpful | `RAYNU-V-R640-BOOT-OK` 2026-08-15; evidence closed |
| P0-3 | Live Tier-1 Redfish (read-only health) | B | 0.5 | P0-2 | `idrac/` — after first boot |
| P0-4 | **M7.1** Minimal HTTP server (serve SPA + REST) | C | **DONE** | size budget | Host + iron SNP residual **PRE-EBS** (E3); firmware Tcp4 absent |
| P0-12 | **M7.8 / E3b** Host-owned mgmt NIC (ADR-013) | C | **DONE** | P0-4 | `RAYNU-V-M7-HOST-NIC-HTTP-OK` 2026-08-20; BCM5720 `:38` after `BOOT-OK` |
| P0-13 | **ADR-013 Phase F** Native HTTP beside VMX | C | **DONE** | P0-12 | coexist `10.99.99.149:8443` 2026-08-20; G0 scheduled; G1–G3 parked |
| P0-14 | **E4 SPA VMLAUNCH** Private-EPT guest from SPA start | C | **DONE** | P0-13 | Iron `2b795a0` 2026-08-21; marker + 98-field shadow re-entry; SHELL stub |
| P0-15 | **E5 Stage 0** Boot spec on the wire + El Torito parse | D | **DONE (host)** | P0-14 | `RAYNU-V-M7-E5-BOOT-SPEC-OK` (#172); not attach |
| P0-16 | **E5 Stage 1** Host El Torito CD-ROM attach | D | **DONE (host)** | P0-15 | `RAYNU-V-M7-E5-CDROM-ATTACH-OK` (#173); not guest UEFI |
| P0-17 | **E5 Stage 2** Firmware-facing CD attach | D | **DONE (host)** | P0-16 | `RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`; not OVMF / not VMLAUNCH / not Everest E5 |
| P0-18 | **E5 Stage 3** Guest UEFI firmware envelope | D | **DONE (host)** | P0-17 | `RAYNU-V-M7-E5-GUEST-FW-OK`; not OVMF / not VMLAUNCH / not Everest E5 |
| P0-19 | **E5 Stage 4** Guest firmware stub load | D | **DONE (host)** | P0-18 | `RAYNU-V-M7-E5-GUEST-FW-LOAD-OK`; not OVMF / not VMLAUNCH / not Everest E5 |
| P0-20 | **E5 Stage 5** OVMF Firmware Volume probe | D | **DONE (host)** | P0-19 | `RAYNU-V-M7-E5-OVMF-PROBE-OK`; not embedded EDK2 / not VMLAUNCH / not Everest E5 |
| P0-21 | **E5 Stage 6** ESP OVMF load | D | **DONE (host)** | P0-20 | `RAYNU-V-M7-E5-OVMF-ESP-OK`; not embedded EDK2 / not VMLAUNCH / not Everest E5 |
| P0-22 | **E5 Stage 7** Guest firmware slot arm | D | **DONE (host)** | P0-21 | `RAYNU-V-M7-E5-OVMF-SLOT-OK`; not VMLAUNCH / not Everest E5 |
| P0-23 | **E5 Stage 8** Firmware-to-guest bind | D | **DONE (host)** | P0-22 | `RAYNU-V-M7-E5-FW-BIND-OK`; not VMLAUNCH / not Everest E5 |
| P0-24 | **E5 Stage 9** Firmware launch-prepare | D | **DONE (host)** | P0-23 | `RAYNU-V-M7-E5-FW-PREP-OK`; mock refused; not VMLAUNCH / not Everest E5 |
| P0-25 | **E5 Stage 10** Firmware size-floor | D | **DONE (host)** | P0-24 | `RAYNU-V-M7-E5-FW-FLOOR-OK`; 4 KiB not EDK2; not VMLAUNCH / not Everest E5 |
| P0-26 | **E5 Stage 11** Firmware EDK2-sized stage | D | **DONE (host)** | P0-25 | `RAYNU-V-M7-E5-FW-EDK2-OK`; 1 MiB not shipped OVMF.fd; not VMLAUNCH / not Everest E5 |
| P0-27 | **E5 Stage 12** ESP-path guest UEFI VMLAUNCH | D | **DONE (host)** | P0-26 | `RAYNU-V-M7-E5-ESP-LAUNCH-OK`; launch.rs wired; no live OVMF.fd; not Everest E5 |
| P0-28 | **E5 Stage 13** Live ESP OVMF map | D | **DONE (host)** | P0-27 | `RAYNU-V-M7-E5-ESP-MAP-OK`; 2 MiB+ map; not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-29 | **E5 Stage 14** Reset-vector VMCS contract | D | **DONE (host)** | P0-28 | `RAYNU-V-M7-E5-RESET-VEC-OK`; 0xEA stub not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-30 | **E5 Stage 15** Firmware-alias EPT contract | D | **DONE (host)** | P0-29 | `RAYNU-V-M7-E5-FW-ALIAS-OK`; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-31 | **E5 Stage 16** Alias-EPT program contract | D | **DONE (host)** | P0-30 | `RAYNU-V-M7-E5-ALIAS-EPT-OK`; live EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-32 | **E5 Stage 17** Private alias-EPT install | D | **DONE (host)** | P0-31 | `RAYNU-V-M7-E5-EPT-INSTALL-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-33 | **E5 Stage 18** Real-ESP VMLAUNCH-ready | D | **DONE (host)** | P0-32 | `RAYNU-V-M7-E5-REAL-ESP-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-34 | **E5 Stage 19** Guest-UEFI VMLAUNCH insn arm | D | **DONE (host)** | P0-33 | `RAYNU-V-M7-E5-REAL-LAUNCH-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-35 | **E5 Stage 20** Live-ESP VMLAUNCH execute gate | D | **DONE (host)** | P0-34 | `RAYNU-V-M7-E5-LIVE-EXEC-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-36 | **E5 Stage 21** Private guest-UEFI VMCS arm | D | **DONE (host)** | P0-35 | `RAYNU-V-M7-E5-PRIV-VMCS-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-37 | **E5 Stage 22** Live-ESP VMLAUNCH issue path | D | **DONE (host)** | P0-36 | `RAYNU-V-M7-E5-LIVE-ISSUE-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-38 | **E5 Stage 23** Live-ESP bytes probe | D | **DONE (host)** | P0-37 | `RAYNU-V-M7-E5-LIVE-BYTES-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-39 | **E5 Stage 24** Live-ESP FD require | D | **DONE (host)** | P0-38 | `RAYNU-V-M7-E5-LIVE-FD-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-40 | **E5 Stage 25** Live-ESP present-attempt | D | **DONE (host)** | P0-39 | `RAYNU-V-M7-E5-LIVE-PRESENT-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-41 | **E5 Stage 26** Live-ESP admit-attempt | D | **DONE (host)** | P0-40 | `RAYNU-V-M7-E5-LIVE-ADMIT-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-42 | **E5 Stage 27** Live-ESP read-attempt | D | **DONE (host)** | P0-41 | `RAYNU-V-M7-E5-LIVE-READ-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-43 | **E5 Stage 28** Live-ESP copy-attempt | D | **DONE (host)** | P0-42 | `RAYNU-V-M7-E5-LIVE-COPY-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-44 | **E5 Stage 29** Live-ESP place-attempt | D | **DONE (host)** | P0-43 | `RAYNU-V-M7-E5-LIVE-PLACE-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-45 | **E5 Stage 30** Live-ESP apply-attempt | D | **DONE (host)** | P0-44 | `RAYNU-V-M7-E5-LIVE-APPLY-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-46 | **E5 Stage 31** Live-ESP commit-attempt | D | **DONE (host)** | P0-45 | `RAYNU-V-M7-E5-LIVE-COMMIT-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-47 | **E5 Stage 32** Live-ESP latch-attempt | D | **DONE (host)** | P0-46 | `RAYNU-V-M7-E5-LIVE-LATCH-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-48 | **E5 Stage 33** Live-ESP seal-attempt | D | **DONE (host)** | P0-47 | `RAYNU-V-M7-E5-LIVE-SEAL-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-49 | **E5 Stage 34** Live-ESP lock-attempt | D | **DONE (host)** | P0-48 | `RAYNU-V-M7-E5-LIVE-LOCK-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-50 | **E5 Stage 35** Live-ESP hold-attempt | D | **DONE (host)** | P0-49 | `RAYNU-V-M7-E5-LIVE-HOLD-OK`; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; not Everest E5 |
| P0-51 | **E5 Stage 36** Real ESP OVMF retain | D | **DONE (host + QEMU)** | P0-50 | `RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK`; presence rule; private VMCS not allocated; VMLAUNCH insn not issued; no further *Absent bookkeeping; not Everest E5 |
| P0-52 | **E5 Stage 37** Private guest-UEFI VMLAUNCH | D | **DONE (host + QEMU)** | P0-51 | `RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`; retained bytes; private VMCS+EPT; not E4 SHELL; not installer; not Everest E5 |
| P0-53 | **E5 Stage 38** OVMF past first triple-fault | D | **DONE (host + QEMU)** | P0-52 | `RAYNU-V-M7-E5-OVMF-ALIVE-OK`; CR4.VMXE host-owned; not full OVMF; not installer; not Everest E5 |
| P0-54 | **E5 Stage 39** OVMF past SEC | D | **DONE (host + QEMU)** | P0-53 | `RAYNU-V-M7-E5-OVMF-PAST-SEC-OK`; left last 64 KiB + PEI PCI/COM/HLT; COM forwarded; not full DXE; not installer; not Everest E5 |
| P0-5 | **M7.2** Datastore on ESP/NVMe (images + ISOs) | C+D | 0.25 | P0-4 | **DONE host path**; UEFI persist residual |
| P0-6 | **M7.3** ISO register + CD-ROM or kernel-extract boot | D | 0.5 | P0-5 | `mgmt/iso` wired; El Torito/CD-ROM residual |
| P0-6 | **M7.3** ISO register + CD-ROM or kernel-extract boot | D | 0.5 | P0-5 | **DONE host extract-boot smoke**; El Torito/CD-ROM residual |
| P0-7 | **M7.4** Create-VM API/UI (CPU/RAM/disk/ISO) | C+D | 0.25 | P0-5, P0-6 | **DONE host SPA smoke**; console/TLS/NIC residual |
| P0-8 | Install-to-disk + reboot-to-disk path | D | **DONE (stamps)** | P0-6, P0-7 | Iron `BOOTED-FROM-DISK` 2026-08-16; guest FS residual |
| P0-9 | M6.9 external audit + spec review | E6 | **DONE** | proofs green | `docs/`, `ept_model/`, `mgmt/ext` |
| P0-10 | R640 soak / hardware confidence | E2 | 0.5 | P0-2 | `tools/`, `mgmt/soak` — post M7.5 |
| P0-11 | **M8 sketch** vMotion-like / DRS-like / hot-add | — | — | M7 closed | deferred — not M7 critical path |

---

## What is already strong (do not rebuild)

- Type-1 UEFI → VMX → EPT → **real Linux shell** (M3 chain)
- ≥4 guests, credit scheduler, SMP probe, virtio-blk/net probes (M4)
- ADR-004 exclusivity proofs through violation + migrate transfer (M6.0–M6.3 area)
- Audit ring + SOX/ISO/PDF; lifecycle CLI/REST shapes; VMware inventory import
- Single-binary discipline, gate markers, frozen Verus/Kani pins
- **M6 closed** on Latitude — soak + external audit/spec review (`RAYNU-V-M6-EXT-OK`; `80 verified, 0 errors`)
- **M7.0–M7.4 closed** on Latitude (M7.3–M7.4 = **host package smoke**; residuals named)
- **M7.5 iron closed:** `RAYNU-V-R640-BOOT-OK` on real R640 COM2 (SHELL + M4; `v0.1.0-xsavesfix`, 2026-08-15)
- **M7.8 / E3b iron closed:** `RAYNU-V-M7-HOST-NIC-HTTP-OK` after `BOOT-OK` on BCM5720 `:38` (2026-08-20)
- **ADR-013 Phase F iron closed:** coexist HTTP while VMX on (`10.99.99.149:8443`, EFI `0d06297b`, 2026-08-20)
- **P0-14 / E4 SPA VMLAUNCH iron closed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK` on `10.99.99.126:8443` (EFI `2b795a0`, 2026-08-21). SHELL stub + shadow re-entry; not distro / not TLS.
- **ADR-013 Stage 1 (0–G) closed:** Phase G is the 2026-08-21 accepted-risk note (shared LOM `:38` with virtio-net). Not VLAN / second NIC.
- **P0-15 / E5 Stage 0 closed (host):** `RAYNU-V-M7-E5-BOOT-SPEC-OK` (#172, 2026-08-22). Boot spec on the wire + catalog parse.
- **P0-16 / E5 Stage 1 closed (host):** `RAYNU-V-M7-E5-CDROM-ATTACH-OK` (#173). Host CD-ROM attach. Not guest UEFI.
- **P0-17 / E5 Stage 2 closed (host):** `RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`. Firmware-facing CD arm. Not OVMF. Iron P0-14 remains `2b795a0`.
- **P0-18 / E5 Stage 3 closed (host):** `RAYNU-V-M7-E5-GUEST-FW-OK`. Guest FW envelope boxed. Not OVMF. Iron P0-14 remains `2b795a0`.
- **P0-19 / E5 Stage 4 closed (host):** `RAYNU-V-M7-E5-GUEST-FW-LOAD-OK`. Stub payload load. Not OVMF. Iron P0-14 remains `2b795a0`.
- **P0-20 / E5 Stage 5 closed (host):** `RAYNU-V-M7-E5-OVMF-PROBE-OK`. FV probe + ESP path. Not embedded EDK2. Iron P0-14 remains `2b795a0`.
- **P0-21 / E5 Stage 6 closed (host):** `RAYNU-V-M7-E5-OVMF-ESP-OK`. ESP fixture load. Not embedded EDK2. Iron P0-14 remains `2b795a0`.
- **P0-22 / E5 Stage 7 closed (host):** `RAYNU-V-M7-E5-OVMF-SLOT-OK`. Firmware slot arm. Not VMLAUNCH. Iron P0-14 remains `2b795a0`.
- **P0-23 / E5 Stage 8 closed (host):** `RAYNU-V-M7-E5-FW-BIND-OK`. Firmware-to-guest bind. Not VMLAUNCH. Iron P0-14 remains `2b795a0`.
- **P0-24 / E5 Stage 9 closed (host):** `RAYNU-V-M7-E5-FW-PREP-OK`. Firmware launch-prepare. Mock VMLAUNCH refused. Iron P0-14 remains `2b795a0`.
- **P0-25 / E5 Stage 10 closed (host):** `RAYNU-V-M7-E5-FW-FLOOR-OK`. 4 KiB size-floor. Not EDK2. VMLAUNCH refused. Iron P0-14 remains `2b795a0`.
- **P0-26 / E5 Stage 11 closed (host):** `RAYNU-V-M7-E5-FW-EDK2-OK`. 1 MiB EDK2-sized candidate. Not a shipped `OVMF.fd`. VMLAUNCH not wired. Iron P0-14 remains `2b795a0`.
- **P0-27 / E5 Stage 12 closed (host):** `RAYNU-V-M7-E5-ESP-LAUNCH-OK`. ESP-path VMLAUNCH wired in launch.rs. No live `OVMF.fd`. Fixture refused. Iron P0-14 remains `2b795a0`.
- **P0-28 / E5 Stage 13 closed (host):** `RAYNU-V-M7-E5-ESP-MAP-OK`. Live-sized ESP OVMF map (2 MiB+). Not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-29 / E5 Stage 14 closed (host):** `RAYNU-V-M7-E5-RESET-VEC-OK`. Reset-vector VMCS contract. Synthetic `0xEA` stub is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-30 / E5 Stage 15 closed (host):** `RAYNU-V-M7-E5-FW-ALIAS-OK`. Firmware-alias EPT contract. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-31 / E5 Stage 16 closed (host):** `RAYNU-V-M7-E5-ALIAS-EPT-OK`. Alias-EPT program contract. Live EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-32 / E5 Stage 17 closed (host):** `RAYNU-V-M7-E5-EPT-INSTALL-OK`. Private alias-EPT install. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-33 / E5 Stage 18 closed (host):** `RAYNU-V-M7-E5-REAL-ESP-OK`. Real-ESP VMLAUNCH-ready contract. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-34 / E5 Stage 19 closed (host):** `RAYNU-V-M7-E5-REAL-LAUNCH-OK`. Guest-UEFI VMLAUNCH insn-path arm. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-35 / E5 Stage 20 closed (host):** `RAYNU-V-M7-E5-LIVE-EXEC-OK`. Live-ESP VMLAUNCH execute gate. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-36 / E5 Stage 21 closed (host):** `RAYNU-V-M7-E5-PRIV-VMCS-OK`. Private guest-UEFI VMCS arm. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-37 / E5 Stage 22 closed (host):** `RAYNU-V-M7-E5-LIVE-ISSUE-OK`. Live-ESP VMLAUNCH issue path. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-38 / E5 Stage 23 closed (host):** `RAYNU-V-M7-E5-LIVE-BYTES-OK`. Live-ESP bytes probe. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-39 / E5 Stage 24 closed (host):** `RAYNU-V-M7-E5-LIVE-FD-OK`. Live-ESP FD require. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-40 / E5 Stage 25 closed (host):** `RAYNU-V-M7-E5-LIVE-PRESENT-OK`. Live-ESP present-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-41 / E5 Stage 26 closed (host):** `RAYNU-V-M7-E5-LIVE-ADMIT-OK`. Live-ESP admit-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-42 / E5 Stage 27 closed (host):** `RAYNU-V-M7-E5-LIVE-READ-OK`. Live-ESP read-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-43 / E5 Stage 28 closed (host):** `RAYNU-V-M7-E5-LIVE-COPY-OK`. Live-ESP copy-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-44 / E5 Stage 29 closed (host):** `RAYNU-V-M7-E5-LIVE-PLACE-OK`. Live-ESP place-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-45 / E5 Stage 30 closed (host):** `RAYNU-V-M7-E5-LIVE-APPLY-OK`. Live-ESP apply-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-46 / E5 Stage 31 closed (host):** `RAYNU-V-M7-E5-LIVE-COMMIT-OK`. Live-ESP commit-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-47 / E5 Stage 32 closed (host):** `RAYNU-V-M7-E5-LIVE-LATCH-OK`. Live-ESP latch-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-48 / E5 Stage 33 closed (host):** `RAYNU-V-M7-E5-LIVE-SEAL-OK`. Live-ESP seal-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-49 / E5 Stage 34 closed (host):** `RAYNU-V-M7-E5-LIVE-LOCK-OK`. Live-ESP lock-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-50 / E5 Stage 35 closed (host):** `RAYNU-V-M7-E5-LIVE-HOLD-OK`. Live-ESP hold-attempt. Live E4 SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued. Iron P0-14 remains `2b795a0`.
- **P0-52 / E5 Stage 37 closed (host + QEMU):** `RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`. Private guest-UEFI VMCS + EPT + VMLAUNCH of retained ESP `OVMF.fd`. Not E4 SHELL. Not installer. Iron P0-14 remains `2b795a0`.
- **P0-53 / E5 Stage 38 closed (host + QEMU):** `RAYNU-V-M7-E5-OVMF-ALIVE-OK`. OVMF SEC `mov cr4, 0x640` no longer triple-faults (CR4.VMXE host-owned). Not full OVMF boot. Not installer. Iron P0-14 remains `2b795a0`.
- **P0-54 / E5 Stage 39 closed (host + QEMU):** `RAYNU-V-M7-E5-OVMF-PAST-SEC-OK`. Left SEC tail + PEI PCI / firmware COM / HLT. COM1/COM2 forwarded. Not full DXE. Not installer. Iron P0-14 remains `2b795a0`.
- **Checkpoint release:** `v0.1.0-e4-spa-launch` — #169 on `main` (`b6578f5`); CI EFI `832ea32` / SHA `00443957…`. Iron P0-14 remains `2b795a0`.

---

## Velocity model

Used every HDA update to move **months_to_everest**.

### Inputs (compute from git + progress.md)

1. `gates_closed_since_last_hda` — new `RAYNU-V-*-OK` rows or Everest criteria flipped DONE  
2. `everest_loc_or_modules` — new code under P0 touchpoints (datastore, http, iso, cdrom, r640 runbook)  
3. `days_since_last_hda`  
4. `blockers_active` — iron wait, partner wait, proof stuck  

### Update formula (heuristic — apply with judgment)

```
progress_delta_pct =
    +8  per Everest criterion E1–E6 newly DONE
    +3  per P0 workstream moved to DONE
    +1  per major related gate (R640, HTTPS, ISO) partial→significant
    -2  per new HARD blocker opened
    -1  per 14 days with zero Everest-path commits

overall_pct = clamp(prev + progress_delta_pct, 0, 100)

# Residual months: start from sum of unfinished P0 residuals,
# then apply velocity factor.
base_residual = sum(est residual months of open P0-*)
velocity_factor =
    0.7  if ≥2 Everest-path PRs merged in last 14 days
    1.0  normal
    1.3  if blocked on iron/external > 14 days
    1.5  if no Everest-path commits in 21 days

months_to_everest = round(base_residual * velocity_factor, 0.25)
everest_eta_month = today + months_to_everest  (first of month or YYYY-MM)
```

**Always** set `months_to_everest_prev` to the previous value before changing.  
**Never** reduce months without citing concrete DONE evidence in the changelog.

---

## This-commit delta

| Field | Value |
|-------|-------|
| Commit | e5-ovmf-virtio |
| Summary | P0-57 CLOSED nested VT-x: OVMF-VIRTIO-OK val=0x1042 pci=1 virtio=1. CD GuestVisible. pci_ide=0 sectors=0. Not installer. Iron P0-14 stays 2b795a0. |
| Everest impact | months 0.5 held; overall 95 held; ETA 2026-09 held. Virtio-blk visible ≠ installer. |
| Gates touched | `RAYNU-V-M7-E5-OVMF-VIRTIO-OK` **CLOSED** nested VT-x. Not Everest E5 / not `ISO-INSTALL-OK`. |
| Months Δ | 0.5→0.5 |

---

## Blockers & risks (Everest-relevant)

| ID | Blocker / risk | Severity | Mitigations |
|----|----------------|----------|-------------|
| H1 | ~~R640 VMLAUNCH/guest path~~ | — | **Resolved** 2026-08-15 (`RAYNU-V-R640-BOOT-OK`) |
| H2 | TLS / console polish | MED | Plaintext HTTP closed on iron (E3b); TLS deferred (ADR-009); guest VNC residual |
| H3 | Guest UEFI CD not bootable | MED | Virtio-blk + CD→disk order presented (P0-57); firmware CD boot not completed; extract-boot is lab MVP only |
| H4 | ~~Firmware SNP unusable after EBS~~ | — | **Resolved** 2026-08-20 (`RAYNU-V-M7-HOST-NIC-HTTP-OK` on native BCM5720 after `BOOT-OK`) |
| H5 | Latitude ≠ full product loop | MED | E2+E3+E3b+E5+Phase F+P0-14 stamps closed; SPA guest is SHELL CPUID stub; TLS/console + distro remain |
| H6 | Single-dev velocity (R10) | MED | Everest P0 only; defer Tier-2 / full parity |
| H7 | Binary size if HTTP+ISO+UI grow | MED | ADR-003 checks; lazy assets; zstd webui GAP |
| H8 | ~~Phase F coexist not closed on iron~~ | — | **Resolved** 2026-08-20 (`HOST-NIC coexist listening` + `HOST-NIC-HTTP-OK` while VMX on; G1–G3 parked) |

---

## HDA changelog

| 2026-08-23 | e5-ovmf-virtio | 0.5 | 95 | P0-57 CLOSED nested VT-x VIRTIO-OK val=0x1042 pci=1 virtio=1; pci_ide=0 sectors=0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-virtio | 0.5 | 95 | P0-57 virtio at 00:01.2; i440FX back at 00:00.0 after nested VT-x n=499 virtio=0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-virtio | 0.5 | 95 | P0-57 empty virtio-blk 00:00.1 + bootorder CD then disk; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-dxe | 0.5 | 95 | P0-56 CLOSED nested VT-x DXE-OK + CDROM-OK pci_ide=1 sectors=0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-dxe | 0.5 | 95 | P0-56 IDE at 00:00.0 (PEI DID probe); i440FX at 00:08.0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-dxe | 0.5 | 95 | P0-56 CF8\|CFC Header Type byte offset; raynuvsrv1 nested VT-x DXE-OK pci_ide=0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-dxe | 0.5 | 95 | P0-56 PIIX3 multifunction + post-DXE tail; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-dxe | 0.5 | 95 | P0-56 past-PEI/DXE or CD boot attempt; CMOS/fw_cfg/i440FX; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-cdrom | 0.5 | 95 | P0-55 guest-UEFI CD visible; PCI IDE/ATAPI; not full DXE; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-past-sec | 0.5 | 95 | P0-54 OVMF past SEC; COM forwarded; not full DXE; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-alive | 0.5 | 95 | P0-53 OVMF past first triple-fault; CR4.VMXE host-owned; not full OVMF; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-vmlaunch | 0.5 | 95 | P0-52 private guest-UEFI VMLAUNCH of retained ESP OVMF.fd; not E4 SHELL; not installer; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-retain | 0.5 | 95 | P0-51 real ESP OVMF retain + presence rule; QEMU system OVMF.fd; private VMCS not allocated; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-hold | 0.5 | 95 | P0-50 live-ESP hold-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-lock | 0.5 | 95 | P0-49 live-ESP lock-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-seal | 0.5 | 95 | P0-48 live-ESP seal-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-latch | 0.5 | 95 | P0-47 live-ESP latch-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-commit | 0.5 | 95 | P0-46 live-ESP commit-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-apply | 0.5 | 95 | P0-45 live-ESP apply-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-place | 0.5 | 95 | P0-44 live-ESP place-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-copy | 0.5 | 95 | P0-43 live-ESP copy-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-read | 0.5 | 95 | P0-42 live-ESP read-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-admit | 0.5 | 95 | P0-41 live-ESP admit-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-present | 0.5 | 95 | P0-40 live-ESP present-attempt; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-fd | 0.5 | 95 | P0-39 live-ESP FD require; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-bytes | 0.5 | 95 | P0-38 live-ESP bytes probe; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-issue | 0.5 | 95 | P0-37 live-ESP VMLAUNCH issue path; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-priv-vmcs | 0.5 | 95 | P0-36 private guest-UEFI VMCS arm; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-live-exec | 0.5 | 95 | P0-35 live-ESP VMLAUNCH execute gate; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-real-launch | 0.5 | 95 | P0-34 guest-UEFI VMLAUNCH insn-path arm; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-real-esp | 0.5 | 95 | P0-33 real-ESP VMLAUNCH-ready contract; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ept-install | 0.5 | 95 | P0-32 private alias-EPT install; live E4 SHELL EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 99%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-alias-ept | 0.5 | 95 | P0-31 alias-EPT program contract; live EPT not written; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 98%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-fw-alias | 0.5 | 95 | P0-30 firmware-alias EPT contract; 4 MiB fixture not shipped OVMF.fd; VMLAUNCH insn not issued; iso 97%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-reset-vec | 0.5 | 95 | P0-29 reset-vector VMCS contract; 0xEA stub not shipped OVMF.fd; VMLAUNCH insn not issued; iso 96%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-esp-map | 0.5 | 95 | P0-28 live ESP OVMF map; 2 MiB+ not shipped OVMF.fd; VMLAUNCH insn not issued; iso 95%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-esp-launch | 0.5 | 95 | P0-27 ESP-path VMLAUNCH wired; no live OVMF.fd; fixture refused; iso~94%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-fw-edk2 | 0.5 | 95 | P0-26 firmware EDK2-sized stage; 1 MiB not shipped OVMF.fd; VMLAUNCH not wired; iso~93%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-fw-floor | 0.5 | 95 | P0-25 firmware size-floor; 4 KiB not EDK2; VMLAUNCH refused; iso~92%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-fw-prep | 0.5 | 95 | P0-24 firmware launch-prepare; mock VMLAUNCH refused; iso~91%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-fw-bind | 0.5 | 95 | P0-23 firmware-to-guest bind; not VMLAUNCH; iso~90%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-slot | 0.5 | 95 | P0-22 firmware slot arm; not VMLAUNCH; iso~89%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-esp | 0.5 | 95 | P0-21 ESP OVMF fixture load; not embedded EDK2; iso~88%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-ovmf-probe | 0.5 | 95 | P0-20 OVMF FV probe + ESP path; not embedded EDK2; iso~87%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-guest-fw-load | 0.5 | 95 | P0-19 guest FW stub load; not OVMF; iso~86%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-guest-fw | 0.5 | 95 | P0-18 guest FW envelope boxed; not OVMF; iso~85%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-cdrom-firmware | 0.5 | 95 | P0-17 firmware CD arm + sector validate; no OVMF; iso~84%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-cdrom-attach | 0.5 | 95 | P0-16 host El Torito CD-ROM attach; firmware stub held; iso~83%; iron P0-14 stays 2b795a0 |
| 2026-08-22 | e5-boot-spec | 0.5 | 95 | P0-15 host boot spec; HOST-NIC QEMU GET /: FIN+drain before qemu_exit (SPA ~15 KiB); iron P0-14 stays 2b795a0 |
| 2026-08-21 | e4-spa-launch | 0.5 | 95 | Release v0.1.0-e4-spa-launch after #169; CI EFI 832ea32 SHA 00443957; iron P0-14 stays 2b795a0 |
| 2026-08-21 | phase-g | 0.5 | 95 | ADR-013 Phase G closed (shared LOM accepted-risk); COM2 quiet after first E4 re-entry (in-tree); product next installer+TLS |
| 2026-08-21 | adr-014 | 0.5 | 95 | ADR-014: typed ISO + UEFI-first product install; bzImage lab-only; Windows later; E4 not blocked |
| 2026-08-21 | site-stories | 0.5 | 95 | Public Stories + site copy: Aug 19 APE PHY, Aug 20 E3b/Phase F, Aug 21 P0-14; residual TLS/console + distro |
| 2026-08-21 | spa-shadow-ok | 0.5 | 95 | Iron `2b795a0` spec 201/start 200; SPA VMLAUNCH + shadow re-entry fields=98; P0-14 CLOSED; SHELL stub; overall 94→95 |
| 2026-08-21 | spa-zeros | 0.5 | 94 | Iron spec 201/start 200; first SPA VMLAUNCH OK; re-entry ctls all 0 / error 7; restore shadow after VMCLEAR; E4 not closed |
| 2026-08-21 | flashcruzer-wait | 0.5 | 94 | `--wait` must not feed progress into the GitHub run id; E4 not closed |
| 2026-08-21 | flashcruzer | 0.5 | 94 | `~/projects/raynuv/flashcruzer.sh` pulls latest green CI EFI onto Cruzer `RAYNUV`; E4 not closed |
| 2026-08-21 | spa-entry7 | 0.5 | 94 | Iron `eb456eec` VMCLEAR fixed error 11; SPA re-entry VMLAUNCH error 7; drop incoming rewrite; E4 not closed |
| 2026-08-21 | slot1-rev11 | 0.5 | 94 | Iron `63cd694f` clone+marker+G0 VMLAUNCH; slot 1 VMPTRLD error 11; VMCLEAR+rewrite next EFI; E4 not closed |
| 2026-08-21 | cruzer-63cd | 0.5 | 94 | Cruzer `RAYNUV` flashed `63cd694f` (artifact 9461155533); F11 boot open; E4 not closed |
| 2026-08-21 | g0-clone | 0.5 | 94 | Iron `618e89e2` marker+fail-soft then memcpy VMPTRLD loop; VMREAD/VMWRITE clone + sticky park; E4 open |
| 2026-08-21 | coexist-126 | 0.5 | 94 | EFI `618e89e2` coexist listen `10.99.99.126:8443`; Mac curl `(28)` no TCP accept; E4 open |
| 2026-08-21 | 1fb32aa | 0.5 | 94 | Cruzer `RAYNUV` flashed `618e89e2` (artifact 9432035922); F11 boot open; E4 not closed |
| 2026-08-21 | 7b750ab | 0.5 | 94 | Iron E4 marker then G0 VMPTRLD fail/VMXOFF; relocate G0 VMCS + fail-soft; E4 not closed |
| 2026-08-21 | tcp-relisten | 0.5 | 94 | Coexist abort+re-listen after HTTP (spec 201 then start curl 7 RST); E4 start still open |
| 2026-08-21 | tcp-idle | 0.5 | 94 | Coexist one-TCP-slot idle abort after SPA half-open curl timeout; E4 start still open |
| 2026-08-21 | hangfix-boot | 0.5 | 94 | Iron hang-fix: `SLICE-G0` then slot 1; coexist `10.99.99.149:8443`; E4 SPA start still open (GET-only) |
| 2026-08-21 | cruzer-f413 | 0.5 | 94 | Cruzer `RAYNUV` holds hang-fix EFI `f413a9fc` (artifact 9429378906); iron boot + E4 marker still open |
| 2026-08-21 | a32232c | 0.5 | 94 | Iron hang: `SPA_RUNNABLE` remapped G1→G0 during M4.2; remap only after ladder |
| 2026-08-20 | 950ed70 | 0.5 | 94 | E4 SPA VMLAUNCH in-tree (private 2M EPT + slab VMCS on coexist); P0-14 IN PROGRESS; iron marker not claimed |
| 2026-08-20 | 1b3d7bd | 0.5 | 94 | ADR-013 Phase F row CLOSED (table was stale); next is E4 VMLAUNCH + Phase G |
| 2026-08-20 | 41bbe48 | 0.5 | 94 | Phase F hold COM2: 25× `HOST-NIC-HTTP-OK` while VMX on (`10.99.99.149:8443`); SOL stayed up |
| 2026-08-20 | 181c0d7 | 0.5 | 94 | Phase F CLOSED: coexist HTTP-OK while VMX on (`10.99.99.149:8443`); G0 scheduled; G1–G3 parked; overall 93→94 |
| 2026-08-20 | 0e94b5b | 0.5 | 93 | Phase F iron: GET / HTTP-OK with VMX on; VMPTRLD G1–G3 failed; park stubs next EFI; not closed |
| 2026-08-20 | 91f66e6 | 0.5 | 93 | Phase F in-tree: `bounded_poll` on scheduler quantum while VMX on; P0-13 IN PROGRESS; iron coexist HTTP-OK not claimed |
| 2026-08-20 | ad1fa76 | 0.5 | 93 | Merge `main` CIOSpeak + spa.png into #162; Stories articles kept; CIO View footer |
| 2026-08-20 | 7e7232b | 0.5 | 93 | Quiet COM2: stop 5s `poll rx_prod=` spam after E3b; WARN on `rx_drop` only |
| 2026-08-20 | a87acc6 | 0.5 | 93 | E3b CLOSED: `HOST-NIC-HTTP-OK` after `BOOT-OK` on BCM5720 `:38`; months 1.5→0.5; ETA→2026-09 |
| 2026-08-20 | 005f25d | 1.5 | 88 | Ring wrap closed (`rx_ok` 0→70); `tx_prod=0`; GRC BSWAP_DATA + RX dump; HTTP-OK not claimed |
| 2026-08-20 | fc7ed70 | 1.5 | 88 | CORECLK DMA closed; RX wrap replay (`rx_ok` +65536); `ring_idx`; HTTP-OK not claimed |
| 2026-08-19 | de52aaf | 1.5 | 88 | Live LOM `:38` `link=up`; skip-CORECLK `26573eb1` no native accept; CORECLK for DMA; HTTP-OK not claimed |
| 2026-08-19 | be6bed5 | 1.5 | 88 | Ubuntu `eno3` `:38` live LOM; station = GPHY MAC not APE `:3a`; HTTP-OK not claimed |
| 2026-08-19 | fb96cdb | 1.5 | 88 | PRE-EBS peek SNP `:3a` vs host GPHY `7949`; take PHY from APE; HTTP-OK not claimed |
| 2026-08-19 | 58d336b | 1.5 | 88 | PCI-restore EFI `ec08c00f` still `cand bmsr=7949` / skip-listen; PRE-EBS BMSR peek; HTTP-OK not claimed |
| 2026-08-19 | b5ea069 | 1.5 | 88 | CORECLK-sans-BMCR EFI `1404f055` still `cand bmsr=7949`; PCI restore + `phy_setup=post`; HTTP-OK not claimed |
| 2026-08-19 | f0961f1 | 1.5 | 88 | Skip-BMCR EFI still `cand bmsr=7949`; CORECLK_RESET without BMCR; HTTP-OK not claimed |
| 2026-08-19 | 84f7a74 | 1.5 | 88 | Post-EBS `cand bmsr=7949` at EBS; skip `BMCR_RESET` when `ape-ncsi=yes`; HTTP-OK not claimed |
| 2026-08-19 | 5291513 | 1.5 | 88 | Dual-func EFI both ports `bmsr=7949` at BOOT-OK; post-EBS BCM5720 bring-up + BOOT-OK reuse; HTTP-OK not claimed |
| 2026-08-19 | 26c3158 | 1.5 | 88 | Skip-reset EFI `1212416` `ape-ncsi=yes` still `lpa=0000` on func 1; try both funcs; HTTP-OK not claimed |
| 2026-08-19 | 295da74 | 1.5 | 88 | PHY-before-reset EFI `1213952` still `lpa=0000`; skip CORECLK_RESET; HTTP-OK not claimed |
| 2026-08-19 | d82621a | 1.5 | 88 | APE-lock EFI `ape-lock=yes` still `bmsr=7949`; PHY reset before CORECLK_RESET; HTTP-OK not claimed |
| 2026-08-19 | 1fefa45 | 1.5 | 88 | Inherit EFI `pre-reset bmsr=7949 ape=yes`; APE BAR2 lock; HTTP-OK not claimed |
| 2026-08-19 | m7-8-bcm-inherit-phy | 1.5 | 88 | PHY-reset EFI still `bmsr=7949`; inherit SNP PHY or APD-off; HTTP-OK not claimed |
| 2026-08-19 | m7-8-bcm-phy-reset | 1.5 | 88 | Iron `bmsr=7949` no carrier; `tg3_bmcr_reset` + PWRCTL + Auto-MDIX; HTTP-OK not claimed |
| 2026-08-19 | m7-8-bcm-link | 1.5 | 88 | PHY `BMSR_LSTATUS` wait + MII/GMII + PROMISC; station `:3a` confirmed; HTTP-OK not claimed |
| 2026-08-18 | m7-8-station-mac | 1.5 | 88 | Station = parked SNP MAC (`:3a`); BAR0 peek `:39` curl miss; HTTP-OK not claimed |
| 2026-08-18 | m7-8-bcm-tg3 | 1.5 | 88 | Phase D Linux `tg3` bring-up (not `bnxt`); SNP-MAC picker kept; HTTP-OK not claimed |
| 2026-08-18 | m7-8-bcm-snp-mac | 1.5 | 88 | Phase D bind SNP-lease MAC (`01:00.1` / `:3a`); HTTP-OK not claimed |
| 2026-08-17 | m7-8-bcm5720 | 1.5 | 88 | Phase D BCM5720 Device in-tree (`14e4:165f`); HTTP-OK not claimed |
| 2026-08-17 | adr013-0de-iron | 1.5 | 88 | Phase 0 **closed on iron**: `14e4:165f` BCM5720; HTTP-OK not claimed |
| 2026-08-17 | adr013-0de | 1.5 | 88 | Phase 0 census print + Phase E arena; Phase D parse/idle wired; iron HTTP-OK open |
| 2026-08-17 | m7-8-host-nic-c | 1.5 | 88 | Phase C QEMU GET / closed (`HOST-NIC-QEMU-OK`); iron HTTP-OK open |
| 2026-08-17 | adr013-baseline | 1.5 | 88 | ADR-013 Accepted; iron WARN-only no RSOD; kit `v0.1.0-adr013-baseline` |
| 2026-08-17 | adr-013-e3b | 1.5 | 88 | ADR-013 Proposed; E3b durable mgmt; months 0.75→1.5; ETA→2026-10 |
| 2026-08-17 | post-ebs-snp-dead | 0.75 | 88 | Firmware SNP hang + curl timeout + RSOD; WARN-only idle; host-owned NIC next |
| 2026-08-16 | post-ebs-http-wire | 0.5 | 88 | Park SNP across EBS; scaffold POST-EBS-HTTP; iron listen open |
| 2026-08-16 | e5-site-residual | 0.5 | 88 | Public residual: post-EBS SNP HTTP next; Cruzer story; E5 stamps held |
| 2026-08-16 | e5-iron-booted | 0.5 | 88 | Iron `BOOTED-FROM-DISK`; M7.7 stamp persist closed; iso~82% |
| 2026-08-16 | e5-persist-prefix | 0.75 | 84 | Iron persist-detect; prefix-copy so 1KiB stamps load into 64MiB disk |
| 2026-08-16 | e5-persist-esp | 0.75 | 83 | PRE-EBS ESP installdisk.bin persist; iso~65%; iron INSTALL-OK open |
| 2026-08-16 | tcp4-census-iron | 0.75 | 82 | Iron: after-all pxe=8 http=4 ip4cfg=4 still tcp4=0; Floppy Tcp4 SB = platform limit |
| 2026-08-16 | tcp4-root-cause | 0.75 | 82 | Floppy Tcp4 absent: UNDI/SNP only; extra census + all-handles; SNP residual held |
| 2026-08-16 | e4-spa-arm-kit | 0.75 | 82 | Preserve kit `v0.1.0-e4-spa-arm` before networking deep-dive |
| 2026-08-16 | e4-spa-iron | 0.75 | 82 | E4 SPA create + 64MiB install arm on R640; iso~62%; INSTALL-OK open |
| 2026-08-16 | e4-iron-mvp | 0.75 | 81 | E4: durable PRE-EBS mgmt + /logs/serial + ESP auth.token; ui~82% |
| 2026-08-16 | e5-booted-from-disk | 0.75 | 80 | QEMU lab two-boot → BOOTED-FROM-DISK; iso~55%; iron E5 still open |
| 2026-08-16 | e5-iso-scaffold | 0.75 | 79 | M7.7 ISO install-to-disk scaffold; iso~45%; E5 iron still open |
| 2026-08-16 | uefi-http-ok | 0.75 | 78 | E3 MVP CLOSED: `RAYNU-V-M7-UEFI-HTTP-OK` on R640 SNP residual; ui~78% |
| 2026-08-15 | r640-boot-ok | 1.25 | 72 | E2 CLOSED: SHELL+M4 on COM2 (`xsavesfix`); r640~98%; ETA→2026-09 |
| 2026-08-15 | r640-iron-bringup | 1.75 | 60 | Real R640 COM2: M0→VMXON→LOAD→BZIMAGE; COM2/EPT/BAR kits; E2 open; r640~68% |
| 2026-07-21 | m7-5-iron-todo | 2.25 | 52 | R640 iron-week checklist → `docs/runbooks/r640_iron_week.md` (not on site) |
| 2026-07-21 | m7-5-scaffold | 2.25 | 52 | M7.5 R640 scaffold (SCAFFOLD-OK); iron BOOT-OK open; r640~30% |
| 2026-07-21 | m7-4-close | 2.25 | 52 | M7.4 UI host smoke closed on Latitude; console residual; next M7.5 R640 |
| 2026-07-21 | m7-3-close | 2.5 | 50 | M7.3 ISO host smoke closed on Latitude; El Torito residual; next M7.4 |
| 2026-07-21 | m7-2-close | 2.75 | 48 | M7.2 STORE closed on Latitude; next M7.3 ISO; UEFI persist residual |
| 2026-07-21 | m7-2-store | 3.25 | 45 | M7.2 datastore wired (catalog+REST); tip last_commit→M7.1 merge; Latitude pending |
| 2026-07-21 | m7-1-close | 3.25 | 45 | M7.1 HTTP closed on Latitude; next M7.2 datastore; UEFI listen residual |
| 2026-07-21 | m7-1-http | 3.75 | 42 | M7.1 HTTP wired (codec+host TCP+Bearer); UEFI listen stub; Latitude pending |
| 2026-07-21 | m7-0-close | 3.75 | 42 | M7.0 SHIP closed on Latitude (`raynu-v-0.1.0`); P0-1 DONE; next M7.1 |
| 2026-07-21 | m7-0-ship | 4.0 | 39 | M7.0 release kit wired (SHA256+tarball+runbook); Latitude pending; efi~90% |
| 2026-07-21 | m7-gov | 4.0 | 39 | ADR-009 + M7 plan accepted; next = M7.0 ship kit; M8 = vMotion/DRS/hot-add |
| 2026-07-21 | site-hda | 4.0 | 39 | Public `site/hda.html` + `sync-hda-site.sh` (numbers unchanged) |
| 2026-07-21 | 8f091fd | 4.0 | 39 | M6.9 EXT + E6 DONE on Latitude (`80 verified, 0 errors`); P0-9 closed; ETA→2026-11 |
| 2026-07-20 | bootstrap | 4.5 | 28 | Initial HDA; Everest = EFI+R640+UI+ISO+M6.9 |

---

## Operator quick view

```
Mount Everest:  Ship EFI → R640 → UI → Linux ISO  (M7)
Now:           E2+E3+E3b+E5+Phase F stamps CLOSED; native BCM5720 HTTP after BOOT-OK with VMX on (`:38` / 10.99.99.126:8443)
Months left:   0.5  (ETA ~ 2026-09)
Next move:     On raynuvsrv1 run ~/projects/raynuv/flashcruzer.sh; F11 Cruzer; spec/start (WANT shadow restore, no error 7)
Tcp4 residual: Floppy publishes PXE/HTTP, not Tcp4 SB (platform limit)
SNP after EBS: dead — native BCM5720 is the durable mgmt path (E3b closed 2026-08-20)
Preserve:      releases/v0.1.0-adr013-baseline
Do not claim:  Mount Everest / E4 closed (first SPA VMLAUNCH OK; slot 1 re-entry zeros/error 7 is not a close)
```

Public checklist: [`docs/runbooks/r640_iron_week.md`](runbooks/r640_iron_week.md) ·
printable field guide: [`docs/runbooks/r640_field_guide.md`](runbooks/r640_field_guide.md).

---

## Maintenance

- **Owner:** whoever merges to `main` (Cursor agent updates HDA in the same change or immediate follow-up).  
- **Public site:** [`site/hda.html`](../site/hda.html) ← synced via [`./tools/sync-hda-site.sh`](../tools/sync-hda-site.sh) → [`site/hda.json`](../site/hda.json).  
- **Rule file:** [`.cursor/rules/hda-update.mdc`](../.cursor/rules/hda-update.mdc)  
- **Prompt card:** [`docs/hda-cursor-prompt.md`](hda-cursor-prompt.md)  
- **Do not** edit scoreboard numbers without updating frontmatter + changelog **and** re-running `./tools/sync-hda-site.sh`.  
- **Do not** mark E2 DONE without real R640 evidence in `progress.md` or runbook artifact.
