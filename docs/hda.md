---
hda_version: 1
last_updated: 2026-09-01
last_commit: 2b795a0bef4ae5a5c356a0131205f9de439ffe57
last_commit_short: 2b795a0
updated_by: cursor
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO (M7 Mount Everest)"
months_to_everest: 0.5
months_to_everest_prev: 0.5
velocity_commits_30d: 376
velocity_gates_30d: 61
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
| ISO parse / El Torito / EFI boot img | DONE (guest CD EFI) | Iron COM2 `0be7283` `OVMF-ELTORITO-OK` `RN-ELT` n=197992; not distro installer |
| CD-ROM attach | DONE (firmware StartImage) | GuestVisible PCI IDE/ATAPI + El Torito FAT ESP BOOTX64; not `ISO-INSTALL-OK` |
| Guest UEFI firmware blob | PARTIAL (ESP retained + El Torito) | Real ESP OVMF.fd retained; private VMCS ran CD EFI on iron; not distro installer / not Everest E5 |
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
- **P0-59 / E5 Stage 44 closed (iron COM2 `bf696ca`):** `RAYNU-V-M7-E5-OVMF-ATAPI-OK`. `sectors=1` `packet=9` `scsi=0x28`. Not El Torito. Iron P0-14 remains `2b795a0`.
- **P0-61 / E5 Stage 45 closed (iron COM2 `0be7283`):** `RAYNU-V-M7-E5-OVMF-ELTORITO-OK`. `RN-ELT` n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1. Not installer. Iron P0-14 remains `2b795a0`.
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
| Commit | e5-stage46-iso |
| Summary | Retrigger ee82483 wait-for-PIT after nested-KVM QEMU clwb flake (33554248661). Not ISO-INSTALL-OK. Iron P0-14 stays 2b795a0. |
| Everest impact | months 0.5 held; overall 95 held; ETA 2026-09 held. dest_ok proved; ATA not started. |
| Gates touched | Stage 46 OPEN (3a done; 3b fail HLT; wait-for-PIT next flash). Not Everest E5. |
| Months Δ | 0.5→0.5 |

---

## Blockers & risks (Everest-relevant)

