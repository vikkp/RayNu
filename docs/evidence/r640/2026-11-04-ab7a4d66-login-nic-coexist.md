# 2026-11-04 — `ab7a4d668fef`: the drop is `:38`/`:39` NIC coexistence, not the webui

**EFI:** `ab7a4d668fef` (CI push run `41912339727`). COM2
`build: sha=ab7a4d668fef run=41912339727`. Fourth iron boot of the wave-5 E4
sign-in work (after `c3cdc6c1`, `d57ff3b9`, `6076a652`). Only code change
over `6076a652` is `assets/webui.html` (the login retry).

## Three launches this boot

1. **First launch — SPA did not come up at all.** The whole boot printed
   **zero** `HOST-NIC TCP accept` lines. The browser's packets reached us
   (`rx to=us etype=0800 src=fc:3f:a6:99:a3:92` repeated during RayNu-F and
   early guest boot) but no smoltcp socket ever accepted the connection.
2. **Subsequent launch — SPA loaded.** Two accepts printed (`slot=0`,
   `slot=1`). The static page, `GET /auth/operator` (`Operator: vikkp`),
   `GET /host/cert` (`Certificate: lab`) and the first `GET /host/overview`
   (prefix 24, DHCP) all succeeded.
3. **Then the connection wedged.** The Host card flipped to **Cannot reach
   the server** (a later `/host/overview` poll failed) and sign-in showed
   **Signing in…** then **Account dropped** — the webui retried the POST six
   times over ~5.6 s and every attempt threw.

## Why the webui retry cannot fix this

Whether a `HOST-NIC TCP accept` prints is decided at the smoltcp/TCP layer,
before any HTML or HTTP is served. The webui is bytes we hand back *after*
an accept. So:

- Launch 1 (no accept at all) is untouchable from `webui.html`.
- In launch 2 the failing requests include `GET /host/overview`, a plain
  poll with no auth body. It failed at the same time as the login POST. This
  is not an auth fault and not a login-specific fault — the **connection**
  died. A client retry over a dead keep-alive connection just throws again,
  which is exactly what the six attempts did.

The earlier `6076a652` note guessed this was a one-time, self-clearing
transient at the egress reset. This boot disproves "one-time": in launch 2
the accepts happened *after* the M8.9 egress had already armed, the page
still loaded, and the connection still wedged a few requests later. The
perturbation is **ongoing while the guest network is up**, not a single
reset event.

## Root cause (strongly supported, not yet iron-instrumented)

The host management listener (`:38`, `10.99.99.146`) and the M8.9 guest
egress (`:39`) are the **two PCI functions of the same physical BCM5720**.
They share the chip's DMA engines and memory arbiter. `arm_egress`
(`mgmt/bcm5720_mmio.rs`) brings function 1 up with `grc=bswap+wswap
memarb=1 bufmgr=1` and then the guest drives real traffic on `:39`
(`M8.9 net post-tx … vtx/vrx` rising). The `:38` listener is **poll-mode,
MSI-X off** (ADR-013). Once `:39` is actively DMAing:

- a SYN can land in a window where the shared rings are mid-reconfigure and
  no `:38` socket completes the accept (launch 1, and the `1da151be`
  `curl: (7)`), and
- an established `:38` connection stalls when its poll-mode rx/tx DMA is
  starved or desynced by `:39` (launch 2: Host poll + login both throw).

It clears only when `:39` goes quiet. This matches every prior boot:
sign-in worked on `1cf82795`/`bac1f0ec` when the user hit it before the
guest network was busy, and failed on `c3cdc6c1` / `d57ff3b9` / `6076a652` /
`ab7a4d66` when it landed while `:39` was active.

## What this means for E4

The fix is **server-side on the shared BCM5720**, e.g. recover / re-point
the `:38` host rings when the tick detects `:38` cannot make progress while
`:39` is active (touching only `:38`'s own BAR registers — not the PHY, not
phylock, not a GRC core reset), or move `:38` off naïve poll-mode. That is
the E4 iron thread and must be **developed and validated on the rack**: it
cannot be reproduced on the host (std TCP has an accept backlog and there is
no shared dual-function NIC), and a wrong write to shared chip state can
drop the iDRAC NCSI analog or regress the **lived** M8.9 egress. It is not
shipped blind.

The cheap confirming experiment before any re-arm: log `:38`'s
`rx_prod`/`rx_cons`/`rx_drop` and `tx_prod`/`tx_cons` on the coexist tick
once `:39` is armed (read-only MMIO + COM2), to show the `:38` rx DMA
stalling while `:39` runs.

**E4 stays in flight.** Not lived. HDA stays 99%. The webui login retry
(`ab7a4d66`) is harmless but is **not** the fix and is not what closes E4.
