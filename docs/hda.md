---
hda_version: 1
last_updated: 2026-08-19
last_commit: f0961f1fb8ef2fd863c79d2aa8e94e021a6c196d
last_commit_short: f0961f1
updated_by: cursor
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO (M7 Mount Everest)"
months_to_everest: 1.5
months_to_everest_prev: 0.75
velocity_commits_30d: 345
velocity_gates_30d: 19
overall_pct: 88
confidence: medium
baseline_date: 2026-07-20
baseline_months: 4.5
everest_eta_month: "2026-10"
summit_core_pct: 88
summit_efi_pct: 95
summit_r640_pct: 98
summit_ui_pct: 85
summit_iso_pct: 82
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
| **Overall product readiness** | **88%** | held (E2+E3+E5 stamps; E3b named, not closed) |
| **Months to Mount Everest** | **1.5** | +0.75 (ADR-013 native NIC; SNP after EBS rejected) |
| **ETA month** | **2026-10** | slipped from 2026-09 |
| **Confidence** | medium | E2+E3+E5 on COM2; skip-BMCR EFI still `cand bmsr=7949`; CORECLK_RESET without BMCR next; HTTP-OK open |
| **Hypervisor core (VMX/EPT/Linux/multi-VM)** | ~88% | proved on real R640 through M4 |
| **Ship EFI artifact** | ~95% | M7.0 + iron kits under `releases/` |
| **Real R640 boot** | ~98% | E2 closed; Redfish/soak follow-ons only |
| **vSphere-like UI (network)** | ~85% | E3 PRE-EBS closed; E3b durable HTTP missing (firmware SNP dead) |
| **Deploy Linux ISO** | ~82% | iron two-boot LBA persist closed; guest FS / distro installer later |
| **Production bar (M6.8–M6.9)** | **100%** | soak + EXT closed on Latitude |

