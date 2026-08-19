# Runbook — Network HTTP mgmt plane (M7.1 + M7.6 + M7.8)

**Markers:**
- M7.1 host: `RAYNU-V-M7-HTTP-OK` — `./tools/m7-http-smoke.sh`
- M7.6 scaffold: `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK` — `./tools/m7-uefi-http-smoke.sh`
- M7.6 firmware: `RAYNU-V-M7-UEFI-HTTP-OK` — PRE-EBS UEFI NIC served ≥1 HTTP exchange (ADR-012); iron closed 2026-08-16 (SNP residual)
- Post-EBS scaffold: `RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK` — `./tools/m7-post-ebs-http-smoke.sh`
- Post-EBS firmware: `RAYNU-V-M7-POST-EBS-HTTP-OK` — **not claimed**. Firmware SNP is dead after EBS on this boot method (iron 2026-08-17).
- M7.8 scaffold: `RAYNU-V-M7-HOST-NIC-SCAFFOLD-OK` — `./tools/m7-host-nic-smoke.sh` (ADR-013 Phase 0/C/D/E wiring)
- M7.8 QEMU: `RAYNU-V-M7-HOST-NIC-QEMU-OK` — post-EBS `GET /` on QEMU `e1000` (`8086:100e`); `./tools/m7-host-nic-qemu-smoke.sh` (also greps PRE-EBS `vid:did=8086:100e`)
- M7.8 iron: `RAYNU-V-M7-HOST-NIC-HTTP-OK` — **Phase D only**, after `BOOT-OK` on a **non-QEMU** census NIC. Do not claim from host or QEMU.

## Story

M7.1 makes the control plane **network-reachable**: an in-binary HTTP/1.1 codec
serves the embedded SPA (`GET /`) and REST (`/vms…`) with
`Authorization: Bearer` (M6.4 bring-up token). Host/`cfg(test)` proves a real
`TcpListener` exchange.

**ADR-012 / M7.6** binds that codec to a **UEFI NIC** via Tcp4 **before**
`ExitBootServices` (Boot Services protocols are invalid after EBS). Soft-fail
continues into the guest path when Tcp4 is absent (common on minimal OVMF).

| Mode | What runs | Where |
|------|-----------|--------|
| **Host / CI (M7.1)** | `std` `TcpListener` one-shot | `m7-http-smoke.sh` |
| **Host / CI (M7.6)** | Scaffold gate (wiring only) | `m7-uefi-http-smoke.sh` |
| **Host / CI (M7.8)** | Bounded poll + e1000 wiring | `m7-host-nic-smoke.sh` |
| **QEMU (M7.8 Phase C)** | Post-EBS `GET /` on `e1000` | `m7-host-nic-qemu-smoke.sh` |
| **Firmware** | PRE-EBS Tcp4 listen window (~15s) | Soft-fail → EBS + guests |
| **Lab** | Plaintext HTTP (TLS deferred — ADR-003/009/012) | QEMU `hostfwd` below |

## Auth

```http
Authorization: Bearer raynu-v-bringup
```

- SPA (`GET /`) — no auth (page load).
- REST — Bearer required; missing/wrong → `401`.
- **E4 operator token:** if ESP `EFI/RayNu/auth.token` is present at PRE-EBS,
  that secret is required and the bring-up token is **rejected**.
- Without `auth.token`, lab bring-up token remains valid (host CI / QEMU).

Also available during PRE-EBS:

| Path | Notes |
|------|--------|
| `GET /logs/serial` | Host UART log ring (Bearer); SPA “Host serial log” panel |
| Durable tables | Shared `pre_ebs_mgmt` across exchanges (create survives Refresh) |

Lab uses **plaintext HTTP** (TLS deferred — ADR-003/009/012).

## Host proof (CI)

```bash
./tools/m7-http-smoke.sh
./tools/m7-uefi-http-smoke.sh
./tools/m7-host-nic-smoke.sh
```

Exercises SPA `200 text/html` and authed `GET /vms` over loopback TCP (M7.1),
plus M7.6 scaffold wiring (never prints the iron firmware marker from host smoke).

## QEMU user-net forward (lab)

Tcp4 must be present in the OVMF build (NetworkPkg). Forward the mgmt port:

