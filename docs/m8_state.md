# M8 state — START HERE (persist / LOI Bar A recovery)

> **Read this before touching `mgmt/xhci.rs`, `mgmt/durable_lun.rs`, `mgmt/disk_persist.rs`, `vmx/guest_uefi.rs` (RayNu-F boot source), or the trackers.**  
> Last rewrite: 2026-09-25 (`1f33eeda72f9`: sixth `login:`, one TLS session, Host green, guest COM1 in Activity, chassis stayed up. A4s lived. Rollback kit `v0.1.0-m8-a4s`. Next is A6). Trackers: [`hda.md`](hda.md) (Everest, closed) · [`loihda.md`](loihda.md) (LOI, open). Plan: [`m8_plan.md`](m8_plan.md). ADR: [ADR-018](adr/ADR-018.md). Evidence: [`2026-09-24-36d3b559-persist-login.md`](evidence/r640/2026-09-24-36d3b559-persist-login.md). Kit: [`releases/v0.1.0-m8-a4s/`](../releases/v0.1.0-m8-a4s/).

## One paragraph

Everest (M7) is closed on iron. M8 is operator hardening. Bar A of the LOI needs one R640 to boot the **installed** Alpine from the persist disk (Toshiba USB, 8 GiB virtio slice) after Force Off and sit at `localhost:~#`, then a browser stays on HTTPS (A4s) and types into the guest (A6). `1f33eeda72f9` did the sixth login of UUID `a0ad99ac-ca25-4458-ade1-cd8599ca4928` (lease `10.99.99.154`) and kept one Firefox session up: one `TCP accept`, then `HTTP keep-alive`, Host green, Activity showed `localhost login:`. The `cmpl=0xff` tear is closed (`e5cca2e0` soak 239/239). A4s is lived. A6 is the open step. Leave this page up. Do not flash over it.

## Known-good iron builds (flash these, nothing else, for a demo)

| Purpose | EFI | What it proved | Not this |
|---------|-----|----------------|----------|
| Everest rollback | `f72b4276` (GitHub Latest `v0.1.0-everest-closed`) | SPA Start → ISO install → disk boot → `login:` on leftover DRAM | persist across HV reboot, TLS |
| Persist + TLS close | `928d6224` | Force Off → `keep=1` → `DISK-BOOTX64` → menu → `root=UUID=dd673a9a` → `login:`; `RAYNU-V-M8-DISK-PERSIST-OK`; `RAYNU-V-M8-TLS-OK` on `10.99.99.140:8443` | that filesystem was wiped; standing SPA after login |
| Persist repeat (this disk) | `36d3b559` | Fresh install, then three boots → menu → `root=UUID=a0ad99ac-…` → `login:`. Opening the SPA was followed twice by `SYS1003` then `SYS1001` with no `RAC1195` | Do not F11 this EFI to open Firefox |
| Fourth login, SOL paced | `fa6ce771` | Same UUID, `[vda] 16777216`, vda2 counts match boots 2 and 3, lease `.150`, `localhost:~#`. SPA then went green and the dashboard showed Power State OFF. Lifecycle: seq 22520/22521 at 00:30:20 | Do not F11 this EFI to open Firefox. Paced SOL RX did not stop the power-off |
| Fifth login, 30 s listen hold | `3e9ce45e` | Same UUID, lease `.151`, vda2 `102909/2084352`. Host red, then green, then red. Seq 22543/22544 at 12:21:23, no `RAC1195` | Do not F11 this EFI to open Firefox. The re-listen is what turned Host green |
| A4s lived (rollback) | `1f33eeda` | Sixth login, lease `.154`, same UUID, vda2 `102909/2084352`. One `TCP accept`, then `HTTP keep-alive`. Host green. Activity showed Alpine login. Chassis stayed up. Kit `v0.1.0-m8-a4s`, CI `36137732145`, SHA256 `5539d83806e120eb6c331c11281407c0f902b121a770c7faa46d6a1a26280693` | Leave the live page. Do not Send. Do not flash over this session. Flash this kit later to return to this path |

