# Runbook — F7 product ISO on R640 (4 GB Cruzer)

**Iron close (COM2 only):** `RAYNU-V-M7-ISO-INSTALL-OK` — **seen on R640 2026-09-10** (`59ac070`, run `34425781629`, evidence [`2026-09-10-59ac070-iso-install-ok.md`](../evidence/r640/2026-09-10-59ac070-iso-install-ok.md); seen again on `975f8fc` and `56a3ffd`)  
**Iron reboot-to-disk (COM2 only):** `RAYNU-V-RAYNU-F-DISK-BOOT-OK` + second `Linux version` with `root=UUID=` from `vda` + `login:` — **seen on R640 2026-09-10** (`56a3ffd`, run `34480107961`, evidence [`2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md`](../evidence/r640/2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md)). **Phase A is closed on iron.** What remains is Phase B (section 5): the SPA/REST path starts *this* installed disk instead of the SHELL stub  
**EFI pin (Phase A reference):** `56a3ffd` / run `34480107961` — the UEFI-release artifact that closed Phase A. Later green artifacts of `cursor/pit-during-apk-b7a8` must reproduce the same COM2 sequence before they replace it. `--run` takes a numeric GitHub Actions id with **no** angle brackets. Check the second COM2 line: `build: sha=` must match the run's commit. Superseded: `975f8fc` run `34474850361` (**F7 relaunch worked on iron**: `guest reset requested src=kbc n=1` → `relaunch after reset` → `GPT ESP lba=2048 sectors=98304 part=1` → `disk whole-disk path` → `image=DISK-BOOTX64` → `CONOUT-OK` → GRUB 2.12 menu `*Alpine Linux v3.21, with Linux lts … executed automatically in 2s` — then `stop exit-cap exits=1048577 svc=492778 … blk_rd=59` and the hold: GRUB's menu loop polls `ReadKeyStroke` + serial LSR with no idle, ~2 exits/µs on the R640, so the 1 M exit cap fired before GRUB's own 2 s timeout; install itself re-ran clean, marker printed again) · `59ac070` run `34425781629` (**installed on iron**: apk 25 packages in 0.5 s, `setup-disk -m sys /dev/vda` GPT + grub x86_64-efi + initramfs, `Installation finished. No error reported.`, **`RAYNU-V-M7-ISO-INSTALL-OK` printed on COM2** — then `reboot` src=kbc → `RayNu-F relaunch after reset (F7)` → `launch failed: VMCLEAR/VMPTRLD (F2b)` → leave_to_e4 hold: `raynu_f_reset_relaunch` built a 79.7 KiB `FirmwareState::new()` on the 16 KiB guest-UEFI host stack and overwrote the VMCS revision dword one frame below; installs, cannot reboot to disk) · `916af96` run `34420783162` (UART THRE chain telemetry lived and **named the link**: every counted link agreed — `take4` ≈ `iir` THRE ≈ `lsr_thre` ON 1:1, PIC `irr/imr/isr` sane, `etbei` set — but `pend=0` even with `ring=0` because `com2_lsr=0x0` on every heartbeat: guest THRE was host COM2 THRE **and** empty ring, sampled only at ~15 idle exits/s, each draining one byte; `ring=2048` full from our own per-hit `virtio MMIO gpa=` lines; `apk` still in `n_tty_write`) · `8b6ed1a` run `34415711199` (UART THRE level until stop_tx lived, COM2 identical to the probe: `apk` still in `n_tty_write → wait_woken`, text still 16–32 bytes per SysRq IRQ 4, stall dump `n=1373`, `PIT paced n=1375` for minutes; the level model alone changed nothing visible) · `f229d14` run `34354322953` (probe answered: `apk` in `n_tty_write → wait_woken`, 16 bytes flushed per SysRq IRQ 4; lost 16550 THRE interrupt) · `09b5842` run `34302053558` (virtio stall dump PIT paced lived: no soft lockup, `PIT paced n=1375` for minutes, apk never kicked again; rings `last==used==avail==live`) · `2b3cc1e` run `34300317331` (virtio stall dump PIT hold lived: apk dump `disk_last=1100 iso_last=1057 live==last n=1373` then INTx + `PIT hold n=1375` then unpaced IRQ0 `soft lockup` in `handle_softirqs` 26s/52s/78s and `reason=0x1e`; watchdog proved jiffies already move) · `b8ac361` run `34297632324` (virtio stall dump notify reset lived: usbdelay dump PIT-only; apk dump `disk_last=1162 iso_last=1111 live==last n=1370` then INTx + ISR `n=1371/1372` and no further `0x300`; harvest no-op) · `6d0c58a` run `34292570282` (virtio stall dump INTx lived then dump/ISR flood: ISR ACK `off=0x100` re-armed dump; `n=+2` with no `0x300`) · `ba5bf8f` run `34290078274` (virtio stall dump PIT lived: `disk_live==disk_last` `iso_live==iso_last` then still apk freeze `n=1373`) · `a580299` run `34227607779` (shared INTx hold lived past `n=1345`; second stall dump `last==used==avail` `isr=0` `kick=0` `n=1373` during `Installing packages`; empty rings) · `34968f7` run `34224368343` (used.idx lived; usbdelay stall dump `disk_last=1048 iso_last=542 disk_used=1048 iso_used=542 disk_isr=0 iso_isr=0 n=1118` then apk freeze at `n=1345` after ISO notify `n=1281` + disk ISR ACK; stall dump one-shot fired too early) · `e717fb4` run `34220740109` (DATA_SEGS=128 still froze at apk `n=1345`; do not wait) · `6cfabee` run `34218742196` (`linux PIC IRQ11 yield until mount` lived, then apk freeze at virtio `n=1345`; 3:1 is not the bottleneck) · `c2cd099` run `34172103709` (`Mounting boot media: ok.` then apk freeze at virtio `n=1345`; stopping yield after `media: ok` did not move apk) · `6692898` run `34170268004` (`Mounting boot media: ok.` then apk freeze at virtio `n=1345`; FLUSH drain did not help; yield starved PIC 11) · `9652262` run `34149809660` (`Mounting boot media: ok.` then apk freeze at virtio `n=1345` after disk ISR ACK; yield-PIT unblocked `usbdelay=30`; drain ignored FLUSH) · `fc3053a` run `34148045050` (`linux PIC IRQ0` + `linux virtio PIC 11` then freeze at mount virtio `n=1089` / `usbdelay=30`; level INTx starved IRQ0) · `fcac0cd` run `34145652767` (`linux virtio INTx reassert` then apk freeze `n=1345`; eight `inject vec=0x30` is Linux PIC IRQ0, not NMI; PIC 11 log looked for 0x2b) · `0a9b552` run `34141401594` (leftover GSI 2 was a no-op on RayNu-F; same apk freeze `n=1345`) · `c61942b` run `34135448354` (DRIVER_OK + `linux PIT resume paced`, then apk freeze `n=1345` with **no** `linux PIC IRQ0`) · `896424f` run `34131274237` (DRIVER_OK then `virtblk_probe` `iowrite8` IRQ0 storm / soft lockup) · `69102aa` run `34080595540` (MADT live; virtio_pci_probe `vp_set_status` IRQ0 storm / soft lockup) · `c815ccc` run `34078335291` (PIT hold from firmware queue-arm; APIC MADT then I/O `reason=0x1e` hold) · `20e8b70` run `34076175624` (PIT-once on HLT; `idle=poll` never HLT; froze at apk `n=1345`) · `63c7e05` run `34074349118` (PIT-once printed, boot media ok, then froze at apk `n=1345`) · `46fd345` run `34069352671` (no PIT-once, stall n=1281) · `55a3602` run `34067879816` (RayNu-F direct, then cap=1 killed Linux WRMSR) · `088ab25` run `33978770315`.  
**Do not flash (for Phase A):** `34474850361` / `975f8fc` (installs, relaunches into the installed GRUB menu, then `stop exit-cap exits=1048577` inside the 2 s countdown — no reboot-to-disk) · `34425781629` / `59ac070` (installs, then F7 relaunch `VMCLEAR/VMPTRLD (F2b)` — no reboot-to-disk) · `34420783162` / `916af96` · `34415711199` / `8b6ed1a` · `34354322953` / `f229d14` · `34302053558` / `09b5842` · `34300317331` / `2b3cc1e` · `34297632324` / `b8ac361` · `34292570282` / `6d0c58a` · `34290078274` / `ba5bf8f` · `34227607779` / `a580299` · `34224368343` / `34968f7` · `34220740109` / `e717fb4` · `34218742196` / `6cfabee` · `34172103709` / `c2cd099` · `34170268004` / `6692898` · `34149809660` / `9652262` · `34148045050` / `fc3053a` · `34145652767` / `fcac0cd` · `34141401594` / `0a9b552` · `34135448354` / `c61942b` · `34131274237` / `896424f` (iron 2026-09-07: `linux virtio DRIVER_OK` then `virtblk_probe` soft lockup then `reason=0x1e`, no `ISO-INSTALL-OK`) · `34080595540` / `69102aa` · `34078335291` / `c815ccc` (MADT then `restore host xcr0 reason=0x1e`) · `34076175624` / `20e8b70` · `34074349118` / `63c7e05` · `34069352671` / `46fd345` · `34067879816` / `55a3602` · `088ab25` for F7 · P0-14 `2b795a0` · parked OVMF pins · PR #231  
**Honesty:** Nested QEMU ≠ R640. Host/CI must never print `RAYNU-V-M7-ISO-INSTALL-OK`.  
**M8.0 iron rollback:** GitHub Latest [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) → `f72b4276` / `--run 34552377351` / EFI SHA256 `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc`. COM2 `build: sha=f72b4276d198`. Do not flash a later persist prototype as the known-good.

