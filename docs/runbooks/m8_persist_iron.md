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
- `xhci enum pN sc=0x… speed= usb3= csz= cmd= cmpl=0x…` on Enum fail (cmd `1` reset `2` slot `3` addr `4` desc `5` cfg/`6` SET_CONFIG). `xhci setcfg pN val=` means SET_CONFIGURATION completed before Configure Endpoint. `cmpl=0x11` is Parameter Error. `cmpl=0xff` + `timeout` means the command never completed (not a real CC). Iron `3473a0b9` `cmpl=0x40` was **MaxSlots**, not CC 64. COM2 fail lines still print `bot=cbw|data|csw` and `scsi=`. `BULK_SPINS` bounds bulk Transfer Event waits. Cap line prints `hcc1=` next to `csz=`.
- After Address Device: `xhci enum pN vid= did= class= proto=` so UDisk vs Toshiba is named. `xhci eval pN mps=` is Evaluate Context EP0 MPS (cmd type 13). `xhci bot pN iface=08/06/50 ep_out= ep_in=` is the config-descriptor parse (MSC 08/06/50 lives on the interface, not the device descriptor). `xhci cfg-skip pN toshiba` means GET_CONFIG failed and the iron `96024edc` Toshiba BOT layout was used. `xhci maxlun pN val=` is Get Max LUN after Configure Endpoint (stall is not fatal). `uas-no-bot` means MSC protocol 0x62 only (BOT 0x50 missing). `hub skip` / `err=9` is USB class 09 (not BOT) and must **not** clobber a prior BOT `cmpl`. GET_DESC retries print `desc retry n=`. `xhci hcrst` means the controller was halted and rebuilt on our rings (not UEFI BlockIo).

