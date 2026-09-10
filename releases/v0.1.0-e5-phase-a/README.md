# v0.1.0-e5-phase-a — E5 Phase A on real R640

**Preserve point** for the lived ISO → installer → virtio-blk → reboot-to-disk
loop. Merge vehicle onto `main`: see [`docs/main_and_releases.md`](../../docs/main_and_releases.md).

Does **not** claim Mount Everest, Phase B (SPA Start of this disk), TLS, or
guest console UI.

## What this freezes

| Area | State |
|------|--------|
| E2 | `RAYNU-V-R640-BOOT-OK` (closed) |
| E3 / E3b | UEFI HTTP + native BCM5720 `:8443` after `BOOT-OK` (closed) |
| P0-14 / E4 | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` — SHELL stub `2b795a0` |
| E5 Phase A | Alpine 3.21 ISO → RayNu-F `setup-disk` → `ISO-INSTALL-OK` → F7 → installed GRUB → `DISK-BOOT-OK` → second Linux `root=UUID=` → `login:` |
| Open | Phase B (SPA/REST starts that disk). Iron SPA is still the SHELL stub until a later flash. |

## Provenance

| Field | Value |
|-------|--------|
| Git pin | `56a3ffda8406` (`cursor/pit-during-apk-b7a8`) |
| CI run | [34480107961](https://github.com/vikkp/RayNu/actions/runs/34480107961) |
| CI EFI SHA256 | `d472191f2010045f9357ac187e5f4cb5e52647a4f83d3455f3cc8fb71754ab23` |
| Iron evidence | `docs/evidence/r640/2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md` |
| Flash | `--run 34480107961 --raynu-f` + alpine-extended (Phase A auto-path). Do not F11 `34474850361` / `34425781629`. |

COM2 fingerprint (first lines that close Phase A — see the evidence doc for the full log):

```text
RAYNU-V-M7-ISO-INSTALL-OK
guest reset requested src=kbc n=1
RayNu-F relaunch after reset (F7)
RAYNU-V-RAYNU-F-DISK-BOOT-OK
Linux version 6.12.13-0-lts ... root=UUID=9af18543-...
login:
```

`build: sha=` on that log is `56a3ffda8406`. This kit is the Linux CI rebuild
of that source — same caveat as `v0.1.0-e4-spa-launch`.

## Verify / remap

```bash
( cd releases/v0.1.0-e5-phase-a && sha256sum -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-e5-phase-a
```

Or Cruzer via `flashcruzer.sh --run 34480107961 --raynu-f --linux-iso <alpine-extended>`.

## Honesty

- Nested QEMU ≠ R640.
- `raynuf.txt` is the Phase A auto-path. Phase B flashes **omit** `--raynu-f`.
- Host/CI never prints `ISO-INSTALL-OK`.
- Next kit `v0.1.0-e5-phase-b` waits on COM2 of SPA-started RayNu-F.