Related: [`usb_idrac.md`](usb_idrac.md) · [`iso_install.md`](iso_install.md) ·
[ADR-016](../adr/ADR-016.md) · [ADR-017](../adr/ADR-017.md) ·
nested evidence [`2026-09-05-088ab25-f7-reset-src-kbc.md`](../evidence/nested/2026-09-05-088ab25-f7-reset-src-kbc.md)

---

## What F7 already proved (nested, not iron)

On `raynuvsrv1` nested KVM, alpine-extended under **RayNu-F** (own UEFI tables,
not puppeted OVMF):

1. `setup-disk -m sys -s 0 /dev/vda` on a leftover-DRAM virtio disk (~512 MiB).
2. `Installation is complete. Please reboot.`
3. F7 reset `src=kbc` (i8042 `0x64 <- 0xFE`) → relaunch with disk kept.
4. GPT ESP whole-disk Vendor path → GRUB `hd0` (not `,gpt2` rescue).
5. `RAYNU-V-RAYNU-F-DISK-BOOT-OK` then second Linux `root=UUID=…` on ext4
   `/dev/vda2`, FAT ESP `/dev/vda1`.

That is the **mechanism**. It is not Everest E5.

The 2026-08-16 Cruzer `BOOTED-FROM-DISK` stamp is **extract-boot persist**,
not this distro install.

---

## Why the 4 GB stick matters

| Media | Size | alpine-extended (~994 MiB) |
|-------|------|----------------------------|
| Old Cruzer Micro (front USB 2) | 977.5 MiB | Does not fit |
| LogiLink UDisk (front USB 2, 2026-09-06) | 4026531840 bytes (~3.8 G), serial `General_UDisk-0:0`, `lsusb abcd:1234` | Fits |

`flashcruzer.sh` still defaults to Micro identity (`lsusb 0781:5151`, serial
`200524441218e7503e33`, existing `RAYNUV` + `installdisk.bin`). This UDisk is
unlabeled and is not a Cruzer by model name. First flash uses `--init-new-cruzer`
`--allow-new-serial` `--any-cruzer-usb` (UDisk / LogiLink accepted).

---

## Two closes (do not fuse)

**Phase A — mechanism on iron (this runbook).** Same RayNu-F + alpine-extended
+ F7 reboot-to-disk, driven by the in-firmware serial auto-answer (the nested
1800s harness equivalent). Watch **iDRAC SOL COM2**. BIOS boot order stays
Ubuntu on PERC; **one-time F11** the Cruzer.

**Phase B — operator product.** SPA/REST on `:8443` registers the ISO, creates
a VM, attaches media. Iron SPA today still launches a **SHELL CPUID stub**
(`2b795a0`). Closing Phase A does not make E4 honest until the SPA starts that
installed disk.

TLS, guest VNC, live Redfish, ISO blob upload, Secure Boot, Windows: accepted
residuals. None of them close M7 by themselves.

---

## 0. Before you touch the stick

On `raynuvsrv1` (Ubuntu on the PERC), with the 4 GB Cruzer **already** in
**front USB 2**:

```bash
lsusb | grep -iE 'cruzer|udisk|logilink|abcd:1234'
lsblk -o NAME,MODEL,TRAN,SIZE,LABEL,SERIAL,FSTYPE,TYPE,MOUNTPOINT
```

Want: one USB disk, model `UDisk` or `Cruzer`, ~4 G, **not** PERC `sda`/`sdb`,
**not** the Ubuntu root. Unplug any other USB stick. Never hardcode `/dev/sdc`
as the only check — match model + serial. Never format PERC.

Refresh the launcher from this clone (the `~/projects/raynuv` copy goes stale):

```bash
cd ~/projects/raynu
git fetch origin cursor/e5-stage46-iso-a623
git checkout -B cursor/e5-stage46-iso-a623 origin/cursor/e5-stage46-iso-a623
./tools/flashcruzer.sh --install-launcher
./tools/flashcruzer.sh --self-test
./tools/flash-cruzer-esp.sh --self-test
```

Fetch alpine-extended **onto the Ubuntu disk**, not onto the Cruzer:

```bash
mkdir -p ~/projects/raynuv
curl -fL -o ~/projects/raynuv/alpine-extended-3.21.3-x86_64.iso \
  https://dl-cdn.alpinelinux.org/alpine/v3.21/releases/x86_64/alpine-extended-3.21.3-x86_64.iso
wc -c ~/projects/raynuv/alpine-extended-3.21.3-x86_64.iso
# want 1042284544 (or whatever the current 3.21.3 extended size is; must be ≫ 73728)
```

`alpine-virt` (63 MiB) and `alpine-standard` (245 MiB) do **not** ship
`grub-efi` + `dosfstools`. Do not substitute them.

---

## 1. First flash (unlabeled 4 GB Cruzer)

Pin the last **green** F7 EFI, not the docs-only `fa81f5e` push (M4.8 verus
flake) and not P0-14 `2b795a0`. Replace `RUNID` with the numeric GitHub
Actions run id of this branch **after** the paced-resume-PIT commit.
Do **not** wrap it in angle brackets (bash treats `<…>` as a redirect).
Do **not** flash `34131274237` (`896424f` DRIVER_OK then virtblk_probe soft lockup)
or `34080595540` (`69102aa` virtio_pci_probe soft lockup)
or `34078335291` (`c815ccc` MADT then I/O `reason=0x1e` hold)
or `34290078274` (`ba5bf8f` stall dump PIT lived, live==last, still n=1373) or
`34227607779` (`a580299` INTx hold lived past n=1345 then empty-ring dump n=1373) or
`34224368343` (`34968f7` used.idx, usbdelay stall dump then apk `n=1345`) or
`34220740109` (`e717fb4` DATA_SEGS=128, same apk `n=1345`) or
`34218742196` (`6cfabee` 3:1 latch lived, same apk `n=1345`) or
`34172103709` (`c2cd099` stop-yield after mount, same apk `n=1345`) or
`34170268004` (`6692898` FLUSH drain, same apk `n=1345`) or
`34149809660` (`9652262` yield-PIT then apk `n=1345`) or
`34148045050` (`fc3053a` mount `n=1089` / usbdelay) or
`34145652767` (`fcac0cd` INTx reassert, same freeze) or
`34076175624` (`20e8b70` froze at apk `n=1345` under `idle=poll`) or
`34074349118` (`63c7e05` same freeze).

```bash
cd ~/projects/raynu
~/projects/raynuv/flashcruzer.sh \
  --install-launcher \
  --branch cursor/pit-during-apk-b7a8 \
  --no-git \
  --run RUNID \
  --any-cruzer-usb \
  --init-new-cruzer \
  --allow-new-serial \
  --raynu-f \
  --linux-iso ~/projects/raynuv/alpine-extended-3.21.3-x86_64.iso
```

`--init-new-cruzer` formats **only** if `lsblk` shows exactly one USB Cruzer
in **[2 GiB, 8 GiB]**. It writes FAT32 label `RAYNUV`, `\EFI\BOOT\`, and a
1 MiB `installdisk.bin` **identity file** (not the guest install disk; leftover
DRAM still backs virtio-blk).

Want:

```text
RAYNU-V-CRUZER-FLASH-OK
RAYNU-V-FLASHCRUZER-OK
```

Record `lsblk` **SERIAL** after the flash. Later refreshes (UDisk already
formatted — **no** `--init-new-cruzer`):

```bash
~/projects/raynuv/flashcruzer.sh \
  --branch cursor/pit-during-apk-b7a8 \
  --no-git \
  --run RUNID \
  --any-cruzer-usb --allow-new-serial --raynu-f \
  --linux-iso ~/projects/raynuv/alpine-extended-3.21.3-x86_64.iso
