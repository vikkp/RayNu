# R640 M7.6 / ADR-012 — UEFI NIC HTTP OK (2026-08-16)

**Claim:** `RAYNU-V-M7-UEFI-HTTP-OK` on real Dell PowerEdge R640.  
**Also still true:** `RAYNU-V-R640-BOOT-OK` (E2) on the same boot.

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
