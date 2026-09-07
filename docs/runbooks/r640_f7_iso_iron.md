# Runbook — F7 product ISO on R640 (4 GB Cruzer)

**Iron close (COM2 only):** `RAYNU-V-M7-ISO-INSTALL-OK`  
**Nested close (not this gate):** `RAYNU-V-RAYNU-F-DISK-BOOT-OK`  
**EFI pin:** the **next** green UEFI-release artifact of `cursor/pit-during-apk-b7a8` (Linux PIC despite leftover firmware GSI 2; paced overlay PIT; virtio beats PIT hold). `--run` takes a numeric GitHub Actions id with **no** angle brackets. Superseded: `c61942b` run `34135448354` (DRIVER_OK + `linux PIT resume paced`, then apk freeze `n=1345` with **no** `linux PIC IRQ0`) · `896424f` run `34131274237` (DRIVER_OK then `virtblk_probe` `iowrite8` IRQ0 storm / soft lockup) · `69102aa` run `34080595540` (MADT live; virtio_pci_probe `vp_set_status` IRQ0 storm / soft lockup) · `c815ccc` run `34078335291` (PIT hold from firmware queue-arm; APIC MADT then I/O `reason=0x1e` hold) · `20e8b70` run `34076175624` (PIT-once on HLT; `idle=poll` never HLT; froze at apk `n=1345`) · `63c7e05` run `34074349118` (PIT-once printed, boot media ok, then froze at apk `n=1345`) · `46fd345` run `34069352671` (no PIT-once, stall n=1281) · `55a3602` run `34067879816` (RayNu-F direct, then cap=1 killed Linux WRMSR) · `088ab25` run `33978770315`.  
**Do not flash:** `34131274237` / `896424f` (iron 2026-09-07: `linux virtio DRIVER_OK` then `virtblk_probe` soft lockup then `reason=0x1e`, no `ISO-INSTALL-OK`) · `34080595540` / `69102aa` · `34078335291` / `c815ccc` (MADT then `restore host xcr0 reason=0x1e`) · `34076175624` / `20e8b70` · `34074349118` / `63c7e05` · `34069352671` / `46fd345` · `34067879816` / `55a3602` · `088ab25` for F7 · P0-14 `2b795a0` · parked OVMF pins · PR #231  
**Honesty:** Nested QEMU ≠ R640. Host/CI must never print `RAYNU-V-M7-ISO-INSTALL-OK`.

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
or `34076175624` (`20e8b70` froze at apk `n=1345` under `idle=poll`) or
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
| Virtio probe | nowait `linux virtio DRIVER_OK` then `linux PIT hold until login` then `linux PIT resume paced` then `linux PIC before leftover GSI 2` / `linux PIC IRQ0` — **no** `soft lockup` in `modprobe` |
| apk overlay | `Installing packages to root filesystem...` with virtio MMIO `n=` continuing (heartbeat every 64) — **not** a freeze after `n=1281` or `n=1345` |
| Install | `setup-disk -m sys` → `Installation is complete. Please reboot.` |
| F7 reset | `guest reset requested src=` (`kbc` is what nested saw) · `relaunch after reset` |
| Disk boot | `GPT ESP` · `disk whole-disk path` · `image=DISK-BOOTX64` |
| Nested-style marker | `RAYNU-V-RAYNU-F-DISK-BOOT-OK` |
| Second Linux | second `Linux version` · `root=UUID=` · ext4 `/dev/vda2` |
| Iron close | **`RAYNU-V-M7-ISO-INSTALL-OK`** (firmware prints this only when the
  hypervisor CPUID bit is clear **and** GPT install evidence is real) |

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

If COM2 shows `linux virtio DRIVER_OK` then `linux PIT hold until login` then
`linux PIT resume paced` then `Installing packages` with last virtio MMIO
`n=1345` and **no** `linux PIC IRQ0` / **no** `linux PIC before leftover GSI 2`,
you flashed `c61942b` / run `34135448354`. Slice 1 (paced resume) lived;
leftover firmware GSI 2 stole PIC virtio. Re-flash from this leftover-GSI2
pin. Do not F11 `34135448354` again expecting `ISO-INSTALL-OK`.

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
  (`MADT or MP tables are not detected`; leftover GSI 2 stole PIC).
  This pin keeps paced overlay PIT but injects Linux PIC until Linux
  writes pin 2 (`linux PIC before leftover GSI 2`). Do not F11
  `34135448354`, `34131274237`, or `34080595540`.
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

## 5. After Phase A (Phase B, not this flash)

1. Keep the installed leftover disk across a **guest** F7 reset only (reset cap
   is 1). A host reboot of RayNu-V zeros leftover DRAM — the nested disk does
   not persist across HV reboot unless we later persist it.
2. SPA/REST: create-VM + attach so E4 is “SPA starts that installed disk,” not
   “list/start/stop a SHELL stub.”
3. Still no TLS requirement for M7 (deferred).

---

## Honesty

- Do not print `RAYNU-V-M7-ISO-INSTALL-OK` from nested, host, or CI.
- Do not treat `RAYNU-V-RAYNU-F-DISK-BOOT-OK` as the iron marker.
- Do not treat `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` as this distro install.
- HDA `last_commit` stays `2b795a0` until iron E5 evidence exists.
- Months stay 0.5 until COM2 prints the iron marker.