The page on `1f33eeda` is up. Leave it. Do not Power On `3e9ce45e` or `3f80abd0` to open Firefox. The rows below are prototypes that did not reach a repeated login.

## USB bench passed (do not F11 this EFI again for `login:`)

| EFI | What lived | Next |
|-----|-----------|------|
| `e5cca2e0` soak | `xhci trb order …` then Toshiba `0480:a004`, peek `installed=1 guest=8589934592`, `USBSOAK` 239/239, `RAYNU-V-USBSOAK-DONE`, halt, **no guest** | Bench is closed. |
| `e5cca2e0` boot 2 | No soak flag. `keep=1`, `image=DISK-BOOTX64`, `BOOTX64.EFI bytes=139264`, GRUB 2.12 `grub>`, `diskprime past pin` (that function READ LBA 0), wall cap `blk_rd=33 blk_wr=0` | **Do not F11 again.** See the prototype row. |

## Prototypes that did not reach the installed login (do not F11 again expecting the menu)

| EFI | What lived | Class |
|-----|-----------|-------|
| `b5e290be` | per-exit COM2 `inb` powered the chassis **off** | never reintroduce per-exit SOL poll |
| `17d120c5` | HTTPS during diskprime → `no GPT`, no Alpine | USB read after idle |
| `3b388279` | peek `gpt=1` → `no GPT` ×3 → `image=test-app` → dead `product ISO hold` | USB read after idle + dead hold |
| `08202468` | peek `gpt=1 usb_err=8` → `image=ISO-BOOTX64` → `vda` I/O error / RCU stall | **ISO fall-through on a probe miss** |
| `d60431ee` | `lba1=pin` → `DISK-BOOTX64` → `grub>` → wall cap 180 s → dead hold | pin skipped the warm; GRUB reads cold |
| `28cd4ff1` | `warm=1` ×2 → `DISK-BOOTX64` → `grub>` | warm before GRUB, `grub.cfg` read cold |
| `15e3d665` | peek `efi=EFI PART … usb_err=8 installed=0` → `diskprime lba1=miss err=8` → `image=ISO-BOOTX64` → live installer | **ISO fall-through on a probe miss** (worse USB) |
| `5c32bd06` | `USB soak requested` then p11 and p10 Address Device `cmd=3 cmpl=0xff` (`err=3` Enum). No `USBSOAK`. `image=ISO-BOOTX64` onto **1 GiB leftover DRAM**; F7 `DISK-BOOTX64` → `login:` UUID `4c27e121`. Toshiba never opened | **enumeration timeout before the soak**; RAM login is not persist |
| `1fa231df` | `legacy pre/post forced=0` (no BIOS SMIs armed); No-Op, p11 Address Device, p10 Enable Slot all `cmpl=0xff` with `crcr=0x8` idle and **`slotst=2 ep0st=1`** (Addressed) on p11; retry `cmpl=0x13`. `USBSOAK abort err=3` → halt, **no guest** | **completion events lost to a torn 16-byte event read**; the soak halt worked (no RAM install) |
| `e5cca2e0` boot 2 | No `USB soak requested`. Peek `installed=1`, `keep=1`, `image=DISK-BOOTX64`, GRUB 2.12 `grub>`, `diskprime past pin`, wall cap `blk_rd=33 blk_wr=0 wall_ms=180005`, then `Stage 46 hold alive` | **in-GRUB warm read LBA 0**; same `blk_rd` as the pin-only `grub>` on `d60431ee`. Evidence: [2026-09-24-e5cca2e0-grub-wallcap.md](evidence/r640/2026-09-24-e5cca2e0-grub-wallcap.md) |
| `c4a41a17` | No soak. No `diskprime past pin`. `keep=1`, `image=DISK-BOOTX64`, `BOOTX64.EFI bytes=139264`, `disk past pin lba=2048 n=512 ok` during the ESP BPB read, then GRUB 2.12 `grub>`. Force Off before the wall cap | **one-shot consumed by firmware staging**, not by GRUB. Evidence: [2026-09-24-c4a41a17-grub-staging-oneshot.md](evidence/r640/2026-09-24-c4a41a17-grub-staging-oneshot.md) |

