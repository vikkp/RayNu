---
loihda_version: 1
last_updated: 2026-09-22
last_commit: PENDING
last_commit_short: PENDING
updated_by: cursor
loi_target: "Non-prod Letter of Intent. Bar A = dedicated-box lab. Bar B = RAID-fleet replacement. HDA 99% is Everest, not an LOI."
months_to_loi_a: 0.75
months_to_loi_a_prev: 1.0
months_to_loi_b: 3.5
months_to_loi_b_prev: 3.5
overall_pct: 63
confidence: medium
baseline_date: 2026-09-14
baseline_months: 1.5
loi_a_eta_month: "2026-10"
loi_b_eta_month: "2026-12"
bar_a_pct: 78
bar_b_pct: 18
piece_everest_pct: 100
piece_persist_pct: 97
piece_sku_pct: 92
piece_tls_pct: 80
piece_auth_pct: 38
piece_console_pct: 58
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
| **Overall LOI readiness** | **63%** | Nearest honest conversation is Bar A. Not Bar B. Not GA. |
| **Bar A — dedicated-box** | **78%** | One PowerEdge we own or they dedicate. Disk survive HV reboot is evidence-closed. SKU card names ships vs does-not. **A4 TLS iron CLOSED** (`RAYNU-V-M8-TLS-OK`). **NOW** is the installed GRUB menu: `28cd4ff1` printed `warm=1` and still stopped at `grub>`. Standing SPA stays firmware until `login:`. Host auth HostReady; iron still lab latch. Firmware SPA keys + guest log in-tree; iron CONSOLE-OK is typing from the SPA. |
| **Bar B — RAID-fleet** | **18%** | Replace the licensed hypervisor on PERC virtual disks they already paid for. |
| **Months to Bar A** | **0.75** | Baseline 2026-09-14. ETA **2026-10**. Shrink only with DONE evidence. |
| **Months to Bar B** | **3.5** | PERC I/O is the long pole. ETA **2026-12**. |
| **Confidence** | medium | Everest is high-confidence. A2 evidence-closed; persist-OK printed on `928d6224`. A3 SKU is DONE. A4 TLS iron is DONE. Standing SPA is firmware, not a lived browser after Alpine. HostReady is not iron AUTH-OK. Host UART keys are not iron CONSOLE-OK. Host ISO blob is not iron ISO-UPLOAD-OK. |

```
Bar A (dedicated-box)  ████████████████░░░░  78%
Bar B (RAID fleet)      ███░░░░░░░░░░░░░░░░░  18%
Overall LOI             ████████████░░░░░░░░  63%
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
| A2 | **Persist across HV reboot** | Force Off → peek `keep=1` → SPA `DISK-BOOTX64` → `root=UUID=` → `login:` (**DONE on evidence** `4af78b43`; minted `RAYNU-V-M8-DISK-PERSIST-OK` **printed** on `928d6224`) | Guest disk is the 8 GiB Toshiba USB slice, not leftover DRAM. Nested-OK ≠ this. Residual: whole-LUN virtio / USB ≠ PERC. |
| A3 | **SKU card** | One page: what ships, what does not, dedicated-box vs fleet | **DONE** ([`docs/sku.md`](sku.md) / [`site/sku.html`](../site/sku.html)). Stops us promising PERC, Windows, or cluster in a Bar A conversation. |
| A4 | **TLS** | Browser/`curl --cacert` on native `CURL NOW → https://` **before RayNu-F** | **DONE** (`928d6224` COM2 `RAYNU-V-M8-TLS-OK`; Mac SPA HTML). Lab millicert. PRE-EBS SNP stays `http://`. |
| A4s | **Standing SPA** | Browser HTTPS **after** RayNu-F / Alpine `login:`; SPA stays; guest COM1 in the page | Investor can watch a product without iDRAC. Firmware-in-tree (coexist ticks on RayNu-F vmexit + USB waits; `GET /logs/guest`). After `login:`. Not closed until COM2 + a lived browser. |
| A5 | **Auth beyond bring-up** | Product default is not `raynu-v-bringup` | A shared lab latch is not an operator credential. HostReady (`RAYNU-V-M8-AUTH-HOST-OK`) is not this close. |
| A6 | **Operator keyboard** | Type in the guest from the SPA (not only iDRAC SOL) | Soft for the first LOI; still on the polish table. Serial already installed Alpine. Firmware SPA `POST /console/keys` is in-tree. Depends on A4s. Iron close is COM2 `RAYNU-V-M8-CONSOLE-OK`. |

