---
hda_version: 1
last_updated: 2026-08-16
last_commit: 56db7d0f78e7f5cae478f07e87c44f5680b3ceac
last_commit_short: 56db7d0
updated_by: cursor
mount_everest_target: "Ship EFI on real R640 + network vSphere-like UI + deploy Linux ISO (M7 Mount Everest)"
months_to_everest: 0.75
months_to_everest_prev: 1.25
velocity_commits_30d: 340
velocity_gates_30d: 17
overall_pct: 78
confidence: medium
baseline_date: 2026-07-20
baseline_months: 4.5
everest_eta_month: "2026-09"
summit_core_pct: 88
summit_efi_pct: 95
summit_r640_pct: 98
summit_ui_pct: 78
summit_iso_pct: 38
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
| **Overall product readiness** | **78%** | +6 (E3 / `RAYNU-V-M7-UEFI-HTTP-OK` closed on iron) |
| **Months to Mount Everest** | **0.75** | −0.5 (M7.6 iron; residual = E4 polish + ISO) |
| **ETA month** | **2026-09** | held |
| **Confidence** | medium | E2+E3 COM2 green; TLS/post-EBS/ISO still open |
| **Hypervisor core (VMX/EPT/Linux/multi-VM)** | ~88% | proved on real R640 through M4 |
| **Ship EFI artifact** | ~95% | M7.0 + iron kits under `releases/` |
| **Real R640 boot** | ~98% | E2 closed; Redfish/soak follow-ons only |
| **vSphere-like UI (network)** | ~78% | M7.6 iron HTTP OK; TLS/console/post-EBS residual |
| **Deploy Linux ISO** | ~38% | M7.3 host extract-boot smoke; El Torito/CD-ROM stub |
| **Production bar (M6.8–M6.9)** | **100%** | soak + EXT closed on Latitude |

```
Months to Everest  █░░░░░░░░░░░░░░░░░░░  0.75 mo  (was 1.25)
Overall %          ███████████████░░░░░  78%
```