## Why it kept failing (diagnosis, 2026-09-22)

1. **Foundation is the flakiest part.** Persist = consumer USB HDD via a from-scratch poll-mode xHCI + BOT driver, single outstanding command, no interrupts, no timeout diagnostics beyond `cmpl=0xff`.
2. **One data point per flash.** Nested QEMU `MODE=usb` always passes; iron fails; there was no USB bench on the R640. ~83 `fix(m8)` commits vs ~103 tracker commits in 30 days.
3. **Destructive default.** A probe miss staged the installer ISO, and the auto-answer carried `ERASE_DISKS=/dev/vda … setup-disk`. That is why every miss was a Force Off drill.
4. **USB inside the guest's critical path.** BlockIo → USB spins up to `FIRST_READ_SPINS` (~25 s per stage), settles ~5 s each, on the BSP, inside a vmexit: frozen guest, dead SPA, silent COM2, then the 180 s wall cap into a dead spin.
5. **Local fixes regressed the working path.** `928d6224` always drained + INQUIRY + READ LBA0 before GRUB; the pin (`08202468`/`d60431ee`) returned early and skipped it; `28cd4ff1` restored it too early; `15e3d665` never reached disk stage because the 34-sector pin store itself needs USB.
6. **Tracker overhead.** Per-COM2 paragraphs in `hda.md`/`loihda.md`/`xhci.rs` grew faster than the code was fixed.

## The plan

### Phase 0 — make a miss safe and visible (lived on `5c32bd06`; the guard did not arm)

`5c32bd06` saw `usbsoak.txt` and then failed **Address Device** on p11 and p10 (`cmd=3 cmpl=0xff`, `err=3` Enum, PORTSC already HS U0). The soak, the ISO skip, and the `setup-disk` withhold all require a serving LUN. None of them ran. The boot installed Alpine onto 1 GiB leftover DRAM and a guest reboot reached `login:`. That disk is RAM. Force Off drops it. The Toshiba was not read or written.

| Change | Where | COM2 line |
|--------|-------|-----------|
| One `EFI PART` read on a serving LUN forbids the ISO for the whole boot, whatever pin/sticky/FAT said | `disk_persist::persist_lun_iso_forbidden`, `guest_uefi.rs` boot source | `WARN LUN saw EFI PART; skip ISO (do not wipe persist)` |
| `setup-disk` auto-answer withheld when a durable LUN serves unless the operator did SPA Start on blank media; never once `EFI PART` was seen | `guest_serial_answer::setup_wipe_allowed` | `auto-answer setup-disk WITHHELD …` |
| Guest-path USB waits bounded by **time** (8 s/attempt), settle 2 s, heartbeat every 2 s; GRUB gets `EFI_DEVICE_ERROR` instead of a frozen box | `xhci.rs` `USB_RW_DEADLINE_MS`, `BOT_SETTLE_MS`, `xhci_wait_expired` | `durable LUN usb rw waiting ms=N of 8000 bot=…` |
| Stage 46 hold is live: coexist SPA ticks, heartbeat every 60 s | `iso_install::stage46_live_hold` | `Stage 46 hold alive t_s=N (SPA ticking …)` |

### Phase 1a — enumeration (root cause found on `1fa231df`; fix **lived** on `e5cca2e0`)

**Lived on `1fa231df` (soak boot, no guest).** The dump lines built after `5c32bd06` did their job. Every command that "timed out" left `cmd=0x1 sts=0x18 crcr=0x8` — controller running, no error, command ring idle — and the first p11 Address Device left the output Slot Context **Addressed** (`slotst=2`, xHCI Table 6-7: 0 Disabled/Enabled, 1 Default, 2 Addressed, 3 Configured) with EP0 Running and its TR Dequeue written. The retry returned `cmpl=0x13` Context State Error, which is Address Device on an already-Addressed slot. The No-Op "timeout" showed `evdeq=4` when only three Port Status Change Events precede it. So: **the commands completed and the driver consumed and discarded their completion events.** The No-Op and Enable Slot failing too rules out the device.