```

`--refat-cruzer` is the old 64 MiB-FAT-on-977.5 MiB Micro path. Do not combine
it with `--init-new-cruzer`.

---

## 2. Boot the box

1. Open iDRAC **SOL `console com2` first** (you will miss RayNu-F if you F11
   blind).
2. Force Power Off; leave the stick seated.
3. Power on → **one-time F11** → Cruzer. Do **not** change BIOS boot order
   (Ubuntu on PERC stays the default).
4. `.124` is Ubuntu on PERC, not the HV lease. Native HTTP, if it comes up,
   is whatever COM2 prints, port **8443**.

---

## 3. What COM2 must show (Phase A)

Grep the SOL log. Nested line numbers are not required; the strings are.

| Step | COM2 |
|------|------|
| Product ISO retained | `Stage 46 product ISO retained from ESP` · `iso=` ~1 GiB |
| RayNu-F, not OVMF | `RayNu-F launch requested` → one OVMF `VMLAUNCH-OK` → `guest-UEFI stop n=1` → `RayNu-F direct — OVMF leg bypassed at first exit` → `image=ISO-BOOTX64` · **no** WFE state4 poke · **no** `guest-UEFI tick` flood · after `EBS-OK` expect `Linux version`, **not** `restore host xcr0 reason=0x20` |
| Leftover disk | `leftover install disk` `bytes=` (iron targets 1 GiB when leftover ≥ 1.75 GiB) |
| First Linux | `Linux version 6.12.13-0-lts` with `modules=loop,squashfs,virtio_pci,virtio_blk` — past MADT / FPU / `Freeing initrd` **without** `restore host xcr0 reason=0x1e` |
| Virtio probe | nowait `linux virtio DRIVER_OK` then `linux PIT hold until login` then `linux PIT resume paced` then `linux virtio INTx reassert` / `linux PIC IRQ0` (`vec=0x30`) / `linux virtio PIC 11` (`vec=0x3b`) — **no** `soft lockup` in `modprobe` |
| apk overlay | `Installing packages to root filesystem...` with virtio MMIO `n=` continuing (heartbeat every 64) — **not** a freeze after `n=1281` or `n=1345` |
| apk console output | `Installing packages to root filesystem... (1/25) Installing alpine-baselayout-data` … progress `%` lines **streaming**, not one 16-byte chunk per stray IRQ 4. `8b6ed1a` (THRE level) did **not** change this; expect the same stall unless the telemetry names a link we then fix. |
| THRE chain heartbeat (this pin) | After the stall dump, every `PIT paced n=` line is followed by `virtio stall dump thre ier=0x.. latch= pend= brk= rx= ring= com2_lsr=0x.. iir=RX/THRE/NONE lsr_thre=ON/OFF thr= etbei=ON/OFF raise=REASSERT/PIO lower= pic irr=0x.. imr=0x.. isr=0x.. rdy= take0= take4=`. Copy **two consecutive** lines (the deltas matter). Read them with the table below. |
| Probe (still armed) | Only if apk stalls again: two `virtio stall dump ring` lines, three `virtio stall probe rip=` samples, then `sysrq w`/`m`/`t`/`l`. On `f229d14` the `t` dump answered: `apk` `state:S` in `n_tty_write → wait_woken`. If it fires again, read the `apk` stack the same way. |
| Host stack (this pin) | at guest-UEFI launch: `boot: guest-UEFI host stack top=0x… pages=32 guard=0x…` (the 4-page stack that `59ac070` overflowed is gone) |
| Install | `setup-disk -m sys` → `Installation finished. No error reported.` (iron `59ac070` wording; nested printed `Installation is complete. Please reboot.`) |
| Iron close | **`RAYNU-V-M7-ISO-INSTALL-OK`** (firmware prints this only when the
  hypervisor CPUID bit is clear **and** GPT install evidence is real) — **seen on `59ac070`** |
| F7 reset | `guest reset requested src=kbc n=1` · `RayNu-F relaunch after reset (F7)` — **seen on `975f8fc`**; **not** `launch failed: VMCLEAR … guard=BREACHED` |
| Disk boot | `GPT ESP lba=2048 sectors=98304 part=1` · `disk whole-disk path` · `found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk…)` · `image=DISK-BOOTX64` — **seen on `975f8fc`** |
| Installed GRUB | `CONOUT-OK` · `error: no suitable video mode found.` (gfxterm; harmless) · `error: unable to determine partition UUID of boot device.` (GRUB `bli` module → our `SetVariable` = `EFI_UNSUPPORTED`, id `0x308`; harmless) · menu `*Alpine Linux v3.21, with Linux lts` mirrored on COM1 (Alpine's grub.cfg adds `serial`/`terminal_* serial console` because `KERNELOPTS` has `console=ttyS0`) · `executed automatically in 2s` — **seen on `975f8fc`** |
| Countdown survives | `executed automatically in 2s` → `1s` → `0s` → `Booting 'Alpine Linux v3.21, with Linux lts'` · `Loading Linux lts ...` · `Loading initial ramdisk ...` — **no** `RayNu-F stop` line (wall clock, not exits, is the bound; cap 180 s) — **seen on `56a3ffd`** |
| Iron reboot-to-disk marker | `RAYNU-V-RAYNU-F-START-IMAGE-OK` · **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** · `RAYNU-V-RAYNU-F-EBS-OK` — **seen on `56a3ffd`** |
| Second Linux | second `Linux version 6.12.13-0-lts` · `root=UUID=9af18543-…` · `modules=sd-mod,usb-storage,ext4` (installed system, not the ISO overlay) · `EXT4-fs (vda2): mounted filesystem` · `Mounting root: ok.` · `fsck` on `/dev/vda2` + `/dev/vda1` · OpenRC · `login:` · `cat /proc/cmdline` shows `root=UUID=` — **seen on `56a3ffd`** — **Phase A closed** |

If COM2 dies at GRUB rescue `disk `,gpt2' not found`, that is the `3492ebc`
HANDLE_DISK bug — you flashed the wrong EFI.

If COM2 shows OVMF `WaitForEvent` / `state4 poke` / `#PF cr2=0xffffffffffffffb8`,
you lost `raynuf.txt` or you flashed a parked OVMF pin. Stop. Do not poke firmware
state (ADR-016).

If COM2 shows `RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK` followed by `OVMF-PAST-SEC` /
`DXE` / `BOTH-OK` and then an endless `guest-UEFI tick ... rip=0x7f0680d0`
stream, you flashed `088ab25` (or older). Re-flash from the Linux-cap-restore pin.

If COM2 shows `RAYNU-V-RAYNU-F-EBS-OK` then `restore host xcr0 … reason=0x20
rip=0xb00013f` and `Stage 46 product ISO hold` with no `Linux version`, you
flashed `55a3602` / run `34067879816`. That pin collapsed the resume cap to 1
for the Linux path too. Re-flash from the PIT-during-apk pin.

### What the probe said on `f229d14` (run `34354322953`)

`sysrq t`: `apk` pid 1043 `state:S`, stack `__schedule → schedule_timeout →
wait_woken → n_tty_write → file_tty_write → vfs_writev → do_writev`,
`RDI=1` (stdout = `/dev/console` = ttyS0). `init` in `do_wait`. No task in
D state (`sysrq w` empty). 133 MiB free (`sysrq m`). CPU idle (`sysrq l`).
And the tell: each SysRq interrupt flushed **exactly 16 bytes** of apk's own
progress text (`ckages to root f`, `ilesystem: (1/25`, `) Installing alp`,
…) — one 8250 `tx_loadsz` per IRQ 4. The guest 16550 THRE interrupt was
being lost: our IIR read cleared the latch, Linux's next LSR read saw
THRE=0 (LSR is paced to iDRAC SOL and to our own diag bytes in the shared
TX ring), `serial8250_handle_irq` skipped `tx_chars`, and nothing ever
re-raised IRQ 4. The 4 KiB xmit buffer filled and `apk` slept on the
console forever, so it issued no more virtio requests. Not virtio. Not a
timer. Ten stimulus pins could not have fixed it. Fix: THRE is a gated
level (ETBEI set **and** ring empty), never cleared by IIR read alone,
re-asserted per resume, ended when Linux `__stop_tx` clears ETBEI —
UART THRE level until stop_tx.

### What `8b6ed1a` showed (run `34415711199`) — and why this pin only measures

