# R640 — ADR-013 Phase 0 PCI census (2026-08-17)

**Claim:** Phase 0 closed on iron. One management NIC chosen:

| Field | Value |
|-------|--------|
| PCI | `01:00.0` (func 0) |
| `vid:did` | **`14e4:165f`** (Broadcom NetXtreme **BCM5720**) |
| BAR0 | `0x92930000` |
| MSI-X | 17 table entries |
| Second port | `01:00.1` same `vid:did`, BAR0 `0x92900000` (do not bind both yet) |
| IOMMU | `ACPI DMAR=no` (print only; VT-d may be off in BIOS or missed by XSDT walk) |

**Not claimed:** `RAYNU-V-M7-HOST-NIC-HTTP-OK`. No native Device for this id yet.

## Media / path

Cruzer Micro, front USB 2. Replace-only `EFI/BOOT/BOOTX64.EFI` (Vignesh).  
PRE-EBS SNP residual still served SPA (`10.99.99.121:8443`) → `RAYNU-V-M7-UEFI-HTTP-OK`.  
Guest path green through M4 + `RAYNU-V-R640-BOOT-OK`. SNP idle WARN-only. **No RSOD.**

## Fingerprint (COM2)

```text
boot: PCI census nics=2
boot: PCI 01:00.00 vid:did=14e4:165f bar0=0x92930000 msix=17
boot: PCI 01:00.01 vid:did=14e4:165f bar0=0x92900000 msix=17
boot: IOMMU ACPI DMAR=no
boot: HOST-NIC census: no lab e1000; do not guess LOM (Phase D waits on this list)
…
RAYNU-V-R640-BOOT-OK
boot: WARN — POST-EBS SNP idle skipped; firmware SNP dead after EBS lease=10.99.99.121:8443
boot: HOST-NIC idle: no native Device for census vid:did=14e4:165f (Phase D waits on this id; do not guess LOM)
```

Full log: [`logs/2026-08-17-phase0-census-com2.txt`](logs/2026-08-17-phase0-census-com2.txt).

## Honesty

- Lab e1000 (`8086:100e`) is **not** on this box. QEMU `HOST-NIC-QEMU-OK` does not count.
- Firmware Tcp4 still absent; PRE-EBS SNP still works. That is E3, not E3b.
- Phase D is now unblocked **only** for `14e4:165f`. Same `smoltcp::phy::Device`
  surface as e1000. Do not start X710/i40e.

### 2026-08-18 addendum — SNP is `01:00.1`

Phase D's first iron bind used **`01:00.0` / `b0:26:28:5c:5a:38`**. A clean
power-cycle still parked SNP lease `10.99.99.134`; Vignesh (`10.99.99.137`)
got **destination host unreachable** (no ARP). Firmware SNP is the other
function: **`01:00.1` / `b0:26:28:5c:5a:3a`**. Bind that MAC. Do **not**
claim `HOST-NIC-HTTP-OK` from this note.

### 2026-08-18 addendum — Linux `tg3`, not `bnxt`

BCM5720 (1G) is mainline **`tg3`**
(`drivers/net/ethernet/broadcom/tg3.c`). **`bnxt`** is BCM57416 10G.
Picker (SNP MAC / func 1) stays. Bring-up now matches `tg3` + PG ch. 7
(magic-before-reset, preserve PCIe bit 29, DMA engines, 5717+ RCB rules).
Do **not** claim `HOST-NIC-HTTP-OK` from this note.

### 2026-08-18 addendum — BAR0 peek `:39` vs SNP `:3a`

Flashed tg3 bring-up EFI. Guest path green (`RAYNU-V-R640-BOOT-OK`).
`fw-magic=yes`, `phy=yes`, rings armed. Pick was **`01:00.1`** (right jack)
but BAR0 peek was **`:5a:39`** while PRE-EBS SNP leased **`:5a:3a`**
(`10.99.99.126`). Fallback `no MAC match`. Native Device used `:39`;
Vignesh curl to the parked lease timed out (ARP identity mismatch).

Station address is now the **parked SNP MAC**, not the BAR0 peek.

### 2026-08-19 addendum — station `:3a` programmed; still no ARP