Fail: `nvme I/O fail` / `usb I/O fail err=` + leftover DRAM. Intel PCH scratchpad ≤ 64 pages; more is `err=1` Cap + `over-budget`. `mmio-dead` means CAPLENGTH was 0 or `0xffffffff` (BAR unread). Iron ep0-eval (2026-09-15): leftover DRAM. p11 Toshiba `0480:a004` + `xhci eval mps=64` + `xhci bot ep_out=2 ep_in=1` + `setcfg val=1` then `usb I/O fail err=8 bot=cbw scsi=capacity cmpl=0` `portsc=…0b00000e03` (p11). Enum/GET_CONFIG/SET_CONFIG lived. `cmpl=0` is p14 Disable Slot clobber. **Leftover DRAM ≠ persist.** This EFI: GET_MAX_LUN after CONFIG_EP (`xhci maxlun`); required INQUIRY; 80M settle; recover_enum holds BOT `cmpl`. Do not Force Off leftover guest. Iron cfg-desc (2026-09-15, EFI `b5338458`): leftover DRAM. p11/p14 GET_DEVICE retry `n=1,n=2` then `cmd=4 cmpl=0xff` — **no** `vid=` (CH on SETUP+DATA broke Lewisburg EP0). p14 fail `portsc=0xe00000e03`. **Leftover DRAM ≠ persist.** Iron ep0-stop (2026-09-15): leftover DRAM. p11 Toshiba `0480:a004` device desc lived (`desc retry n=1`) then 9-byte config GET_DESC `cmd=4 cmpl=0xff` `bot=? scsi=?` `cmpl=0x304ff`. A CH revert returns here. This EFI: unchained control TDs; DATA completes immediately then Stop leftover STATUS; Evaluate Context EP0 MPS (`xhci eval`); one-packet GET_CONFIG 64; Toshiba `0480:a004` BOT fallback (`xhci cfg-skip`, `ep_out=2 ep_in=1` from `96024edc`). Do not Force Off leftover guest. Iron stop-ep (2026-09-15): leftover DRAM. p11 Toshiba `0480:a004` device desc lived (`desc retry n=1`) then config GET_DESC `cmd=4 cmpl=0xff` `bot=? scsi=?` `cmpl=0x304ff`. BOT never started — timeout retries stacked EP0 TRBs (Reset EP0 skipped on timeout). **Leftover DRAM ≠ persist.** This EFI: Stop Endpoint on EP0 GET_DESC timeout, then retry. Do not Force Off leftover guest. Iron first-cbw (2026-09-15): leftover DRAM. p11 Toshiba `0480:a004` SET_CONFIG held then `usb I/O fail err=8 cmpl=0x13 bot=cbw scsi=capacity`. `cmpl=0x13` is Context State Error — Reset Endpoint on a **Running** bulk EP after CBW timeout (xHCI 4.6.8 is Halted-only). **Leftover DRAM ≠ persist.** This EFI: Stop Endpoint then Set TR Dequeue; Reset Endpoint only if Stop fails; recover commands do not clobber BOT `cmpl`. Do not Force Off leftover guest. Iron cap-csw (2026-09-15, skip-INQUIRY EFI): leftover DRAM. p11 Toshiba `0480:a004` SET_CONFIG held then `usb I/O fail err=8 cmpl=0xff bot=cbw scsi=capacity`. First bulk OUT after SET_CONFIG timed out — skipped/ignored INQUIRY was the HDD warmup + delay. **Leftover DRAM ≠ persist.** This EFI: settle + waited INQUIRY (`recover_pipes` on fail, never `let _ =`); then CAPACITY retry. Do not Force Off leftover guest. Iron `0780df21` (2026-09-15): leftover Everest (`keep=0` `vda` 1.07 GiB `root=UUID=1bc8b57d-…`). Coexist `HOST-NIC-HTTP-OK` `10.99.99.145:8443` → SPA Start → `ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → login. USB p11 Toshiba `0480:a004` SET_CONFIG held then `usb I/O fail err=8 cmpl=0xff bot=csw scsi=capacity` (CAPACITY CSW timed out — ignored INQUIRY can leave a pending IN TRB). **Leftover DRAM Everest ≠ persist.** Do not Force Off leftover guest. Iron `6ba076cc` (2026-09-15): leftover Everest (`keep=0` `vda` 1.07 GiB). p11 Toshiba `0480:a004` `xhci bot p11 iface=08/06/50 ep_out=2 ep_in=1` + `xhci setcfg p11 val=1` then `usb I/O fail err=8 bar=… portsc=…0b00000e03 cmpl=0xff bot=cbw scsi=read` (CAPACITY ok; native READ CBW timed out — leftover events no longer false-complete). SNP DHCP failed (`SNP DHCP failed (no lease)`) → `no parked SNP` → analog/coexist skip → Phase B idle `SPA Start needs HTTP`. Native BCM5720 DHCP (`HOST-NIC BCM5720 DHCP discover` / `DHCP lease` / `CURL NOW` / `coexist listening`); analog always post-EBS even without SNP lease; CBW timeout `cmpl=0xff` `recover_pipes` so retries do not stack TRBs. Iron `cbd9bf47` (2026-09-15): p10 Address Device `cmd=3 cmpl=0xff timeout`; p11 Toshiba `0480:a004` device desc then config GET_DESC `cmd=4 cmpl=0xff` (`bot=? scsi=?`); leftover Everest `vda` 1.07 GiB `root=UUID=35bfa02f-…` — **not persist**. Wait GET_DESC STATUS TRB (not any Transfer Event); `DESC_SPINS`; do not Reset EP0 on timeout. Iron `ac3b92cd` (2026-09-14): Lewisburg `8086:a1af` named, then `usb I/O fail err=1 bar=0x92b00000 portsc=0 cmpl=0` — Cap before PORTSC; 16-page budget. Cap EFI `67db8a68` (`build: sha=50b5d8bb29fb`): `scratch=34/64` then Polling `0x000206e1`. Enum EFI `53d1f7f7` (`build: sha=c6bdd671b330`): HS U0 `sc=0x00000e03` PED=1 speed=3 then `xhci enum p10/p11/p14 cmd=3 cmpl=0x11` — Address Device Parameter Error (Slot Context DW1 Number of Ports was the port number; Hub=0). Slot DW1 EFI `3473a0b9`: Parameter Error gone; p10 Address Device then p11/p14 Enable Slot printed `cmpl=0x40` (stale MaxSlots after Address Device never posted; command ring stayed busy so later CCS ports never got a fair try). Guest `vda` was 1.07 GiB leftover DRAM. Toshiba unused. Everest loop (`ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → `login:`) is **not** persist. Do not Force Off that guest expecting `RAYNU-V-M8-DISK-PERSIST-OK`. Do not setup-disk on leftover. Recover EFI `ba0ab8ab` / COM2 `build: sha=568e869632d6`: p10 Address Device `cmpl=0xff timeout`, p11 Toshiba `0480:a004` `usb I/O ready bytes=320072933376`, leftover skipped, peek `efi=read-fail usb_err=3`, Alpine `I/O error` on **both** `vda` and `vdb` (`last_st=0x1`). Enum recover is proven; BOT READ + ISO isolation were not. Isolation EFI `06ca0f95` / COM2 `06ca0f952d69`: product ISO virtio never uses DurableLun — Alpine `Mounting boot media: ok.` `last_st=0x0`, leftover `setup-disk` → `ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → second Linux `root=UUID=` → `login:`. **ISO isolation proven. Leftover DRAM Everest ≠ persist.** USB LUN missed: p11 GET_DESC `cmd=4 cmpl=0`, p14 hub `1604:10c0` class 09 `err=4`. This EFI retries GET_DESC and skips hubs (`err=9`) so a later CCS BOT is not hidden. DESC retry EFI `6c278e85` / COM2 `6c278e859b89`: p11 Toshiba `0480:a004` GET_DESC succeeded (`usb I/O ready bytes=320072933376 lba=512`), leftover skipped, ISO `vdb` `last_st=0x0` `Mounting boot media: ok.` — **enum + ISO isolation proven.** Peek `read-fail usb_err=8 cmpl=0xff` at `off=0x200` (BOT data-stage Transfer Event timeout); guest `vda` IOERR; `sfdisk: cannot open /dev/vda: I/O error`; stall overlay looped `PIT paced n=6402`. INQUIRY/TUR/CAPACITY (≤36 B) are not a 512-byte READ. BOT READ EFI `73dc4d2e` / COM2 `73dc4d2e6eaa`: p11 Toshiba named, p14 `hub skip (err=9)`, then `usb I/O fail err=8 bot=csw cmpl=0 portsc=…0e03` (p14 hub stamped `cmpl=0` over the p11 CSW). **No** `usb I/O ready`. Leftover DRAM Everest completed. **Leftover DRAM Everest ≠ persist.** START STOP + ISP-on-every-IN + auto EP-reset regressed vs `6c278e85`. CSW EFI `68e16633` / COM2 `68e1663317a1`: p11 Toshiba named + p14 hub skip lived, then `usb I/O fail err=8 bot=cbw scsi=tur cmpl=0 portsc=0x0000000e00000e03`. Packed encoding is port **14** + PORTSC `0xe03`, not “LUN stuck on p10”. Scan-time p10/p11/p14 were all Polling `0x206e1` (HCRST already dropped PED). `scsi=tur` was the READ-probe recovery TUR stamped over the READ fail. Device `class=0x00 proto=0x00` is the device descriptor; BOT already parses config `08/06/50` + bulk EP addresses (`parse_bot_eps`). Full HCRST + own DCBAA/rings/scratch already runs after EBS (p11 GET_DESC on the same rings falsifies “cycle bit doesn’t match”). This EFI: always port-reset CCS (never inherit U0); hub skip never stamps `cmpl`/`portsc`; no TUR in bring-up or READ retry; COM2 prints `xhci hcrst` + `xhci bot pN iface=08/06/50 ep_out= ep_in=`; stamp BOT fail on the **current** port. Port EFI `96024edc` / COM2 `build: sha=96024edcf70f` (two boots, SET_CONFIG-first **not** flashed): (1) p11 Toshiba `0480:a004` + `xhci bot p11 iface=08/06/50 ep_out=2 ep_in=1 cfg=1` then `xhci enum p11 cmd=5 cmpl=0` (`err=8` SET_CONFIG after CONFIG_EP); fail `cmpl=0x0e801a40 bot=?` is p14 `reset_port` reset_diag. (2) Same SHA: **no** `cmd=5` line (SET_CONFIG+CONFIG_EP held) then `usb I/O fail err=8 portsc=…0e00000e03 cmpl=0 bot=cbw scsi=read` — CAPACITY (8 B IN) completed; native READ died on the CBW OUT. Packed `portsc` is still p14 on this unflashed EFI. Leftover DRAM Everest (`ISO-INSTALL-OK` → F7 `DISK-BOOT-OK` → `root=UUID=45213e85-…` → `login:`) is **not** persist. This EFI: SET_CONFIGURATION before Configure Endpoint (`xhci setcfg` / cmd `6`); later-port `reset_port` does not clobber Enum/Xfer; Transfer Event must match slot+DCI (`consume_transfer`; leftover CSW IN DCI 3 must not retire CBW OUT DCI 4); dedicated IN bounce; no Reset Endpoint on CBW fail; settle after CAPACITY. Need COM2 Toshiba `0480:a004` + `usb I/O ready` **without** peek `read-fail` / `usb_err=8` and Alpine `vda` **298 GiB** (not 1.07 GiB leftover) with a readable partition table. If BOT still fails, COM2 must keep the **p11** `cmpl` (not `cmpl=0x0e801a40` from p14) and `scsi=read|capacity|inquiry` (not `bot=?`). Do not setup-disk until virtio READ on the 298 GiB LUN works.

### 3. Phase B attach

SPA Start still needs HTTP. If PRE-EBS SNP DHCP failed (`SNP DHCP failed (no lease)`), this EFI still brings analog up after EBS and runs native BCM5720 DHCP at coexist. Expect `HOST-NIC BCM5720 DHCP discover` then `HOST-NIC BCM5720 DHCP lease` then `HOST-NIC coexist listening` / `CURL NOW`. Fail: `native DHCP failed` then `SPA Start needs HTTP` (same idle hang as `6ba076cc`).

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
