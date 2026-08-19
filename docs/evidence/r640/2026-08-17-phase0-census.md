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

Preserve kit: `releases/v0.1.0-adr013-baseline`.