A2 is **DONE on evidence**. A3 SKU is **DONE**. A4 TLS iron is **DONE** (`928d6224` `RAYNU-V-M8-TLS-OK`). **NOW is the installed GRUB menu** (`diskprime past pin` after lived `28cd4ff1` `grub>`). A4s standing SPA is next after `login:`. A5 host-ready is not iron ESP-required default. A6 firmware SPA keys are in-tree; iron SPA keyboard is not closed. M8.4 host-ready is not iron ISO-UPLOAD-OK.

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

### 2. Disk persist (M8.0-mech) — 97%

**What it is.** Guest F7 keeps leftover DRAM (ADR-017). A RayNu-V reboot used to zero it. DurableLun USB BOT on the Toshiba 8 GiB virtio slice now survives Force Off.

**Product effect.** Without persist, “install Linux” dies when the operator Force Offs the box — which they will, because that is how you recover a Type-1. On the R640 we own, the same Alpine now comes back from a USB-backed disk after Force Off, without running the installer again. The guest sees an **8 GiB** window, not the whole stick. USB is not PERC. Nested QEMU ≠ R640.

**Iron CLOSE (evidence + minted marker).** Guest8g skip-CRC EFI (`4af78b43`) A2 **CLOSED on evidence** (UUID `348005a9`). TLS EFI `928d6224` printed **`RAYNU-V-M8-DISK-PERSIST-OK`** after keep=1 `DISK-BOOTX64` → `root=UUID=dd673a9a-…` → `localhost:~#`. Sit there. Do not `setup-disk`. Do not Force Off. Never flash `/dev/sdc`.

**Honest remainder.** 3% is whole-Toshiba virtio + USB ≠ PERC. Do not claim 100%. Do not flash over `localhost:~#`.

### 3. SKU card — 92%

**What it is.** A one-page “what you are buying.” Dedicated-box vs fleet. USB vs PERC. Alpine-patched vs unmodified. No cluster. No Windows.

**Product effect.** The card exists: [`docs/sku.md`](sku.md) / [`site/sku.html`](../site/sku.html). A3 is **DONE**. Native coexist is **HTTPS** (lab millicert). PRE-EBS SNP remains **plaintext HTTP**. Dedicated-box **8 GiB** USB slice. **lab latch** until ESP `auth.token`. 8% remainder is keeping the page in lockstep with A5–A6 — not a missing page.

### 4. TLS — 80%

**What it is.** Firmware coexist feeds `Tls12Listen` (TLS 1.2 ECDHE-RSA-AES128-GCM + EMS). rustls/ring cannot join `uefi-bin`. Post-EBS native HTTPS window is **before RayNu-F**. **Iron CLOSE:** COM2 `928d6224` `RAYNU-V-M8-TLS-OK` after Mac `curl --cacert` SPA on `10.99.99.140:8443`. PRE-EBS SNP remains `http://`.

**Product effect.** A CISO who asks “is management encrypted on the wire I can reach after EBS?” now hears **yes, with a lab millicert**. Residual: not production PKI, not TLS 1.3, SPA `GET /` still unauthenticated. **A 45s window is not a standing product UI** — that is A4s.

### 5. Auth — 38%

**What it is.** HostReady never accepts `raynu-v-bringup` (`RAYNU-V-M8-AUTH-HOST-OK`). ESP `auth.token` is the operator credential path. Firmware HTTP still accepts the lab latch when no ESP token is armed. Iron used `raynu-v-bringup` / `AuthAllowed`.

**Product effect.** Shared lab latch ≠ operator credential. The host package names that. The box you own still answers the lab latch until an ESP token is staged. Soft for a dedicated box you control; not A5 closed.

### 6. Guest console UI — 58%