Station-MAC EFI (`1208832` bytes) programmed **`:5a:3a`**. COM2:
`station SNP-lease MAC=b0:26:28:5c:5a:3a (BAR0 peeked …:39)`, `fw-magic=yes`,
`phy=yes`, `Device MAC=…:3a`, idle on `10.99.99.116:8443`. PRE-EBS SNP HTTP
still worked. After `BOOT-OK`, Vignesh ping from `10.99.99.137` got
**destination host unreachable** (no ARP). He also curled port **8445**
(mgmt is **8443**). `phy=yes` is MDIO write success, not copper link.

Next: wait `BMSR_LSTATUS` (read twice), set `MAC_MODE` MII vs GMII from
speed like Linux `tg3_adjust_link`, enable `RX_MODE_PROMISC`. Do **not**
claim `HOST-NIC-HTTP-OK` from this note.

### 2026-08-19 addendum — `link=timeout bmsr=7949`

PHY-link-wait EFI (`1209856`) still no ARP. COM2:

```
link=timeout bmsr=7949 mac_status=00400000
```

`0x7949` is a live copper PHY: 10/100 capable, extended status, **LSTATUS=0**,
**ANEGCOMPLETE=0**. Station `:3a` and `fw-magic=yes` stayed. AN poke after
CORECLK_RESET does not bring the analog front-end up. Next: Linux
`tg3_bmcr_reset`, AUXCTL PWRCTL=0 (clear isolate), Auto-MDIX, CPMU idle
clear. Do **not** claim `HOST-NIC-HTTP-OK` from this note.

### 2026-08-19 addendum — PHY BMCR-reset still `bmsr=7949` / `lpa=0000`

PHY-reset EFI (`1210880` bytes, SHA
`fcabf7a92e332b64934fa7822f8166207beec4d0743e4df0e14e2de1765a9594`) still no
native HTTP. COM2:

```
station SNP-lease MAC=b0:26:28:5c:5a:3a (BAR0 peeked b0:26:28:5c:5a:39)
fw-magic=yes
phy_addr=02 serdes=no sgdig=00000008
phy_reset=yes pwrctl=clr mdix=yes phy=yes id=0362:5f60
link=timeout bmsr=7949 bmcr=1100 lpa=0000 s1000=0000 mac_status=00400000 cpmu=00004000
idle listening on 10.99.99.116:8443
```

`id=0362:5f60` is a Broadcom GPHY; PHY addr 2 is correct for func 1 copper.
`bmcr=1100` is ANENABLE|FULLDPLX (ANRESTART already cleared). `lpa=0000` /
`s1000=0000` means no link partner at all after `CORECLK_RESET`. PRE-EBS SNP
SPA still worked on this jack.

After `BOOT-OK`, Vignesh ping `10.99.99.116` got replies (`ttl=63`, 25–73 ms)
then `curl -sS -m 5 http://10.99.99.116:8443/` timed out. `ttl=63` is one
routed hop — not L2-local native BCM5720 while `bmsr=7949`. Treat that ping as
not E3b. `curl http://10.99.99.LEASE:8443/` failed DNS because `LEASE` was
typed literally; the URL is `http://<numeric-lease>:8443/`.

`CORECLK_RESET` after SNP park is the leading suspect for killing copper.
Next: peek `BMSR` before reset; inherit SNP PHY when `LSTATUS` is set (skip
`CORECLK_RESET` + `tg3_phy_reset`); else Linux `tg3_phy_toggle_apd(false)`.
Do **not** claim `HOST-NIC-HTTP-OK` from this note.

### 2026-08-19 addendum — inherit EFI: PHY already down before reset

Inherit-PHY EFI (`1211392` bytes, SHA
`69ed29721269173b5c17850721ecb4f55de92c627cee735e4799bd32189d7819`) did **not**
inherit. COM2:

```
pre-reset bmsr=7949 ape=yes
reset…
phy_reset=yes pwrctl=clr apd=off mdix=yes phy=yes id=0362:5f60
link=timeout bmsr=7949 bmcr=1100 lpa=0000 s1000=0000
```

