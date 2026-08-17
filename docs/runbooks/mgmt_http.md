# Runbook — Network HTTP mgmt plane (M7.1 + M7.6)

**Markers:**
- M7.1 host: `RAYNU-V-M7-HTTP-OK` — `./tools/m7-http-smoke.sh`
- M7.6 scaffold: `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK` — `./tools/m7-uefi-http-smoke.sh`
- M7.6 firmware: `RAYNU-V-M7-UEFI-HTTP-OK` — PRE-EBS UEFI NIC served ≥1 HTTP exchange (ADR-012); iron closed 2026-08-16 (SNP residual)
- Post-EBS scaffold: `RAYNU-V-M7-POST-EBS-HTTP-SCAFFOLD-OK` — `./tools/m7-post-ebs-http-smoke.sh`
- Post-EBS firmware: `RAYNU-V-M7-POST-EBS-HTTP-OK` — **not claimed**. Firmware SNP is dead after EBS on this boot method (iron 2026-08-17).

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
**Do not chase** firmware SNP after EBS. Durable post-EBS HTTP needs a host-owned
NIC (MMIO/DMA). Size stays inside ADR-003 (`./tools/check-size.sh`).

If the NIC is unusable after EBS, serial prints a WARN and the guest path
continues. PRE-EBS remains the fallback operator window.

Evidence: [`docs/evidence/r640/2026-08-17-post-ebs-snp-dead.md`](../evidence/r640/2026-08-17-post-ebs-snp-dead.md).

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

- HDA **E3 MVP DONE** on iron (`RAYNU-V-M7-UEFI-HTTP-OK`, 2026-08-16 COM2). TLS remains deferred. Post-EBS listen is **not** firmware SNP: iron hung on immediate poll, timed out after BOOT-OK, then RSOD. `RAYNU-V-M7-POST-EBS-HTTP-OK` is **not claimed**. Next is a host-owned NIC, not more SNP protocol calls.
- Datastore / ISO / create-VM UI polish are **M7.2–M7.4** (host closed).
- Replace bring-up token before production exposure (ESP `EFI/RayNu/auth.token` on Cruzer).
