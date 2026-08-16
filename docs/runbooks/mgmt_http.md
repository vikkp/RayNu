# Runbook — Network HTTP mgmt plane (M7.1 + M7.6)

**Markers:**
- M7.1 host: `RAYNU-V-M7-HTTP-OK` — `./tools/m7-http-smoke.sh`
- M7.6 scaffold: `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK` — `./tools/m7-uefi-http-smoke.sh`
- M7.6 firmware: `RAYNU-V-M7-UEFI-HTTP-OK` — PRE-EBS UEFI NIC served ≥1 HTTP exchange (ADR-012); iron closed 2026-08-16 (SNP residual)

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

UEFI Tcp4/SNP/DHCP are Boot Services protocols. After `leave_firmware()` /
ExitBootServices they are gone. Concurrent guest + mgmt listen requires a
post-EBS NIC driver (follow-on). M7.6 MVP = PRE-EBS window + soft-fail.

## TLS

**Deferred.** Prefer TLS before any untrusted LAN exposure (ADR-003/009/012).
M7.1 closed on **plaintext HTTP** lab MVP with an explicit size-budget note.

## Limits

- HDA **E3 MVP DONE** on iron (`RAYNU-V-M7-UEFI-HTTP-OK`, 2026-08-16 COM2). TLS / post-EBS listen remain follow-ons.
- Datastore / ISO / create-VM UI polish are **M7.2–M7.4** (host closed).
- Replace bring-up token before production exposure.