Copper was already down **before** `CORECLK_RESET` (`inherit_snp_phy` is false
for `0x7949`). `ape=yes` means SRAM `NIC_SRAM_DATA_CFG_APE_ENABLE`. After
`BOOT-OK`, Mac ping of `10.99.99.116` was **100% loss** (the earlier `ttl=63`
replies were not native). Next: Linux `tg3_ape_lock` on BAR2 + driver START
around MDIO (this branch). Do **not** claim `HOST-NIC-HTTP-OK` from this note.
Reject inherit EFI `1211392`.

### 2026-08-19 addendum — APE-lock EFI: MDIO works, analog still down

APE-lock EFI (`1213440` bytes, SHA
`5858c482f68445bdd8b402c2ad952005670b8a70cc49e8b26fb1a60800a8f761`) took the
PHY mutex. COM2:

```
pre-reset bmsr=7949 ape=yes ape-bar=0x92910000 ape-fw=ready ape-evt=sent ape-lock=yes
phy_reset=yes pwrctl=clr apd=off mdix=yes phy=yes id=0362:5f60
link=timeout bmsr=7949 bmcr=1000 lpa=0000 s1000=0000 mac_status=00400000 cpmu=00004000
```

`ape-lock=yes` + a real PHY id means MDIO is no longer the blocker. `bmcr=1000`
(ANENABLE, writes sticking) vs the inherit EFI `bmcr=1100`. Still
`lpa=0000`. `cpmu=00004000` is Linux `CPMU_CTRL_LINK_SPEED_MODE`, not idle.
Mac ping/curl of the lease after `BOOT-OK` still failed.

Linux `tg3_reset_hw` BMCR-resets the PHY **before** `CORECLK_RESET`, then
`tg3_setup_phy(false)` (AN restart, no second analog reset). Next EFI does
that. Do **not** claim `HOST-NIC-HTTP-OK` from this note.
Reject APE-lock EFI `1213440`.

### 2026-08-19 addendum — PHY-before-chip-reset still `lpa=0000`

PHY-before-chip-reset EFI (`1213952` bytes, SHA
`df94a20aad78f678b68c839efff0390246ac6b228b496b4df2a6a307a20c5bd8`, CI run
`32278648924`) still no native HTTP. COM2:

```
pre-reset bmsr=7949 ape=yes ape-bar=0x92910000 ape-fw=ready ape-evt=sent ape-lock=yes
phy_addr=02 serdes=no sgdig=00000008
phy_reset=pre yes pwrctl=clr apd=off mdix=yes phy=yes id=0362:5f60
reset…
fw-magic=yes
MAC=b0:26:28:5c:5a:3a
an-restart=yes pwrctl=clr
link=timeout bmsr=7949 bmcr=1000 lpa=0000 s1000=0000 mac_status=00400000 cpmu=00004000
rings armed (poll-mode, MSI-X off)
idle listening on 10.99.99.116:8443
```

APE lock and MDIO stayed proven (`ape-lock=yes` / `id=0362:5f60` / BMCR
writes stick at `1000`). Linux `tg3_reset_hw` order ran (`phy_reset=pre`
then `CORECLK_RESET` then `an-restart=`). Copper still never saw a link
partner. Mac ping/curl of the lease after `BOOT-OK` still failed.

`CORECLK_RESET` itself is the remaining analog-kill suspect. Next EFI
**never chip-resets**: steal rings, BMCR_RESET only when `LSTATUS` is
clear, clear CPMU EEE LPI, print APE `NCSI`. Do **not** claim
`HOST-NIC-HTTP-OK` from this note.
Reject PHY-before-chip-reset EFI `1213952`.

### 2026-08-19 addendum — skip-`CORECLK_RESET` still `lpa=0000`; `ape-ncsi=yes`

Skip-chip-reset EFI (`1212416` bytes, SHA
`867651668efcfb5ba13b5f2a431a0b4c711cc5517ca4590b39177abc5f49b9e7`, CI run
`32283740560`) skipped `CORECLK_RESET` as designed. COM2:

```
SNP residual MAC=b0:26:28:5c:5a:3a
SNP lease 10.99.99.116/24
cand pci=01:00.00 MAC=b0:26:28:5c:5a:38
cand pci=01:00.01 MAC=b0:26:28:5c:5a:39
pick pci=01:00.01 … fallback=func1 (no MAC match)
pre-reset bmsr=7949 ape=yes ape-bar=0x92910000 ape-fw=ready ape-evt=sent ape-ncsi=yes ape-lock=yes
skip CORECLK_RESET (keep GPHY analog)
phy_reset=pre yes … eee=off phy=yes id=0362:5f60
link=timeout bmsr=7949 bmcr=1000 lpa=0000
```

