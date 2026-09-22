# M8 state — START HERE (persist / LOI Bar A recovery)

> **Read this before touching `mgmt/xhci.rs`, `mgmt/durable_lun.rs`, `mgmt/disk_persist.rs`, `vmx/guest_uefi.rs` (RayNu-F boot source), or the trackers.**  
> Last rewrite: 2026-09-22 (Phase 0 EFI `5c32bd06` lived: soak flag seen, USB never enumerated, RAM install reached `login:`; enumeration fix + soak halt built, **not yet lived**). Trackers: [`hda.md`](hda.md) (Everest, closed) · [`loihda.md`](loihda.md) (LOI, open). Plan: [`m8_plan.md`](m8_plan.md). ADR: [ADR-018](adr/ADR-018.md). Evidence: [`2026-09-22-5c32bd06-soak-enum-timeout.md`](evidence/r640/2026-09-22-5c32bd06-soak-enum-timeout.md).

## One paragraph

Everest (M7) is closed on iron. M8 is operator hardening. Bar A of the LOI needs one R640 to boot the **installed** Alpine from the persist disk (Toshiba USB, 8 GiB virtio slice) after Force Off and sit at `localhost:~#`, then a browser stays on HTTPS (A4s) and types into the guest (A6). That worked on `4af78b43` and `928d6224` (`RAYNU-V-M8-DISK-PERSIST-OK`, `RAYNU-V-M8-TLS-OK`). Seven flashes since have **not** reached the installed login. Every failure has the same shape: a USB BOT command on the Toshiba gets no Transfer Event (`cmpl=0xff` / `err=8`), usually after tens of seconds of idle. The fixes since have been scheduling heuristics (`warm`, `diskprime`, `peekretry`, `pin`) written against one COM2 each. Phase 0 makes a miss **safe and visible**; Phase 1 makes the USB driver **deterministic from evidence** instead of guesses.

## Known-good iron builds (flash these, nothing else, for a demo)

| Purpose | EFI | What it proved | Not this |
|---------|-----|----------------|----------|
| Everest rollback | `f72b4276` (GitHub Latest `v0.1.0-everest-closed`) | SPA Start → ISO install → disk boot → `login:` on leftover DRAM | persist across HV reboot, TLS |
| Persist + TLS close | `928d6224` | Force Off → `keep=1` → `DISK-BOOTX64` → menu → `root=UUID=dd673a9a` → `login:`; `RAYNU-V-M8-DISK-PERSIST-OK`; `RAYNU-V-M8-TLS-OK` on `10.99.99.140:8443` | standing SPA after login, console keys |

Every EFI after `928d6224` is a **prototype**. Do not treat a later tip as known-good until it reaches the installed `login:` on COM2 **more than once**.

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

### Phase 1a — enumeration (built after `5c32bd06`; **not yet lived**)

`5c32bd06` COM2, read closely: the No-Op posted, three Enable Slots posted, the p14 hub's Address Device and GET_DESCRIPTOR posted. The only commands that never posted were **SET_ADDRESS on the two external HS devices** (Toshiba p11, UDisk p10). Two spec gaps fit that exactly and are fixed in this EFI:

| Gap | Fix | COM2 line |
|-----|-----|-----------|
| `reset_port` returned the instant PED=1 and Address Device went on the wire microseconds later. USB 2.0 §9.2.6.2 says no request for 10 ms after reset (TRSTRCY); Linux waits 50 ms. A device still in recovery NAKs the STATUS stage and the xHC retries NAKs forever = `cmd=3 cmpl=0xff`. The hub is a different silicon and happened to be ready. | `USB_PORT_RESET_RECOVERY_MS = 50` after every port reset and before the Address Device retry; 1 ms after HCRST (Intel). | none (silent) |
| `handshake_legacy` set OS-owned and hoped. It never disabled BIOS SMIs in USBLEGCTLSTS, never forced a BIOS that keeps the controller, and RayNu-V writes the PCI command register (an SMI source) before the handoff. Dell legacy USB (iDRAC keyboard behind the p14 hub) can keep driving the controller from SMM. | Force ownership after the wait; clear every SMI enable; ack pending SMI events (same mask as Linux `quirk_usb_handoff_xhci`). | `xhci legacy pre sup=0x… ctl=0x… -> sup=0x… ctl=0x… forced=0/1` and `xhci legacy post …` |
| A command timeout printed only `cmpl=0xff`. | `xhci cmd timeout nop/slot/addr/addr2 p… cmd= sts= crcr= iman= erdp= cmdenq= cmdcyc= evdeq= evcyc= evtrb= portsc= pmsc= slotst= ep0st= ep0deq=` before every abort. | that line |
| A soak boot whose USB failed to enumerate fell through to the ISO on leftover DRAM and printed a RAM `login:`. | `usbsoak.txt` + enumeration failure → `USBSOAK abort — USB enumeration failed err=N` and halt. No guest. | that line, then `USBSOAK halt alive` |

Reading the next `xhci cmd timeout addr` line if it still appears: `sts` bit 12 (HCE) or bit 2 (HSE) set = the controller stopped; `iman` bit 0 set with `evtrb` cycle ≠ `evcyc` = an event was posted that we are not seeing; `slotst=1` (Enabled) with `ep0st=0` = the xHC never ran the command; `slotst=2` (Default) with `ep0st=1` (Running) = SET_ADDRESS is on the wire and the device is NAKing; `portsc` PLS ≠ 0 = the link left U0 mid-command. `ctl` on the `legacy pre` line with bit 0 / 13 / 14 / 15 set = the BIOS had SMIs armed on this controller.