```bash
./tools/run-qemu.sh \
  -netdev user,id=n0,hostfwd=tcp::8443-:8443 \
  -device e1000,netdev=n0
```

During the PRE-EBS window, from the operator laptop:

```bash
curl -sS http://127.0.0.1:8443/ | head
curl -sS -H 'Authorization: Bearer raynu-v-bringup' http://127.0.0.1:8443/vms
```

Expect serial: `boot: mgmt HTTP listening on 0.0.0.0:8443 (PRE-EBS Tcp4 window)`
then `RAYNU-V-M7-UEFI-HTTP-OK`. If Tcp4 is missing: `WARN — Tcp4 stack absent`
and boot continues (honest residual).

Default lab port: **8443** (`MGMT_HTTP_DEFAULT_PORT`).

## PRE-EBS constraint (ADR-012)

UEFI Tcp4/SNP/DHCP **open** paths are Boot Services. After `leave_firmware()` /
ExitBootServices, `locate_handle` / `stall` / CloseProtocol are invalid.
RayNu-V **parks** the SNP + smoltcp session (leaked protocol, no CloseProtocol)
so the PRE-EBS lease can be printed after EBS. **Do not** Transmit/Receive on
firmware SNP after EBS.

| Phase | What | Soft-fail |
|-------|------|-----------|
| PRE-EBS | Existing 45s SNP window (`RAYNU-V-M7-UEFI-HTTP-OK`) | timeout → continue to EBS |
| Immediately after EBS | Serial-only: parked lease printed; **no SNP poll** (iron hung on immediate poll 2026-08-16) | no parked NIC → skip |
| After VMXOFF / BOOT-OK | WARN only — **firmware SNP dead** (2026-08-17 curl timeout + RSOD RIP=`0x17`); no SNP poll | PRE-EBS remains mgmt |

**Do not chase** firmware Tcp4 on this boot method (Virtual Floppy / Cruzer UNDI).
**Do not chase** firmware SNP after EBS (do not chase either protocol after EBS).
Durable post-EBS HTTP is **[ADR-013](../adr/ADR-013.md)** (host-owned NIC). Size stays inside
ADR-003 (`./tools/check-size.sh`).

If the NIC is unusable after EBS, serial prints a WARN and the guest path
continues. PRE-EBS remains the fallback operator window.

Evidence: [`docs/evidence/r640/2026-08-17-post-ebs-snp-dead.md`](../evidence/r640/2026-08-17-post-ebs-snp-dead.md).

## ADR-013 Phase C — QEMU e1000 after EBS (M7.8)

Lifetime HTTP is a **host-owned NIC** + smoltcp ([ADR-013](../adr/ADR-013.md)). Phase C
is the lab driver: QEMU `-device e1000` (PCI `8086:100e` only). All NIC `unsafe`
(PCI config, MMIO, DMA rings) lives in `mgmt/e1000_mmio.rs`.

```bash
HOST_NIC_LAB=1 ./tools/run-qemu.sh
# or:
./tools/m7-host-nic-qemu-smoke.sh
```

`HOST_NIC_LAB=1` stages `EFI/RayNu/hostnic.txt`, adds user-net `hostfwd` to `:8443`
(host port **18443**), and exits after the first post-EBS `GET /`. Serial:

```
boot: QEMU e1000 8086:100e — skip PRE-EBS SNP/Tcp4 (ADR-013 Phase C)
boot: HOST-NIC listening on 10.0.2.15:8443 (post-EBS e1000)
RAYNU-V-M7-HOST-NIC-QEMU-OK
```

From the host:

```bash
curl -sS http://127.0.0.1:18443/ | head
```

Do **not** print `RAYNU-V-M7-HOST-NIC-HTTP-OK` from this path (iron Phase D).
Firmware SNP stays WARN-only after EBS. Bounded poll: `HOST_NIC_POLL_BUDGET`
(32) per listen tick so the credit scheduler is not starved.

### Phase 0 — PCI census (PRE-EBS)

Before SNP/Tcp4 (and before the e1000 skip), firmware prints Ethernet-class
PCI functions:

```
boot: PCI census nics=1
boot: PCI 00:03.0 vid:did=8086:100e bar0=0x… msix=none
boot: IOMMU ACPI DMAR=yes|no
```

