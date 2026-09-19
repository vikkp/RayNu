# RayNu-V SKU card — dedicated-box lab (Bar A)

**Status: DONE (A3).** One page: what ships, what does not.  
**Audience:** a lab / innovation owner who can dedicate one Dell PowerEdge.  
**Not this page:** RAID-fleet replacement (Bar B). HDA 99% is Everest, not an LOI.

Living LOI tracker: [`docs/loihda.md`](loihda.md) · public: [`site/sku.html`](../site/sku.html) · [`site/loi.html`](../site/loi.html).

This card is commercial honesty. Overstated copy does not survive diligence.

---

## What you are buying

A **dedicated-box** non-production LOI on **one** PowerEdge we own or they dedicate. One EFI binary. Alpine on virtio. Guest disk on an **8 GiB** USB slice. Native coexist is **TLS 1.2 HTTPS** with a lab millicert. PRE-EBS SNP remains **plaintext HTTP**.

It is **not** a fleet hypervisor, **not PERC**, **not cluster**, **not Windows**.

---

## Ships today (Bar A dedicated-box)

| Ships | Honest bound |
|-------|----------------|
| Single `r640-hypervisor.efi` | No OS, no packages, no config files (ADR-003). |
| Everest loop on a real R640 | Network SPA → Alpine ISO → reboot to disk → `login:` (`f72b4276`). |
| Guest disk survives Force Off | USB BOT on Toshiba `0480:a004`. Guest sees **8 GiB**, not the whole ~298 GiB stick, not leftover DRAM. A2 **DONE on evidence** (`4af78b43`, UUID `348005a9`). Minted persist-OK **printed** on `928d6224` (`RAYNU-V-M8-DISK-PERSIST-OK`, UUID `dd673a9a`). |
| Network SPA + REST | Native coexist `https://10.99.99.x:8443/` **while Alpine runs** (firmware standing SPA). A4 encrypt **DONE** (`928d6224`). Standing listen is **in-tree, not iron** until a browser stays up after login. Lab millicert, TLS 1.2 ECDHE-RSA-AES128-GCM only. PRE-EBS SNP remains **plaintext HTTP**. |
| Lab operator latch | `raynu-v-bringup` until ESP `EFI/RayNu/auth.token` is staged. HostReady tests refuse the latch; iron still accepts it. |
| Serial keyboard | SPA `POST /console/keys` → guest COM1. `GET /logs/guest` is guest COM1 TX (not iDRAC SOL). `GET /logs/serial` is HV UART. Iron CONSOLE-OK is typing from the SPA on COM2. Not VNC. |

---

## Does not ship (say this out loud)

- **not PERC.** DurableLun **skips** the Ubuntu H740P VD. USB persist is not RAID-fleet persist. Do not format Ubuntu.
- **not cluster.** vMotion-like / DRS-like / hot-add are **M9**, not Bar A.
- **not Windows.** Typed `windows_iso` exists; install is M8.6 later. The guest is Alpine-on-virtio (patched ISO + serial auto-answer).
- **Iron HTTPS (A4) is DONE** on COM2 `928d6224`. Residual: lab millicert, one TLS 1.2 cipher, PRE-EBS SNP **plaintext HTTP**. rustls cannot join `uefi-bin`. **Standing SPA during Alpine is firmware-in-tree, not iron.**
- **Iron ESP-required auth (A5) is open.** Shared **lab latch** ≠ operator credential. Stage `auth.token` on the stick if you want a real secret; the flashed EFI still accepts bring-up without it.
- **Iron SPA keyboard / VNC (A6) is open** until standing HTTPS is lived. Firmware `POST /console/keys` and `GET /logs/guest` are in-tree. Not VNC.
- **Whole-Toshiba virtio is not the product window.** 8 GiB slice by design.
- **ISO blob PUT on coexist is not iron.** ESP-staged `linux.iso` stays valid. Host tests can register ISO bytes; the box you own still uses the stick.
- Nested QEMU ≠ R640. Latitude ≠ R640. Host/CI never print iron markers.

---

## Dedicated-box vs fleet

| | Dedicated-box (Bar A) | RAID-fleet (Bar B) |
|--|------------------------|---------------------|
| Who | Lab owner with a spare / dedicated R640 | Someone whose disks already live on PERC |
| Disk | 8 GiB USB slice (Toshiba), survives Force Off | Spare PERC virtual disk that is **not** Ubuntu |
| Close | A1–A6. A1/A2/A3/A4 done. **Standing SPA is NOW** (then A6). | `RAYNU-V-M8-PERC-LUN-OK` on a spare VD |
| Do not | Promise PERC, cluster, or Windows | Skip Bar A and sell USB as fleet persist |

Bar B stays **out of commercial conversation** until its close criteria are met. A USB demonstration is the Bar A mechanism. It is not evidence that RayNu-V runs on the customer’s RAID virtual disks.

---

## Residuals named on purpose

| Row | State | Why it is on the card |
|-----|--------|------------------------|
| A1 Everest | DONE | There is a product loop to intent toward. |
| A2 Persist | DONE on evidence | Force Off → Alpine from USB. Remainder: 8 GiB slice / USB ≠ PERC. Persist-OK printed on `928d6224`. |
| A3 This SKU card | **DONE** | This page. |
| A4 TLS | **DONE** on COM2 `928d6224` | Native `curl --cacert` SPA + `RAYNU-V-M8-TLS-OK`. Lab millicert. PRE-EBS SNP stays **plaintext HTTP**. |
| A4s Standing SPA | Firmware in-tree, not iron | HTTPS ticks during RayNu-F / USB waits. Browser after Alpine login is the close. |
| A5 Auth | Host-ready, not iron | Lab latch until ESP `auth.token`. |
| A6 Console | Firmware SPA keys + guest log, not iron | Not VNC. Depends on standing HTTPS. |

**NOW is standing SPA (A4s).** Then A6 (type from the page). Sit at `localhost:~#`. Do not `setup-disk`. Do not flash Toshiba `/dev/sdc`.

---

## Pillars this card serves

**[Z]** one binary · **[D]** Dell PowerEdge first · **[A]** audit-first honesty. Formal verification **[V]** is the north star; it is not an LOI gate. USB/xHCI/BOT/mgmt HTTP stay **outside** Proven Core (ADR-002 / ADR-018).
