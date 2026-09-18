---
loihda_version: 1
last_updated: 2026-09-18
last_commit: PENDING
last_commit_short: PENDING
updated_by: cursor
loi_target: "Non-prod Letter of Intent. Bar A = dedicated-box lab. Bar B = RAID-fleet replacement. HDA 99% is Everest, not an LOI."
months_to_loi_a: 1.5
months_to_loi_a_prev: 1.5
months_to_loi_b: 3.5
months_to_loi_b_prev: 3.5
overall_pct: 38
confidence: medium
baseline_date: 2026-09-14
baseline_months: 1.5
loi_a_eta_month: "2026-11"
loi_b_eta_month: "2026-12"
bar_a_pct: 42
bar_b_pct: 18
piece_everest_pct: 100
piece_persist_pct: 70
piece_sku_pct: 25
piece_tls_pct: 8
piece_auth_pct: 28
piece_console_pct: 22
piece_perc_pct: 15
piece_unmodified_pct: 20
---

# LOIHDA — Honest Distance to a Letter of Intent

> **Living document.** Updated on every LOI-relevant commit (see `.cursor/rules/loihda-update.mdc`).  
> **Sibling of Everest HDA:** [`docs/hda.md`](hda.md) measures the product loop (closed). This file measures **whether a non-production LOI conversation is in scope yet**.  
> Public page: [`site/loi.html`](../site/loi.html) (fed by [`site/loi.json`](../site/loi.json)).  
> **HDA 99% is not an LOI.** Everest closed the loop on one path. An LOI asks a buyer to put intent in writing.

Pillars: **[Z]** single binary · **[D]** Dell-native · **[A]** audit. Formal verification **[V]** is the north star; it is **not** an LOI gate.  
Lived: [`docs/progress.md`](progress.md) · M8: [`docs/m8_plan.md`](m8_plan.md) · ADR-018: [`docs/adr/ADR-018.md`](adr/ADR-018.md).

---

## Scoreboard (read this first)

| Metric | Value | Meaning |
|--------|------:|---------|
| **Overall LOI readiness** | **38%** | Nearest honest conversation is Bar A. Not Bar B. Not GA. |
| **Bar A — dedicated-box** | **42%** | One PowerEdge we own or they dedicate. Disk must survive HV reboot. TLS for InfoSec. |
| **Bar B — RAID-fleet** | **18%** | Replace the licensed hypervisor on PERC virtual disks they already paid for. |
| **Months to Bar A** | **1.5** | Baseline 2026-09-14. ETA **2026-11**. Shrink only with DONE evidence. |
| **Months to Bar B** | **3.5** | PERC I/O is the long pole. ETA **2026-12**. |
| **Confidence** | medium | Everest is high-confidence. LOI is not, until persist prints on COM2. |

```
Bar A (dedicated-box)  ████████░░░░░░░░░░░░  42%
Bar B (RAID fleet)      ███░░░░░░░░░░░░░░░░░  18%
Overall LOI             ███████░░░░░░░░░░░░░  38%
```

**How the month number moves:** same honesty as Everest HDA. Closed iron gates shrink `months_to_loi_*`. Nested QEMU, host tests, and design docs do **not**. Stalls or new scope slip the ETA. Prefer under-claiming.

---

## What an LOI is (and is not)

A **Letter of Intent** here is a fleet owner saying, in writing, that they intend to run RayNu-V in **non-production** on named iron. It is not a purchase order, not GA, not cluster, not Windows WHQL.

There are **two** LOIs. Treating them as one conversation overstates what is ready.

| Bar | Who signs | What they are buying | Close when |
|-----|-----------|----------------------|------------|
| **A — dedicated-box** | A lab / innovation / “spare R640” owner | One binary on a box they can afford to dedicate. Guest disk survives Force Off. HTTPS they can show InfoSec. | M8.0-mech on COM2 + SKU card + TLS path |
| **B — RAID-fleet** | Someone whose disks already live on PERC H740P | “Keep the iron, replace the licensed hypervisor.” USB is not this. | `RAYNU-V-M8-PERC-LUN-OK` on a **spare** VD, not Ubuntu |

