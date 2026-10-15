# RayNu-V SKU card — what a signature holds today

**Status: DONE (A3).** One page for the person who would sign.  
**Audience:** a lab / innovation owner who can dedicate one Dell PowerEdge.  
**Not this page:** the RAID-fleet letter (Bar B). HDA 99% is Everest, not an LOI.

Living LOI tracker: [`docs/loihda.md`](loihda.md) · public: [`site/sku.html`](../site/sku.html) · [`site/loi.html`](../site/loi.html).

If you signed a non-production letter today, this is the box you would be holding. Overstated copy does not survive diligence.

---

## If you signed today

A **dedicated-box** letter on **one** PowerEdge. One EFI. You would have an Ubuntu 26.04 guest that has already reached `vikkp@raynusrv2` on a **10 GiB** tail of the spare. The disk that has come back after a chassis power-off is an **8 GiB** USB window. The page is **TLS 1.2 HTTPS** with a lab millicert. The first minutes of boot are still **plaintext HTTP**. The door is a **lab latch** until `auth.token` is on the stick.

This letter does not hand you the whole 2.9 TB virtual disk, a guest LAN, **not cluster**, and **not Windows**.

---

## What you would be holding

| You hold | Bound |
|----------|--------|
| One `r640-hypervisor.efi` | No second OS, no packages, no config files (ADR-003). |
| The install loop on a real R640 | Open the page, start a Linux install, reboot to the disk, reach `login:` (`f72b4276`). |
| Ubuntu you can log into | `c3a70e62` reached `vikkp@raynusrv2:~$` from a **10 GiB** tail of RAYNU-SPARE. COM2 `33c9b141` logged in as `vikkp` again. The rest of the 2.9 TB VD is outside the letter. Leave UBUNTU0 alone. |
| A disk that survives Force Off | USB BOT on Toshiba `0480:a004`. You get **8 GiB**, not the whole ~298 GiB stick, and not leftover DRAM. A2 **DONE on evidence** (`4af78b43`, UUID `348005a9`). Persist-OK **printed** on `928d6224` (`RAYNU-V-M8-DISK-PERSIST-OK`, UUID `dd673a9a`). |
| A page on the management port | `https://10.99.99.x:8443/` while a guest runs. A4 encrypt **DONE** (`928d6224`). The page stayed up on `1f33eeda`. Lab millicert, TLS 1.2 only. PRE-EBS SNP remains **plaintext HTTP**. |
| A way in | The **lab latch** `raynu-v-bringup` until ESP `EFI/RayNu/auth.token` is staged. The box you would own still accepts that string. |
| A console | The page typed `abc` into Alpine once. The Ubuntu login you can repeat is COM2 `ttyS0`. The guest log still mixes host lines. This is not a remote desktop. |

---

## What the letter does not cover

- **not PERC fleet.** You are not taking the array. A **10 GiB** tail of **RAYNU-SPARE** holds Ubuntu (`c3a70e62`, login again on `33c9b141`). The rest of the 2.9 TB VD stays hidden. The second controller stays closed. The USB window is a different disk. Leave UBUNTU0 alone. Map: [`runbooks/r640_perc_lab.md`](runbooks/r640_perc_lab.md).
- **not a guest LAN.** The Ubuntu shell is up. It has no working interface. No packet from that guest has left `:39`. An interrupt-pin change is in test. It is outside today's signature.
- **not cluster.** No move of a running guest, no placement across boxes, no hot-add. Those are a later letter.
- **not Windows.** Ubuntu 26.04 and Alpine have both reached a login. Windows is a later letter.
- The HTTPS you could show is the lab millicert and one TLS 1.2 cipher. The boot window is still **plaintext HTTP**.
- There is no operator account. The **lab latch** is what the box accepts until ESP `auth.token` exists.
- The page typed into Alpine once. It is not a remote desktop. Chassis power is the iDRAC control.
- The USB window is **8 GiB** by design.
- Putting an ISO through the management page is not what the box does today. The stick still carries the media.
- A nested lab and a laptop are not the PowerEdge you would be signing for.

---

## Two letters

| | Dedicated-box, the letter on the table | Fleet, a different signature |
|--|------------------------|---------------------|
| Who signs | A lab owner with one R640 they can set aside | Someone whose disks already live on the array |
| Disk | **8 GiB** USB window for the power-off path. Ubuntu login is a 10 GiB tail, not the whole VD. | The spare array disk that is not the 400 GB boot VD. A 10 GiB tail has logged in. The 2.9 TB VD is still hidden. |
| What closes it | Install loop, this card, HTTPS. An operator password is still open. | A guest login from the spare virtual disk they already own, on the whole disk they mean. |
| Leave out | **not PERC** fleet, **not cluster**, **not Windows** | Selling the USB window as the array |

The fleet letter stays off the table. The USB window is the dedicated-box disk. It is not evidence that RayNu-V is running on a customer's array.

---

## Residuals named on purpose

| Row | State | Why it is on the card |
|-----|--------|------------------------|
| A1 Everest | DONE | The install loop you could point at. |
| A2 Persist | DONE on evidence | Force Off, then the USB window comes back. Remainder: **8 GiB**, and that stick is not the array. |
| A3 This SKU card | **DONE** | This page. |
| A4 TLS | **DONE** on COM2 `928d6224` | The page answered over TLS. Lab millicert. PRE-EBS SNP stays **plaintext HTTP**. |
| A4s Standing SPA | **LIVED** on `1f33eeda` | The page stayed up. |
| A5 Auth | Host-ready, not iron | **lab latch** until ESP `auth.token`. |
| A6 Console | Typed once, not a desktop | The page reached Alpine once. Ubuntu login is the serial console. |

**If you signed today, the guest still has no LAN.** `33c9b141` reached `vikkp` on the serial console. `virtio_net` returned `-22`. Leave UBUNTU0 alone. Leave `setup-disk` alone. Chassis power is iDRAC Force Off.

---

## Pillars this card serves

**[Z]** one binary · **[D]** Dell PowerEdge first · **[A]** audit-first honesty. Formal verification **[V]** is the north star; it is not an LOI gate. USB/xHCI/BOT/mgmt HTTP stay **outside** Proven Core (ADR-002 / ADR-018).