```
Months to Everest  ██░░░░░░░░░░░░░░░░░░  1.5 mo  (was 0.75)
Overall %          █████████████████░░░  88%
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
| E5 | **Linux ISO deploy** | Operator registers a distro ISO → VM boots installer (or documented extract path) → installs to virtio-blk → reboot to disk | [Z] |
| E6 | **Production bar** | M6.8 soak + M6.9 external audit/spec review closed per `progress.md` | [V][A] |

**Out of Everest / M7 scope (→ M8 or later):** vMotion-like live migrate, DRS-like placement, hot-add, full vSphere parity, Dell Tier-2 PERC OEM, multi-site DR, Windows guest WHQL.

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
**Status: NEAR · ~85% · ~1.5 months residual (E3b host-owned NIC per ADR-013, then TLS/console)**

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
| Guest console / serial log UI | PARTIAL | Host UART ring via `GET /logs/serial` + SPA; guest VNC residual — **after** host-owned post-EBS listen |
| Auth beyond bring-up toy | PARTIAL | ESP `auth.token` overrides bring-up; iron used lab bring-up |
| Networking/storage ops UI | MISSING | probes only |
| Audit/tasks pane | PARTIAL | ring exists; UI thin |
| E4 SPA create on iron | DONE | Firefox create-VM + Bearer; [2026-08-16-e4-spa-install-arm.md](evidence/r640/2026-08-16-e4-spa-install-arm.md) |
| **Post-EBS durable HTTP (E3b)** | MISSING | Skip-`BMCR_RESET` EFI: `phy_reset=pre skip (ape-ncsi)` still `cand bmsr=7949` at EBS. CORECLK_RESET without BMCR. HTTP-OK not claimed |

### Summit D — Deploy Linux ISO
**Status: NEAR · ~82% · ~0.25–0.5 months residual (real distro installer; after E3b)**

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
| ISO parse / El Torito / EFI boot img | MISSING | later — not next after E5 stamps |
| CD-ROM attach | STUB | `attach_cdrom_uefi` → UnsupportedOnFirmware |
| Persistent install + reboot-to-disk | **DONE (stamps)** | Iron Cruzer `BOOTED-FROM-DISK` 2026-08-16; guest FS residual |
| Upload ISO via API/UI | PARTIAL | REST `/iso/{id}/deploy` + `/install`; blob upload residual |
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
| M+3 | 2026-10 | E3b iron HTTP + E4 polish | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | **ETA** |
| M+4 | 2026-11 | Buffer / M7 closed on all E1–E6 | **M7 Mount Everest** | BUFFER |
| M+5 | 2026-12 | Buffer / M8 sketch start | — | BUFFER |

### Timeline burn-down

```
2026-07 ████████  HDA + M6 closed (Latitude)
2026-08 ████████  R640 boot (E2 CLOSED)
2026-09 ████░░░░  E3b lab NIC (ADR-013)       ← months_to_everest ≈ 1.5
2026-10 ████░░░░  E3b iron HTTP / Everest polish
2026-11 ░░░░░░░░  buffer
2026-12 ░░░░░░░░  buffer
```

**Pull-forward rule:** E2 closed 2026-08-15; E3 bring-up closed 2026-08-16. Shrink months when **E3b** (ADR-013 native HTTP after `BOOT-OK`) lands. Document why in [Changelog](#hda-changelog).

---

## Everest workstream backlog (P0)

Ordered for critical path (parallelize B with D design):

| ID | Workstream | Summit | Est. residual (mo) | Depends on | Repo touchpoints |
|----|------------|--------|-------------------|------------|------------------|
| P0-1 | **M7.0** Release kit: tag, SHA256, size gate, USB/iDRAC runbook | A | **DONE** | — | `tools/package-release.sh`, runbook |
| P0-2 | **M7.5** R640 boot gate (real iron) | B | **DONE** | P0-1 helpful | `RAYNU-V-R640-BOOT-OK` 2026-08-15; evidence closed |
| P0-3 | Live Tier-1 Redfish (read-only health) | B | 0.5 | P0-2 | `idrac/` — after first boot |
| P0-4 | **M7.1** Minimal HTTP server (serve SPA + REST) | C | **DONE** | size budget | Host + iron SNP residual **PRE-EBS** (E3); firmware Tcp4 absent |
| P0-12 | **M7.8 / E3b** Host-owned mgmt NIC (ADR-013) | C | 1.0 | P0-4 | Skip-BMCR still `cand bmsr=7949`; CORECLK_RESET without BMCR; HTTP-OK **open** |
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
| Commit | f0961f1 |
| Summary | Skip-`BMCR_RESET` EFI still `cand bmsr=7949`. Linux `tg3_chip_reset` without BMCR. HTTP-OK not claimed |
| Everest impact | months 1.5 held; ETA 2026-10; overall 88 held; E3b open (chip reset sans BMCR) |
| Gates touched | `skip_coreclk_reset` false; `HOST-NIC-HTTP-OK` not claimed |
| Months Δ | 1.5 held |

---

## Blockers & risks (Everest-relevant)

| ID | Blocker / risk | Severity | Mitigations |
|----|----------------|----------|-------------|
| H1 | ~~R640 VMLAUNCH/guest path~~ | — | **Resolved** 2026-08-15 (`RAYNU-V-R640-BOOT-OK`) |
| H2 | No in-HV HTTP/TLS stack | HIGH | Size-boxed stack or documented split helper (prefer in-binary for [Z]) |
| H3 | No full El Torito/CD-ROM | MED | Deferred until post-EBS listen works; extract-boot MVP holds |
| H4 | Firmware SNP unusable after EBS | HIGH | Skip-BMCR EFI `phy_reset=pre skip` still `cand bmsr=7949` at EBS. CORECLK_RESET without BMCR; E3b = HTTP after `BOOT-OK` |
| H5 | Latitude ≠ full product loop | MED | E2+E3+E5 stamps closed; Everest residual E3b + E4 polish + distro |
| H6 | Single-dev velocity (R10) | MED | Everest P0 only; defer Tier-2 / full parity |
| H7 | Binary size if HTTP+ISO+UI grow | MED | ADR-003 checks; lazy assets; zstd webui GAP |

---

## HDA changelog

| Date | Commit | Months | Overall % | Note |
|------|--------|-------:|----------:|------|
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
Now:           E2+E3+E5 stamps CLOSED; Phase 0 CLOSED; Phase D CORECLK_RESET without BMCR; E3b OPEN
Months left:   1.5  (ETA ~ 2026-10)
Next move:     flash CORECLK_RESET-sans-BMCR EFI; expect COM2 reset… plus phy_reset=pre skip
Tcp4 residual: Floppy publishes PXE/HTTP, not Tcp4 SB (platform limit)
SNP after EBS: dead — WARN-only idle closed on iron 2026-08-17 (no RSOD)
Preserve:      releases/v0.1.0-adr013-baseline
Do not claim:  Mount Everest (E3b + E4 polish + distro installer remain)
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