### Phase 1b — make USB deterministic from evidence (bench in this EFI; driver fix after)

| Change | Where | COM2 line |
|--------|-------|-----------|
| Register/ring/endpoint dump on every guest-path timeout: USBCMD/USBSTS/CRCR/IMAN/IMOD/ERDP, event dequeue + cycle + the TRB sitting there, PORTSC/PORTPMSC, EP0/OUT/IN state and TR Dequeue vs our enqueue | `xhci.rs` `serial_xhci_timeout_dump` | `xhci timeout rw p11 cmd=… sts=… crcr=… erdp=… evdeq=… evtrb=… portsc=… pmsc=… out(st= enq= deq=) in(…)` |
| **USB soak bench**: ESP `EFI/RayNu/usbsoak.txt` → after `usb I/O ready` read LBA0/1/2048/4096 with idle gaps 0/5/30/120 s (200/24/10/5 reads), print ok/fail/max_ms per gap, dump on every miss, then halt with a heartbeat. No guest. ~17 min. | `boot/usb_soak_flag.rs`, `xhci::xhci_usb_soak` | `USBSOAK gap_s=30 n=10 ok=… fail=… max_ms=… first_fail=…` then `RAYNU-V-USBSOAK-DONE` |
| Hypotheses the dump decides: (a) event ring bookkeeping (`evtrb` cycle ≠ `evcyc` with `erdp` behind), (b) xHC never consumed the TRB (`deq` == ring base + old index, EP `st=1`), (c) device/bridge idle (EP Running, TRB consumed, no event: bridge NAK/spin-up), (d) link state (`portsc` PLS not U0, `pmsc` L1/HLE) | read the dump | — |
| Then: one mechanism replaces `warm`/`diskprime`/`past pin`/`peekretry` (keepalive TUR on idle, or Stop EP + Set TR Deq on the exact stale TRB, or ERDP fix) | after soak evidence | — |

**p1c note:** the Toshiba sits on a **USB 2 (HS)** port (`PORTSC` speed field = HS, `mps=64`). USB3 U1/U2 timeouts do not apply; USB2 L1 needs `HLE`=1, which the driver never sets. LPM is therefore **not** a leading hypothesis; `pmsc` is in the dump to confirm, not to act on.

### Phase 2 — re-close Bar A on the deterministic driver

A2 again (several Force Offs, not one) → A4s Firefox load/reload on `https://raynu-v.lab:8443` after `login:` → A6 type from the SPA (`RAYNU-V-M8-CONSOLE-OK`) → A5 ESP `auth.token` default.

### Phase 3 — Bar B honestly

PERC H740P = MegaRAID SAS 3.5 (MPT3 Fusion) post-EBS driver on a **spare** VD (never Ubuntu). Interim: an NVMe in the lab R640 uses the already host-proven NVMe DurableLun and is a stronger dedicated-box story than a USB stick.

## Next iron step (operator)

1. **Force Off** the live `localhost:~#` if it is still up. It is the 1 GiB RAM disk from `5c32bd06` (`vda` = 2097152 sectors, UUID `4c27e121`). It is not the Toshiba. Tell: the Toshiba slice is `[vda] 16777216 512-byte logical blocks (8.59 GB/8.00 GiB)`; RAM is `2097152 … 1.00 GiB`.
2. Flash this branch's EFI (`flashcruzer.sh --branch cursor/m8-phase0-failsafe-8366 --wait --any-cruzer-usb --allow-new-serial --raynu-f --linux-iso …`). Never `--init-new-cruzer`. Never Toshiba `/dev/sdc`. **Leave `usbsoak.txt` on the Cruzer** (it is still there).
3. **Boot 1 — soak:** F11. Expect `xhci legacy pre …` and `xhci legacy post …` (paste both), then either `usb I/O ready` → `USBSOAK gap_s=…` ×4 → `RAYNU-V-USBSOAK-DONE` (~17 min), **or** `xhci cmd timeout addr …` → `USBSOAK abort — USB enumeration failed` → halt. Either way no guest runs. Paste the whole COM2.
4. **Boot 2 — product path:** only after boot 1 printed `RAYNU-V-USBSOAK-DONE`. Remove `usbsoak.txt`. Expect the installed menu → `login:` with `[vda] 16777216` (sit there; A4s Firefox), or `WARN LUN saw EFI PART; skip ISO` → `Stage 46 hold alive`. **No** `image=ISO-BOOTX64` on the Toshiba.
5. Do not curl `POST /vms/1/start`. Do not run `setup-disk`. Do not F11 any EFI in the prototype table above.

## Rules that stay true

- Latitude/QEMU ≠ R640. Host TLS ≠ iron HTTPS. Nested ≠ iron. Leftover DRAM ≠ persist. USB ≠ PERC.
- Iron markers (`RAYNU-V-M8-*-OK`, `RAYNU-V-M7-ISO-INSTALL-OK`) are never printed from host/CI/nested. `RAYNU-V-USBSOAK-DONE` is iron-only too.
- Do not put an HD node on `HANDLE_DISK` (ADR-017). Do not `recover_pipes` (Reset Endpoint) on a Running pipe. Do not revive the Enter-key / wall-cap-rebase theory. Do not uncap the 8 GiB virtio. Do not add rustls to `uefi-bin`.
- Score changes: down is allowed on evidence of regression; up only on a repeated iron close.
- Tracker updates: one row per **iron boot**, one scoreboard sentence per **gate**. Put COM2 narrative in `docs/evidence/`, not in code comments.