| ID | Blocker / risk | Severity | Mitigations |
|----|----------------|----------|-------------|
| H1 | ~~R640 VMLAUNCH/guest path~~ | — | **Resolved** 2026-08-15 (`RAYNU-V-R640-BOOT-OK`) |
| H2 | TLS / console polish | MED | Plaintext HTTP closed on iron (E3b); TLS deferred (ADR-009); guest VNC residual |
| H3 | Guest UEFI CD not bootable | MED | ATAPI `sectors>0` closed (P0-59); Stage 45 El Torito closed on iron COM2 `0be7283`; P0-60 G1 EPT closed; G0 relocate closed (`M4-NVM-OK`); M4.3 host-slab closed (`M4-BLK-OK` `0x10c00000`); Stage 46 OPEN (ESP product ISO + virtio-pci queues + PIC/IOAPIC inject + 16550/ttyS0 + SOL RX + Alpine auto-answer `BOOTLOADER=grub` `USE_EFI=1` mkdir `/media/cdrom` `virtio_pci` `mdev -s` wait `/dev/vda` `sr_mod` `isofs` `-t iso9660` `-s 0` `[y/N]` still hears `bootloader?` / `Which disk` / `No disks available`→n + `How would you like`→sys + apk repos overwrite + `squashfs,virtio_blk console=ttyS0` + xAPIC 4K trap/`lapic_virt` + IOAPIC→LAPIC IRR/ISR + guest-UEFI CR8 TPR exiting + `alpine_dev=vdb` + PIT IRQ 0 + i8253 16-bit + GRUB `set timeout=1` / efi_gop / all_video / `terminal_output console` serial + MMIO XCHG/MOVSX/moffs + group-1 AND/OR/XOR/ADD/ADC/SBB/SUB + group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR + CMOV/SETCC + PREFETCH/NOP/CLFLUSH + BSF/BSR + IMUL + MUL/IMUL DX:AX + DIV/IDIV + MOVNTI + SHLD/SHRD + CMPXCHG8B + TZCNT/LZCNT/POPCNT + PUSH/POP r/m + MOVS/STOS/LODS + CALL/JMP r/m + CMPS/SCAS + MOVUPS/MOVDQU XMM trampoline + flash-RIP insn fetch + reserve install disk before scratch and report-RAM + iron product-ISO 512MiB pool (`iso=0`/nested 256MiB) + CS.base+RIP MMIO fetch + peek RIP when CS.base+RIP misses flash + MMIO skip-len from fetched bytes when VMCS len is 0 + Alpine BOOT_SIZE=48 ESP + port 0x61 TMR2_OUT + arm ISO window before disk attach + skip 256MiB disk when report-RAM would starve + ISO patches ASCII-only (NUL pad either side so alpine-virt grub.cfg `set timeout=1` still patches; no gzip vmlinuz) + flashcruzer detached HEAD infers origin branch + xAPIC EAX fallback when skip-len 1-15 even if n>0 + register-form ALU (mem and dest-reg) + TEST/CMP/ALU RFLAGS + INC/DEC/NOT/NEG + BT/BTS/BTR/BTC + CMPXCHG/XADD + 31-sector ATAPI PIO + chained virtio OUT + read-only ISO virtio `00:03.0` + 4KiB GPA copies + virtio IOAPIC pin 11 + MMIO fetch across pages + hold when armed; lab stub still E4; armed nested product cap 16_777_216 + leftover DRAM above PRECISE fills 1008 report-RAM 2MiB, iso=0 stays 32; nested product-ISO seeds leftover + delay_loop skip-10 to ret + RAX=1 + Linux hide hypervisor/0x4000 scan + GRUB lpj=4194304 no_timer_check tsc=reliable clocksource=tsc idle=poll + HPET 1ms on CPUID/MSR/EPT + HPET TSC-delta on UART COM I/O cap 4us + ISO9660 grub.cfg Data Length 143→208 + Linux hypervisor_cpuid_base GPR bump to 0x4000FF00 + alpine-virt native_cpuid push %rbx RSP slot + Linux printk ticks every 4096 + guest UART nowait (do not clear COM2_LIVE) + Linux CPUID GenuineIntel + NX + guest UART TX ring drain + guest UART TX ring drain 4/exit + linux earlycon share TX ring + linux earlycon quiet ticks + linux earlycon hush HV + linux earlycon share product ISO + cpu_flush on tick cadence even when share + linux earlycon share first CPUID + linux earlycon skip #PF dump + linux earlycon skip exc deliver + linux earlycon share first high-half + poll ISO-INSTALL-OK every resume + ISO-INSTALL-OK on GPT not 16KiB + setup-disk before apk update + 256MiB disk leftover report-RAM + linux earlycon share first bootimg + guest UART TX drain COM2 independent + linux earlycon pace LSR THRE + report-RAM EPT pre-map + cpu_flush skip leftover pre-map + cpu_flush leftover per walk + linux unhandled nowait stop + virtio MMIO eax fallback + linux NMI inject + linux MMIO decode retry + linux EAX fallback skip 3 + IOAPIC decode fail nowait + linux MOV DR skip + virtio BAR trap over scratch + PIIX3 ISA BAR RAZ + packed virtio common cfg + virtio MMIO raises PIT + virtio MMIO off= + virtio MMIO eax fallback size; packed virtio common cfg write; virtio MMIO polls lapic; linux I/O does not raise PIT (iron MADT stop); linux xAPIC EPT insn_len 0; linux preempt deadloop noskip; linux PIT prefer once; linux PIT prefer until DRIVER_OK; UART reassert RX not THRE; product ISO fw_cfg ACPI MADT (iso=0 named files stay 3); linux PIC before LAPIC; linux PIC IRQ0; MADT IRQ0 ISO GSI 2; PIT skips IOAPIC pin 0; linux GSI 2 before PIC; fw_cfg IoReadFifo8 fills RAM (skip HV identity PML4 dest); PIIX4 PM1 SCI_EN; DSDT PCI0 _PRT; DSDT PCI0 _CRS; linux hides duplicate slot0 IDE; linux hides PIIX IDE; linux high-half hides PIIX; linux-line alpine_dev=vdb; linux-line virtio_pci; linux ATA floating bus; fw_cfg skip dest n=; fw_cfg identity overlay; HV identity PML4 0x400000; PEI dest holds ACPI tables; fw_cfg dest_ok fill dest=; dest_ok fill log cap 8; ACPI tables ZONE_FSEG; FSEG dest holds ACPI tables; linux-line ata_piix blacklist; linux-line piix_init blacklist; FADT FACS; flash 8663f56; flashcruzer reject 2d6b109 dest skip; auto-answer / # without login; product ISO POST_DXE_TAIL skip; emergency mount+exit; linux-line usbdelay; io string (rep insb); 0xB000 dword timer; HLT stall quiet tick print-only; do not F11 9ce65ae; firmware virtual-wire PIC; firmware virtual-wire AEOI; firmware virtual-wire GSI 2; firmware HLT force IF; firmware HLT skip after inject; firmware HLT activity active; firmware LAPIC timer expiry; IOAPIC I/O over PIT; firmware virtual-wire GSI 14; flash 5c0f7a2; do not F11 2ae4544; product ISO fw_cfg bootorder virtio-iso scsi@3 first; do not F11 eac424b; do not F11 8e81c2e; do not F11 daf3195; do not F11 b26c86a; firmware HLT wait-for-PIT before ATA; iron COM2 b5c3a9c dest_ok then skip-after-inject ataio=0). extract-boot is lab MVP only | |
| H4 | ~~Firmware SNP unusable after EBS~~ | — | **Resolved** 2026-08-20 (`RAYNU-V-M7-HOST-NIC-HTTP-OK` on native BCM5720 after `BOOT-OK`) |
| H5 | Latitude ≠ full product loop | MED | E2+E3+E3b+E5+Phase F+P0-14 stamps closed; SPA guest is SHELL CPUID stub; TLS/console + distro remain |
| H6 | Single-dev velocity (R10) | MED | Everest P0 only; defer Tier-2 / full parity |
| H7 | Binary size if HTTP+ISO+UI grow | MED | ADR-003 checks; lazy assets; zstd webui GAP |
| H8 | ~~Phase F coexist not closed on iron~~ | — | **Resolved** 2026-08-20 (`HOST-NIC coexist listening` + `HOST-NIC-HTTP-OK` while VMX on; G1–G3 parked) |
| H9 | PR #231 IdeBus/SCI fork | LOW | **Parked** 2026-09-01 (ADR-015). Tip `8024439` stays parked. Close path #229 is dest_ok `b5c3a9c` plus HLT wait-for-PIT. Do not F11 `33440050729` again. Do not flash `8024439`. Do not OR PCI command `0x0001`. |

---

## HDA changelog

| 2026-09-01 | e5-stage46-iso | 0.5 | 95 | Retrigger ee82483 after nested-KVM QEMU clwb #UD kill-init (33554248661 UEFI+host green); wait-for-PIT unchanged; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-09-01 | e5-stage46-iso | 0.5 | 95 | Iron COM2 b5c3a9c dest_ok + ACPI MADT then HLT ataio=0; #229 wait-for-PIT before ATA; do not F11 33440050729; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-sixth slice: firmware HLT insn_len 0 skip (nested CpuSleep f4c3 ataio=0; PIC ATA vector follows ICW2 unchanged; F11 pin stays 33436232227); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-fifth slice: retrigger 0d36b53 CI after nested ATAPI miss (33437881901 ataio=0 packet=0; PIC ATA vector follows ICW2 unchanged; F11 pin stays 33436232227); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-fourth slice: PIC ATA vector follows ICW2 (leftover 0x2E rewrite on ICW2 cycle, not ICW4 ready; F11 pin stays 33436232227 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-third slice: flash a14223f pin 33436232227 (do not clobber PIC ICW2 CI green; EFI 9774506155; do not F11 3b7bbac / --run 33433126839); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-second slice: residual do not clobber PIC ICW2 (E5_OVMF_VMLAUNCH_RESIDUAL_NOTE; F11 pin stays 33433126839 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirty-first slice: do not clobber PIC ICW2 (arm during ICW4-pending overwrote 0x70→0x20; F11 pin stays 33433126839 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirtieth slice: flash 3b7bbac pin 33433126839 (leftover IOAPIC 0x2E CI green; do not F11 e4faceb / --run 33429494930); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-ninth slice: do not inject leftover IOAPIC 0x2E after PIC remap to 0x76 (F11 pin stays 33429494930 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-eighth slice: retrigger 5a69de2 CI after nested-KVM kill-init (33430294210 iso=0 after GTIMER2; firmware OVMF ATA vector unchanged; F11 pin stays 33429494930); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-seventh slice: flash e4faceb pin 33429494930 (firmware OVMF ATA vector CI green; do not F11 d7d63ca / --run 33426291731); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-sixth slice: firmware OVMF ATA vector (do not clobber IOAPIC ATA to 0x2E; EDK2 8259 0x70→0x76; F11 pin stays 33426291731 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-fifth slice: flash d7d63ca pin 33426291731 (firmware PIC ATA CI green; do not F11 8e581c7 / --run 33424573770); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-fourth slice: firmware PIC ATA (take PIC 0x2E when the 8259 can deliver it; F11 pin stays 33424573770 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-third slice: flash 8e581c7 pin 33424573770 (IOAPIC edge no remote IRR CI green; do not F11 30b78a0 / --run 33422323257); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-second slice: IOAPIC edge no remote IRR (PACKET after IDENTIFY without IOAPIC EOI; F11 pin stays 33422323257 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twenty-first slice: flash 30b78a0 pin 33422323257 (firmware take IOAPIC ATA CI green; do not F11 0bb06a2 / --run 33418246409); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twentieth slice: firmware take IOAPIC ATA (pin 14 only; do not latch virtio into IRR that ata_irr_only will not inject; F11 pin stays 33418246409 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-nineteenth slice: flash 0bb06a2 pin 33418246409 (firmware ATA IRR only CI green; do not F11 12926eb / --run 33415083012); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-eighteenth slice: retrigger cdbee39 CI after nested-KVM kill-init (33417361559 iso=0 after GTIMER2; firmware ATA IRR only unchanged; F11 pin stays 33415083012); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-seventeenth slice: firmware ATA IRR only (do not take_highest_irr LVT 0xEF before PACKET; F11 pin stays 33415083012 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-sixteenth slice: flash 12926eb pin 33415083012 (firmware ATA over PIC keeps latched 0x2E CI green; do not F11 eaa580d / --run 33413425759); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-fifteenth slice: firmware ATA over PIC keeps latched 0x2E (next HLT raise_pit must not steal PIC 0x20 after take_ioapic; F11 pin stays 33413425759 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-fourteenth slice: flash eaa580d pin 33413425759 (firmware ATA over PIC CI green; do not F11 bce5bbb / --run 33411580450); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-thirteenth slice: firmware ATA over PIC (HLT raise_pit PIC IRQ 0 must not skip latching 0x2E; F11 pin stays 33411580450 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-twelfth slice: flash bce5bbb pin 33411580450 (firmware prefer ATA IRR CI green; do not F11 489d938 / --run 33408594472); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-eleventh slice: firmware prefer ATA IRR (PACKET 0x2E ignores TPR; not take_highest_irr; F11 pin stays 33408594472 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-tenth slice: flash 489d938 pin 33408594472 (firmware arm ATA GSI 14 CI green; do not F11 5227ad9 / --run 33404368817); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-ninth slice: firmware arm ATA GSI 14 (wait_for_irq false never unmasked pin 14; F11 pin stays 33404368817 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-eighth slice: flash 5227ad9 pin 33404368817 (firmware force IF for inject CI green; do not F11 77f5866 / --run 33402411199); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-seventh slice: retrigger 9df52c5 CI after nested-KVM SHELL flake (33402411199 iso=0 5/5 after GTIMER2; force-IF firmware unchanged; do not F11 77f5866); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-sixth slice: firmware force IF for inject (PACKET nIEN=0 after ataio>0 still needs IF; do not F11 77f5866); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-fifth slice: flash 77f5866 pin 33399209557 (firmware skip PIT inject CI green; do not F11 e70a295); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-fourth slice: firmware skip PIT inject (ATA 14 / virtio INTx still inject; do not F11 e70a295); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-third slice: flash e70a295 pin 33397104645 (firmware HLT skip after ataio CI green; do not F11 90da03d); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-second slice: firmware HLT skip after ataio (PACKET HLT after ataio>0 still skips + Active; do not F11 90da03d); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundred-first slice: flash 90da03d pin 33394776080 (product ISO fw_cfg bootorder El Torito ide@ first CI green; do not F11 56f31d3); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN two-hundredth slice: product ISO fw_cfg bootorder El Torito ide@ first (scsi@3 first was not a BDS CD option; do not F11 56f31d3); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-ninth slice: flash 56f31d3 pin 33392055961 (firmware HLT skip without inject CI green; do not F11 ea30da1 / a2acfc8); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-eighth slice: firmware HLT skip without inject + un-hide PIIX IDE (iron COM2 ea30da1 inject vec=0x20 timer ISR to n=16777216; do not F11 ea30da1 / a2acfc8 / d61dc7e); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-seventh slice: product ISO HLT stall before n=16384 (hide-IDE virtio-iso CpuSleep; do not F11 ea30da1 / d61dc7e); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-sixth slice: flash ea30da1 pin 33389381409 (skip-after-inject uses pci_ready CI green; do not F11 b824789); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-fifth slice: skip-after-inject uses pci_ready (hide-IDE virtio enum; do not flash b824789 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-fourth slice: flash b824789 pin 33387614559 (product ISO hides PIIX IDE CI green; do not F11 d61dc7e); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-third slice: retrigger hide-IDE CI after nested-KVM kill-init flake (8336a06 run 33387083800; iso=0 CDROM-OK BOTH-OK; pin stays 33349142609); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-second slice: product ISO hides PIIX IDE (iron COM2 d61dc7e scsi@3 first then ConnectAll CpuSleep pci_ide=1 ataio=0; iso=0 keeps IDE); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninety-first slice: flash d61dc7e pin 33349142609 (virtio-iso scsi@3 first CI green; iron COM2 ticks printing; do not F11 5c0f7a2); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninetieth slice: product ISO fw_cfg bootorder virtio-iso scsi@3 first (ConnectDevicesFromQemu skips IdeBus CpuSleep; iso=0 CD then disk); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-ninth slice: flash 5c0f7a2 pin 33347766697 (IOAPIC I/O over PIT CI green; do not F11 2ae4544); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-eighth slice: IOAPIC I/O over PIT + firmware virtual-wire GSI 14 (virtual-wire pin 2 would starve ATA 14; F11 pin stays 2ae4544 until this CI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-seventh slice: flash 2ae4544 pin 33345731636 (firmware LAPIC timer expiry CI 49/49; do not F11 b26c86a); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-sixth slice: firmware LAPIC timer expiry (HLT-exiting never lets CUR_COUNT hit 0; do not F11 b26c86a); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-fifth slice: firmware HLT activity active (skip RIP while activity HLT parks RET; do not F11 daf3195); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-fourth slice: firmware HLT skip after inject (iron eac424b IRET-to-HLT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-31 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-third slice: firmware virtual-wire GSI 2 + HLT force IF (iron eac424b pic=1 sparse inject CR8 ataio=0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-second slice: firmware virtual-wire AEOI (OVMF IDT[0x20] EOIs LAPIC not PIC; do not F11 eac424b); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighty-first slice: firmware virtual-wire PIC (iron beb1576 pic=0 gsi2=0 IF=1 TPR=0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eightieth slice: firmware HLT stall waits for IRQ (do not skip_hlt after BOTH-OK ataio=0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-ninth slice: firmware HLT ignores TPR (iron 084430f inject vec=0x20 only after CR8; PIC-first still needs pic_ready); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-eighth slice: HLT stall quiet tick print-only (nested 9ce65ae ATAPI-OK missing after quiet skipped cpu_flush; do not F11 9ce65ae / c08a13d); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-seventh slice: firmware PIC before GSI 2 + HLT stall quiet tick (iron 084430f Delay via 0xB008 then HLT 0x7f0680d0 ataio=0; do not F11 c08a13d); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-sixth slice: 0xB000 dword timer (iron 8663f56 unh=4 then handled Delay not 0xB008; F11 pin stays 084430f); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-fifth slice: flash 084430f pin 33337287432 (0xAF00 PM timer CI 49/49; do not F11 8663f56 / run 33333506987 dest_ok then Delay); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-fourth slice: 0xAF00 PM timer (iron COM2 8663f56 dest_ok pde0=0x40b027 then unhandled 0xAF00/0xAF05 + IN EAX,DX Delay stop n=33297 sectors=0) + tick port=; do not F11 8663f56 again; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-third slice: linux-line usbdelay (alpine-virt 3.21 mkinitfs myopts has no alpine_dev; same-length usbdelay=30 so nlplug waits 30s not 5s) + io string (rep insb); F11 pin stays 8663f56; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-second slice: product ISO POST_DXE_TAIL skip (armed Stage 46 does not stop at n=33297 sectors=0; lab iso=0 still uses the tail) + emergency mount+exit (3.21 /init has no setup-disk; F11 pin stays 8663f56); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventy-first slice: iron COM2 2d6b109 pde0=0x20b027 (identity 0x200000; no dest_ok fill; DXE n=529 stop n=33297 sectors=0 catalog=0 ataio=0 POST_DXE_TAIL; not 8663f56); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventieth slice: auto-answer / # without login (alpine-virt 3.21 /init emergency shell is already root; no getty login:; F11 pin stays 8663f56 run 33333506987); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-ninth slice: flashcruzer reject 2d6b109 dest skip (EFI prefix 6fc742b0 / run 33321642509 FLASHCRUZER-OK is not F11; pin 8663f56 run 33333506987); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-eighth slice: flash 8663f56 (CI 49/49; 2d6b109 IoReadFifo8 still skips dest 0x205f18 inside identity 0x200000 so ACPI cannot install; identity 0x400000 + FSEG + FACS); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-seventh slice: flashcruzer --branch checkout -B origin (git fetch origin NAME only writes FETCH_HEAD; checkout -B origin/NAME then --no-git flashes 2d6b109); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-sixth slice: FADT FACS (FIRMWARE_CTRL at FACP+36; 64-byte FACS 64-aligned after DSDT; ADD_POINTER before FACP CKSUM; not an SDT checksum); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-fifth slice: PM1 SCI_EN at reset (FADT SMI_CMD is 0 so Linux acpi_hw_get_mode never writes SCI_EN; PM1_CNT starts with bit 0 so ACPI-on matches); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-fourth slice: linux-line piix_init blacklist (initcall_blacklist=piix_init; Linux 6.12 ata_piix.c is module_init(piix_init), not ata_piix_init; grub.cfg Data Length stays 143→294); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-third slice: linux-line ata_piix blacklist (initcall_blacklist=ata_piix_init so built-in ata_piix does not ata_msleep after Freeing initrd; grub.cfg Data Length 143→294); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-second slice: FSEG dest holds ACPI tables (ZONE_FSEG ALLOC dest in conventional 640KiB identity, not PEI stack 0x205f18 and not ZONE_HIGH leftover); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixty-first slice: ACPI tables ZONE_FSEG (etc/acpi/tables ALLOC dest in FSEG below 1MiB identity slab, not ZONE_HIGH ~2GiB leftover); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixtieth slice: dest_ok fill log cap 8 (COM2 after successful store so ZONE_HIGH ACPI dest is visible, not only file dir); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-ninth slice: fw_cfg dest_ok fill dest= (COM2 dest n= when IoReadFifo8 n>16 lands in ordinary RAM; file dir/table-loader/ACPI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-eighth slice: PEI dest holds ACPI tables (overlay n<=16 cannot; dest_ok 0x205f18 fills etc/acpi/tables RSDT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-seventh slice: HV identity PML4 0x400000 (off PEI stack dest 0x205f18 so fw_cfg file-dir/ACPI fill ordinary RAM); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-sixth slice: fw_cfg identity overlay (n<=16 QEMU signature into HV identity dest; restore PTEs on next VM-exit); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-fifth slice: fw_cfg skip dest n= (COM2 dest= n= if QemuFwCfgInitialize overlaps HV identity PML4); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-fourth slice: linux ATA floating bus (compat ISA 0x1F0/0x170 stays decoded after PCI hide; 0xFF after high-half so leftover ata_piix SRST skips without ata_msleep); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-third slice: linux-line virtio_pci (initramfs modules= loads PCI transport before nlplug -b vdb); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-second slice: linux-line alpine_dev=vdb (iron COM2 cmdline had no alpine_dev; alpine-virt grub.cfg alpine_dev=cdrom swap is 0 hits; after high-half hides PIIX nlplug needs -b vdb); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifty-first slice: linux high-half hides PIIX (bootimg earlycon share is too early; GRUB still needs PIIX ATAPI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fiftieth slice: linux hides PIIX IDE (00:01.1 gone after earlycon so built-in ata_piix does not SRST-msleep past Freeing initrd; firmware still sees PIIX; media virtio-iso); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-ninth slice: DSDT PCI0 _CRS (ACPI-on BAR window 0xC0000000..0xFEBFFFFF, not 0x80000000 scratch); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-eighth slice: linux hides duplicate slot0 IDE (00:00.1 gone after earlycon; PIIX 00:01.1 stays); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-seventh slice: DSDT PCI0 _PRT (virtio slot 2/3 INTA GSI 17/18 after ACPI tables); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-sixth slice: PIIX4 PM1 SCI_EN (Linux acpi_enable reads SCI_EN back; FADT 0xB004 was RAZ/WI); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-fifth slice: fw_cfg IoReadFifo8 fills RAM (OVMF QemuFwCfgInitialize rep insb; skip HV identity PML4 dest); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-fourth slice: linux GSI 2 before PIC (ACPI pin 2 RTE beats leftover PIC IRQ 0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-third slice: PIT skips IOAPIC pin 0 (ACPI GSI 2 not stolen by leftover pin 0 after PIC mask); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-second slice: MADT IRQ0 ISO GSI 2 + linux PIC IRQ0 nowait (ACPI timer on pin 2; COM2 sees IRQ0 through earlycon hush); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-forty-first slice: linux PIC before LAPIC (iron a525340 Freeing initrd then silent; OVMF leftover IOAPIC stole PIT from virtual-wire); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fortieth slice: product ISO fw_cfg ACPI MADT (iso=0 named files stay 3; OVMF InstallQemuFwCfgTables so Linux EFI ST has ACPI=/MADT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-ninth slice: linux I/O does not raise PIT (iron MADT stop) + linux xAPIC EPT insn_len 0 (bc6fb70 APIC MADT then restore host xcr0; 4b0d96a reached Freeing initrd); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-eighth slice: virtio drain every resume + COM2 `linux virtio DRIVER_OK`; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-seventh slice: UART reassert RX not THRE (THRE reassert every resume kept IRQ 4 ahead of PIT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-sixth slice: linux PIT prefer until DRIVER_OK (keep raising PIT; UART beats PIT after both virtio functions DRIVER_OK); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-fifth slice: PIT on every Linux non-UART exit + COM2 `linux I/O raises PIT` log; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-fourth slice: linux I/O raises PIT + linux preempt deadloop noskip + linux PIT prefer once (iron 8x virtio MMIO off=0x14 then ATA/PCI I/O then Freeing initrd); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-third slice: packed virtio common cfg write + LAPIC timer poll on Linux MMIO; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-second slice: virtio MMIO eax fallback size (status 0x14 is a byte); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirty-first slice: packed virtio common cfg + PIT on Linux MMIO (iron deefa7c BAR trap ok=1, MMIO 0x14 x8, Freeing initrd then silent); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirtieth slice: virtio BAR trap over scratch + PIIX3 ISA BAR RAZ (iron df0c118 Freeing initrd then silent; Linux BAR 0x80000000 on UC scratch); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-ninth slice: linux MMIO decode retry + EAX skip 3 + IOAPIC decode fail nowait + linux MOV DR skip; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-eighth slice: iso=0 virtio decode fail still stops (nested 1a4b687 /init SIGSEGV); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-seventh slice: virtio MMIO eax fallback + linux NMI inject (iron 1a2544d Freeing initrd then xcr0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-sixth slice: linux unhandled nowait stop (iron 1a2544d past PAT/initrd then restore host xcr0); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-fifth slice: cpu_flush leftover per walk (iron abfb008 skip n=944 then tick n=256 hung); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-fourth slice: cpu_flush skip leftover pre-map (iron f0eb84e tick n=256 ram=1008 hung); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-30 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-third slice: report-RAM EPT pre-map (iron 113a08a complete PAT then no more COM2); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-second slice: linux earlycon pace LSR THRE (iron 029ac8f/3dc7d11 hush-on-bootimg still cut mid-e820; guest LSR always THRE); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twenty-first slice: guest UART TX drain COM2 independent (iron b983ef8 COM2 froze after e820; drain waited on COM1 THRE); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twentieth slice: linux earlycon share first bootimg (iron b983ef8 256MiB Loaded initrd then readable Linux version 6.12.13-0-virt + e820 0x7eb3efff; identity RIP not bit 63); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-nineteenth slice: 256MiB disk leftover report-RAM (iron 9a3cbfa extra=846); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighteenth slice: ISO-INSTALL-OK on GPT not 16KiB + setup-disk before apk update; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventeenth slice: linux earlycon share first high-half (iron 202312f e820 before CPUID); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixteenth slice: poll ISO-INSTALL-OK every resume + skip linux exc deliver; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifteenth slice: linux earlycon skip #PF dump (iron 9a3cbfa); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fourteenth slice: linux earlycon share first CPUID (iron 9a3cbfa n=1 before #PF); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-thirteenth slice: linux earlycon share product ISO + cpu_flush on tick cadence even when share (nested e0019a3/4f875d6 /init SIGSEGV); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-twelfth slice: linux earlycon hush HV (iron 9a3cbfa linux cpuid shredded Linux version); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eleventh slice: linux earlycon quiet ticks + drain CHUNK (iron 202312f tick interleaved e820; 9a3cbfa still printed ticks into shared FIFO); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-tenth slice: linux earlycon share TX ring (iron 202312f readable Linux version then e820 cut); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-ninth slice: guest UART TX ring drain 4/exit (nested QEMU be0f1cd /init SIGSEGV 3/3); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-eighth slice: guest UART TX ring drain so nowait does not drop Linux printk (iron f423d03 PAT shredded); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-seventh slice: Linux CPUID GenuineIntel + NX and EFER NXE after high-half (iron 45aec97 GenuineIntEl / NX missing then PAT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-sixth slice: guest UART nowait (do not clear COM2_LIVE on THR timeout; iron 115e5ee PAT freeze); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fifth slice: Linux printk ticks every 4096 after #PF deliver (iron 115e5ee every-256 UART split PAT); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-fourth slice: HPET TSC-delta on UART COM I/O cap 4us (not 1ms/byte); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundred-third slice: HPET 1ms on UART COM I/O so Linux earlycon udelay advances; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN hundredth slice: alpine-virt native_cpuid push %rbx RSP slot (base in EBX); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-ninth slice: Linux hypervisor_cpuid_base callee-saved GPR bump to 0x4000FF00; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-eighth slice: ISO9660 grub.cfg Data Length 143→208 so NUL-pad grow is visible to GRUB; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-seventh slice: HPET 1ms on non-HPET EPT so leftover-DRAM storms do not freeze the counter; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-sixth slice: HPET 1ms on CPUID/MSR so identify_cpu does not freeze the counter; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-fifth slice: alpine-virt GRUB tsc=reliable clocksource=tsc idle=poll via NUL-pad grow; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-fourth slice: alpine-virt GRUB lpj=4194304 no_timer_check via NUL-pad grow; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-third slice: Linux CPUID hides hypervisor bit + 0x4000 scan after iron n=128/256 0x4000 walk; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-29 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-second slice: delay_loop skip-10 to ret + RAX=1 after nested f1afc27 preempt noskip; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninety-first slice: always skip Linux CPUID by 2; terminate leaf-4 cache probes after a8b3547 native_cpuid livelock; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninetieth slice: nested product-ISO leftover DRAM + QEMU_MEM=2560M; iso=0 stays 512M; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-ninth slice: force high-half CPUID skip if GUEST_RIP stuck after leftover+#PF err=0x0; heartbeat leaf logs; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-seventh slice: INVLPG 0F 01 /7 skip-decode after Linux #PF; empty fetch does not guess; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-sixth slice: hide CLFLUSHOPT/CLWB in guest CPUID after nested G0 clwb #UD; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-fifth slice: high-half HLT skip-1 after CPUID/RDTSC; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-fourth slice: high-half RDTSC/INVD/WBINVD/PAUSE skip-2 after iron d0735bd CPUID; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-third slice: high-half insn CR3 walk + CPUID/MSR skip-2 after iron d0735bd #PF linux deliver then CPUID; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-second slice: Linux exception bitmap on high-half #PF (no #UD/#GP intercept; M3.10); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighty-first slice: VM-entry inject Linux high-half #PF so PIC/LAPIC cannot steal CR2; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eightieth slice: deliver Linux high-half #PF after extra DRAM pool=1008 Loaded initrd; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-ninth slice: prefer leftover DRAM just above PRECISE + extra no-zero; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-eighth slice: extra-DRAM skip logs after iron 4a62e06 pool=162 Loaded initrd; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-seventh slice: leftover DRAM above PRECISE backs 2GiB CMOS lie after iron Loaded initrd; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-sixth slice: lazy-map report-RAM on string/PUSH/virtqueue + denser post-CD ticks after iron Loaded initrd; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-fifth slice: lazy-map report-RAM on string INS (EFI stub gzip); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-fourth slice: ATAPI multi-DRQ so READ(10) >31 sectors is not short; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-third slice: alpine-virt grub.cfg NUL-prefix timeout + flashcruzer detached HEAD; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-second slice: ISO patches ASCII-only after iron EFI stub uncompression error; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventy-first slice: skip 256MiB disk when leftover starves OVMF report-RAM; 64MiB GPT; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventieth slice: port 0x61 TMR2_OUT + arm ISO before disk attach (iron 1MiB); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-ninth slice: MMIO peek RIP when CS.base+RIP misses flash window; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-eighth slice: xAPIC EAX fallback when skip-len is 1-15 even if peek got bytes; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-seventh slice: Alpine BOOT_SIZE=48 so ESP fits 256MiB/64MiB virtio-blk; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-sixth slice: MMIO skip-len from fetched bytes when VMCS insn_len is 0; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-fifth slice: iron product-ISO pool 512MiB (256MiB disk) + disk before scratch + insn linear fetch; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-fourth slice: xAPIC fetch-miss EAX fallback when insn_len valid; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN: CI green on 2f662c9 (disk reserve + flash-RIP fetch); flash this EFI; stick still e3f56aa; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-third slice: reserve virtio-blk before greedy report-RAM so Alpine gets ≥64MiB; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-second slice: copy OVMF flash HPA for MMIO insn fetch (iron xAPIC SVR insn= empty); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixty-first slice: SSE MOVUPS/MOVDQU MMIO + XMM trampoline save; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixtieth slice: iron fsck proved 64MiB FAT healthy; skip remount/fsck when FAT size < need; mkfs.vfat -I; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-ninth slice: --refat-cruzer mkfs.vfat 64MiB FAT on 977.5MiB Cruzer so alpine-virt fits; keep installdisk.bin; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-eighth slice: remount + fsck.vfat -a reclaim stale FAT32 FSInfo after ENOSPC (not format); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-seventh slice: prune Cruzer ESP leftover/partial ISOs then df-check before alpine-virt linux.iso; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-sixth slice: wait for /dev/vda then mdev so setup-disk opens a node; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-fifth slice: modprobe isofs + mount -t iso9660 so apk sees virtio-iso; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-fourth slice: modprobe sr_mod so ATAPI sr0 fallback exists; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-third slice: mount /dev/sr0 if virtio-iso vdb fails; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-second slice: MMIO CMPS/SCAS (A6/A7/AE/AF, F3 REPE / F2 REPNE); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifty-first slice: sleep 1 after mdev so virtio probe wins find_disks; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fiftieth slice: virtio_pci + mdev -s before setup-disk so find_disks sees vda; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-ninth slice: apk repos overwrite (not append) + How would you like → sys (not Which disk like to use); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-eighth slice: apk local repo + like to use sys + MMIO CALL/JMP r/m (FF /2 /4); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-seventh slice: setup-disk -s 0 + Which disk + No disks available answers n; lazy report-RAM virtqueue GPA; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-sixth slice: MMIO MOVS/STOS/LODS (A4/A5/AA/AB/AC/AD, F3 REP); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-fifth slice: MMIO PUSH/POP r/m (FF /6, 8F /0); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-fourth slice: MMIO TZCNT/LZCNT/POPCNT (F3 0F BC/BD/B8); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-third slice: MMIO CMPXCHG8B (0F C7 /1) EDX:EAX vs 8-byte BAR; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-second slice: MMIO SHLD/SHRD (0F A4/A5/AC/AD) into the BAR; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN forty-first slice: armed product ISO uses 16_777_216 resume cap on nested QEMU too (lab stub nested stays 65536); not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fortieth slice: MMIO F6/F7 DIV/IDIV into AX or DX:AX (#DE on 0/overflow, RIP not skipped) + MOVNTI 0F C3; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-ninth slice: MMIO F6/F7 MUL/IMUL into AX or DX:AX; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-eighth slice: MMIO IMUL (0F AF dest-reg, 69/6B imm); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-seventh slice: auto-answer mkdir -p /media/cdrom before mount (nlplug may skip virtio-iso); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-sixth slice: MMIO PREFETCH/NOP/CLFLUSH skip (no BAR access) + BSF/BSR (0F BC/BD); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-fifth slice: MMIO CMOVcc/SETcc (0F 40-4F / 0F 90-9F); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-fourth slice: MMIO group-2 SHL/SHR/SAR/ROL/ROR/RCL/RCR (C0/C1/D0/D1/D2/D3); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-third slice: MMIO ADC/SBB (10/11/12/13, 18/19/1A/1B, group-1 /2 /3) with RFLAGS.CF; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-second slice: guest-UEFI CR8-load/store exiting syncs Linux TPR to lapic_virt (E4 VMCS does not request CR8); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirty-first slice: MMIO CMPXCHG/XADD so lock cmpxchg on virtio/IOAPIC does not spin; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirtieth slice: MMIO BT/BTS/BTR/BTC (0F BA /4-7 + 0F A3/AB/B3/BB) so lock bts on virtio/IOAPIC does not spin; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-ninth slice: IOAPIC vectors latch LAPIC IRR (M3.12 EOI) + remote IRR/level retry; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-eighth slice: MMIO dest-reg ALU (02/03 ADD r,r/m … 32/33 XOR) + ALU RFLAGS + INC/DEC/NOT/NEG; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-seventh slice: auto-answer mounts /dev/vdb + MMIO CMP 3A/3B (reg-mem) + SUB; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-sixth slice: MMIO TEST/CMP RFLAGS + auto-answer CONFIRM after yes still answers bootloader?; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-fifth slice: product ISO xAPIC 4K trap + lapic_virt CUR_COUNT/EOI + ISO patch virtio_blk in modules= (drop nolapic); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-fourth slice: PIT unlatched lo/hi after 0x34 + MMIO AH/CH/DH/BH (no REX); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-third slice: MMIO group-1 AND/OR/XOR (virtio RMW); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-second slice: MMIO XCHG/MOVSX/moffs + GRUB insmod all_video serial; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twenty-first slice: auto-answer [y/N] erase prompt (alpine-conf confirm_erase); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twentieth slice: Alpine USE_EFI=1 + GRUB set timeout=1 / efi_gop serial; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN nineteenth slice: i8253 channel 0 16-bit lo/hi + latch (Linux nolapic inb 0x40); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighteenth slice: ISO patch keeps squashfs in modules= + MMIO insn fetch across 4KiB pages + MOVZX/r32 zero-extend; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventeenth slice: Alpine auto-answer BOOTLOADER=grub + bootloader? prompt; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixteenth slice: ISO patch nolapic so Linux uses PIT not static xAPIC; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifteenth slice: PIT IRQ 0 on HLT/preempt (UART/virtio beat timer); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fourteenth slice: ISO patch console=ttyS0 noapic + alpine_dev=vdb (virtio-iso media, PIC IRQ 11); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN thirteenth slice: virtio INTx raises IOAPIC pin 11 (PCI interrupt line) so Linux without _PRT can complete I/O; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN twelfth slice: virtio GPA copies stop at 4KiB so report-RAM 2MiB slots do not bleed; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eleventh slice: read-only virtio-blk ISO at 00:03.0 (/dev/vdb) for alpine-virt media; packed num_queues; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN tenth slice: virtio-blk OUT walks every data descriptor in the chain; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN ninth slice: 31-sector ATAPI PIO DRQ, IDENTIFY PIO-only, nIEN masks IRQ 14; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN eighth slice: ata_piix+sr-mod ISO patch, GRUB gfxterm→serial, BusyBox / # auto-answer, 64-bit virtqueue GPA writes; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN seventh slice: chunked virtio-blk OUT, sr-mod+ttyS0 ISO patch, virtio PCI INTA line 11; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN sixth slice: Alpine serial auto-answer login+setup-disk /dev/vda; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fifth slice: host SOL/COM RX into guest COM1 RBR + 16550 loopback; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN fourth slice: product ISO 16550 + ttyS0 cmdline (Alpine modules= kept valid); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN third slice: product ISO PIC+IOAPIC inject (ATA GSI 14, virtio GSI 17); lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN second slice: ESP linux.iso retain + virtio-pci queues gated on product window + hold (not E4) when armed; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | e5-stage46-iso | 0.5 | 95 | Stage 46 OPEN first slice: product ISO window + continue past lab El Torito; lab 72KiB stub still E4; not ISO-INSTALL-OK; iron P0-14 stays 2b795a0 |
| 2026-08-28 | p0-60-g1-ept | 0.5 | 95 | M4.3 host-slab CLOSED iron after 22e28d0 M4-BLK-OK 0x10c00000 NET-OK SMP-OK BOOT-OK; ISO-BOOTED-FROM-DISK is persist-detect not installer; Stage 46 next; iron P0-14 stays 2b795a0 |
| 2026-08-28 | p0-60-g1-ept | 0.5 | 95 | G0 VMCS relocate CLOSED iron after b7259c1/10e7984 M4-NVM-OK SLICE-G0 HPA=0x10a00000; M4.3 blk triple-fault 0x02 at 0xfc0f000 residual; Stage 45+P0-60 held; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-28 | p0-60-g1-ept | 0.5 | 95 | P0-60 CLOSED iron after 5147222 M4-SHELL-G1 M4-2VM-OK no GPA=0x10403000; G0 VMCS sched error 11 residual; Stage 45 held; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN: 2048-byte FAT BPB + ISO9660 EFI/BOOT/BOOTX64 after iron df7d158 512-byte BPB catalog=1 bootimg=1 elt=0 (not ELTORITO-OK; not installer); iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN: 262144-exit cap after iron df7d158 catalog=1 bootimg=1 elt=0 stop n=131072 (131072-exit cap; BDS ATA PIO; not ELTORITO-OK; not installer); iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN: do not apply 32768 post-ATAPI tail after PACKET (first sector often LBA 0 dummy); EDK2 FatDxe+LoadImage host walk; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN: GenFw PE 0x2022 + COM1 LCR DLAB clear before RN-ELT; no short tail after catalog+load; 131072-exit cap; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN: no short tail after catalog+load READ; 131072-exit cap; BAR ATA data-port rep insw fills RAM; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN PE .reloc + ISO terminator; FAT12 ESP BOOTX64 + catalog checksum; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-eltorito | 0.5 | 95 | Stage 45 OPEN host package: keep VMCS after first ATAPI sector; PE32+ CD EFI; catalog+load READ + RN-ELT; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-atapi | 0.5 | 95 | After Stage 44 named: Stage 45 El Torito then P0-60 G1 EPT (not an E5 stage) then Stage 46 ISO-INSTALL-OK; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-27 | e5-ovmf-atapi | 0.5 | 95 | P0-59 CLOSED iron COM2 bf696ca ATAPI-OK sectors=1 packet=9 scsi=0x28 stop n=30769 pci_ide=1 virtio=1; BOTH-OK n=12411 virtio 00:02.0 + IDE 00:00.1; no AcpiTimerLib ASSERT; not El Torito; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 10cb881 VCNT=8 power-on still ASSERT callerrip=0x1d25193 mtrrdef=0xc06 mtrr0=0x80000000; VCNT=32 power-on no hole; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron aee545f DXE assert skip then #UD 0x109d stop n=5364; revert skip; MTRR power-on E=0 VCNT=8 no UC hole; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron c40f4a8 pcdsig=1 after 32-pair MTRR still ASSERT callerrip=0x1d25193; guarded DXE ebec skip when RIP/caller in [1MiB,32MiB); not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron b4b4847 EFER.LMA efer=0xd00 pg=1 csl=1 still ASSERT callerrip=0x1d25193; r8 is gPcdDataBaseSignatureGuid; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 0b7d647 VCNT=32 still ASSERT callerrip=0x1d25193 lastmsr=EFER; QEMU BOTH skipped ebf3; EFER.LMA=LME&&CR0.PG + IA-32e entry + debugcon 0x402; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 8700cbb hypervisor CPUID still ASSERT callerrip=0x1d25193; MTRR VCNT=32 + PCI UC hole + bootorder NUL; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 408788c MTRR walk done still ASSERT after CPUID 0x1cf11b5; guest-UEFI hypervisor CPUID + KVMKVMKVM; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 3f417ca xAPIC 4K mapped still ASSERT after MTRR 0xFE/0x2FF/0x250; guest MTRR shadow; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron 891eb5b skipped ebecc9c3 leave;ret then #UD 0x109D; do not skip ASSERT epilogue; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron e2af81e insn=ebec jmp -20; fw_cfg CD master drive@0 (not slave); not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron d5f9431 n=8192 rip=0x6e81ca; e2af81e missed GCC eb fc / 0F 84; preempt eb/jcc32 skip; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron d5f9431 n=8192 reason=0x34 rip=0x6e81ca pause CpuDeadLoop; preempt pause/jcc skip; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron d5f9431 #UD gone; DXE then tick n=1280 reason=0x34 rip=0x6e81ca (no stop n=); not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN fw_cfg etc/boot-menu-wait 0ms skip BdsWait; guest-UEFI XSETBV executes XCR0; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron COM2 #UD RIP 0x109D pci_ide=0 com=15515; guest-UEFI INVPCID/RDTSCP/XSAVES; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN nested VT-x 2674629 n=32768 ataio=0 acpi=16612 port=0; ACPI PM 1s step; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN nested VT-x 5d9e346 n=8192 ataio=0 port=0xcf8; HPET 1s on preemption only; 8042; 32768 cap; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN iron COM2 skipped guest-UEFI (Cruzer lacked EFI/RayNu/OVMF.fd); flash stages host OVMF; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-atapi | 0.5 | 95 | P0-59 OPEN PIIX3 ISA PIRQ after nested VT-x 8e55abf cf8=0x80000838 ISA 00:01.0:0x38; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 CLOSED nested VT-x 1b07692 OVMF-BOTH-OK pci select 00:00.01 val=0x70108086 pci_ide=1 virtio=1 sectors=0 spin=1; E4 #DF fail-soft; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN 3dbafb7 spin-jmp skip SKIP-only on push+PR; nested VT-x 707a849 insn=ebf3 still required; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN nested VT-x 707a849 n=2048 rip=0x6e812d insn=ebf3 pci_ide=0 (CpuDeadLoop); spin-jmp skip; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN stop RIP insn dump after fd88785 SKIP-only; nested VT-x 105ffbe n=2048 rip=0x6e812d pci_ide=0; 1s HPET step; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN nested VT-x 105ffbe n=2048 reason=0x34 rip=0x6e812d pci_ide=0 (10ms HPET); 1s step; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN nested VT-x 20763e4 VARS mapped alias_gpa=0xffc00000 then 300s kill no 00:00.1; live HPET; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN empty VARS _FVH in 4MiB flash pad after 1991a27 EPT 0xffc00000; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN nested VT-x 1991a27 dxe=1 acpi=13 then EPT gpa=0xffc00000 VARS gap; 4MiB flash window; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK cmp bx i440FX DID remap (not LZMA 37 12) + RAM remap after decompress; virtio 00:00.0 + IDE 00:00.1; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK PIIX4 PM 00:01.3 + guest-private i440FX DID remap; virtio 00:00.0 + IDE 00:00.1; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK ACPI PM timer after 699c9a6 n=2048 pci_ide=0; virtio 00:00.0 + IDE 00:00.1; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK virtio 00:00.0 + PIIX IDE 00:01.1; HLT skip so DXE can walk PCI; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK virtio 00:00.0 + PIIX IDE 00:01.1 (ISA multifunction walk); virtio-alone no longer stops DXE; not installer; iron P0-14 stays 2b795a0 |
| 2026-08-23 | e5-ovmf-both | 0.5 | 95 | P0-58 OPEN host BOTH-OK simultaneous virtio 00:00.0 + IDE 00:00.1; virtio-alone no longer stops DXE; not installer; iron P0-14 stays 2b795a0 |
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
