# 2026-09-23 — `1fa231df` soak boot: commands completed, completion events lost (torn event read)

EFI `1fa231df46a3` (CI run `35798413296`). ESP had empty `EFI/RayNu/usbsoak.txt` and `raynuf.txt`.
Lease `10.99.99.150/24`. No guest ran. Boot ended in `USBSOAK abort — USB enumeration failed err=3` and the halt heartbeat.

## What printed

```
xhci legacy pre  sup=0x01002201 ctl=0xe0010000 -> sup=0x01002201 ctl=0x00010000 forced=0
xhci hcrst
xhci legacy post sup=0x01002201 ctl=0x00000000 -> sup=0x01002201 ctl=0x00000000 forced=0
xhci ccs p10=0x000206e1 p11=0x000206e1 p14=0x000206e1
xhci p11first
xhci nop
xhci cmd timeout nop   p0  cmd=0x1 sts=0x18 crcr=0x8 iman=0x1 erdp=0x1406ff040 cmdenq=1 cmdcyc=1 evdeq=4  evcyc=1 evtrb=0x0/0x0 portsc=0x2a0 pmsc=0 slotst=0 ep0st=0
xhci abort crr=0 · nopretry
xhci cmd timeout addr  p11 cmd=0x1 sts=0x18 crcr=0x8 iman=0x1 erdp=0x1406ff090 cmdenq=3 cmdcyc=1 evdeq=9  evcyc=1 evtrb=0x0/0x0 portsc=0xe03 pmsc=0 slotst=2 ep0st=1 ep0deq=0x1406fe001
xhci abort crr=0 · addrretry p11
xhci cmd timeout addr2 p11 … evdeq=11 … slotst=0 ep0st=0
xhci enum p11 sc=0xe03 speed=3 cmd=3 cmpl=0x13
xhci enum p14 vid=0x1604 did=0x10c0 class=0x09 proto=0x01 · hub skip
xhci cmd timeout slot  p10 … evdeq=24 … slotst=0 ep0st=0 · abort · slotretry p10
xhci cmd timeout addr  p10 … cmdenq=2 evdeq=26 … slotst=0 ep0st=0 · abort · addrretry p10
xhci cmd timeout addr2 p10 … evdeq=28 … slotst=0 ep0st=0 · abort
xhci enum p10 sc=0xe03 speed=3 cmd=3 cmpl=0xff timeout
durable LUN usb I/O fail err=3 portsc=0x0000000a00000e03 cmpl=0x00000000000303ff bot=? scsi=?
USBSOAK abort — USB enumeration failed err=3; no guest this boot
USBSOAK halt — Force Off when done reading COM2
```

## Decode

**The two Phase 1a hypotheses are falsified.**

- BIOS SMIs were never armed. `legacy pre ctl=0xe0010000` is bits 29–31 (RW1C SMI *event* flags) plus bit 16 (RO status). Every SMI *enable* (bits 0/4/13/14/15) was already clear, and USBLEGSUP already read OS-owned (`0x01002201`, bit 24 set, bit 16 clear, `forced=0`). The handoff code is correct practice and stays, but it was not the fault.
- 50 ms TRSTRCY did not change the outcome. And the **No-Op** (`cmd timeout nop`) and **Enable Slot** (`cmd timeout slot p10`) timed out too. Neither touches a USB device. This is not the Toshiba.

**What the dumps say.**

- Every dump: `cmd=0x1` (R/S set), `sts=0x18` (EINT + PCD only; HCH=0, HSE=0, HCE=0), `crcr=0x8` (CRR=1, ring running, idle). The controller was healthy and had finished whatever it was given.
- `iman=0x1` (IP) every time: the xHC has posted events. `evtrb=0x0/0x0` at our dequeue: nothing new sits there.
- **`slotst=2 ep0st=1 ep0deq=0x1406fe001` on the first p11 Address Device.** xHCI Table 6-7 Slot State: 0 Disabled/Enabled, 1 Default, **2 Addressed**, 3 Configured. The output Slot Context is Addressed, EP0 is Running, EP0 TR Dequeue was written by the xHC. **SET_ADDRESS completed.** The Command Completion Event was never seen by us.
- The p11 retry then returned `cmpl=0x13` Context State Error — Address Device on a slot that is already Addressed. Independent confirmation that the first one succeeded.
- `evdeq=4` at the No-Op "timeout". The prime No-Op runs before any port reset; the only events before it are the three Port Status Change Events for p10/p11/p14. The **fourth** event was consumed and thrown away. It was the No-Op completion.

**Why the completion was thrown away.**

`consume_posted` (and `consume_control`, `consume_bulk_pair`, `drain_events`, `abort_cmd_ring`) read the 16-byte Event TRB with a byte loop: bytes 0–7 (TRB pointer) first, 8–11 (status/completion code), then 12–15 (control word with the cycle bit) **last**. The xHC writes the whole 16-byte event as one transaction. When it lands between our read of byte 0 and byte 12, the loop sees a fresh cycle bit and a fresh TRB type with a **stale zero Command TRB Pointer** and a **stale zero completion code**. `xhci_cmd_event_matches` says "not our TRB", the loop `continue`s past it, and the real completion is gone. The wait then spins to `cmpl=0xff` while the xHC sits idle with CRR=1 and the slot Addressed. Same failure for transfers (`xhci_event_trb_ptr != want_ptr` → skip) and for bulk pairs (code 0 → `Err(Xfer)` stamped `cmpl=0`).

Two earlier iron oddities now have a cause and were the same torn read one byte boundary apart:

- "Some Lewisburg completions arrive with Command TRB Pointer 0 on Success" (doc comment on `xhci_cmd_event_matches`) — landing between byte 7 and byte 8.
- "leftover Invalid (CC=0)" completions, for which `cmd_cc_invalid` was written — landing between byte 11 and byte 12.

An xHC never emits either. Linux guards exactly this with `rmb()` between reading `TRB_CYCLE` and the rest of the TRB (`xhci-ring.c`, `xhci_handle_event`).

**Why it is intermittent.** A completion lands a few µs after the doorbell, while the poll loop is spinning on the cached line. The tear window is the ~12 byte loads before the control word. The hub on p14 got through by luck on both `5c32bd06` and this boot; the persist-OK boots got through thousands of transfer events with the retry heuristics (`warm`, `diskprime`, `peekretry`, `addrretry`, `slotretry`, `nopretry`, `cmdptr`) papering over the misses. Every one of those was written against a torn read.

## Fix (this branch)

- `poll_event`: read the control word first as **one aligned 32-bit load** (`dma_read32`, `read_volatile::<u32>`), compare the cycle bit, acquire fence, then read the TRB. Used by every event wait (`consume_posted`, `consume_control`, `consume_bulk_pair`, `drain_events`, `abort_cmd_ring`).
- `write_trb`: parameter + status first, fence, then the control word as **one aligned 32-bit store** (`dma_write32`) so cycle bit and TRB type land together and last (xHCI 4.9.2).
- COM2 marker on the build: `xhci trb order event cycle-first, write cycle-last` after `xhci legacy post`.
- Host test `event_poll_never_tears_the_completion_it_waits_for`: a DMA mock lands the 16-byte completion after N byte reads for N in 0..48; the old byte-order poll tears for 12 of them, the new poll retires the command for all 48.

## Not this

Not `RAYNU-V-USBSOAK-DONE`. Not persist. Not the Toshiba's fault. Do not F11 `1fa231df` again expecting the soak: the same race hits the first commands. The TRSTRCY delay and the BIOS SMI handoff stay in the driver as correct practice; neither is the fix.