| Finding on `1fa231df` | Meaning |
|-----------------------|---------|
| `legacy pre ctl=0xe0010000 forced=0`, `sup=0x01002201` already OS-owned | BIOS SMIs were never armed; the SMI hypothesis is falsified. Handoff code stays (correct practice). |
| Address Device still `cmpl=0xff` after 50 ms TRSTRCY; No-Op and Enable Slot also `cmpl=0xff` | The reset-recovery hypothesis is falsified. Delay stays (spec). |
| `slotst=2 ep0st=1 ep0deq=0x1406fe001` then retry `cmpl=0x13` | SET_ADDRESS succeeded; its event was lost. |
| `evdeq=4` at the No-Op timeout | The fourth event (the No-Op completion) was consumed as "not ours". |

**Root cause.** `consume_posted` / `consume_control` / `consume_bulk_pair` / `drain_events` / `abort_cmd_ring` read the 16-byte Event TRB with a byte loop, pointer first, control word (cycle bit) **last**. The xHC writes the event as one 16-byte transaction. Land it between our byte 0 and byte 12 and the loop sees a fresh cycle + type with a stale zero Command TRB Pointer and a stale zero completion code; `xhci_cmd_event_matches` rejects it, the loop skips it, and the wait spins to `cmpl=0xff`. The iron "Command TRB Pointer 0 on Success" and "leftover CC=0 Invalid" completions recorded in `xhci.rs` comments were the same tear one byte boundary apart — an xHC emits neither. Linux: `rmb()` after `TRB_CYCLE`. Every retry heuristic since `928d6224` (`nopretry`, `slotretry`, `addrretry`, `cmdptr`, `warm`, `diskprime`, `peekretry`) was written against this race.

| Fix (this EFI) | Where | COM2 line |
|----------------|-------|-----------|
| Event poll reads the control word first as one aligned 32-bit load, checks the cycle bit, acquire fence, then reads the TRB | `xhci::poll_event`, `XhciHw::dma_read32`, all five wait loops | `xhci trb order event cycle-first, write cycle-last` after `xhci legacy post` |
| TRB write stores parameter + status, fence, then the control word as one aligned 32-bit store (xHCI 4.9.2 cycle-last) | `xhci::write_trb`, `XhciHw::dma_write32` | same line |
| Host regression: a DMA mock lands the completion after N byte reads, N in 0..48; old order tears for 12, new order retires all 48 | `xhci_pack_test::event_poll_never_tears_the_completion_it_waits_for` | — |
| Gate needle: every event wait goes through `poll_event`; only the cmd-timeout dump reads a raw TRB at the dequeue | `m8_disk_persist_gate::event_poll_cycle_first_surface_present` | — |

Kept from the `5c32bd06` fix (correct, not the cause): `USB_PORT_RESET_RECOVERY_MS = 50`, 1 ms after HCRST, BIOS SMI handoff + `xhci legacy pre/post` lines, `xhci cmd timeout …` dumps, `USBSOAK abort` halt on enumeration failure.

Reading the next `xhci cmd timeout` line if one still appears **with** the `trb order` line present: `slotst=2 ep0st=1` again would mean the event is being written somewhere other than our dequeue (check `erdp` vs `evdeq`, ERST base); `slotst=0 ep0st=0` with `crcr=0x8` idle would mean the xHC never fetched the command TRB (check the command ring page and CRCR pointer); `sts` bit 12 (HCE) or bit 2 (HSE) = the controller stopped.

### Phase 1b — USB soak (**lived** on `e5cca2e0`: 239/239, including 120 s idle)

