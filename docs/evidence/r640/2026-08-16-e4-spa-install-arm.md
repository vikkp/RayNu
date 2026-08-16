# R640 E4 — SPA create-VM + install arm (2026-08-16)

**Claim (E4 polish on iron):** Operator Firefox SPA on PRE-EBS SNP HTTP
created guests with Bearer auth; install REST armed a **64 MiB** virtio-blk
contract that survived ExitBootServices.  
**Also still true:** `RAYNU-V-M7-UEFI-HTTP-OK`, `RAYNU-V-R640-BOOT-OK`.

**Not claimed:** `RAYNU-V-M7-ISO-INSTALL-OK` / Mount Everest / iron reboot-to-disk.

## Closing tip

| Field | Value |
|-------|-------|
| Git | `46090df` — `feat(e4): durable PRE-EBS mgmt + serial log + ESP auth` |
| Media | iDRAC Virtual Floppy (tip kit after #151 merge) |
| Path | SNP residual + smoltcp (Tcp4 absent on floppy) |
| Lease | `10.99.99.127:8443` |
| Auth | bring-up token (`boot: E4 auth: bring-up token (lab; no auth.token)`) |
| Operator UI | Firefox `http://10.99.99.127:8443` during `CURL NOW` window |

## What this run proves

| Step | Evidence |
|------|----------|
| SPA served | Multiple `SNP TCP accept` + `SNP HTTP exchange ok` |
| Bearer create-VM | `AuthAllowed method_tag=2` + `VmCreated guest_id=1` (and `2`) |
| Authed GET | `AuthAllowed method_tag=1` (list / log refresh) |
| Listen close | `RAYNU-V-M7-UEFI-HTTP-OK` |
| Install arm across EBS | `bytes=67108864` + `E5 install-sized virtio-blk armed (PRE-EBS contract)` |
| Disk write honesty | `RAYNU-V-M7-ISO-DISK-WRITTEN` → `LAB-OK` → `REBOOT-PENDING` |
| E2 still green | `RAYNU-V-R640-BOOT-OK` after M4.5 / VMXOFF |

## Serial log

Full PRE-EBS + arm excerpt: [`logs/2026-08-16-e4-spa-install-arm-com2.txt`](logs/2026-08-16-e4-spa-install-arm-com2.txt)

```text
boot: SNP lease 10.99.99.127/24 router=10.99.99.1
boot: CURL NOW → http://10.99.99.127:8443/
RAYNU-V-AUDIT: AuthAllowed method_tag=2
RAYNU-V-AUDIT: VmCreated guest_id=1
…
RAYNU-V-M7-UEFI-HTTP-OK
…
boot: M4.3 virtio-blk … bytes=67108864
boot: E5 install-sized virtio-blk armed (PRE-EBS contract)
RAYNU-V-M7-ISO-DISK-WRITTEN
RAYNU-V-M7-ISO-INSTALL-LAB-OK
RAYNU-V-M7-ISO-REBOOT-PENDING
…
RAYNU-V-R640-BOOT-OK
```

## Honesty / residuals

- **E4:** SPA create + Bearer + durable PRE-EBS exchanges exercised on iron.
- **E5:** Install **arm + LBA write + REBOOT-PENDING** on iron with 64 MiB disk —
  still **not** iron `RAYNU-V-M7-ISO-INSTALL-OK` (no second-boot / persist close).
- TLS / post-EBS listen / El Torito / guest console remain follow-ons.
- Lab markers (`LAB-OK`, `REBOOT-PENDING`) on iron are honest progress, not Everest close.

## HDA / Everest

- E4 operator MVP on iron: **exercised** (this evidence).
- `STATUS-iso-install` remains **open** until iron install-to-disk + reboot-to-disk.
- Mount Everest still open (E5 iron close residual).

## Reproduce

```bash
git checkout main && git pull
./tools/rebuild-uefi-http-boot-media.sh
# remap Virtual Floppy; SOL console com2
# on CURL NOW: open http://LEASE_IP:8443
# SPA: Save token raynu-v-bringup → Create → Deploy ISO → Install → Refresh log
```