On R640 this list is how we pick **one** `vid:did`. Iron 2026-08-17 (Cruzer):

```
boot: PCI census nics=2
boot: PCI 01:00.00 vid:did=14e4:165f bar0=0x92930000 msix=17
boot: PCI 01:00.01 vid:did=14e4:165f bar0=0x92900000 msix=17
boot: IOMMU ACPI DMAR=no
```

**Chosen:** `14e4:165f` (BCM5720) dual-port LOM. Iron 2026-08-18: PRE-EBS SNP
DHCP used **`01:00.1` MAC `b0:26:28:5c:5a:3a`**. Binding **`01:00.0`**
(`:5a:38`) left Vignesh's laptop with **no ARP** / ping unreachable /
curl fail on the parked lease (that bind used station `:38`, not SNP `:3a`).
Phase D programs the parked SNP MAC on whichever function actually has
copper: peek each candidate `BMSR`, try func 0 (NCSI/LOM1) then func 1 until
`LSTATUS`. Iron skip-reset EFI still saw `ape-ncsi=yes` and func 1
`lpa=0000`. After bind, the **parked SNP MAC** is
programmed as the station address (iron: BAR0 peek on `01:00.1` was `:39`
while SNP leased `:3a`; station-MAC EFI programmed `:3a` and still saw
ping **host unreachable** — PHY link / `MAC_MODE` next). Evidence:
[`docs/evidence/r640/2026-08-17-phase0-census.md`](../evidence/r640/2026-08-17-phase0-census.md).

### Phase D — same `Device` trait; idle after BOOT-OK

QEMU e1000 already implements `smoltcp::phy::Device`. After `BOOT-OK` the
idle path calls `run_post_boot_ok_native_idle`. On R640 that now binds
**BCM5720 `14e4:165f`** (poll-mode, MSI-X off) on the **SNP-lease MAC**
(R640 expect `pci=01:00.01` / `:5a:3a`) and reuses the
PRE-EBS SNP lease. Bring-up follows **Linux `tg3`** (`tg3.c` / PG ch. 7),
**not** `bnxt` (that is BCM57416 10G). Peek `BMSR` **before** any analog work.
If `LSTATUS` is set, inherit SNP copper (skip chip reset and `tg3_phy_reset`).
If the PHY is already down, Linux `tg3_phy_reset` / `tg3_setup_copper_phy`
(BMCR_RESET, AUXCTL PWRCTL=0, Auto-MDIX, `tg3_phy_toggle_apd(false)`, CPMU EEE
LPI off) **with** Linux `tg3_ape_lock` on BAR2 around MDIO — **without**
`CORECLK_RESET` (iron `1213952` still `lpa=0000` after chip reset). Then wait
`BMSR_LSTATUS` and set `MAC_MODE` MII vs GMII (`tg3_adjust_link`).
`RX_MODE_PROMISC` is on. `RAYNU-V-M7-HOST-NIC-HTTP-OK` prints only after a
native HTTP exchange on that id — never from QEMU/host.

**Iron flash (replace-only):** copy the new `BOOTX64.EFI` onto the Cruzer
ESP (`EFI/BOOT/BOOTX64.EFI`). Leave `EFI/RayNu/installdisk.bin` (and
`auth.token` if present) alone. After `RAYNU-V-R640-BOOT-OK`, curl the
**same** SNP lease on the host LAN (not iDRAC):

```
curl -sS -m 5 "http://<lease>:8443/"
```

Use the numeric lease from COM2 (example `10.99.99.116`). Do **not** type the
word `LEASE`. Port is **8443** (not 8445, not iDRAC).

Expect COM2 `HOST-NIC BCM5720 cand pci=… MAC=… bmsr=…` for each function, then
`try pci=… fallback=func0 (NCSI/LOM1)` (or `link-up BMSR` / `matched SNP lease`)
then (if peek ≠ lease) `station SNP-lease MAC=…` then
`pre-reset bmsr=… ape=yes|no ape-bar=… ape-fw=ready|no ape-evt=sent|skip ape-ncsi=yes|no ape-lock=yes|timeout|skip`
then either `inherit SNP PHY (skip CORECLK_RESET)` or
`skip CORECLK_RESET (keep GPHY analog)` then `phy_reset=pre … eee=off …`
then **no** `reset…` / `fw-magic=` / `an-restart=` then
`link=up speed=… duplex=…`
(or `link=timeout` then `try next func` and the other PCI function)
then `rings armed` then `HOST-NIC idle listening on <lease>:8443`.