**How the month number moves:** faster closed Everest-path work → `months_to_everest` shrinks and `everest_eta_month` pulls closer. Stalls / new scope → it slips. See [Velocity model](#velocity-model).

---

## Mount Everest — definition of done

All must be true (no hand-waving):

| # | Criterion | Done when | Pillar |
|---|-----------|-----------|--------|
| E1 | **Ship EFI** | Versioned `r640-hypervisor.efi` + checksums; `tools/check-size.sh` green; USB/iDRAC media runbook | [Z] |
| E2 | **R640 boot** | Marker `RAYNU-V-R640-BOOT-OK` (or equiv.) on **real PowerEdge R640**; serial via iDRAC; VMX+EPT+Linux shell | [D][Z] |
| E3 | **Network UI** | Browser/curl on operator LAN reaches SPA/REST (HTTP MVP; TLS deferred); not host-only | [Z][A] |
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
**Status: NEAR · ~78% · ~0.5–1.0 months residual (TLS / console / E4 polish)**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| Embedded SPA list/start/stop | DONE | `assets/webui.html`, M5.2 |
| In-process REST shapes + auth token | DONE | `mgmt/api.rs` M5.1/M6.4 |
| HTTP/1.1 codec + Bearer wire | DONE | `mgmt/http.rs` (M7.1 Latitude) |
| Host TCP proof (loopback) | DONE | `mgmt/http_listen.rs` (M7.1 Latitude) |
| Create-VM fields (CPU/RAM/disk/ISO) | DONE (host) | M7.4 SPA + `POST /vms/{id}/spec/...` Latitude smoke |
| Datastore / ISO media buttons | DONE (host) | SPA → `/images`, `/iso/{id}/deploy` |
| **UEFI NIC HTTP listen** | DONE (M7.6 iron) | `RAYNU-V-M7-UEFI-HTTP-OK` R640 SNP residual; [2026-08-16-uefi-http-ok.md](evidence/r640/2026-08-16-uefi-http-ok.md) |
| TLS | DEFERRED | plaintext lab HTTP (ADR-009) |
| Guest console / serial log UI | MISSING | residual |
| Networking/storage ops UI | MISSING | probes only |
| Audit/tasks pane | PARTIAL | ring exists; UI thin |

### Summit D — Deploy Linux ISO
**Status: MEDIUM · ~38% · ~0.75–1.5 months residual**

| Item | Status | Evidence / gap |
|------|--------|----------------|
| bzImage + initrd boot | DONE | real tiny Linux → shell |
| Image library (register/list/delete) | DONE | `mgmt/datastore.rs` (M7.2 Latitude) |
| Host ESP-shaped catalog | DONE | `EFI/RAYNU/images/catalog.txt` (host `std::fs`) |
| UEFI catalog persist | STUB | `UnsupportedOnFirmware` until SFS/NVMe write |
| ISO register + extract-boot bind | DONE (host) | `mgmt/iso.rs` Latitude package smoke (~0s) |
| Virtio-blk install target surface | DONE (plan) | capacity + empty disk size in deploy plan |
| ISO parse / El Torito / EFI boot img | MISSING | residual |
| CD-ROM attach | STUB | `attach_cdrom_uefi` → UnsupportedOnFirmware |
| Persistent install disk workflow | PARTIAL | size planned; reboot-to-disk → later |
| Upload ISO via API/UI | PARTIAL | REST `/iso/{id}/deploy`; blob upload residual |
| Multi-distro matrix | MISSING | — |

---

## Rolling month timeline (Mount Everest)

Months are **calendar months from `baseline_date`**, adjusted by velocity.  
When work finishes early, **pull rows upward** (shrink residual). When blocked, **push ETA**.  

| Month | Calendar | Planned focus | Exit criteria | Status |
|-------|----------|---------------|---------------|--------|
| M+0 | 2026-07 | **M7.0–M7.4 closed** (lab host); **M7.5 R640 next** | M7.4 Latitude host smoke | **DONE (M7.4 host)** |
| M+1 | 2026-08 | **R640 iron bring-up** → **E2 closed** | `RAYNU-V-R640-BOOT-OK` on COM2 | **DONE (M7.5 iron)** |
| M+2 | 2026-09 | HTTPS/UEFI NIC + ISO install path | E3–E5 residuals | **ETA / IN PROGRESS** |
| M+3 | 2026-10 | Create-VM polish + install-to-disk MVP | M7 product loop | PLANNED |
| M+4 | 2026-11 | Buffer / M7 closed on all E1–E6 | **M7 Mount Everest** | BUFFER |
| M+5 | 2026-12 | Buffer / M8 sketch start | — | BUFFER |

### Timeline burn-down

```
2026-07 ████████  HDA + M6 closed (Latitude)
2026-08 ████████  R640 boot (E2 CLOSED)
2026-09 ████░░░░  Network UI + ISO residual   ← months_to_everest ≈ 1.25
2026-10 ████░░░░  Everest polish / E3–E5
2026-11 ░░░░░░░░  buffer
2026-12 ░░░░░░░░  buffer
```

**Pull-forward rule:** E2 closed 2026-08-15; shrink months when UEFI NIC/TLS or ISO install-to-disk land. Document why in [Changelog](#hda-changelog).

---

## Everest workstream backlog (P0)

Ordered for critical path (parallelize B with D design):

| ID | Workstream | Summit | Est. residual (mo) | Depends on | Repo touchpoints |
|----|------------|--------|-------------------|------------|------------------|
| P0-1 | **M7.0** Release kit: tag, SHA256, size gate, USB/iDRAC runbook | A | **DONE** | — | `tools/package-release.sh`, runbook |
| P0-2 | **M7.5** R640 boot gate (real iron) | B | **DONE** | P0-1 helpful | `RAYNU-V-R640-BOOT-OK` 2026-08-15; evidence closed |
| P0-3 | Live Tier-1 Redfish (read-only health) | B | 0.5 | P0-2 | `idrac/` — after first boot |
| P0-4 | **M7.1** Minimal HTTP server (serve SPA + REST) | C | 0.25 | size budget | **DONE host path**; UEFI listen + TLS residual |
| P0-5 | **M7.2** Datastore on ESP/NVMe (images + ISOs) | C+D | 0.25 | P0-4 | **DONE host path**; UEFI persist residual |
| P0-6 | **M7.3** ISO register + CD-ROM or kernel-extract boot | D | 0.5 | P0-5 | `mgmt/iso` wired; El Torito/CD-ROM residual |
| P0-6 | **M7.3** ISO register + CD-ROM or kernel-extract boot | D | 0.5 | P0-5 | **DONE host extract-boot smoke**; El Torito/CD-ROM residual |
| P0-7 | **M7.4** Create-VM API/UI (CPU/RAM/disk/ISO) | C+D | 0.25 | P0-5, P0-6 | **DONE host SPA smoke**; console/TLS/NIC residual |
| P0-8 | Install-to-disk + reboot-to-disk path | D | 0.5 | P0-6, P0-7 | `guest/`, `devices/virtio_blk` |
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
| Commit | Close M7.5 / E2 after xsavesfix iron proof |
| Summary | Real R640 COM2: SHELL-OK + full M4; `STATUS=closed`; `GAP(CLOSED M7.5)` |
| Everest impact | months 1.75→1.25; overall 60→72; r640 68→98; ETA→2026-09 |
| Gates touched | `RAYNU-V-R640-BOOT-OK` claimed; scaffold smoke still host-only |
| Months Δ | 1.75 → 1.25 |

---

## Blockers & risks (Everest-relevant)

| ID | Blocker / risk | Severity | Mitigations |
|----|----------------|----------|-------------|
| H1 | ~~R640 VMLAUNCH/guest path~~ | — | **Resolved** 2026-08-15 (`RAYNU-V-R640-BOOT-OK`) |
| H2 | No in-HV HTTP/TLS stack | HIGH | Size-boxed stack or documented split helper (prefer in-binary for [Z]) |
| H3 | No full El Torito/CD-ROM | MED | M7.3 extract-boot MVP; CD-ROM stub residual |
| H4 | Console / TLS / post-EBS listen still open | MED | E3 MVP closed (PRE-EBS HTTP); TLS + persistent listen follow-on |
| H5 | Latitude ≠ full product loop | MED | E2+E3 closed on iron; still need ISO install-to-disk (E5) |
| H6 | Single-dev velocity (R10) | MED | Everest P0 only; defer Tier-2 / full parity |
| H7 | Binary size if HTTP+ISO+UI grow | MED | ADR-003 checks; lazy assets; zstd webui GAP |

---

## HDA changelog

| Date | Commit | Months | Overall % | Note |
|------|--------|-------:|----------:|------|
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
Now:           E2+E3 CLOSED — BOOT-OK + UEFI-HTTP-OK (2026-08-16 COM2)
Months left:   0.75  (ETA ~ 2026-09)
Next move:     ISO install-to-disk (E5) + vSphere MVP polish (E4); TLS/post-EBS optional
Do not claim:  Mount Everest until E4–E5 green
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