**What it is.** Firmware SPA `POST /console/keys` injects operator keys into guest COM1. `GET /logs/guest` is the guest COM1 TX copy (survives SOL drain; **not iDRAC COM2**). HostReady UART (`RAYNU-V-M8-CONSOLE-HOST-OK`) still round-trips. `GET /logs/serial` remains HV UART. Not VNC. Standing HTTPS during RayNu-F is firmware-in-tree.

**Product effect.** Operators should not need iDRAC SOL to see Alpine or type. The page now has a keyboard field and a guest log panel. Iron close for the keyboard is A6. Iron close for “the SPA stays up” is A4s. Soft for Bar A if we name it. Neither closed.

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
| 2026-09-13 | Persist mechanism | Nested-OK + DurableLun USB/NVMe I/O | **DONE nested / host** |
| 2026-09-18 | Bar A A2 | USB Force Off on COM2 | **DONE on evidence** (`4af78b43` keep=1 DISK-BOOT UUID `348005a9`; minted persist-OK not printed) |
| 2026-09-18 | Bar A A3 | SKU card | **DONE** ([`docs/sku.md`](sku.md) / [`site/sku.html`](../site/sku.html)) |
| **2026-09-19** | Bar A A4 | TLS on native `:8443` **before RayNu-F** | **DONE** (`928d6224` COM2 `RAYNU-V-M8-TLS-OK`; Mac `curl --cacert` SPA `.140`) |
| **NOW** | Bar A open `grub.cfg` | `28cd4ff1` `warm=1` still `grub>` | **NEXT** — Force Off, then F11 the EFI that prints `diskprime past pin`. The pin still serves LBA0–LBA33, so the early warm never reached `grub.cfg`. Not lived. Do not curl Start |
| then | Bar A A6 | SPA keyboard | after A4s; firmware `POST /console/keys` in-tree; not VNC |
| then | Bar B B2 | Spare PERC VD persist | after A6; census skip is lab safety |
| later | Auth / unmodified ISO | A5, Gen-1 Phase 2 | named residuals, not fake closes |

```
2026-09  ████████  Everest closed
2026-10  █████░░░  Bar A: A4 TLS iron DONE; standing SPA firmware in-tree; A4s iron next
2026-11  ░░░░░░░░  Bar A: first dedicated-box LOI window
2026-12  ░░░░░░░░  Bar B: PERC VD I/O
```

---

## Scope of claims

- Latitude and nested QEMU are not PowerEdge R640 evidence.
- Leftover DRAM, USB persist, NVMe persist, and PERC persist are distinct closes.
- Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`, `RAYNU-V-M8-DISK-PERSIST-OK`, `RAYNU-V-M8-TLS-OK`, `RAYNU-V-M8-AUTH-OK`, `RAYNU-V-M8-CONSOLE-OK`, `RAYNU-V-M8-ISO-UPLOAD-OK`, or `RAYNU-V-M8-PERC-LUN-OK`.
- The Ubuntu PERC on `raynuvsrv1` is not formatted as an LOI strategy.
- RAID-fleet LOI waits on PERC virtual-disk persist. A USB demonstration is not that close.
- Everest remains closed. A patched Alpine ISO is a named residual, not a reopened summit.
- Cluster / vMotion is **M9**, not an LOI closer.
- Proven Core stays closed unless a new ADR says otherwise. MegaRAID/TLS/RayNu-F stay outside.

---

## This-commit delta

| Field | Value |
|-------|-------|
| Commit | m8-grub-past-pin |
| Summary | **Warm lived, menu did not.** `28cd4ff1` printed `warm=1`, staged `DISK-BOOTX64`, and stopped at GRUB `grub>`. The pin still answers LBA0–LBA33, so that warm never covered `grub.cfg`. This EFI warms once on the first read past the pin (`diskprime past pin`). Not lived. Scores held. |
| Everest impact | none — HDA months 0.0 / 99% held |
| LOI impact | Bar A **78% held** / Bar B **18% held** / overall **63% held** / months A **0.75 held**. Console piece **58% held**. A4s not closed. |
| Gates touched | `fn durable_lun_arm_past_pin_warm`. `fn xhci_live_warm_past_pin`. `diskprime past pin`. `./tools/sync-loihda-site.sh --check`. No MegaRAID. Sit at `localhost:~#`. |

