# RayNu-V SKU card — dedicated-box lab (Bar A)

**Status: DONE (A3).** One page: what ships, what does not.  
**Audience:** a lab / innovation owner who can dedicate one Dell PowerEdge.  
**Not this page:** RAID-fleet replacement (Bar B). HDA 99% is Everest, not an LOI.

Living LOI tracker: [`docs/loihda.md`](loihda.md) · public: [`site/sku.html`](../site/sku.html) · [`site/loi.html`](../site/loi.html).

This card is commercial honesty. Overstated copy does not survive diligence.

---

## What you are buying

A **dedicated-box** non-production LOI on **one** PowerEdge we own or they dedicate. One EFI binary. Alpine on virtio. Guest disk on an **8 GiB** USB slice. Management is **plaintext HTTP**.

It is **not** a fleet hypervisor, **not PERC**, **not cluster**, **not Windows**.

---

## Ships today (Bar A dedicated-box)

| Ships | Honest bound |
|-------|----------------|
| Single `r640-hypervisor.efi` | No OS, no packages, no config files (ADR-003). |
| Everest loop on a real R640 | Network SPA → Alpine ISO → reboot to disk → `login:` (`f72b4276`). |
| Guest disk survives Force Off | USB BOT on Toshiba `0480:a004`. Guest sees **8 GiB**, not the whole ~298 GiB stick, not leftover DRAM. A2 **DONE on evidence** (`4af78b43`, UUID `348005a9`). Minted persist-OK is wired in-tree, not printed on that flash. |
| Network SPA + REST | Coexist `10.99.99.x:8443`. Flashed `4af78b43` CURL NOW is `http://` (**plaintext HTTP**). In-tree TLS 1.2 EFI listens **HTTPS before RayNu-F**; `1647a8d8` auto-launched Alpine with no native `https://` (Mac `curl: (28)` on SNP `.150`). |
| Lab operator latch | `raynu-v-bringup` until ESP `EFI/RayNu/auth.token` is staged. HostReady tests refuse the latch; iron still accepts it. |
| Serial keyboard | iDRAC `console com2` / HV UART. Host UART keys exist. SPA has no guest keyboard. |

---

## Does not ship (say this out loud)

- **not PERC.** DurableLun **skips** the Ubuntu H740P VD. USB persist is not RAID-fleet persist. Do not format Ubuntu.
- **not cluster.** vMotion-like / DRS-like / hot-add are **M9**, not Bar A.
- **not Windows.** Typed `windows_iso` exists; install is M8.6 later. The guest is Alpine-on-virtio (patched ISO + serial auto-answer).
- **Iron HTTPS (A4) is open.** In-tree firmware is TLS 1.2 (`Tls12Listen`) with a native HTTPS window **before RayNu-F**. rustls cannot join `uefi-bin`. Flashed `4af78b43` is still **plaintext HTTP**. Flashed `1647a8d8` launched Alpine with no native `https://`.
- **Iron ESP-required auth (A5) is open.** Shared **lab latch** ≠ operator credential. Stage `auth.token` on the stick if you want a real secret; the flashed EFI still accepts bring-up without it.
- **Iron SPA keyboard / VNC (A6) is open.** Not VNC. Type in the guest from iDRAC SOL.
- **Whole-Toshiba virtio is not the product window.** 8 GiB slice by design.
- **ISO blob PUT on coexist is not iron.** ESP-staged `linux.iso` stays valid. Host tests can register ISO bytes; the box you own still uses the stick.
- Nested QEMU ≠ R640. Latitude ≠ R640. Host/CI never print iron markers.

---

## Dedicated-box vs fleet

| | Dedicated-box (Bar A) | RAID-fleet (Bar B) |
|--|------------------------|---------------------|
| Who | Lab owner with a spare / dedicated R640 | Someone whose disks already live on PERC |
| Disk | 8 GiB USB slice (Toshiba), survives Force Off | Spare PERC virtual disk that is **not** Ubuntu |
| Close | A1–A6. A1/A2/A3 done. **A4 TLS iron is NOW.** | `RAYNU-V-M8-PERC-LUN-OK` on a spare VD |
| Do not | Promise PERC, cluster, or Windows | Skip Bar A and sell USB as fleet persist |

Bar B stays **out of commercial conversation** until its close criteria are met. A USB demonstration is the Bar A mechanism. It is not evidence that RayNu-V runs on the customer’s RAID virtual disks.

---

## Residuals named on purpose

| Row | State | Why it is on the card |
|-----|--------|------------------------|
| A1 Everest | DONE | There is a product loop to intent toward. |
| A2 Persist | DONE on evidence | Force Off → same Alpine UUID. 5% remainder: minted persist-OK print / whole-LUN virtio. |
| A3 This SKU card | **DONE** | This page. |
| A4 TLS | **NOW** / open on iron | InfoSec will not sign **plaintext HTTP** + bring-up token. In-tree TLS 1.2 is not COM2. |
| A5 Auth | Host-ready, not iron | Lab latch until ESP `auth.token`. |
| A6 Console | Host-ready, not iron | Not VNC. SPA is still host serial log. |

**NOW is A4 TLS iron.** Sit at `localhost:~#` until F11 of this TLS EFI. Do not `setup-disk`. Do not flash Toshiba `/dev/sdc`.

---

## Pillars this card serves

**[Z]** one binary · **[D]** Dell PowerEdge first · **[A]** audit-first honesty. Formal verification **[V]** is the north star; it is not an LOI gate. USB/xHCI/BOT/mgmt HTTP stay **outside** Proven Core (ADR-002 / ADR-018).