No `reset…` / `fw-magic` / `an-restart`. PRE-EBS SNP still got DHCP on `:3a`
(`media_present`). Func 1 was already down at bind (`pre-reset bmsr=7949`).
`ape-ncsi=yes` — APE NCSI is on; SNP `:3a` is the management MAC (not either
BAR0 peek). Next EFI peeks each candidate `BMSR` and tries **func 0
(NCSI/LOM1) then func 1** until `LSTATUS`; station stays the SNP lease MAC.
Do **not** claim `HOST-NIC-HTTP-OK` from this note.
Reject skip-reset EFI `1212416`.

### 2026-08-19 addendum — both funcs `bmsr=7949` at BOOT-OK

Dual-func EFI (`1213952` bytes, SHA
`9fc6a3c2f4e689232a385715aaa54f80c25d55d68eb22b755bd9680fcbc996e5`, CI run
`32285501656` — **size collides** with PHY-before-reset `df94a20a…`) tried
both ports. PRE-EBS SNP HTTP closed (`RAYNU-V-M7-UEFI-HTTP-OK`) on `:3a`.
After the guest path, COM2:

```
cand pci=01:00.00 MAC=…:38 bmsr=7949
cand pci=01:00.01 MAC=…:39 bmsr=7949
try pci=01:00.00 … fallback=func0 (NCSI/LOM1) phy_addr=01 ape-bar=0x92940000
link=timeout bmsr=7949 lpa=0000
try next func
try pci=01:00.01 … fallback=func1 (retry) phy_addr=02 ape-bar=0x92910000
link=timeout bmsr=7949 lpa=0000
```

Wrong-port is **closed**. Copper was up for SNP, then both host PHYs were
down at BOOT-OK. Next EFI brings BCM5720 up **immediately after EBS**
(before VMX/Linux) and reuses that Device after BOOT-OK. Do **not** claim
`HOST-NIC-HTTP-OK` from this note.
Reject dual-func EFI unless SHA is a later build.

### 2026-08-19 addendum — post-EBS `cand bmsr=7949` (guest path closed)

Post-EBS bring-up EFI (`1214464` bytes, SHA
`4a2d4f0a927f0d95cfc518fdadab6855df29450011edc95449b9559dbbdc767c`, CI run
`32287320437`) bound immediately after EBS. PRE-EBS SNP DHCP on `:3a`
(`10.99.99.116`). COM2 **before VMX**:

```
post-EBS bring-up (keep analog before guest path)
cand pci=01:00.00 MAC=…:38 bmsr=7949
cand pci=01:00.01 MAC=…:39 bmsr=7949
try pci=01:00.00 … phy_reset=pre yes … ape-ncsi=yes
link=timeout bmsr=7949 lpa=0000
try next func
try pci=01:00.01 … phy_reset=pre yes
link=timeout bmsr=7949 lpa=0000
reuse (armed post-EBS)
```

Guest-path delay is **closed**. Analog is already down at EBS. Next EFI
skips `BMCR_RESET` when `ape-ncsi=yes` (`phy_reset=pre skip (ape-ncsi)`).
Do **not** claim `HOST-NIC-HTTP-OK` from this note.
Reject post-EBS bring-up EFI `4a2d4f0a`.

### 2026-08-19 addendum — skip-BMCR still `cand bmsr=7949`

Skip-`BMCR_RESET` EFI (`1214464` bytes, SHA
`b6fcf3bb0f6ab9330606b696a9e041295527388348c2a76fc47e6d94a38b81c5`, CI run
`32290233834` — **size collides** with `4a2d4f0a`) printed
`phy_reset=pre skip (ape-ncsi)` on both funcs. PRE-EBS SNP HTTP closed on
`:3a`. COM2 before VMX still `cand bmsr=7949` then `link=timeout`. Analog is
down at EBS before host PHY writes. Next EFI runs `CORECLK_RESET` without
`BMCR_RESET` (Linux `tg3_chip_reset`). Do **not** claim `HOST-NIC-HTTP-OK`.
Reject skip-BMCR EFI `b6fcf3bb`.

