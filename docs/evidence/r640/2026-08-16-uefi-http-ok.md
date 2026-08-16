# R640 M7.6 / ADR-012 — UEFI NIC HTTP OK (2026-08-16)

**Claim:** `RAYNU-V-M7-UEFI-HTTP-OK` on real Dell PowerEdge R640.  
**Also still true:** `RAYNU-V-R640-BOOT-OK` (E2) on the same boot.

## Closing kit

| Field | Value |
|-------|-------|
| Kit | [`releases/v0.1.0-m76-snp-http/`](../../releases/v0.1.0-m76-snp-http/) |
| Git (feature) | `b59927e` — first SNP+smoltcp residual (no `CURL NOW` yet) |
| **Iron EFI (Mac dist, authoritative)** | `be1645f458e83bc39be160d160c6b47ef36254a309f317dd6118722627a3b0d5` |
| **Iron Virtual Floppy img** | `86c432cf7f4a49239cb7b8066863abf83c075c7def70cdddda2b5a2331823484` |
| CI rebuild EFI (same git, Linux) | `5a8651698b6766dc8f46daa7c5db1da034a8e3b2e7e7db13dedd1f67686e4262` |
| COM2 fingerprint | listen line then OK; **no** `CURL NOW` / `window_ms` |

Operator paths hashed:

```text
dist/raynu-v-0.1.0/r640-hypervisor.efi
dist/raynu-v-0.1.0-boot-media/raynu-v-0.1.0-uefi-boot.img
```

See [`releases/v0.1.0-m76-snp-http/OPERATOR-SHA256SUMS`](../../releases/v0.1.0-m76-snp-http/OPERATOR-SHA256SUMS).
Mac vs CI digests differ by toolchain; runtime proof is COM2 + curl.
## What closed

| Gate | Marker | Evidence |
|------|--------|----------|
| M7.6 iron | `RAYNU-V-M7-UEFI-HTTP-OK` | [`logs/2026-08-16-uefi-http-ok-com2.txt`](logs/2026-08-16-uefi-http-ok-com2.txt) |
| E2 (prior) | `RAYNU-V-R640-BOOT-OK` | same boot after VMXOFF |

## Path (honest)

Virtual Floppy boot has **no** firmware Tcp4/Ip4/Dhcp4. Bring-up was:

1. `ConnectController` → `snp=12`
2. SNP residual + smoltcp DHCP → `10.99.99.127`
3. PRE-EBS TCP listen `:8443`
4. Operator LAN HTTP exchange → OK marker
5. Soft-fail continues → guest SHELL / M4 / E2

## Serial excerpt

```text
boot: SNP residual MAC=b0:26:28:5c:5a:3a
boot: SNP DHCP discover…
boot: mgmt HTTP listening on 10.99.99.127:8443 (PRE-EBS SNP window)
RAYNU-V-M7-UEFI-HTTP-OK
```

## Operator curl (Mac)

First close: [`logs/2026-08-16-uefi-http-ok-curl.txt`](logs/2026-08-16-uefi-http-ok-curl.txt) — `GET /` → 200 SPA.

Clarifying retest (same floppy digests): [`logs/2026-08-16-uefi-http-ok-curl-retest.txt`](logs/2026-08-16-uefi-http-ok-curl-retest.txt):

```text
* Connected to 10.99.99.127 (10.99.99.127) port 8443
> GET / HTTP/1.1
< HTTP/1.1 200 OK
< Content-Type: text/html; charset=utf-8
< Content-Length: 10757

> GET /vms HTTP/1.1
> Authorization: Bearer raynu-v-bringup
< HTTP/1.1 200 OK
< Content-Type: application/json
< Content-Length: 25
{"ok":true,"listed":true}
```

SPA body includes `<title>RayNu-V</title>` and `data-raynu-webui="1"`.  
Bearer `/vms` returns codec JSON — ADR-012 dual acceptance met.
## HDA / Everest

- **M7.6** iron OK: **closed**
- **E3 Network UI:** closed at MVP (plaintext HTTP, PRE-EBS window) per ADR-012 serial bind + OK marker (≥1 exchange). TLS / post-EBS listen remain follow-ons.
- **Not claimed:** Mount Everest (E4/E5 still open); post-EBS persistent mgmt HTTP.

## Reproduce

```bash
./tools/rebuild-uefi-http-boot-media.sh
# remap iDRAC Virtual Floppy; SOL console com2
# on CURL NOW / listen line:
curl -sS http://HOST_NIC_IP:8443/
curl -sS -H 'Authorization: Bearer raynu-v-bringup' http://HOST_NIC_IP:8443/vms
```