THRE as a gated level (IIR read no longer clears it; re-asserted every
resume while ETBEI is set and the ring is empty) was the fix the probe
called for. Iron ran it and COM2 was indistinguishable from `f229d14`:
`apk` pid 1043 `state:S` in `n_tty_write → wait_woken`, `RDI=1`; apk's own
text still moved only around SysRq IRQ 4s (`(1/25) Installin` → `g
alpine-baselayout-data (3.6.8-` → `r1)\n(2` → `/25) Installing ` → `musl
(1.2.5-r9)\n  0%` → `(3/25)`); stall dump at `n=1373`, `PIT paced n=1375`
for minutes; `sysrq l` idle in `cpu_idle_poll`. Not a regression — the
same stall, one probe deeper. It means one of the links that the level
model *assumes* is not what iron does, and host tests cannot tell which:
Linux may have ETBEI off (`__stop_tx` / flow), the latch may be off, the
shared ring may never be empty when the guest asks, COM2 (iDRAC SOL) may
hold THRE low, the 8259 may have IRQ 4 masked or stuck in ISR, or IRQ 4 may
be taken but answered with IIR `NONE` / LSR `THRE=0` every time. This pin
counts every one of those and prints them with the heartbeat. It also
prints `build: sha=` under the M0 banner so an iron log can be tied to a
commit (the `8b6ed1a` vs `f229d14` question could not be settled from
behaviour alone).

### Reading the `thre` heartbeat

Take two consecutive `virtio stall dump thre` lines during the stall.

| What the line says | Meaning | Next pin |
|---|---|---|
| `ier=0x0?` with bit 1 clear (`0x01`/`0x05`), `etbei=…/N` with N growing | Linux itself turned THRI off (`__stop_tx`) while apk still has 4 KiB queued → Linux thinks TX is stopped (`uart_tx_stopped`: CTS / `tty->flow.stopped` XOFF) or believes the xmit fifo is empty | MSR/CTS + flow-control semantics (`serial8250_modem_status`, MSR delta bits), or find who sent XOFF on RX (`rx=`, auto-answer bytes) |
| `ier` has bit 1, `latch=1`, `pend=0`, `ring>0` on both lines | the shared TX ring is never empty when asked: our own diag bytes (heartbeat / `PIT paced` / probe lines) keep it non-empty | stop HV nowait output while apk overlay is active, or drop the ring-empty term from THRE (COM2 THRE only) |
| `ier` has bit 1, `latch=1`, `pend=0`, `ring=0`, `com2_lsr` bit 5 (`0x20`) clear on both lines | iDRAC SOL holds COM2 THRE low for long stretches (backpressure) | decouple guest THRE from COM2 THRE (ring capacity is the buffer), keep drain paced |
| `pend=1`, `raise=` growing, `take4` **not** growing, `pic irr` bit 4 set | IRQ 4 latched but never taken: `imr` bit 4 set (Linux masked it), `isr` bit 4 stuck (EOI model), or PIT hold starves it (`take0` racing) | fix the 8259 link the registers name (specific-EOI clears the *named* level; PIT preference must not starve IRQ 4 forever) |
| `pend=1`, `take4` growing, `iir=…/THRE/NONE` with NONE growing | IRQ 4 delivered but IIR answered `no interrupt` at Linux's read (level fell between raise and read: ring refilled or COM2 THRE dropped) | latch the IIR answer at raise time (report THRE until Linux writes THR or clears ETBEI), stop re-evaluating the level on IIR read |
| `take4` growing, `iir THRE` growing, `lsr_thre=ON/OFF` with OFF growing | IIR said THRE, LSR said THRE=0, Linux skipped `tx_chars` | same fix: LSR THRE must agree with the IIR class we just reported |
| `take4` growing, `iir THRE` growing, `lsr ON` growing, `thr=` growing by 16 per IRQ, apk text streaming | the chain works; the stall is somewhere else — re-read `sysrq t` | back to the probe decision tree |

### What `916af96` showed (run `34420783162`) — the link, named

`build: sha=916af960b985` confirmed the artifact. Stall again at `n=1370`,
`apk` in `n_tty_write`. Consecutive heartbeats (abridged):

```
thre ier=0x7 latch=1 pend=0 brk=0 rx=0 ring=2048 com2_lsr=0x0 iir=0/17/17 lsr_thre=15/0 thr=21478 etbei=13/433 raise=144/72 lower=247570 pic irr=0x5 imr=0xe8 isr=0x4 rdy=1 take0=2795 take4=14
thre ier=0x7 latch=1 pend=0 ... ring=1    com2_lsr=0x0 iir=1/19/19 lsr_thre=17/3 thr=21552 ... take0=2824 take4=16
thre ier=0x7 latch=1 pend=0 ... ring=0    com2_lsr=0x0 iir=4/32/32 lsr_thre=30/12 thr=73633 etbei=942/1362 raise=2280/2000 ... take0=3381 take4=29
thre ... ring=1988 ... (unchanged for 6+ heartbeats; take0 +30 each, take4 stuck)
```

Row three of the table above, exactly: `ier` bit 1 set, `latch=1`,
`pend=0`, `ring=0`, `com2_lsr` bit 5 clear. Every counted link agreed when
it fired (`take4` ≈ `iir` THRE ≈ `lsr_thre` ON, one THRE IRQ per 16 `thr`;
`iir` NONE is the loop-exit read; `isr=0x4` is IRQ 2 cascade in service,
`imr=0xe8` leaves 0/1/2/4 open) — the *level* was almost never true. Guest
THRE was `com2_lsr` bit 5 **and** an empty ring, sampled only at VM exits,
and with `idle=poll` the only exits during the stall were the ~15
preemption ticks a second (`take0` +30 per 2 s heartbeat); each tick drained
one byte (the drain polled LSR before every byte). Our own per-hit
`virtio MMIO gpa=` lines and the SysRq printk filled the 2 KiB ring
(`ring=2048`, drop-oldest — the shredded SOL text). Net: apk's tty got one
16-byte THRE IRQ per rare coincidence, ~8–240 B/s, and its 4 KiB xmit
buffer never emptied. Not virtio, not the timer: a console-throughput
model that tied the guest's UART to the instantaneous state of the iDRAC's
and only looked when the guest happened to exit.

The pin after `916af96` changes that model, nothing else:

- **guest UART TX ring room** — guest LSR THRE/TEMT and the THRE IRQ level
  are `ring room ≥ 1024 + 16` (ring is 4 KiB; 1 KiB reserved for HV lines).
  Host COM2 LSR is no longer consulted for the guest's THRE.
- **guest UART line-rate pace** — while the ring holds bytes the VMX
  preemption timer is clamped to 250 µs (`IA32_VMX_MISC` rate shift read at
  launch; TSC calibrated pre-EBS against `Stall`, now unconditional).
  Paced ticks drain 16 bytes and re-inject; they step no HPET, raise no
  PIT, and do not count as exits. The nominal tick keeps its deadline.
- **guest UART COM2 FIFO burst** — when COM2 reported a 16550A FIFO at
  init (IIR bits 7:6), a THRE=1 window takes 16 bytes, as Linux
  `tx_loadsz` does on ttyS1. Floating / no-FIFO ports stay at one byte.
- **virtio MMIO kick throttle** — overlay kick lines at most one per
  ~250 ms; the every-64 heartbeat is unchanged.

### Reading the `thre` heartbeat after the fix pin

The line gains `room= pace= fifo= drained= win=`:

| What the line says | Meaning | Next pin |
|---|---|---|
| `pace=` growing while `ring>0`, `drained=` growing ~16 per `win=`, `fifo=1`, apk text streams | line-rate pace and FIFO bursts work; the ring is the buffer | if apk still stalls, it is not the console — re-read `sysrq t` |
| `pace=` growing, `win=` growing, `drained=` growing by exactly `win=` | COM2 has no usable FIFO (`fifo=0`) or the BMC-side UART drops bursts | keep one byte per window; raise pace rate only if SOL still lags |
| `pace=` growing, `win=` **not** growing, `com2_lsr` bit 5 clear for seconds, `ring` full | iDRAC SOL genuinely holds COM2 THRE low (BMC backpressure / SOL session not consuming) | check the SOL client (`console com2` session alive, `racadm` SOL enabled at 115200), then MCR AFE / CTS |
| `pace=` **not** growing while `ring>0` | pacing did not arm: share off, window not armed, or the `pace ticks=` launch line is missing | read the `VMX preemption timer for live HPET shift=` line at launch |
| `room` < 1040 for many heartbeats with `drained` growing | guest is producing faster than SOL takes it (progress-bar redraws); expected during package install, not a stall | none — text is delayed, not lost, unless `ring` hits 4096 |
| `pend=1`, `take4` growing, `thr=` +16 per IRQ, apk text streaming, then `login:` / `#` | console model fixed; apk finished | watch for `RAYNU-V-M7-ISO-INSTALL-OK` (iron COM2 only) |