### 2026-08-19 addendum — CORECLK without BMCR still `cand bmsr=7949`

CORECLK-without-BMCR EFI (`1216000` bytes, SHA
`1404f055f210d1c4d7c551a31041e669681d002cafe3b4614a09e7a0f737865c`, CI run
`32291875771`, commit `27937a9` / feature `f0961f1`) ran the intended
path. Complete COM2:

```
SNP residual MAC=b0:26:28:5c:5a:3a
SNP lease 10.99.99.116/24
RAYNU-V-M7-UEFI-HTTP-OK
post-EBS bring-up (keep analog before guest path)
cand pci=01:00.00 MAC=…:38 bmsr=7949
cand pci=01:00.01 MAC=…:39 bmsr=7949
try pci=01:00.00 … fallback=func0 (NCSI/LOM1)
phy_reset=pre skip (ape-ncsi)
reset…
fw-magic=yes
an-restart=yes pwrctl=clr
link=timeout bmsr=7949 bmcr=1000 lpa=0000 s1000=0000 mac_status=00400000 cpmu=00004000
try next func
try pci=01:00.01 … phy_reset=pre skip (ape-ncsi) reset… fw-magic=yes
link=timeout …
RAYNU-V-R640-BOOT-OK
reuse (armed post-EBS)
idle listening on 10.99.99.116:8443
CURL NOW → http://10.99.99.116:8443/  (native BCM5720; SNP is dead)
```

No `HOST-NIC TCP accept`, no `HOST-NIC HTTP exchange ok`, no
`RAYNU-V-M7-HOST-NIC-HTTP-OK`. PRE-EBS SNP HTTP (three exchanges on `:3a`)
is **not** E3b.

`reset…` + `fw-magic=yes` closed **CORECLK_RESET without BMCR**. Analog
was already down at the post-EBS census (`cand bmsr=7949`) **before**
chip reset. That EFI skipped the rest of `setup_copper_phy` (no
`phy_addr=` / `apd=` / `mdix=` / PHY id) and still ran `an-restart=`
only after reset.

Next EFI: Linux `pci_save_state` / `pci_restore_state` (64 dwords) + APE
GRC lock around `CORECLK_RESET`; always `setup_copper_phy` before reset
(BMCR still skipped when NCSI); full `tg3_setup_phy(false)` after reset
(`phy_setup=post`); skip AfterBootOk listen when `LSTATUS` is clear
(`skip listen (no LSTATUS; do not curl)`). Do **not** curl unless COM2
prints `link=up`. Do **not** claim `HOST-NIC-HTTP-OK`.
Reject CORECLK-sans-BMCR EFI `1404f055`.

### 2026-08-19 addendum — PCI restore + `phy_setup=post` still `cand bmsr=7949`

PCI-restore EFI (`1216512` bytes, SHA
`ec08c00f8771ee1250501c784b96998cdf8b6ac891fe6259adb6ad2c08007b02`, CI run
`32294654288`, feature `b5ea069`) ran the intended path. Complete COM2:

```
SNP residual MAC=b0:26:28:5c:5a:3a
SNP lease 10.99.99.116/24
WARN — mgmt HTTP accept timeout
post-EBS bring-up (keep analog before guest path)
cand pci=01:00.00 MAC=…:38 bmsr=7949
cand pci=01:00.01 MAC=…:39 bmsr=7949
phy_addr=01 … phy_reset=pre skip (ape-ncsi) … id=0362:5f60
reset…
ape-grc=yes pci-restore=64
fw-magic=yes
phy_setup=post apd=off mdix=yes eee=off an-restart=yes pwrctl=clr
link=timeout bmsr=7949 bmcr=1000 lpa=0000 s1000=0000 mac_status=00400000 cpmu=00004000
try next func
… phy_addr=02 … same timeout
RAYNU-V-R640-BOOT-OK
reuse (armed post-EBS)
WARN — HOST-NIC BCM5720 skip listen (no LSTATUS; do not curl)
```

`ape-grc=yes` / `pci-restore=64` / `phy_setup=post` / skip-listen all
worked. Analog was already down at the post-EBS census. PCI restore and
full post-reset PHY setup are **closed**. Skip-listen is the correct
negative (do **not** curl). PRE-EBS SNP HTTP timed out this boot (no
Mac curl in the 45s window) — that is **not** E3b.