**Bar B stays out of commercial conversation until its close criteria are met.** A USB persist demonstration is the Bar A mechanism. It is not evidence that RayNu-V runs on the customer’s RAID virtual disks.

Everest (HDA) answered: *can we install Linux from a browser on a real R640?*  
LOIHDA answers: *is a non-production LOI in scope, and which bar?*

---

## Definition of done

### Bar A — dedicated-box non-prod LOI

All must be true:

| # | Criterion | Done when | Product effect |
|---|-----------|-----------|----------------|
| A1 | **Everest loop** | Iron `ISO-INSTALL-OK` → `DISK-BOOT-OK` → `login:` | Without this there is no product to intent toward. **DONE** (`f72b4276`). |
| A2 | **Persist across HV reboot** | COM2 `RAYNU-V-M8-DISK-PERSIST-OK` after Force Off | Today the guest disk is leftover DRAM. Force Off = gone. A buyer who reboots RayNu-V must not lose the VM. Nested-OK ≠ this. |
| A3 | **SKU card** | One page: what ships, what does not, dedicated-box vs fleet | Stops us promising PERC, Windows, or cluster in a Bar A conversation. |
| A4 | **TLS** | Browser/`curl --cacert` on `:8443` after `BOOT-OK` | InfoSec will not sign plaintext HTTP + bring-up token. Everest deferred this on purpose. |
| A5 | **Auth beyond bring-up** | Product default is not `raynu-v-bringup` | A shared lab latch is not an operator credential. |
| A6 | **Operator keyboard** | Type in the guest from the SPA (not only iDRAC SOL) | Soft for the first LOI; still on the polish table. Serial already installed Alpine. |

A2 is the **NOW** gate. A4 is the usual InfoSec latch. A3 can close in docs the same week as A2. A5/A6 may trail a first dedicated-box LOI if named as residuals.

### Bar B — RAID-fleet non-prod LOI

| # | Criterion | Done when | Product effect |
|---|-----------|-----------|----------------|
| B1 | Bar A honest | Bar A criteria closed or explicitly waived in the SKU card | Do not skip A to sell B. |
| B2 | **PERC virtual disk I/O** | COM2 `RAYNU-V-M8-PERC-LUN-OK` on a VD that is **not** Ubuntu | ~80% of R640 fleets boot guests from RAID. USB persist does not extend those fleets. |
| B3 | Census skip ≠ product policy | Mapper still refuses the **lab** Ubuntu VD; product talks to a **spare** VD | Formatting `raynuvsrv1` Ubuntu is not an LOI strategy. |

---

## The pieces (what they mean)

Each row is a product effect, not a feature checkbox. Percents are **this tracker’s** scores, not HDA.

### 1. Everest loop — 100%

**What it is.** Ship EFI → real R640 → network SPA → Linux ISO → reboot to disk → `login:`. Closed 2026-09-11.

**Product effect.** There is a thing a buyer can watch. Without it, an LOI is a research pitch.

**Not this.** Disks that survive a **hypervisor** reboot. HTTPS. RAID. Unmodified Ubuntu.

### 2. Disk persist (M8.0-mech) — 70%

**What it is.** The virtio disk `setup-disk` wrote is leftover DRAM above PRECISE. Guest F7 keeps it (ADR-017). A RayNu-V reboot **zeros** it.

**Product effect.** “Install Linux” without persist is a demo that dies when the operator Force Offs the box — which they will, because that is how you recover a Type-1. Nested File persist (`ce3d8a09`) and DurableLun USB/NVMe I/O are the mechanism. Iron Force Off is still open. USB ≥ 16 GiB (not the ESP Cruzer / 2–8 GiB UDisk) is the Toshiba LUN. Nested QEMU ≠ R640.

**Iron NOW (not a score bump).** Writequeue COM2 (`f32b9238`): Toshiba `0480:a004` named; `xhci cswqueue p11`; `usb I/O ready bytes=320072933376`; leftover skip; peek `usb_err=0`; Alpine `[vda] 298 GiB` `vda1` login; first `wr=1` then `RAYNU-V-M7-ISO-INSTALL-OK` on the 298 GiB USB LUN; `usb rw ok wr=1` flood mashed COM2. **Not persist.** USB `ISO-INSTALL-OK` ≠ leftover DRAM Everest ≠ Force-Off persist. This EFI: quiet `usb rw ok` (`xhci_rw_ok_should_print`). Keep writequeue / CSW queue / nopretry / addrretry. Do not Force Off mid-install. Keep Toshiba. Never flash `/dev/sdc`.