| Change | Where | COM2 line |
|--------|-------|-----------|
| Register/ring/endpoint dump on every guest-path timeout: USBCMD/USBSTS/CRCR/IMAN/IMOD/ERDP, event dequeue + cycle + the TRB sitting there, PORTSC/PORTPMSC, EP0/OUT/IN state and TR Dequeue vs our enqueue | `xhci.rs` `serial_xhci_timeout_dump` | `xhci timeout rw p11 cmd=… sts=… crcr=… erdp=… evdeq=… evtrb=… portsc=… pmsc=… out(st= enq= deq=) in(…)` |
| **USB soak bench**: ESP `EFI/RayNu/usbsoak.txt` → after `usb I/O ready` read LBA0/1/2048/4096 with idle gaps 0/5/30/120 s (200/24/10/5 reads), print ok/fail/max_ms per gap, dump on every miss, then halt with a heartbeat. No guest. ~17 min. | `boot/usb_soak_flag.rs`, `xhci::xhci_usb_soak` | `USBSOAK gap_s=30 n=10 ok=… fail=… max_ms=… first_fail=…` then `RAYNU-V-USBSOAK-DONE` |
| Hypotheses the dump decides: (a) event ring bookkeeping (`evtrb` cycle ≠ `evcyc` with `erdp` behind), (b) xHC never consumed the TRB (`deq` == ring base + old index, EP `st=1`), (c) device/bridge idle (EP Running, TRB consumed, no event: bridge NAK/spin-up), (d) link state (`portsc` PLS not U0, `pmsc` L1/HLE) | read the dump | — |
| The mechanism the bench was going to choose is the cycle-first poll from Phase 1a. Idle reads no longer miss. The pre-GRUB `diskprime` LBA0 warm stays (that is what `928d6224` did). The in-GRUB “past pin” warm is deleted: it READ LBA 0 inside the first unpinned `BlockIo` | `xhci::poll_event`, `raynu_f_disk_read` | `RAYNU-V-USBSOAK-DONE` on `e5cca2e0`; boot 2 `blk_rd=33` |

**p1c note:** the Toshiba sits on a **USB 2 (HS)** port (`PORTSC` speed field = HS, `mps=64`). USB3 U1/U2 timeouts do not apply; USB2 L1 needs `HLE`=1, which the driver never sets. LPM is therefore **not** a leading hypothesis; `pmsc` is in the dump to confirm, not to act on.

### Phase 2 — re-close Bar A on the deterministic driver

A2 again (several Force Offs, not one) → A4s lived on `1f33eeda` → A6 type from the SPA (`RAYNU-V-M8-CONSOLE-OK`) → A5 ESP `auth.token` default.

### Phase 3 — Bar B honestly

PERC H740P = MegaRAID SAS 3.5 (MPT3 Fusion) post-EBS driver on a **spare** VD (never Ubuntu). Interim: an NVMe in the lab R640 uses the already host-proven NVMe DurableLun and is a stronger dedicated-box story than a USB stick.

## Product vs probe

The standing page is the product (ADR-013 Stage 1, ADR-018 M8.3, LOI Bar A A4s). One browser session stays up beside the running guest: the Host card stays green, and later the guest console is in that same page. A page that goes red and never answers again is a failed A4s, and it is not something to put in front of an LOI.

`3f80abd0` (no re-listen, Host stays red) is a probe already on this branch. It exists because `3e9ce45e` showed the first exchange leave the chassis up and the second handshake (Host green) precede seq 22543/22544. Do not flash `3f80abd0` for a demo or an LOI. Do not Power On to open Firefox on it.

The product session is in this EFI, still one smoltcp socket on the shared BCM5720:

- One TCP accept and one TLS handshake per browser session.
- HTTP/1.1 `Connection: keep-alive`. The next `/vms` poll is another request on that session. `clear_http` drops that request and leaves the TLS keys. COM2: `HTTP exchange ok` then `HTTP keep-alive`. A later poll prints `HTTP keep-alive` again and does not print a new `TCP accept`.
- A new handshake only after the browser closes (`CloseWait`), the request says `Connection: close`, or 10 minutes with no HTTP (`COEXIST_KEEPALIVE_IDLE_MS`). Those print `TCP re-listen after HTTP`. The 15 s idle abort does not run while the session is kept. Draining a close stays on later ticks. No NIC spin inside the vmexit.
- The SPA does not open a second connection while that session is live. Overview lists sequentially. The Activity log poll waits its turn on the same socket.
- Shared LOM stays as lived: keep the APE PHY, no phylock, no BMCR reset.
- Lines the operator must see after `login:` use `write_line_nowait`. Linux hushes `write_line`.