Next EFI peeks host GPHY `BMSR` **during the PRE-EBS SNP window**
(`pre-EBS cand`) after DHCP, before listen. If that peek is already
`7949` while SNP has copper on `:3a`, the cable is not on the host
GPHY (APE/NCSI datapath). If it has `LSTATUS`, EBS is what drops analog.
Do **not** claim `HOST-NIC-HTTP-OK`.
Reject PCI-restore EFI `ec08c00f`.

### 2026-08-19 addendum — PRE-EBS host GPHY already `bmsr=7949`

PRE-EBS BMSR-peek EFI (`1216512` bytes, SHA
`42b42c99199258fceecbdd94cbe30d4359ca88c4f9e92080a4732de0cc81bc71`,
HEAD `15cf084` / feature `58d336b` — **size collides** with `ec08c00f`)
peeked host GPHY **while SNP still had copper**. Complete COM2:

```
SNP residual MAC=b0:26:28:5c:5a:3a
SNP lease 10.99.99.116/24
pre-EBS BMSR peek (SNP live; MDIO read only)
pre-EBS cand pci=01:00.00 MAC=…:38 bmsr=7949
pre-EBS cand pci=01:00.01 MAC=…:39 bmsr=7949
… guest path …
RAYNU-V-R640-BOOT-OK
skip listen (no LSTATUS; do not curl)
```

SNP `:3a` had a lease; both host GPHYs were already `7949` **before EBS**.
BAR0 peeks stay `:38`/`:39`. “EBS killed analog” is **closed**. The cable
SNP used is the APE/NCSI MAC, not host MDIO.

**Do not take the PHY from APE.** iDRAC Shared LOM / NCSI rides that analog;
`BMCR_RESET` + dropping `APE_HOST_BEHAV_NO_PHYLOCK` can knock iDRAC off
the LOM. Linux `tg3` keeps `NO_PHYLOCK` for `APE_HAS_NCSI`. This branch
keeps `ape-nophylock=yes` / `phy_reset=pre skip (ape-ncsi)` /
`keep-ape-phy=yes`.

**iDRAC-safe path (locked 2026-08-19):** Dedicated iDRAC NIC + host LOM
jack — [`r640_idrac_dedicated.md`](../../runbooks/r640_idrac_dedicated.md).
Cable the dedicated iDRAC RJ45 **before** switching NIC Selection off Shared.
Plug host mgmt into the unused LOM jack (`01:00.0` / `:38` = Ubuntu `eno3`).
Picker prefers live `LSTATUS` over APE MAC `:3a`. Station is that live BAR0
MAC, not SNP `:3a`. Keep `ape-nophylock=yes`.
Do **not** take the PHY. Do **not** curl unless `link=up`. Do **not** claim
`HOST-NIC-HTTP-OK`.
Reject peek EFI `42b42c99` (and `ec08c00f`). **Do not flash take-PHY**
(`fb96cdb` / `ape-nophylock=no`).

Preserve kit: `releases/v0.1.0-adr013-baseline`.

### 2026-08-19 addendum — live LOM `:38` `link=up`; skip-CORECLK no native accept

Live-LOM station EFI (`1217024` bytes, SHA
`26573eb1e02973e2904e30f344269f324c7e78a7f342551e8357194a6e66f713`,
HEAD `f68f61f` / feature `be6bed5`) bound **host GPHY `:38`**. Complete COM2:

```
SNP residual MAC=b0:26:28:5c:5a:38
SNP lease 10.99.99.144/24
pre-EBS cand pci=01:00.00 MAC=…:38 bmsr=796d
pre-EBS cand pci=01:00.01 MAC=…:39 bmsr=7949
SNP HTTP exchange ok
RAYNU-V-M7-UEFI-HTTP-OK
post-EBS bring-up (keep analog before guest path)
cand pci=01:00.00 MAC=…:38 bmsr=796d
try pci=01:00.00 … link-up BMSR
station live LOM MAC=b0:26:28:5c:5a:38
ape-nophylock=yes keep-ape-phy=yes
inherit SNP PHY (skip CORECLK_RESET)
link=up speed=1000 duplex=full
rings armed (poll-mode, MSI-X off)
RAYNU-V-R640-BOOT-OK
reuse (armed post-EBS)
idle listening on 10.99.99.144:8443
CURL NOW → http://10.99.99.144:8443/  (native BCM5720; SNP is dead)
```