**Honest remainder.** 30% is iron COM2 `RAYNU-V-M8-DISK-PERSIST-OK`. Do not F11 until `I/O ready` + virtio on the LUN.

### 3. SKU card — 25%

**What it is.** A one-page “what you are buying.” Dedicated-box vs fleet. USB vs PERC. Alpine-patched vs unmodified. No cluster. No Windows.

**Product effect.** This page **is** the start of that card. Until A2/A4 close, the card must say “not yet.” Overstated copy does not survive diligence.

### 4. TLS — 8%

**What it is.** Coexist listen is plaintext HTTP on `10.99.99.x:8443`. That closed Everest (ADR-009 deferred TLS).

**Product effect.** A CISO will ask “is management encrypted?” The honest answer today is no. Bar A that skips TLS is a lab handshake, not an InfoSec-safe LOI.

### 5. Auth — 28%

**What it is.** ESP `auth.token` can override the bring-up token. Iron used `raynu-v-bringup` / `AuthAllowed`.

**Product effect.** Shared lab latch ≠ operator credential. Soft for a dedicated box you control; not soft if two people on the LAN can start VMs.

### 6. Guest console UI — 22%

**What it is.** Serial auto-answer already typed `setup-disk`. SPA has serial logs. No in-browser guest keyboard/VNC.

**Product effect.** Operators should not need iDRAC SOL to log into Alpine. Soft for Bar A if we name it. Hard for anyone who thinks this is vSphere.

### 7. PERC RAID I/O — 15%

**What it is.** The R640 “HDD” is Ubuntu on PERC H740P (`01:04`). The DurableLun mapper **classifies** RAID and prints `skip PERC` so we do not destroy the lab OS. There is no MegaRAID mailbox driver. PRE-EBS UEFI RAID BlockIo dies at ExitBootServices.

**Product effect.** This is the difference between a lab hypervisor and “extend the life of the fleet you already paid for.” USB persist is mechanism. Fleet persist is a spare virtual disk. Do not format Ubuntu.

### 8. Unmodified media (Gen-1) — 20%

**What it is.** Iron Alpine still uses an ISO patcher + serial auto-answer. RayNu-F is the firmware; the guest is not yet a specified Generation 1 VM on stock ISOs.

**Product effect.** Fine for a dedicated-box demo. A Bar B claim of “unmodified Linux ISO” is false until that patcher is gone. Windows is later still. **Not** a Bar A blocker if the SKU card says Alpine-on-virtio.

---

## Two-path timeline

| When | Focus | Exit | Status |
|------|-------|------|--------|
| 2026-09-11 | Everest | Iron ISO → disk → login | **DONE** — HDA 99%, months 0.0 |
| 2026-09-13 | Persist mechanism | Nested-OK + DurableLun USB/NVMe I/O | **DONE nested / host**; iron Force Off **open** |
| **NOW** | Bar A A2 | USB/NVMe Force Off on COM2 | **NEXT** (okquiet COM2 Toshiba named then SET_CONFIG `cmd=6 cmpl=0xff` leftover 1.07 GiB Everest; this EFI `xhci setcfgretry`; leftover DRAM ≠ persist) |
| then | Bar A A3+A4 | SKU card + TLS | design overlap OK; do not close TLS before persist COM2 |
| then | Bar B B2 | Spare PERC VD persist | after A2; census skip is lab safety |
| later | Auth / console / unmodified ISO | A5, A6, Gen-1 Phase 2 | named residuals, not fake closes |

```
2026-09  ████████  Everest closed
2026-10  ░░░░░░░░  Bar A: iron persist (CONFIG_EP CC=0 `bot=?`; Force Off open)
2026-11  ░░░░░░░░  Bar A: SKU + TLS  → first dedicated-box LOI window
2026-12  ░░░░░░░░  Bar B: PERC VD I/O
```

