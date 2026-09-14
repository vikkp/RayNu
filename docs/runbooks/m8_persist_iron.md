# Runbook — M8.0 iron DurableLun (PCI pick → post-EBS I/O → Phase B attach)

**Iron marker:** `RAYNU-V-M8-DISK-PERSIST-OK` (Force Off / reboot **RayNu-V** — **not this slice**)  
**Nested marker:** `RAYNU-V-M8-DISK-PERSIST-NESTED-OK` (QEMU File RAM — **not this runbook**)  
**Host marker:** `RAYNU-V-M8-DISK-PERSIST-HOST-OK` (cargo — **not this runbook**)  
**Never print:** `RAYNU-V-M7-ISO-INSTALL-OK`  
**M8.0 known-good flash:** GitHub Latest [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed). Do not F11 a persist prototype until COM2 shows I/O ready + virtio on the LUN.

Nested QEMU ≠ R640. QEMU NVMe/USB ≠ Intel PCH xHCI. Nested File RAM (`M8_PERSIST_IMG` `share=on`) does not exist on iron.

---

## Hardware (required before F11)

| Role | What | Notes |
|------|--------|--------|
| ESP | Cruzer / UDisk 2–8 GiB with alpine-extended | Boot stick. Mapper **refuses** this window as a LUN. |
| LUN | NVMe class `01:08`, **or** USB ≥ 1 GiB **outside** 2–8 GiB | 16 GiB+ stick is the usual USB pick. 1–2 GiB also eligible. never PERC. |
| Standing boot | Ubuntu on PERC | Do **not** format PERC. One-time F11 the ESP only. |

Without a LUN, COM2 prints `durable LUN none (leftover DRAM)` / `skip PERC` / `skip ESP Cruzer` plus `need NVMe class 01:08 or USB ≥1GiB outside Cruzer 2-8GiB`. Phase B then installs to leftover DRAM (dies on Force Off).

---

## COM2 needles (this slice)

Open iDRAC **SOL `console com2` first**.

### 1. PCI pick

Expect storage-class lines, then a pick or a reject:

- `durable LUN pci … nvme` → pick NVMe (`01:08`)
- `skip PERC` / `skip AHCI` / `skip ICH9 SATA` / `skip IDE` — not LUNs
- `durable LUN xhci … 8086:a1af` (Lewisburg) — USB I/O after EBS
- `durable LUN picked nvme|usb …`
- or `durable LUN none (leftover DRAM)` + the need-media line

### 2. Post-EBS I/O

After ExitBootServices:

- `durable LUN nvme I/O ready bytes=…` **or**
- `durable LUN usb I/O ready bytes=…` (ESP Cruzer window skipped; next port)
- `xhci cap caplen=… slots=… ports=… scratch=…/64 csz=` on **every** USB attempt (before PORTSC)
- `xhci cap … scan=… p1=0x…` (first 16 PORTSC words) then `xhci ccs pN=0x…` for every CCS port (Lewisburg 26 ports, not a 16-port walk)
- `xhci enum pN sc=0x… speed= usb3= csz= cmd= cmpl=0x…` on Enum fail (cmd `1` reset `2` slot `3` addr `4` desc `5` cfg). `cmpl=0x11` is Parameter Error. `cmpl=0xff` + `timeout` means the command never completed (not a real CC). Iron `3473a0b9` `cmpl=0x40` was **MaxSlots**, not CC 64.
- After Address Device: `xhci enum pN vid= did= class= proto=` so UDisk vs Toshiba is named. `xhci bot pN iface=08/06/50 ep_out= ep_in=` is the config-descriptor parse (MSC 08/06/50 lives on the interface, not the device descriptor). `uas-no-bot` means MSC protocol 0x62 only (BOT 0x50 missing). `hub skip` / `err=9` is USB class 09 (not BOT) and must **not** clobber a prior BOT `cmpl`. GET_DESC retries print `desc retry n=`. `xhci hcrst` means the controller was halted and rebuilt on our rings (not UEFI BlockIo).