### What `59ac070` showed (run `34425781629`) — install closed, F7 relaunch failed

`build: sha=59ac070…` confirmed the artifact. The console model held:
heartbeats read `room=3177..4096 pace=3360→5811 fifo=1 drained→97346
win→17488`, apk streamed `(1/25) … (25/25)` and `OK: 25 packages` in about
half a second, `setup-disk -m sys /dev/vda` wrote the GPT, ran `grub-install
--target=x86_64-efi`, built the initramfs and printed `Installation
finished. No error reported.` The firmware then printed
**`RAYNU-V-M7-ISO-INSTALL-OK`** from the iron-only path (hypervisor CPUID
bit clear, product queues armed, GPT header on `vda`, ≥ 512 B written).
That is the E5 install close on real R640. Evidence:
[`2026-09-10-59ac070-iso-install-ok.md`](../evidence/r640/2026-09-10-59ac070-iso-install-ok.md).

Then the auto-answer typed `reboot`. COM2 showed `restore host xcr0 …
reason=0x1e` (the i8042 `0xFE` I/O exit), `RayNu-F relaunch after reset
(F7)`, then `boot: RayNu-F launch failed: VMCLEAR/VMPTRLD (F2b)` and the
leave_to_e4 hold. The `guest reset requested src=kbc` line never reached
COM2 because the earlycon share was dropped with bytes still in the ring.

Root cause, from the release asm (not a guess): `raynu_f_reset_relaunch`
compiled to `movl $81024,%eax; call __rust_probestack` — `RAYNU_F_STATE =
FirmwareState::new()` built the 79,704-byte `PagePool` on the stack and
memcpy'd it. The guest-UEFI host stack was **4 pages** (16 KiB) and the
private VMCS was the next frame down, so the 81 KiB frame overwrote the VMCS
revision dword and VMPTRLD answered VMfailValid. Every VM exit re-enters at
stack top, so only the deepest single exit matters; F7 is the deepest and it
runs exactly once, after the install — nested KVM never tripped it because
KVM's own stack layout differs. It is host-side (RayNu-F is ours; no OVMF
state is touched — ADR-016).

The pin after `59ac070` changes that, nothing else:

- **RayNu-F F7 template reset** — `RAYNU_F_STATE` is reset by memcpy from a
  `.rdata` `RAYNU_F_STATE_TEMPLATE`; the frame is now 40 bytes.
- **guest-UEFI host stack pages** — 4 → 32 pages, plus one **guard page**
  filled with `0x5A` below the stack. The launch banner prints
  `boot: guest-UEFI host stack top=0x… pages=32 guard=0x…`.
- **RayNu-F launch VMCS fail** — VMCLEAR and VMPTRLD failures are reported
  separately with the VMCS revision seen vs wanted and the guard state:
  `boot: RayNu-F launch failed: vmptrld rev=0x… want=0x… guard=ok|BREACHED`.
  If the guard is breached before F7 VMCLEAR a `WARN host stack guard
  breached` line prints first.
- **guest UART TX flush** — the guest TX ring is flushed to COM2 before the
  earlycon share is turned off, so `guest reset requested src=kbc` is now
  visible.

What this flash must show after `RAYNU-V-M7-ISO-INSTALL-OK`:

| COM2 | Meaning |
|---|---|
| `guest reset requested src=kbc` · `RayNu-F relaunch after reset (F7)` · **no** `launch failed` | relaunch on the same VMCS worked |
| `GPT ESP` · `disk whole-disk path` · `image=DISK-BOOTX64` · `RAYNU-V-RAYNU-F-DISK-BOOT-OK` | RayNu-F booted the installed GRUB from `vda` (iron first) |
| second `Linux version 6.12.13-0-lts` with `root=UUID=` (not `modules=loop,squashfs`) then `login:` | reboot-to-disk on iron; E5 whole loop closed — capture the SOL log |
| `launch failed: vmptrld rev=0x… want=0x… guard=BREACHED` | some other frame still overflows 32 pages: the `rev=` value is what overwrote the VMCS; copy the line |
| `launch failed: vmclear …` or `vmptrld … guard=ok` with `rev==want` | not the stack: VMCS region/state issue (ADR-015 F2b class); copy the line and stop |

### What `975f8fc` showed (run `34474850361`) — relaunch worked, exit-cap fired in the GRUB menu

`build: sha=975f8fc2d756`. `guest-UEFI host stack top=0x142d000 pages=32
guard=0x140c000` at launch. The install re-ran clean (apk 0.5 s,
`Installation finished. No error reported.`, `RAYNU-V-M7-ISO-INSTALL-OK`,
`Installation is complete. Please reboot.`). Then `reboot`:

```
[   65.770698] reboot: Restarting system
boot: RayNu-F guest reset requested src=kbc n=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F relaunch after reset (F7; not ISO-INSTALL-OK)
boot: RayNu-F GPT ESP lba=2048 sectors=98304 part=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F disk whole-disk path (F7; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xb23000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 (F2b/F5; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-MEM-OK … RAYNU-V-RAYNU-F-BLOCKIO-OK … RAYNU-V-RAYNU-F-CONOUT-OK
error: no suitable video mode found.
error: unable to determine partition UUID of boot device.
GNU GRUB  version 2.12   *Alpine Linux v3.21, with Linux lts   UEFI Firmware Settings
   The highlighted entry will be executed automatically in 2s.
boot: RayNu-F stop exit-cap exits=1048577 svc=492778 svc_err=4 conout_ok=1 blk_rd=59 blk_wr=0 allocs=6 free_pages=62553 (F2b; not ISO-INSTALL-OK)
```

The `59ac070` VMCS overwrite is gone (template reset + 32-page stack
worked). RayNu-F booted the installed disk's GRUB from the GPT ESP, GRUB
read `grub.cfg`, drew its menu and started the 2 s countdown. Then the
firmware phase's runaway guard — a fixed **1,048,576 exit count** — fired.
GRUB's `run_menu` loop (`grub-core/normal/menu.c`) calls
`grub_getkey_noblock()` with no `grub_cpu_idle`; with Alpine's
`terminal_input serial console` each iteration is one `ReadKeyStroke`
service call plus one serial LSR `inb` — two VM exits — and on the R640
that loop runs at roughly 2 exits/µs (`svc=492778` ≈ half of `exits`).
1 M exits is under a second of wall time on iron, less than GRUB's own
timeout; on nested KVM each exit costs 10–50× more so the same loop
never approached the cap. GRUB's clock is fine: no ACPI (pmtimer skipped),
PIT channel 2 reads `0xff` on port `0x61` so `grub_pit_wait` bails, and
`grub_tsc_calibrate_from_efi` measures our honest `Stall(1000)`.

The pin after `975f8fc` changes that guard, nothing else:

- **RayNu-F wall cap** — the loader phase (launch → `ExitBootServices`) is
  bounded by wall time (`RAYNU_F_WALL_CAP_S` = 180 s from `rdtsc` at
  launch/relaunch, sampled every 4096 exits). The exit count stays only as a
  u32-wrap guard (`1 << 30`). `raynu_f_stop` lines now carry `wall_ms=`.

What this flash must show after the GRUB menu:

| COM2 | Meaning |
|---|---|
| menu, ~2 s of silence (or a redraw `in 1s`), then GRUB `linux`/`initrd` reads (`BlockIo.ReadBlocks` bursts, `blk_rd` climbing) | countdown finished; kernel + initramfs loading from `vda2` |
| `RAYNU-V-RAYNU-F-EBS-OK` on the disk leg · `RAYNU-V-RAYNU-F-DISK-BOOT-OK` | RayNu-F handed the installed kernel to the Linux path (iron first) |
| second `Linux version 6.12.13-0-lts` with `root=UUID=` · `EXT4-fs (vda2)` · `login:` | reboot-to-disk on iron; E5 whole loop closed — capture the SOL log, fill the evidence doc |
| `RayNu-F stop wall-cap … wall_ms=180…` | the loader really spun for 3 minutes: read `svc=`/`blk_rd=` to see whether GRUB was still polling the menu (timeout never counted: GRUB clock) or reading the disk |
| `RayNu-F stop exit-cap …` | you flashed `975f8fc` — re-flash |
| `RayNu-F stop exception …` / `ept …` after the menu | GRUB's `linux` loader hit a service/mapping we do not provide on iron: copy the line |

### What `56a3ffd` showed (run `34480107961`) — Phase A CLOSED: reboot-to-disk on iron

`build: sha=56a3ffda8406`. Install re-ran clean (`RAYNU-V-M7-ISO-INSTALL-OK`,
`Installation is complete. Please reboot.`). Then `reboot`:

```
[   65.374822] reboot: Restarting system
boot: RayNu-F guest reset requested src=kbc n=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F relaunch after reset (F7; not ISO-INSTALL-OK)
boot: RayNu-F GPT ESP lba=2048 sectors=98304 part=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F disk whole-disk path (F7; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\BOOT\BOOTX64.EFI bytes=139264 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xb23000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 (F2b/F5; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-MEM-OK … RAYNU-V-RAYNU-F-BLOCKIO-OK … RAYNU-V-RAYNU-F-CONOUT-OK
GNU GRUB  version 2.12   *Alpine Linux v3.21, with Linux lts   UEFI Firmware Settings
   The highlighted entry will be executed automatically in 2s.
   The highlighted entry will be executed automatically in 1s.
   The highlighted entry will be executed automatically in 0s.   Booting `Alpine Linux v3.21, with Linux lts'
Loading Linux lts ...
Loading initial ramdisk ...
boot: RayNu-F StartImage entry=0x2b09515 handle=0x5246000000000022 (F5; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-START-IMAGE-OK
RAYNU-V-RAYNU-F-DISK-BOOT-OK
EFI stub: Loaded initrd from LINUX_EFI_INITRD_MEDIA_GUID device path
RAYNU-V-RAYNU-F-EBS-OK
[    0.000000] Linux version 6.12.13-0-lts (buildozer@build-3-21-x86_64) …
[    0.000000] Command line: BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=9af18543-bbaf-475e-925a-60ab53d49847 ro modules=sd-mod,usb-storage,ext4 console=ttyS0 … rootfstype=ext4
[    3.096813] EXT4-fs (vda2): mounted filesystem 9af18543-bbaf-475e-925a-60ab53d49847 ro with ordered data mode. Quota mode: none.
[    3.099045] Mounting root: ok.
   OpenRC 0.55.1 is starting up Linux 6.12.13-0-lts (x86_64)
/dev/vda2: clean, 6377/62336 files, 52818/249344 blocks
/dev/vda1: 5 files, 548/96760 clusters
Welcome to Alpine Linux 3.21
localhost login: root
localhost:~# cat /proc/cmdline
BOOT_IMAGE=/boot/vmlinuz-lts root=UUID=9af18543-bbaf-475e-925a-60ab53d49847 ro …
```

The GRUB countdown survived (the `975f8fc` exit-cap is gone; no `RayNu-F
stop` line anywhere), GRUB read `vmlinuz-lts` + `initramfs-lts` from `vda2`
through our `BlockIo`, `StartImage` ran the kernel's EFI stub from the disk
(`DISK-BOOT-OK`), the stub took `ExitBootServices`, and the **installed**
Alpine booted: `root=UUID=` root on ext4 `vda2`, FAT ESP `vda1`, OpenRC,
login. `/proc/cmdline` from the installed system carries the GRUB config
that `setup-disk` wrote on the first boot. That is the E5 mechanism —
ISO → UEFI installer → virtio-blk → reboot to disk — on the real R640.

Reading it: `guest-UEFI virtio MMIO` heartbeats and the idle `virtio stall
dump` lines after `login:` are diagnostics with nothing to diagnose (the
guest is at a shell); they are cosmetic now and are the first post-Everest
demotion candidates.

### Reading the probe (decision tree)

`sysrq w` lists only TASK_UNINTERRUPTIBLE tasks; `sysrq t` lists all. Find
`apk` (and any `busybox` / `sh` child) and read the top frames:

| apk (or child) stack | Meaning | Next pin |
|---|---|---|
| `io_schedule` / `blk_mq_get_tag` / `folio_wait_bit` / `__wait_on_buffer` | Linux believes a block request is still in flight while our rings say empty | completion semantics in `process_blk_queue` (used `id`/`len`, status byte, header/status heuristics); compare with the `stall dump ring` `mem_used` / `last_id` / `last_st` line |
| `n_tty_write` / `wait_woken` in `apk` (this is what `f229d14` showed) | console TX stuck: 8250 THRE interrupt lost | UART THRE level until stop_tx (this pin) |
| `do_wait` / `kernel_wait4` in `apk`, child in `n_tty_read` / `pipe_read` / `hrtimer_nanosleep` | a package script child is stuck on tty / pipe / timer | fix that device (console/tty semantics, or the timer model) |
| `hrtimer_nanosleep` / `schedule_hrtimeout` / `do_nanosleep` in `apk` | a real timer-programming bug (clockevent never fires for that expiry) | honest i8254 countdown + LAPIC timer + ICR (Pin C); stop heuristic PIT pins |
| `inet_wait_for_connect` / `wait_woken` (netlink) / `sk_wait_data` | apk is trying a network repository | cmdline / `/etc/apk/repositories` fix, not a device fix |
| no `apk` task; `/init` in `do_wait` | apk already exited (error swallowed) | look at what init spawned; check `sysrq m` for tmpfs/OOM pressure (guest has 288 MiB) |
| `sysrq` prints nothing at all | BREAK not recognised (no `sysrq:` lines) | check `sysrq_always_enabled` reached the cmdline (`Command line:` echo) and LSR.BI path; the RIP samples still say idle (`cpu_idle_poll` kernel RIP, `cs=0x10`) vs user (`cs=0x33`) |

Whatever it says, the next pin fixes that one thing and re-runs the probe.
No PIT/INTx heuristic pin ships without a stack that names a timer or an
interrupt as the wait.

If COM2 shows `RAYNU-V-M7-ISO-INSTALL-OK`, then `guest reset requested
src=kbc`, `GPT ESP lba=2048`, `image=DISK-BOOTX64`, the GRUB 2.12 menu with
`executed automatically in 2s`, and then `RayNu-F stop exit-cap
exits=1048577`, you flashed `975f8fc` / run `34474850361`. The relaunch and
the installed-disk GRUB are real; the count cap fired inside GRUB's own
timeout. **Do not wait.** Re-flash from this wall-cap pin. Do not F11
`34474850361` again expecting `RAYNU-V-RAYNU-F-DISK-BOOT-OK`.

If COM2 shows `Installation finished. No error reported.` and
`RAYNU-V-M7-ISO-INSTALL-OK`, then `restore host xcr0 … reason=0x1e`,
`RayNu-F relaunch after reset (F7)` and `launch failed: VMCLEAR/VMPTRLD
(F2b)` with **no** `guest-UEFI host stack top=` line at launch, you flashed
`59ac070` / run `34425781629`. The install is real and already counted; the
reboot-to-disk is not. **Do not wait.** Re-flash from this F7 template-reset
pin. Do not F11 `34425781629` again expecting `RAYNU-V-RAYNU-F-DISK-BOOT-OK`.

If COM2 shows `virtio stall dump ring` + `virtio stall probe rip=` + `sysrq
w`/`m`/`t`/`l` with `apk` in `n_tty_write`, apk's text advancing 16–32
bytes per SysRq, and **no** `virtio stall dump thre ier=` lines after the
`PIT paced n=` heartbeat, you flashed `f229d14` / run `34354322953` or
`8b6ed1a` / run `34415711199` (no `build: sha=` line under the banner on
either). Both answered what they could. **Do not wait.** Re-flash from
this telemetry pin. Do not F11 either run again expecting `ISO-INSTALL-OK`.

If COM2 shows `virtio stall dump PIT paced` then repeating `PIT paced n=1375`
for minutes, **no** `soft lockup`, and **no** `virtio stall dump ring` /
`virtio stall dump sysrq` lines, you flashed `09b5842` / run `34302053558`.
Paced IRQ0 lived; apk still never kicked; rings empty; CPU idle. Timer and
interrupt hypotheses are exhausted. **Do not wait.** Re-flash from this
probe pin. Do not F11 `34302053558` again expecting `ISO-INSTALL-OK`.

If COM2 shows `virtio stall dump PIT hold` then repeating `PIT hold n=`
and `watchdog: BUG: soft lockup` in `handle_softirqs` (26s/52s/78s) then
`restore host xcr0 … reason=0x1e`, you flashed `2b3cc1e` / run
`34300317331`. Unpaced dump-path PIT stormed IRQ0. Watchdog proved
jiffies already move. **Do not wait.** Re-flash from this probe pin.
Do not F11 `34300317331` again expecting `ISO-INSTALL-OK`.

If COM2 shows `virtio stall dump notify reset` plus apk dump `live==last`
then `virtio stall dump INTx` and ISR ACK (`n=+2`) and **no** further
`0x300` and **no** `virtio stall dump PIT hold`, you flashed `b8ac361` /
run `34297632324`. Flood is gone; harvest was a no-op. **Do not wait.**
Re-flash from this PIT-paced pin. Do not F11 `34297632324` again expecting
`ISO-INSTALL-OK`.

If COM2 shows repeating `virtio stall dump` / `virtio stall dump INTx` with
`n=` climbing by 2 (disk+ISO ISR `off=0x100`) and **no** `virtio stall dump
notify reset`, you flashed `6d0c58a` / run `34292570282`. ISR ACK re-armed
the dump. **Do not wait.** Re-flash from this notify-reset pin. Do not F11
`34292570282` again expecting `ISO-INSTALL-OK`.

If COM2 shows `virtio stall dump PIT` plus `disk_live=` matching `disk_last=`
(and iso) at apk `n=1373` and **no** `virtio stall dump INTx`, you flashed
`ba5bf8f` / run `34290078274`. PIT lived; queues were empty. **Do not wait.**
Re-flash from this stall-dump-INTx pin. Do not F11 `34290078274` again expecting
`ISO-INSTALL-OK`.

If COM2 shows `linux PIC IRQ0` and `linux virtio PIC 11` and `Mounting boot
media: ok.` then `Installing packages` with virtio `n=` past 1345 and a
second stall dump `last==used==avail` `isr=0` `kick=0` (iron `n=1373`)
and no `virtio stall dump PIT`, you flashed `a580299` / run `34227607779`.
Shared INTx hold lived; queues were empty. **Do not wait.** Re-flash from
this stall-dump-INTx pin. Do not F11 `34227607779` again expecting
`ISO-INSTALL-OK`.

If COM2 shows `linux PIC IRQ0` and `linux virtio PIC 11` and `Mounting boot
media: ok.` then `Installing packages` with last virtio MMIO `n=1345` (disk
ISR `off=0x100 wr=0`) and no further `n=1409` / `n=1473`, you flashed
`34968f7` / run `34224368343` (used.idx + stall dump; dump at usbdelay
`n=1118` then same apk freeze) or `e717fb4` / run `34220740109` (DATA_SEGS=128
did not move the freeze) or `6cfabee` / run `34218742196` (or `c2cd099` /
`34172103709` / `6692898` / `9652262`). `linux PIC IRQ11 yield until mount` on
`6cfabee` proves the 3:1 latch fired; IRQ ratio is not the apk stall. **Do not
wait** — guest time stays at `Installing packages`. Re-flash from this
stall-dump-PIT pin. Do not F11 `34227607779` or `34224368343` or `34220740109` or
`34218742196` again expecting `ISO-INSTALL-OK`.

If COM2 shows `linux PIC IRQ0` and `linux virtio PIC 11` then `Mounting boot
media` with last virtio MMIO `n=1089` and **no** `Mounting boot media: ok`,
you flashed `fc3053a` / run `34148045050`. Level INTx + unmask made IRQ 11
live; it then starved PIT so `usbdelay=30` jiffies never finished. Re-flash
from this yield-PIT pin. Do not F11 `34148045050` again expecting `ISO-INSTALL-OK`.

If COM2 shows `linux virtio INTx reassert` then `Installing packages` with last
virtio MMIO `n=1345` and **no** `linux virtio PIC 11`, you flashed `fcac0cd` /
run `34145652767`. Shared INTx reassert ran; eight `inject vec=0x30` is Linux
x86_64 PIC IRQ0 (`IRQ0_VECTOR`), not a virtio inject. PCI INTx was edge-INTA so
Linux EOI before ISR read dropped IRQ 11. Re-flash from this level-INTx pin.
Do not F11 `34145652767` again expecting `ISO-INSTALL-OK`.

If COM2 shows `linux virtio DRIVER_OK` then `linux PIT hold until login` then
`linux PIT resume paced` then `Installing packages` with last virtio MMIO
`n=1345` and **no** `linux virtio PIC 11` / **no** `linux virtio INTx reassert`,
you flashed `0a9b552` / run `34141401594` or `c61942b` / run `34135448354`.
Slice 1 (paced resume) lived; leftover GSI 2 was a no-op on RayNu-F (never
armed pin 2). Shared vda+vdb INTx + missed kicks starved apk. Re-flash from
this level-INTx pin. Do not F11 `34141401594` or `34135448354` again
expecting `ISO-INSTALL-OK`.

If COM2 shows `linux virtio DRIVER_OK` then `linux PIT hold until login` /
`linux PIT once after DRIVER_OK` then `watchdog: BUG: soft lockup` in
`modprobe` `virtblk_probe` / `vp_set_status` / `iowrite8` then `restore host
xcr0 … reason=0x1e`, you flashed `896424f` / run `34131274237`. Raise-PIT on
every resume after DRIVER_OK nested IRQ0 inside the last DEVICE_STATUS write.
Re-flash from this paced-resume pin (`linux PIT resume paced`). Do not F11
`34131274237` again expecting `ISO-INSTALL-OK`.

If COM2 shows `Linux version 6.12.13-0-lts` past MADT / FPU / `Freeing initrd`
then Alpine Init `Loading boot drivers` then `virtio MMIO … off=0x14` then
`watchdog: BUG: soft lockup` in `modprobe` `vp_set_status`/`iowrite8` then
`restore host xcr0 … reason=0x1e`, you flashed `69102aa` / run `34080595540`.
The first DEVICE_STATUS write armed raise-PIT-on-every-resume and IRQ0
stormed virtio_pci_probe. Re-flash from this paced-resume pin. Do
not F11 `34080595540` again expecting `ISO-INSTALL-OK`.

If COM2 shows `Linux version 6.12.13-0-lts` then `APIC: ACPI MADT or MP tables
are not detected` then several `Stage 46 inject vec=0x30` then `restore host
xcr0 … reason=0x1e` and `Stage 46 product ISO hold` with no further kernel
timestamps, you flashed `c815ccc` / run `34078335291`. `reason=0x1e` is I/O
(30), not XSETBV (55). PIT hold+raise on every resume treated firmware
queue-arm + PHASE_LOGIN as virtio probe and injected IRQ0 into a half-built
IDT. Re-flash from this PIT-after-virtio-probe pin. Do not F11 `34078335291`
again expecting `ISO-INSTALL-OK`.

If COM2 shows `Linux version 6.12.13-0-lts` and `Installing packages to root
filesystem...` then goes silent after virtio MMIO `n=1281` with no
`linux PIT once after DRIVER_OK`, you flashed `46fd345` / run `34069352671`.
UART beat PIT after DRIVER_OK. Re-flash from this PIT-after-virtio-probe pin. Do not
F11 `34069352671` again expecting `ISO-INSTALL-OK`.

If COM2 shows `linux PIT once after DRIVER_OK` and `Mounting boot media: ok`
then `Installing packages` with last virtio MMIO `n=1345` and no further
kernel timestamps, you flashed `63c7e05` / `20e8b70` (runs `34074349118` /
`34076175624`). Product cmdline is `idle=poll` so Linux never HLTs; PIT-once
is consumed and UART wins until the next virtio kick. Re-flash from this
PIT-after-virtio-probe pin. Do not F11 `34074349118` or `34076175624` again.

Capture the full SOL log into `docs/evidence/r640/logs/` and fill
[`TEMPLATE-iso-install.md`](../evidence/r640/TEMPLATE-iso-install.md). Nested
or CI logs do not close the gate.

---

## 4. Iron risks that can reopen science

- **Pool / leftover DRAM** under real `pages_above_1MiB` (~63 GiB on this R640).
  Nested used 512 MiB disk at `-m 4096M`; iron should carve 1 GiB when leftover
  ≥ 1.75 GiB (768 MiB guest floor).
- **ESP retain** of a ~1 GiB ISO PRE-EBS (watchdog-off path is already in-tree).
- **UART / SOL** vs Linux earlycon share (F7 reset lines must be nowait —
  that is why the pin is at or after `088ab25`, not `fe4785a`).
- **virtio_pci_probe / virtblk_probe IRQ0 storm** (2026-09-07):
  `69102aa` / `34080595540` stormed at first DEVICE_STATUS. `896424f` /
  `34131274237` lived to `linux virtio DRIVER_OK` then the same
  `iowrite8` / `handle_softirqs` lockup because resume still raised PIT
  on every VM-entry. `c61942b` / `34135448354` paced resume (~1 ms) and
  probe lived, then apk froze at `n=1345` with no `linux PIC IRQ0`
  (`MADT or MP tables are not detected`). `0a9b552` / `34141401594` was
  a no-op (RayNu-F never armed leftover GSI 2). `fcac0cd` / `34145652767`
  printed `linux virtio INTx reassert` then froze at `n=1345`; `vec=0x30`
  is Linux PIC IRQ0. `fc3053a` / `34148045050` printed `linux PIC IRQ0` and
  `linux virtio PIC 11` then froze at mount `n=1089` (`usbdelay=30`); level
  INTx starved IRQ0. `9652262` / `34149809660` yielded PIT so `usbdelay`
  finished (`Mounting boot media: ok.`) then apk froze at `n=1345` after a
  disk ISR ACK: drain treated virtio FLUSH as 0 OUT bytes and never raised
  PIC 11. `6692898` / `34170268004` raised INTx on FLUSH drain and
  **still** froze at apk `n=1345`. `c2cd099` / `34172103709` stopped
  yield after `media: ok` and **still** froze at the same `n=1345`.
  `6cfabee` / `34218742196` printed `linux PIC IRQ11 yield until mount`
  then **still** froze at `n=1345` (3:1 is not the bottleneck).
  `e717fb4` / `34220740109` `DATA_SEGS=128` still froze at `n=1345`.
  `34968f7` / `34224368343` used.idx + stall dump: dump at usbdelay
  `n=1118` (`last==used` `isr=0`) then same apk freeze at `n=1345` after
  ISO notify `n=1281` + disk ISR ACK (no second dump). `a580299` /
  `34227607779` held PIC 11 on sibling ISR: `n=` went past 1345, then
  second dump `last==used==avail` `isr=0` `kick=0` `n=1373` during apk
  (empty rings). `ba5bf8f` / `34290078274` raised PIT on that dump
  (`disk_live==disk_last` `iso_live==iso_last`) and **still** froze at
  apk `n=1373`. This pin pulses shared virtio INTx so Linux re-harvests
  used.idx. Do not F11 `34290078274`, `34227607779`, `34224368343`, `34220740109`, `34218742196`, `34172103709`, `34170268004`, `34149809660`, `34148045050`, `34145652767`, `34141401594`, `34135448354`,
  `34131274237`, or `34080595540`.
- **PIT hold from firmware queue-arm during APIC setup** (2026-09-07,
  `c815ccc` / `34078335291`): Linux reached `APIC: ACPI MADT or MP tables
  are not detected`, then 8× `inject vec=0x30`, then `restore host xcr0
  reason=0x1e` (I/O, not XSETBV) + `product ISO hold`. `apk_overlay_needs_pit`
  is true from boot (`PHASE_LOGIN`) and firmware already armed virtio queues,
  so `c815ccc` raised/held PIT on every Linux resume including UART printk
  of the MADT line. This pin arms PIT only after Linux writes DEVICE_STATUS
  (probe started) or after both DRIVER_OK until `login:`. Early kernel UART
  beats PIT. Do not F11 `34078335291` again.
- **UART beats PIT after virtio DRIVER_OK** (2026-09-07, `46fd345` /
  `34069352671`): Alpine apk overlay stalled at `Installing packages` (last
  virtio MMIO `n=1281`). PIT-once on virtio MMIO + preempt was not enough:
  iron `63c7e05` / `34074349118` printed PIT-once, mounted media, then froze
  at `n=1345`. Iron `20e8b70` / `34076175624` armed PIT-once on HLT + UART LSR
  but the product cmdline is `idle=poll` (never HLT); PIT-once is consumed so
  UART wins until the next virtio kick. After Linux virtio probe / both
  DRIVER_OK this pin holds PIT until `login:` (`prefer_pit_hold`, not consumed)
  and raises PIT on overlay resume. Do not permanently prefer PIT after login
  — that starves auto-answer. Do not F11 `34076175624`, `34074349118`, or
  `34069352671` again.
- **OVMF scaffold leg on iron** (2026-09-06, `088ab25`): the leg is now capped
  at one exit when `raynuf.txt` is present (`GUEST_UEFI_RAYNU_F_DIRECT_CAP`).
  If RayNu-F still does not start, look for `RayNu-F launch skipped` /
  `RayNu-F launch failed` — those are RayNu-F-side reasons, not OVMF.
- **Virtio BARs** under the real RAM map.
- **New Cruzer VID/PID** (hence `--any-cruzer-usb`).
- **F11 vs Ubuntu-on-PERC** (do not leave Cruzer as the standing boot order).

If Phase A hits a new device/firmware gap, that is new science — not “flash
again and claim ISO-INSTALL-OK.”

---

## 5. After Phase A (Phase B — closed on iron `f72b4276`)

Phase A closed on iron with `56a3ffd` (run `34480107961`, 2026-09-10).
**Phase B closed on iron with `f72b4276` (run `34552377351`, 2026-09-11).**
This pin is the Phase B / SPA-path reference **and the M8.0 rollback kit**
([`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed),
EFI SHA256 `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc`).
Do not F11 older Phase B fails. Do not flash a later persist prototype as the known-good.