No `HOST-NIC TCP accept`, no `HOST-NIC HTTP exchange ok`, no
`RAYNU-V-M7-HOST-NIC-HTTP-OK`. PRE-EBS SNP HTTP on `:38` is **not** E3b.

Wrong-port is **closed** (`:38` / `bmsr=796d` / `link=up`). Skipping
`CORECLK_RESET` because analog was live left UNDI RX/TX RISC on firmware
rings. Linux `tg3_open` always `tg3_chip_reset`. Next EFI: inherit analog
(skip BMCR / AN restart) **and** `CORECLK_RESET` for DMA; COM2
`poll rx_prod=`. Keep `ape-nophylock=yes`. Do **not** take the PHY.
Do **not** claim `HOST-NIC-HTTP-OK`. Reject skip-CORECLK EFI `26573eb1`.

### 2026-08-20 addendum — CORECLK DMA up; RX ring wrap replay

CORECLK-for-DMA EFI (HEAD `d598701` / feature `de52aaf`, artifact
`9388027630`) ran the intended path. Complete COM2:

```
inherit SNP analog; CORECLK_RESET for DMA (skip BMCR)
reset… ape-grc=yes pci-restore=64
fw-magic=yes
link=up speed=1000 duplex=full
rings armed
idle listening on 10.99.99.144:8443
poll rx_prod=7 rx_cons=0 tx_cons=0 rx_ok=0 rx_drop=0
poll rx_prod=28 rx_cons=28 tx_cons=0 rx_ok=28
poll rx_prod=13 rx_cons=13 tx_cons=0 rx_ok=65549 rx_drop=0
… rx_ok 131086, 196610, 262147, 327709 …
```

`rx_prod` stayed **0..31**. Unmasked software consumer compared 32 against 13
and replayed stale return BDs (~65536 per 5s). `tx_cons=0`. No
`HOST-NIC TCP accept`. CORECLK DMA is **closed**. Next EFI: Linux `tg3`
`rx_ret_ring_mask` (`ring_idx` / `RING_MASK`). Keep `ape-nophylock=yes`.
Do **not** take the PHY. Do **not** claim `HOST-NIC-HTTP-OK`.
Reject skip-CORECLK `26573eb1`. Do not flash take-PHY.

### 2026-08-20 addendum — RX ring wrap closed; tx_prod still 0

Ring-mask EFI (HEAD `7b71cf2` / feature `fc7ed70`) closed wrap replay.
Complete COM2 listen snippet:

```
idle listening on 10.99.99.144:8443
CURL NOW → http://10.99.99.144:8443/  (native BCM5720; SNP is dead)
poll rx_prod=7 rx_cons=0 tx_prod=0 tx_cons=0 rx_ok=0 rx_drop=0
poll rx_prod=13 rx_cons=13 tx_prod=0 tx_cons=0 rx_ok=13
poll rx_prod=24 rx_cons=24 tx_prod=0 tx_cons=0 rx_ok=24
poll rx_prod=0 rx_cons=0 tx_prod=0 tx_cons=0 rx_ok=32
… rx_ok=70 rx_drop=0; wrap 24→0; tx_prod=0 the whole window
```

`rx_ok` tracks real LAN (~2 pps after drain). Guest path still green.
PRE-EBS SNP HTTP timed out this boot (curl after native `CURL NOW`).
No `HOST-NIC TCP accept`. Next EFI: Linux LE `GRC_MODE_BSWAP_DATA`
(`grc=bswap+wswap`) so frame DMA is not word-swapped (ethertype at
offset 12). COM2 dumps first RX `to=` / `etype=` / `hw=` / `n=`.
smoltcp `Checksum::Tx` on BCM5720 only (fill TX, do not require RX
csum). Keep `ape-nophylock=yes`. Do **not** take the PHY. Do **not**
claim `HOST-NIC-HTTP-OK`. Reject skip-CORECLK `26573eb1`.