PRE-EBS SNP HTTP still works; that is **not** E3b. After `BOOT-OK`, ICMP
replies with `ttl=63` (one routed hop) while COM2 shows `bmsr=7949` are **not**
the native stack — curl timeout is expected until `link=up`. The iron HTTP-OK
marker is only after that exchange. Do not claim it from this runbook.

Parse path: e1000 `parse_mocked_rx_desc_bytes` + BCM `parse_mocked_rx_bd_bytes`
(host fuzz + optional `./tools/host-nic-miri-smoke.sh`; Miri skip is OK).

### Phase E — mgmt arena

Listen TCP/HTTP scratch comes from a 64 KiB `MgmtArena`, not
`FrameAllocator`. On `MgmtFatal`: arena `reset`, `AuditEvent::MgmtRestarted`,
retry. Host observable: `induced_fatals_do_not_touch_frame_allocator`.

Preserve kit for iron rollback: `releases/v0.1.0-adr013-baseline`.

## Cruzer `auth.token`

PRE-EBS `probe_operator_auth_token` reads `EFI/RayNu/auth.token` (same folder
as `installdisk.bin` on the Cruzer). If present, that secret replaces the
hard-coded bring-up token. Without the file, lab `raynu-v-bringup` stays valid.

## R640 Tcp4 absent (Virtual Floppy)

Iron (BIOS 2.2.11, iDRAC Virtual Floppy): after PCI+SNP+all-handles,
`snp=12` and `mnp=ip4=dhcp4=tcp4=0`, but `pxe=8 http=4 ip4cfg=4`.
Firmware **Tcp4ServiceBinding** never appears (vendor PXE/HTTP closed
stack). SNP + smoltcp residual is the working path
(`RAYNU-V-M7-UEFI-HTTP-OK`). Platform limitation — see census COM2.

**Working explanation:** Floppy BDS starts UNDI/SNP only. NetworkPkg
(MnpDxe…Tcp4Dxe) is dispatched for **UEFI PXE / HTTP / iSCSI boot
options**, not for Floppy. Enabling “UEFI Network Stack” in BIOS did not
produce Tcp4 on Floppy. Investigation:
[`docs/evidence/r640/2026-08-16-uefi-tcp4-absent-root-cause.md`](../evidence/r640/2026-08-16-uefi-tcp4-absent-root-cause.md).

COM2 diagnostics (tip): `uefi-net extra`, `after-snp`, `stack_ok`,
`after-all`, `extra-after`. If those stay `tcp4=0` and `pxe=http=0`,
treat firmware Tcp4 as a **platform limitation** on this boot path.

Optional BIOS experiments (do not block SNP residual): F2 → Network
Settings → enable **PXE Device 1** and/or **HTTP Device 1**, still boot
Virtual Floppy; compare `extra` census. USB ESP vs Floppy isolates
vMedia. BIOS 2.2.11 → current 14G is a separate window.

## TLS

**Deferred.** Prefer TLS before any untrusted LAN exposure (ADR-003/009/012).
M7.1 closed on **plaintext HTTP** lab MVP with an explicit size-budget note.

## Limits

- HDA **E3 MVP DONE** on iron (`RAYNU-V-M7-UEFI-HTTP-OK`, 2026-08-16 COM2). **E3b** (lifetime HTTP) is [ADR-013](../adr/ADR-013.md) **Accepted**. Phase C (QEMU e1000 post-EBS `GET /`) is M7.8. WARN-only idle closed on iron 2026-08-17 (no RSOD). `RAYNU-V-M7-POST-EBS-HTTP-OK` is **not claimed**. Do not chase SNP/Tcp4 after EBS.
- Datastore / ISO / create-VM UI polish are **M7.2–M7.4** (host closed).
- Replace bring-up token before production exposure (ESP `EFI/RayNu/auth.token` on Cruzer).