1. Keep the installed leftover disk across a **guest** F7 reset only (reset cap
   is 1). A host reboot of RayNu-V zeros leftover DRAM — the nested disk does
   not persist across HV reboot unless we later persist it.
2. SPA/REST: create-VM + attach so E4 is “SPA starts that installed disk,” not
   “list/start/stop a SHELL stub.” **Done on COM2 `f72b4276`.**
   - Host wire: `POST /vms/{id}/start` of a typed product ISO queues
     `SpaStartKind::RayNuF`. Marker `RAYNU-V-M7-PHASE-B-SPA-WIRE-OK`. `iso=0`
     stays E4 SHELL. Do not print `ISO-INSTALL-OK` from host/CI.
   - **Iron Phase B CLOSED:** no `--raynu-f`. COM2
     `build: sha=f72b4276d198`: SKIP-OVMF-OK → E4-CONTINUE-OK → coexist
     `HOST-NIC-HTTP-OK` on `10.99.99.145:8443` (lease `.145` this boot) →
     SPA Start of RayNu-F product ISO → `ISO-INSTALL-OK` → reboot →
     `DISK-BOOT-OK` → second Linux `root=UUID=814a97a0-…` → `login:`.
     Evidence: [`../evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md`](../evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md).
     Bring-up token `raynu-v-bringup`. LOM `:38`, not iDRAC dedicated.
     PRE-EBS SNP timeout is expected if you skip that window.
   - **Do not wait on ticks on `9061ffca`.** That pin (Phase B host wire,
     no `--raynu-f`) still VMLAUNCHes retained OVMF. Without `raynuf.txt` the
     collapse cap stays 16_777_216, so iron parks in Bds CpuSleep
     `rip=0x7f0680d0` `reason=0xc` `insn=f4` and prints HLT ticks every 65536.
     That is the old OVMF stall, not a tick-printer regression. Force Off.
   - **Do not wait on the Stage 46 hold on `2a1c1ef1` / `34544625780`.**
     Skip-OVMF printed (`RAYNU-V-M7-PHASE-B-SKIP-OVMF-OK`) then
     `leave_to_e4` jumped to `resume_e4_shell`, which spun because
     `stage46_hold_e4_shell()` is `product_iso_window_armed()`. COM2:
     `restore host xcr0=0x1 osxsave=0 reason=0x0 rip=0x0` then
     `Stage 46 product ISO hold`. Force Off.
   - **Do not wait on the G0 BAR hole on `31f1ea0c` / `34546680282`.**
     CONTINUE-OK printed, then packed-bzImage G0 died on
     `no virtio-blk BAR hole above G0 guest RAM`. Force Off. Do not flash
     `34546680282` / `31f1ea0c` again.
   - **Do not wait on listen without HTTP-OK on `7f8dc0a9` / `34548550755`.**
     SKIP-OVMF-OK + E4-CONTINUE-OK + `Phase B coexist idle` +
     `HOST-NIC coexist listening on 10.99.99.146:8443` all printed, then
     Mac `curl: (7)` after ~1 s. Cause: `MILLIS += 10` in tight idle raced
     smoltcp Instant. Fixed by TSC Instant on `f72b4276`. Do not flash
     `34548550755` / `7f8dc0a9` again.
3. Still no TLS requirement for M7 (deferred).

---

## Honesty

- Do not print `RAYNU-V-M7-ISO-INSTALL-OK` from nested, host, or CI.
- `RAYNU-V-RAYNU-F-DISK-BOOT-OK` is a guest-exit marker (nested prints it too). It counts as the iron reboot-to-disk close **only** on a COM2 log whose `build: sha=` matches a CI run, as on `56a3ffd` / `34480107961` (Phase A) and `f72b4276` / `34552377351` (Phase B SPA).
- Do not treat `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` as this distro install.
- Phase A `--raynu-f` and Phase B SPA are both closed on iron. Residual polish: leftover-DRAM persist across a **host** reboot, TLS, console UI.
- **M8.0 rollback:** GitHub Latest `v0.1.0-everest-closed` (`f72b4276` / `34552377351`). Do not treat a later persist prototype as the known-good.
- HDA months moved 0.25 → 0.0 on `f72b4276` Phase B COM2 (Everest product loop).
