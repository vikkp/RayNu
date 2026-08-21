# Runbook — Dedicated iDRAC NIC + host LOM (E3b)

**Locked path:** iDRAC uses the **rear dedicated RJ45**. Host mgmt HTTP uses a
**BCM5720 LOM jack**. Do **not** take the PHY from APE (iDRAC Shared LOM /
NCSI). E3b **closed** 2026-08-20: COM2 `link=up` then post-`BOOT-OK` curl
landed `RAYNU-V-M7-HOST-NIC-HTTP-OK` on `:38` / `10.99.99.144:8443`.

Pillars: **[D]** iDRAC-native · **[Z]** single-binary. Outside Proven Core
(ADR-013). Parent: [`mgmt_http.md`](mgmt_http.md).

Iron already proved (EFI `42b42c99`): SNP `:3a` had a lease while both host
GPHYs were `bmsr=7949`. That analog is APE/NCSI. Taking it can drop iDRAC.

---

## Rear ports (R640)

| Jack | What it is | Cable |
|------|------------|--------|
| **Dedicated iDRAC** | Small RJ45, often labeled iDRAC / dedicated Mgmt | SOL + SSH. Stay on this. Not `:8443`. |
| **LOM1 / LOM2** | Dual BCM5720 (`14e4:165f`, `01:00.0` / `01:00.1`) | Host mgmt LAN (Mac curl). Not the iDRAC dedicated jack. |

Iron census (2026-08-17/18) treated `:38` as unused because SNP leased `:3a`.
Ubuntu on this R640 (2026-08-19, cable on the live jack):

| Linux | MAC | PCI | State |
|-------|-----|-----|--------|
| **eno3** | `b0:26:28:5c:5a:38` | `01:00.0` | **UP** `10.99.99.126/24` — host mgmt LOM |
| eno4 | (other LOM, likely `:39`) | `01:00.1` | DOWN |
| eno1np0 | `b0:26:28:5c:5a:3a` | APE/NCSI | DOWN (old SNP MAC) |
| eno2np1 | | | DOWN |

Plug host mgmt into **eno3** / `:38`. That is the jack RayNu-V should bind.
Do **not** overlay APE `:3a` on it. `10.99.99.126` is **Ubuntu**; after F11
RayNu-V, curl the lease COM2 prints (new DHCP), not `.126` unless COM2 says so.

---

## 1. Cable dedicated iDRAC **before** you switch

If iDRAC NIC Selection is **Shared** or **Shared with Failover** today, switching
to Dedicated with the dedicated jack **unplugged** drops SSH and SOL.

1. Plug the dedicated iDRAC RJ45 into the network you already SSH.
2. Confirm `ssh` to the iDRAC IP still works (or use the iDRAC web UI).
3. Only then change NIC Selection.

If iDRAC is **already** Dedicated (field guide default), skip to §3 and confirm
the setting.

---

## 2. Set NIC Selection = Dedicated

iDRAC 9 (typical R640):

1. iDRAC web → **iDRAC Settings** → **Connectivity** → **Network**
   (or **Configuration** → **Network Settings**).
2. **NIC Selection** = **Dedicated**.
3. Not Shared. Not Shared with Failover. Not LOM1 / LOM2 as the iDRAC uplink.
4. Apply. The iDRAC network stack may restart — SOL drops for a few seconds.
5. Re-`ssh` → `console com2`.

iDRAC 8-style: **iDRAC Settings** → **Network** → **NIC Selection** =
Dedicated.

Host BIOS (F2) iDRAC network page must match. Do not leave failover on LOM.

---

## 3. Host mgmt Ethernet on a LOM jack

1. Plug the Mac/LAN cable (subnet `10.99.99.0/24` in the lab) into a **LOM**
   RJ45 — the jack Ubuntu names **eno3** (`b0:26:28:5c:5a:38` / `01:00.0`).
2. Do **not** plug that cable into the iDRAC dedicated jack.
3. Cruzer Micro stays in **front USB 2**. BIOS boot order: Ubuntu on PERC
   first; RayNu-V is one-time F11 Cruzer.

---

## 4. Boot and what COM2 must show

Keep APE PHY EFI (this branch). Flash **by SHA**. Reject take-PHY
(`ape-nophylock=no`) and peek `42b42c99` / `ec08c00f` / `1404f055`.

Expect:

```
HINT — E3b: Dedicated iDRAC NIC; host mgmt on LOM jack (not iDRAC dedicated)
pre-EBS cand pci=… bmsr=…
HINT — keep APE PHY (iDRAC NCSI); will not take phylock
ape-nophylock=yes keep-ape-phy=yes
phy_reset=pre skip (ape-ncsi)
```

**Success:** a `pre-EBS cand` `bmsr` with `LSTATUS` (bit 2; not bare `7949`),
then after EBS `link=up`. Then curl **only** during AfterBootOk listen:

```
curl -sS -m 5 "http://<numeric-lease>:8443/"
```

Port **8443**. Not 8445. Not the iDRAC IP. Not the word `LEASE`.

**Stop:** `skip listen (no LSTATUS; do not curl)` — Dedicated did not free a
host GPHY, or the cable is still on the APE/NCSI jack. Do not curl. Do not
take PHY. Recheck NIC Selection and LOM jack.

PRE-EBS `RAYNU-V-M7-UEFI-HTTP-OK` still does not count as E3b.

---

## 5. If iDRAC vanishes

1. Do not keep rebooting the host hoping LOM failover brings iDRAC back.
2. Plug the dedicated iDRAC jack. Wait for the BMC to come up (~1–2 min after
   AC if needed).
3. Set NIC Selection back to Dedicated from the iDRAC web on that jack
   (or F2 iDRAC settings on a local KVM).
4. SOL is `console com2` on the **iDRAC IP**, never on `10.99.99.116`.
