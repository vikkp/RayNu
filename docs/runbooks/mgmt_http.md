# Runbook — Network HTTP mgmt plane (M7.1)

**Marker:** `RAYNU-V-M7-HTTP-OK`  
**Smoke:** `./tools/m7-http-smoke.sh`

## Story

M7.1 makes the control plane **network-reachable**: an in-binary HTTP/1.1 codec
serves the embedded SPA (`GET /`) and REST (`/vms…`) with
`Authorization: Bearer` (M6.4 bring-up token).

| Mode | What runs | Where |
|------|-----------|--------|
| **Host / CI gate** | `std` `TcpListener` one-shot serve via same codec | `cargo test` / `m7-http-smoke.sh` |
| **Firmware** | Codec linked; `listen_mgmt_http_uefi` stub | Returns `UnsupportedOnFirmware` until UEFI Tcp4/SNP |
| **Lab** | Plaintext HTTP (TLS deferred — ADR-003 size) | Documented below |

## Auth

```http
Authorization: Bearer raynu-v-bringup
```

- SPA (`GET /`) — no auth (page load).  
- REST — M6.4 token required; missing/wrong → `401`.

## Host proof (CI)

```bash
./tools/m7-http-smoke.sh
```

Exercises SPA `200 text/html` and authed `GET /vms` over loopback TCP.

## QEMU user-net forward (lab)

When the firmware listen path lands (or a host helper is used), forward the mgmt
port into the guest/HV:

```bash
# Example shape — adjust once UEFI Tcp4 bind is wired:
qemu-system-x86_64 ... \
  -netdev user,id=n0,hostfwd=tcp::8443-:8443 \
  -device e1000,netdev=n0
```

Then from the operator laptop:

```bash
curl -sS http://127.0.0.1:8443/ | head
curl -sS -H 'Authorization: Bearer raynu-v-bringup' http://127.0.0.1:8443/vms
```

Default lab port: **8443** (`MGMT_HTTP_DEFAULT_PORT`).

## TLS

**Deferred.** M7.1 closes on **plaintext HTTP** lab MVP with an explicit ADR-009 /
ADR-003 note. Prefer TLS before any untrusted LAN exposure.

## Limits

- UEFI NIC listen is **not** claimed done — stub is honest.  
- Datastore / ISO / create-VM UI polish are **M7.2–M7.4**.  
- Replace bring-up token before production exposure.