---

## LOIHDA changelog

| Date | Slice | A% | B% | Note |
|------|-------|----:|---:|------|
| 2026-09-22 | m8-grub-past-pin | 78 | 18 | **Warm lived, still `grub>`.** `28cd4ff1` `warm=1` ×2 then `DISK-BOOTX64` then the GRUB command line. Pin served LBA0–LBA33, so `grub.cfg` stayed cold. This EFI prints `diskprime past pin` on that read. Not lived. Do not F11 `28cd4ff1`. Bar A 78 held. |
| 2026-09-22 | m8-grub-stay | 78 | 18 | **Restore the USB warm `928d6224` used.** That EFI diskprime READ LBA0 and the menu reached `login:`. `d60431ee` `lba1=pin` skipped the warm; `blk_rd=33` is the RAM GPT; GRUB never loaded `grub.cfg` and the 180 s cap fired. This EFI prints `warm=1` and READ LBA0, no `recover_pipes`. Not lived. Bar A 78 held. |
| 2026-09-22 | m8-gpt-pin-hold | 78 | 18 | **Wall-cap at `grub>`.** `d60431ee` `DISK-BOOTX64` → GRUB 2.12 `grub>` → `RayNu-F stop wall-cap exits=308551680 wall_ms=180005 blk_rd=33 blk_wr=0` → `product ISO hold`. Same EFI repeats. Bar A 78 held. |
| 2026-09-22 | m8-gpt-pin-hold | 78 | 18 | **Pin-hold lived on COM2.** `d60431ee` peek `installed=1` `keep=1` `lba1=pin` `BOOTX64` 139264 `image=DISK-BOOTX64` → GRUB 2.12 `grub>`. Not ISO. Not `login:`. Type `normal` was the instruction before the wall-cap. Bar A 78 held. |
| 2026-09-22 | m8-gpt-pin-hold | 78 | 18 | **Public copy.** `site/loi.html` / HDA aside / homepage status now say persist-OK printed on `928d6224`, and NOW is recover `DISK-BOOTX64` then standing SPA. Scores held. Do not F11 `08202468`. Bar A 78 held. |
| 2026-09-21 | m8-gpt-pin-hold | 78 | 18 | **Hold the peek GPT and skip the ISO.** Lived `08202468` peek `gpt=1` `usb_err=8` then `image=ISO-BOOTX64` / `vda` RCU stall. Do not F11 `08202468`. A4s not closed. Console 58 held. Bar A 78 held. |
| 2026-09-21 | m8-gpt-pin-stage | 78 | 18 | **Pin peek GPT for RayNu-F stage.** Lived `3b388279` peek keep=1 then `image=test-app` / `product ISO hold`. Do not F11 `3b388279`. A4s not closed. Console 58 held. Bar A 78 held. |
| 2026-09-21 | m8-spa-safari-idle | 78 | 18 | **Defer standing SPA until RayNu-F launch.** Lived `17d120c5` peek keep=1 then HTTPS during diskprime → `no GPT`, no login. Firefox Unable to connect. A4s not closed. Console 58 held. Bar A 78 held. |
| 2026-09-19 | m8-spa-safari-idle | 78 | 18 | **Paced SOL RX after host power-off.** Lived `b5e290be` SPA HTTPS then chassis off (per-exit COM2 `inb`). This EFI: `poll_host_rx_paced` ~10 ms; keep=1 skip ISO; `ST_CH` idle 2 s. Do not F11 `b5e290be`. A4s not closed. Console 58 held. Bar A 78 held. |
| 2026-09-19 | m8-standing-spa | 78 | 18 | **Standing SPA during RayNu-F in-tree.** AfterEbs arms coexist; ticks on RayNu-F vmexit + USB waits; `GET /logs/guest`. Never print iron CONSOLE-OK. A4s not closed until a browser stays up after Alpine login. Console 50→58. Bar A 78 held. |
| 2026-09-19 | m8-tls-iron | 78 | 18 | **A4 CLOSED on COM2.** EFI `928d6224` native HTTPS GET `.140` → SPA → `RAYNU-V-M8-TLS-OK`. Keep=1 DISK-BOOT also printed `RAYNU-V-M8-DISK-PERSIST-OK` (`UUID=dd673a9a`). rustls/ring stay out of `uefi-bin`. TLS 45→80. Persist 95→97. SKU 90→92. Months A 1.0→0.75. A6 NOW. |
| 2026-09-18 | m8-tls-iron | 68 | 18 | **M8.1 native HTTPS window before RayNu-F.** Analog then `Tls12Listen` (`PRE_RAYNUF_HTTPS_MS`). COM2 `1647a8d8` launched Alpine with no `https://` (`curl: (28)` on SNP `.150`). RDRAND CPUID-gated. rustls/ring stay out of `uefi-bin`. Never print iron TLS-OK. Iron close is `curl --cacert` on native `CURL NOW → https://`. TLS 40→45. Persist 95 held. A4 not closed. |
| 2026-09-18 | m8-iso-upload-sku | 62 | 18 | **A3 SKU DONE + M8.4 host ISO upload HostReady.** Dedicated-box card (`docs/sku.md` / `site/sku.html`). Host blob PUT/POST (`RAYNU-V-M8-ISO-UPLOAD-HOST-OK`). ESP-staged stays valid. Never print iron ISO-UPLOAD-OK. Do not flash. SKU 25→90. Persist 95 held. TLS 22 held. A4 not closed. |
| 2026-09-18 | m8-console-host | 51 | 18 | **M8.3 host console HostReady.** Operator keys reach guest COM1; guest THR echo captured (`RAYNU-V-M8-CONSOLE-HOST-OK`). Firmware SPA is still host serial log. Not VNC. Never print iron CONSOLE-OK. Do not flash. Console 22→32. Persist 95 held. TLS 22 held. Auth 38 held. A6 not closed. |
| 2026-09-18 | m8-auth-host | 50 | 18 | **M8.2 host auth HostReady.** Operator token is the product latch; `raynu-v-bringup` is lab-only (`RAYNU-V-M8-AUTH-HOST-OK`). Firmware REST still accepts the lab latch when no ESP `auth.token`. Never print iron AUTH-OK. Do not flash. Auth 28→38. Persist 95 held. TLS 22 held. A5 not closed. |
| 2026-09-18 | m8-tls-fw | 49 | 18 | **M8.1 firmware TLS wrap host-proven.** Coexist TCP feeds `PlaintextListen`. Host rustls feed/take/wrap SPA (`RAYNU-V-M8-TLS-FW-HOST-OK`). `cargo test --lib -- --test-threads=1`: 768 passed. rustls/ring cannot join `uefi-bin`. CURL NOW stays `http://`. Not iron HTTPS. Do not flash. TLS 18→22. Persist 95 held. |
| 2026-09-18 | m8-usb-bot-guest8g | 46 | 18 | **Public copy.** Persist piece on `site/loi.html` rewritten as a buyer explanation (Force Off used to wipe; USB-backed Alpine now returns after HV reboot). Scores held. Persist 95% held. |
| 2026-09-18 | m8-usb-bot-guest8g | 46 | 18 | **A2 CLOSED on evidence.** Guest8g skip-CRC EFI (`4af78b43`) Force Off persist: peek `keep=1` → SPA `xhci diskprime` `image=DISK-BOOTX64` bytes=139264 → `DISK-BOOT-OK` → `root=UUID=348005a9-…` → `login: root`. Minted persist-OK did not print. Spurious `ISO-INSTALL-OK` on journal recovery. Auto-answer `No disks found`. Sit at `localhost:~#`. Do not setup-disk. Persist 70→95. Months A 1.5→1.0. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Iron leftover skipped, not persist.** Guest8g skip-CRC EFI (`4af78b43`) peek `keep=1` `installed=1` virtio 8 GiB `keep=1`. Coexist `10.99.99.146:8443`. **SPA Start now.** Do not setup-disk. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Iron leftover skipped, not persist.** Guest8g A2 SPA Start (`b661808c`) peek `keep=1` then `image=ISO-BOOTX64` + `vda` I/O error. Skip GPT array CRC + `xhci diskprime`. Force Off. Do not setup-disk. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Iron leftover skipped, not persist.** Guest8g iDRAC SOL `closed by remote host` at `usb rw ok wr=1 n=1163264 off=0x184f72…` after F7 `login:`. Blank reconnect ≠ wipe. Smash Enter; curl `:8443`; Force Off for A2. Do not SPA Start. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Iron leftover skipped, not persist.** Guest8g COM2 (`b661808c`) 8 GiB USB `ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` + `login: root` `UUID=348005a9-…` on `[vda] 8.00 GiB`. After login `usb rw ok wr=1` `n` 1.16 M is 8 GiB ext4lazyinit. Guest F7 ≠ A2. Do not SPA Start. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Host/EFI, not persist.** Guest8g fit peek LBA1 miss retries (not treated as too-big). 8 GiB virtio; leftover 298 GiB GPT `fit=0` so SPA Start SETUP is **intended**. Do not SPA Start setcfgretry `ef7e93ec`. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-guest8g | 42 | 18 | **Host/EFI, not persist.** Guest8g: virtio USB **8 GiB**; leftover 298 GiB GPT `fit=0` so SPA Start SETUP is **intended** (wipes disposable UUID `6d549fd4`). CSW tag + peek retry for Force Off keep-detect. Do not SPA Start setcfgretry `ef7e93ec`. Never Toshiba `/dev/sdc`. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-setcfgretry | 42 | 18 | **Iron leftover skipped, not persist.** Setcfgretry Force Off COM2 (`ef7e93ec`) Toshiba 298 GiB `usb I/O ready` + leftover skip, then peek `gpt_err=2` `installed=0` virtio `keep=0`. Guest F7 `DISK-BOOT-OK` did not survive HV reboot. Do not SPA Start. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-setcfgretry | 42 | 18 | **Iron leftover skipped, not persist.** Setcfgretry COM2 (`ef7e93ec`) hours after USB F7 `DISK-BOOT-OK` / `root=UUID=6d549fd4-…` still `usb rw ok wr=1` (`n` 3.1 M → 10.7 M, `off=0x3b03…` ≈ 252 GB / ~79% of 320 GB). Payload ≈ 3.9 GiB vs ~245 GiB LBA walk — ext4lazyinit on 298 GiB `vda2`, not a hang, not a second setup-disk. Do not Force Off (dirty ext4). Do not reflash. Guest F7 ≠ A2. Keep writequeue / okquiet / setcfgretry. Persist 70% held. A2 open. |
| 2026-09-18 | m8-usb-bot-setcfgretry | 42 | 18 | **Iron leftover skipped, not persist.** Setcfgretry COM2 (`ef7e93ec`) SET_CONFIG first-try + Toshiba 298 GiB USB `ISO-INSTALL-OK` → `Installation is complete` → F7 `DISK-BOOT-OK` + `root=UUID=6d549fd4-…` on 298 GiB `vda`. Post-login `usb rw ok wr=1` overlay is not a hang. Guest F7 ≠ Force Off persist. Do not Force Off to stop the scroll. Keep writequeue / okquiet / setcfgretry. Persist 70% held. A2 open. |
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
Bar A:         78% · 0.75 months · dedicated-box non-prod
Bar B:         18% · 3.5 months · PERC RAID fleet (out of conversation until PERC persist)
Overall:       63% · confidence medium
NOW:           28cd4ff1 warm=1 still grub> · Force Off · F11 diskprime past pin · do not curl Start · do not setup-disk
Open:          iron SPA keyboard · iron AUTH-OK · unmodified ISO · cluster · Ubuntu PERC stays standing boot
Everest:       still closed (HDA 99% / 0.0 months) — different mountain
Sit:           localhost:~# · do not setup-disk · do not flash Toshiba /dev/sdc
```

Public page: [`site/loi.html`](../site/loi.html). Living doc: this file. Sync: `./tools/sync-loihda-site.sh`.