---

## Scope of claims

- Latitude and nested QEMU are not PowerEdge R640 evidence.
- Leftover DRAM, USB persist, NVMe persist, and PERC persist are distinct closes.
- Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`, `RAYNU-V-M8-DISK-PERSIST-OK`, or `RAYNU-V-M8-PERC-LUN-OK`.
- The Ubuntu PERC on `raynuvsrv1` is not formatted as an LOI strategy.
- RAID-fleet LOI waits on PERC virtual-disk persist. A USB demonstration is not that close.
- Everest remains closed. A patched Alpine ISO is a named residual, not a reopened summit.
- Cluster / vMotion is **M9**, not an LOI closer.
- Proven Core stays closed unless a new ADR says otherwise. MegaRAID/TLS/RayNu-F stay outside.

---

## This-commit delta

| Field | Value |
|-------|-------|
| Commit | m8-usb-bot-setcfgretry |
| Summary | **Iron leftover, not persist.** Okquiet COM2 Toshiba named then SET_CONFIG `cmd=6 cmpl=0xff` leftover DRAM 1.07 GiB Everest. This EFI: `xhci setcfgretry`. Never Toshiba `/dev/sdc`. Scores **held**. |
| Everest impact | none — HDA months 0.0 / 99% held |
| LOI impact | Bar A **42% held** / Bar B **18% held** / overall **38% held** / months A **1.5 held**. Persist piece **70% held**. A2 still open. Leftover DRAM `ISO-INSTALL-OK` is not A2. |
| Gates touched | Lived COM2 SET_CONFIG timeout leftover; this EFI `xhci setcfgretry`; [m8_persist_iron.md](runbooks/m8_persist_iron.md). `./tools/sync-loihda-site.sh --check`. No MegaRAID. No TLS. No F11. |

---

## LOIHDA changelog

| Date | Slice | A% | B% | Note |
|------|-------|----:|---:|------|
| 2026-09-18 | m8-usb-bot-setcfgretry | 42 | 18 | **Iron leftover, not persist.** Okquiet COM2 Toshiba named then SET_CONFIG `cmd=6 cmpl=0xff` leftover DRAM 1.07 GiB Everest. This EFI: `xhci setcfgretry`. Keep writequeue / okquiet. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-okquiet | 42 | 18 | **Iron leftover skipped, not persist.** Writequeue COM2 Toshiba 298 GiB + WRITE lived + `ISO-INSTALL-OK` on USB LUN then `usb rw ok` flood mashed COM2. This EFI: quiet oks (`xhci_rw_ok_should_print`). Keep writequeue. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-writequeue | 42 | 18 | **Iron leftover skipped, not persist.** Addrretry COM2 guest Toshiba 298 GiB + Alpine `[vda] 298 GiB` login then WRITE CBW `bot=cbw scsi=write cmpl=0xff` / `sfdisk` I/O error. This EFI: `xhci writequeue` + `xhci writesettle`. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-cswsettle | 42 | 18 | **Iron leftover skipped, not persist.** Sensebot COM2 Toshiba 298 GiB ready + nlb=1 oks then `bot=csw scsi=read cmpl=0xff`; `vda1`; no `scsi=sense`. This EFI: queue CSW IN with DATA + settle between nlb=1 chunks. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-sensebot | 42 | 18 | **Iron leftover skipped, not persist.** Nlb1 COM2 Toshiba 298 GiB ready + peek `usb_err=0` + nlb=1 512-byte oks then `scsi=sense` after Xfer timeout; `vda` no partition table. This EFI: REQUEST SENSE only on Bot, never on Xfer timeout. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-nlb1 | 42 | 18 | **Iron leftover skipped, not persist.** Stopwalk COM2 Toshiba 298 GiB ready + Alpine login; `vda last_st=0x1`; `sfdisk` I/O error. This EFI: nlb=1 BOT + REQUEST SENSE after CSW fail. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-stopwalk | 42 | 18 | **Iron leftover, not persist.** rstdev/udiskkick COM2 Toshiba named p11 then p14/p10 leftover 1 GiB — regression vs guestio I/O ready. This EFI: `xhci stopwalk` keep-slot + BOT retry. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-reexec | 42 | 18 | **Host flash, not persist.** `--branch` ran old flashcruzer.sh so UDisk kick never ran. Re-exec after checkout. Never Toshiba `/dev/sdc`. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-udiskkick | 42 | 18 | **Host flash, not persist.** lsusb UDisk `abcd:1234` but no lsblk. Authorized cycle that VID/PID only; never Toshiba `/dev/sdc`. Persist 70% held. A2 open. |
| 2026-09-17 | m8-usb-bot-rstdev | 42 | 18 | **Iron leftover, not persist.** Stallquiet COM2 GET_DESC timeout + descabort then Address Device `cmd=3 cmpl=0x13` leftover 1 GiB. Toshiba not named. This EFI: Reset Device (`xhci rstdev`) before re-Address. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-stallquiet | 42 | 18 | **Iron leftover skipped, not persist.** Guestio COM2 Toshiba 298 GiB ready + vda1; leftover apk stall dump during ISO mount. This EFI: hold stall dump until apk on USB LUN; usb rw ok/wait. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-guestio | 42 | 18 | **Iron leftover skipped, not persist.** Descabort COM2 Toshiba 298 GiB ready + virtio keep=0; Alpine vda last_st=0x1; sfdisk I/O error. ISO vdb last_st=0x0. This EFI: guest BOT FIRST_READ_SPINS + lun rw fail serial. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-descabort | 42 | 18 | **Iron leftover, not persist.** Slotretry COM2 nop + p11 EP0 GET_DESC timeout then p14 hub. Enable Slot lived. This EFI: keep-slot (`xhci descabort`); no Disable Slot; no p14 walk. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-slotretry | 42 | 18 | **Iron leftover, not persist.** p11first COM2 p11 Enable Slot `cmd=2 cmpl=0xff` then p14 hub. Toshiba not named. This EFI: No-Op prime + Enable Slot ADDR_SPINS retry (`xhci nop` / `xhci slotretry`). Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-p11first | 42 | 18 | **Iron leftover, not persist.** Cmdptr COM2 p10 ADDR then p11 Enable Slot `cmd=2 cmpl=0xff`. Toshiba not named. This EFI: p11-first + abort ADDR_SPINS, no Disable Slot wait. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-cmdptr | 42 | 18 | **Iron leftover, not persist.** Capoverlap COM2 named Toshiba p11 then CONFIG_EP `cmd=5 cmpl=0` `bot=?`. Not device-missing — leftover cmd event + Disable Slot. This EFI: Command TRB Pointer match. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-cfg-retry | 42 | 18 | **Iron leftover, not persist.** Capoverlap COM2 SET_CONFIG then CONFIG_EP `cmd=5 cmpl=0` `err=3 bot=?`. BOT never started. This EFI: drain + skip CC=0 + CONFIG_EP retry. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-cap-overlap | 42 | 18 | **Iron leftover, not persist.** Overlap COM2 `xhci firstcbw overlap` then CAPACITY CBW `cmpl=0xff`. INQUIRY overlap lived. This EFI: overlap CAPACITY + READ IN. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-inq-overlap | 42 | 18 | **Iron leftover, not persist.** Skipmaxlun COM2 `xhci maxlun skip` + `epst ep0=1` then INQUIRY CBW `cmpl=0xff`. EP0-Stopped is falsified. This EFI: overlap first INQUIRY CBW+DATA IN. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-skipmaxlun | 42 | 18 | **Iron leftover, not persist.** Norearm COM2 `xhci firstcbw norearm` then `epst ep0=3` after GET_MAX_LUN DATA-then-Stop; INQUIRY CBW `cmpl=0xff`. This EFI: skip optional GET_MAX_LUN so EP0 stays Running. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-norearm | 42 | 18 | **Iron leftover, not persist.** Firstcbw COM2 `xhci firstcbw` then INQUIRY CBW `cmpl=0xff`. Rearm before unused CBW did not retire a Transfer Event. This EFI: `norearm` + long wait. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-firstcbw | 42 | 18 | **Iron leftover, not persist.** Firstread COM2 `epst`/`botrst`/`maxlun` then INQUIRY CBW `cmpl=0xff`. No `xhci firstread`. This EFI: `prepare_first_cbw` before INQUIRY. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-firstread | 42 | 18 | **Iron leftover, not persist.** Epst COM2 `epst out=1 in=1` + BOT reset; INQUIRY/CAPACITY lived; READ CBW `cmpl=0xff`. This EFI: prepare_first_read + longer first-READ wait. Persist 70% held. A2 open. |
| 2026-09-16 | m8-usb-bot-epst | 42 | 18 | **Iron leftover, not persist.** Maxlun COM2 GET_MAX_LUN `val=0` then INQUIRY CBW `cmpl=0xff`. Enum/eval/setcfg/maxlun lived. This EFI: CONFIG_EP copies Output Slot+A1; `xhci epst`; MSC BOT reset + Clear Halt. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-bot-maxlun | 42 | 18 | **Iron leftover, not persist.** Ep0-eval COM2 SET_CONFIG then CAPACITY CBW `err=8`. Enum/eval/GET_CONFIG lived. This EFI: GET_MAX_LUN + required INQUIRY. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-ep0-eval | 42 | 18 | **Iron leftover, not persist.** Cfg-desc COM2 CH never named Toshiba; ep0-stop 9-byte GET_CONFIG `cmd=4`. This EFI: Evaluate Context EP0 MPS + GET_CONFIG 64 + Toshiba `0480:a004` BOT fallback. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-cfg-desc | 42 | 18 | **Iron leftover, not persist.** EP0-stop COM2 `cmd=4 cmpl=0xff` `bot=? scsi=?` after Toshiba named. 9-byte GET_CONFIG STATUS never posted. This EFI: CH + 256-byte GET_CONFIGURATION + DATA-or-STATUS. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-ep0-stop | 42 | 18 | **Iron leftover, not persist.** Stop-ep COM2 `cmd=4 cmpl=0xff` `bot=? scsi=?` after Toshiba named. GET_DESC timeout stacked EP0 TRBs. This EFI: Stop Endpoint on EP0 timeout. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-stop-ep | 42 | 18 | **Iron leftover, not persist.** First-cbw COM2 `cmpl=0x13 bot=cbw scsi=capacity` after SET_CONFIG. Reset Endpoint on Running. This EFI: Stop Endpoint. LOIHDA now lives on the USB stack. Persist 70% held. A2 open. |
| 2026-09-15 | m8-usb-first-cbw | 42 | 18 | **Iron leftover, not persist.** Skip-INQUIRY was `cmpl=0xff`; settle + waited INQUIRY then CAPACITY. Next COM2 was `cmpl=0x13`. Scores held. |
| 2026-09-15 | loi-page-restore | 42 | 18 | **Restore.** Live `loi.html` vanished because Workers Builds deployed USB persist feature branches to production. Page + tracker still on PR #257; `main` / `gh-pages` never had them. Scores held. Iron persist still open. |
| 2026-09-14 | loi-scope-copy | 42 | 18 | **Scope copy.** Public section retitled “What this tracker does not claim.” USB persist stays Bar A mechanism; RAID-fleet waits on PERC; TLS/ISO/cluster remain open; Everest stays closed. Scores held. |
| 2026-09-14 | loi-hda-page | 42 | 18 | **Born.** Public LOI page + living `docs/loihda.md`. Journey → LOI in site nav. Everest remains the HDA mountain. Iron persist still open. Never `ISO-INSTALL-OK`. |

---

## Operator quick view

```
LOI:           NOT OPEN. Tracker born 2026-09-14.
Bar A:         42% · 1.5 months · dedicated-box non-prod
Bar B:         18% · 3.5 months · PERC RAID fleet (out of conversation until PERC persist)
Overall:       38% · confidence medium
NOW:           M8.0-mech iron Force Off (USB ≥ 16 GiB, not Cruzer, not Ubuntu PERC)
Open:          TLS · unmodified ISO · cluster · Ubuntu PERC stays standing boot
Everest:       still closed (HDA 99% / 0.0 months) — different mountain
```

Public page: [`site/loi.html`](../site/loi.html). Living doc: this file. Sync: `./tools/sync-loihda-site.sh`.