Fail: `nvme I/O fail` / `usb I/O fail err=` + leftover DRAM. Intel PCH scratchpad ≤ 64 pages; more is `err=1` Cap + `over-budget`. `mmio-dead` means CAPLENGTH was 0 or `0xffffffff` (BAR unread). Iron `ac3b92cd` (2026-09-14): Lewisburg `8086:a1af` named, then `usb I/O fail err=1 bar=0x92b00000 portsc=0 cmpl=0` — Cap before PORTSC; 16-page budget. Cap EFI `67db8a68` (`build: sha=50b5d8bb29fb`): `scratch=34/64` then Polling `0x000206e1`. Enum EFI `53d1f7f7` (`build: sha=c6bdd671b330`): HS U0 `sc=0x00000e03` PED=1 speed=3 then `xhci enum p10/p11/p14 cmd=3 cmpl=0x11` — Address Device Parameter Error (Slot Context DW1 Number of Ports was the port number; Hub=0). Slot DW1 EFI `3473a0b9`: Parameter Error gone; p10 Address Device then p11/p14 Enable Slot printed `cmpl=0x40` (stale MaxSlots after Address Device never posted; command ring stayed busy so later CCS ports never got a fair try). Guest `vda` was 1.07 GiB leftover DRAM. Toshiba unused. Everest loop (`ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → `login:`) is **not** persist. Do not Force Off that guest expecting `RAYNU-V-M8-DISK-PERSIST-OK`. Do not setup-disk on leftover. Recover EFI `ba0ab8ab` / COM2 `build: sha=568e869632d6`: p10 Address Device `cmpl=0xff timeout`, p11 Toshiba `0480:a004` `usb I/O ready bytes=320072933376`, leftover skipped, peek `efi=read-fail usb_err=3`, Alpine `I/O error` on **both** `vda` and `vdb` (`last_st=0x1`). Enum recover is proven; BOT READ + ISO isolation were not. Isolation EFI `06ca0f95` / COM2 `06ca0f952d69`: product ISO virtio never uses DurableLun — Alpine `Mounting boot media: ok.` `last_st=0x0`, leftover `setup-disk` → `ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → second Linux `root=UUID=` → `login:`. **ISO isolation proven. Leftover DRAM Everest ≠ persist.** USB LUN missed: p11 GET_DESC `cmd=4 cmpl=0`, p14 hub `1604:10c0` class 09 `err=4`. This EFI retries GET_DESC and skips hubs (`err=9`) so a later CCS BOT is not hidden. DESC retry EFI `6c278e85` / COM2 `6c278e859b89`: p11 Toshiba `0480:a004` GET_DESC succeeded (`usb I/O ready bytes=320072933376 lba=512`), leftover skipped, ISO `vdb` `last_st=0x0` `Mounting boot media: ok.` — **enum + ISO isolation proven.** Peek `read-fail usb_err=8 cmpl=0xff` at `off=0x200` (BOT data-stage Transfer Event timeout); guest `vda` IOERR; `sfdisk: cannot open /dev/vda: I/O error`; stall overlay looped `PIT paced n=6402`. INQUIRY/TUR/CAPACITY (≤36 B) are not a 512-byte READ. BOT READ EFI `73dc4d2e` / COM2 `73dc4d2e6eaa`: p11 Toshiba named, p14 `hub skip (err=9)`, then `usb I/O fail err=8 bot=csw cmpl=0 portsc=…0e03` (p14 hub stamped `cmpl=0` over the p11 CSW). **No** `usb I/O ready`. Leftover DRAM Everest completed. **Leftover DRAM Everest ≠ persist.** START STOP + ISP-on-every-IN + auto EP-reset regressed vs `6c278e85`. CSW EFI `68e16633` / COM2 `68e1663317a1`: p11 Toshiba named + p14 hub skip lived, then `usb I/O fail err=8 bot=cbw scsi=tur cmpl=0 portsc=0x0000000e00000e03`. Packed encoding is port **14** + PORTSC `0xe03`, not “LUN stuck on p10”. Scan-time p10/p11/p14 were all Polling `0x206e1` (HCRST already dropped PED). `scsi=tur` was the READ-probe recovery TUR stamped over the READ fail. Device `class=0x00 proto=0x00` is the device descriptor; BOT already parses config `08/06/50` + bulk EP addresses (`parse_bot_eps`). Full HCRST + own DCBAA/rings/scratch already runs after EBS (p11 GET_DESC on the same rings falsifies “cycle bit doesn’t match”). This EFI: always port-reset CCS (never inherit U0); hub skip never stamps `cmpl`/`portsc`; no TUR in bring-up or READ retry; COM2 prints `xhci hcrst` + `xhci bot pN iface=08/06/50 ep_out= ep_in=`; stamp BOT fail on the **current** port. Need COM2 Toshiba `0480:a004` + `usb I/O ready` **without** peek `read-fail` / `usb_err=8` and Alpine `vda` **298 GiB** (not 1.07 GiB leftover) with a readable partition table. If BOT still fails, COM2 must keep the **p11** `cmpl` (not `cmpl=0` from p14) and `scsi=read|capacity|inquiry` (not a recovery TUR). Do not setup-disk until virtio READ on the 298 GiB LUN works.

### 3. Phase B attach

Same Everest loop (`HOST-NIC-HTTP-OK` → SPA Start → install → F7 `DISK-BOOT-OK`), but virtio must be the LUN:

```
leftover install disk skip durable LUN
virtio-blk install disk bytes=… keep=0 (durable LUN nvme)
```

or `… (durable LUN usb)`.

**Not ready:** `leftover install disk hpa=… (leftover DRAM; durable LUN not ready)` then `virtio-blk … (leftover DRAM; durable LUN not ready)`. Do not Force Off expecting persist.

---

## Do not F11 until

1. A LUN the mapper will pick is seated.
2. COM2 shows `I/O ready` **and** `virtio-blk … (durable LUN nvme|usb)`.
3. Then one-time F11 this EFI (not `ce3d8a09` File-RAM). Ubuntu-on-PERC stays standing boot.

Force Off / reboot RayNu-V (not guest F7) is the iron close (`RAYNU-V-M8-DISK-PERSIST-OK`). That is **after** 1–3. If the prototype misbehaves, re-flash [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed).

## Not this

- Nested `MODE=full` / File RAM
- M8.1 TLS
- Formatting the PERC
- Printing `ISO-INSTALL-OK` from host/CI/nested
