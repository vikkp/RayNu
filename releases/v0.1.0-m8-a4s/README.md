# v0.1.0-m8-a4s — standing SPA rollback

Flash this file to return to the EFI that kept one HTTPS session beside
the installed Alpine login. It is **not** GitHub Latest. Latest stays
`v0.1.0-everest-closed` (`f72b4276`), the original install loop on
leftover DRAM.

Do not flash this kit over a Firefox tab that is already talking to
`1f33eeda`. That session is the lived page.

## What this freezes

| Area | State |
|------|--------|
| Persist | Sixth `login:` of UUID `a0ad99ac-ca25-4458-ade1-cd8599ca4928` on the 8 GiB Toshiba slice |
| A4s | One `TCP accept`, then `HTTP keep-alive`. Host stayed green. Activity showed guest COM1 |
| A6 | Open. `RAYNU-V-M8-CONSOLE-OK` still uses `write_line` and is hushed after Linux shares the UART |
| TLS | Lab millicert, TLS 1.2. `RAYNU-V-M8-TLS-OK` did not print on this persist boot |
| PERC | Untouched. USB is not PERC |

## Provenance

| Field | Value |
|-------|--------|
| EFI source | `1f33eeda72f99e432204dc943386d99d3d341be8` |
| CI | `36137732145` |
| EFI bytes | 2,065,920 |
| EFI SHA256 | `5539d83806e120eb6c331c11281407c0f902b121a770c7faa46d6a1a26280693` |
| COM2 | `build: sha=1f33eeda72f9` |
| Lease on that boot | `10.99.99.154` |
| Evidence | `docs/evidence/r640/2026-09-24-36d3b559-persist-login.md` (boot 6) |

The bytes in this directory are the CI artifact. A later documentation
commit does not change them. A local rebuild will stamp a different
`build: sha=` and is not this rollback.

## Verify

```bash
( cd releases/v0.1.0-m8-a4s && sha256sum -c r640-hypervisor.efi.sha256 )
```

Cruzer: replace only `EFI/BOOT/BOOTX64.EFI`. Leave `raynuf.txt`. Do not
write the Toshiba. Do not pass `--init-new-cruzer`.