Still lab, and labeled as lab until each one has its own iron close: millicert, the bring-up bearer, the 8 GiB USB slice. PERC persist is Bar B and is a different disk. A4s is lived on `1f33eeda72f9`: one handshake, keep-alive through Overview and Activity, Host green, guest COM1 visible, chassis stayed up. The rollback kit is `v0.1.0-m8-a4s`. A6 is not closed.

## Next iron step (operator)

The page on `1f33eeda` is the A4s close. Leave Firefox on it. Leave the Send box empty. Do not refresh. Do not click Create or Start. Do not curl. Do not `setup-disk`. Do not flash a new EFI while this session is the one serving the browser.

Rollback for this path, when a later EFI misbehaves, is the kit in [`releases/v0.1.0-m8-a4s/`](../releases/v0.1.0-m8-a4s/) and GitHub release `v0.1.0-m8-a4s` (not GitHub Latest). Flash that file. COM2 must read `build: sha=1f33eeda72f9`. SHA256 `5539d83806e120eb6c331c11281407c0f902b121a770c7faa46d6a1a26280693`. CI run `36137732145`. Everest Latest stays `v0.1.0-everest-closed` (`f72b4276`) for the original install loop on leftover DRAM.

## A6 — SPA keyboard (next code, not this boot)

Iron close is COM2 `RAYNU-V-M8-CONSOLE-OK` after a key from the SPA reaches guest COM1 on the BCM5720 path. `maybe_print_iron_console_ok` prints that marker with `write_line_nowait`. Linux earlycon share hushes `write_line`; the keep-alive lines on `1f33eeda` were visible because they already use `write_line_nowait`. This tip has the marker on that path. It is not lived. The page that is up is still `1f33eeda`, and a Send on that page can inject a key while the marker stays silent.

After this chassis is off:

1. Flash this tip. Do not F11 it while Firefox is connected to `1f33eeda`.
2. After `localhost:~#`, one harmless character in Send. Not `setup-alpine`. Not `setup-disk`.
3. Proof is the character in the guest console on Activity, and `RAYNU-V-M8-CONSOLE-OK` on COM2. The Activity tail also copies host UART lines (`HTTP keep-alive`), so the character in the guest banner is the proof.

Host/CI still never print `RAYNU-V-M8-CONSOLE-OK`. **not VNC**.

`grubcfg=no` on this disk is expected. Alpine keeps `grub.cfg` on ext4 `/boot/grub`, not on the ESP. The menu still auto-booted. A new `TCP accept` or `TCP re-listen after HTTP` followed by `SYS1003` then `SYS1001` with no `RAC1195` means the browser dropped the session. Leave that EFI off.

## Rules that stay true

- Latitude/QEMU ≠ R640. Host TLS ≠ iron HTTPS. Nested ≠ iron. Leftover DRAM ≠ persist. USB ≠ PERC.
- Iron markers (`RAYNU-V-M8-*-OK`, `RAYNU-V-M7-ISO-INSTALL-OK`) are never printed from host/CI/nested. `RAYNU-V-USBSOAK-DONE` is iron-only too.
- Do not put an HD node on `HANDLE_DISK` (ADR-017). Do not `recover_pipes` (Reset Endpoint) on a Running pipe. Do not revive the Enter-key / wall-cap-rebase theory. Do not uncap the 8 GiB virtio. Do not add rustls to `uefi-bin`.
- Score changes: down is allowed on evidence of regression; up only on a repeated iron close.
- Tracker updates: one row per **iron boot**, one scoreboard sentence per **gate**. Put COM2 narrative in `docs/evidence/`, not in code comments.
