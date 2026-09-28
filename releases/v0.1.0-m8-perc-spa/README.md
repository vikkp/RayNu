# v0.1.0-m8-perc-spa — PERC SPA rollback

Flash this file to return to the EFI that waited on the SPA and then
lived both paths on RAYNU-SPARE. It is **not** GitHub Latest. Latest
stays `v0.1.0-everest-closed` (`f72b4276`).

This is the rollback for ADR-019 and the Guests-screen work. The older
standing-SPA pin `v0.1.0-m8-a4s` does not have this choice.

## What this freezes

| Area | State |
|------|--------|
| Choice | A latched spare image waits. Unattended boot does not run `setup-disk` |
| Reinstall | Choice 2 wrote UUID `d43dbf07-471e-4fee-a4b7-99495937d124` on the 8 GiB window |
| Boot | A later boot, choice 1, reached `localhost login:` from that UUID with `setup-disk` withheld |
| Disk | Guest disk is that 8 GiB window. The rest of RAYNU-SPARE stays hidden. UBUNTU0 is untouched |
| Marker | `RAYNU-V-M8-PERC-BOOT-OK` on the boot-installed path |

## Provenance

| Field | Value |
|-------|--------|
| EFI source | `8ad2ac89125ee055d978f4ac1161770b97da40a2` |
| CI | `36362775808` |
| EFI bytes | 2,088,960 |
| EFI SHA256 | `6d5dc7cbe088182b6dcd7896717f1cb204ba2e3604f477404ef0b809349c6849` |
| COM2 | `build: sha=8ad2ac89125e` |
| Lease on that boot | `10.99.99.151` |

The bytes in this directory are the CI artifact. A later documentation
commit does not change them. A local rebuild will stamp a different
`build: sha=` and is not this rollback.

## Verify

```bash
( cd releases/v0.1.0-m8-perc-spa && sha256sum -c r640-hypervisor.efi.sha256 )
```

Cruzer: replace only `EFI/BOOT/BOOTX64.EFI`. Leave `raynuf.txt`. Do not
write the Toshiba. Do not pass `--init-new-cruzer`.
