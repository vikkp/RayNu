# v0.1.0-everest-closed — Mount Everest iron checkpoint (HDA 99%)

**Checkpoint kit** (not 1.0 / GA). Freezes the iron pin that closed the
Mount Everest product loop on a real PowerEdge R640 via Phase B SPA ISO
install → disk boot. HDA stays **99%**.

## What this freezes

| Area | State |
|------|--------|
| E1 | Ship EFI — this kit (`r640-hypervisor.efi` + checksums) |
| E2 | `RAYNU-V-R640-BOOT-OK` (closed) |
| E3 / E3b | PRE-EBS SNP + post-EBS coexist `RAYNU-V-M7-HOST-NIC-HTTP-OK` on BCM5720 |
| E4 | SPA create-VM + Start of typed product ISO (Phase B; not SHELL) |
| E5 | Iron COM2: `RAYNU-V-M7-ISO-INSTALL-OK` → `RAYNU-V-RAYNU-F-DISK-BOOT-OK` → installed Alpine `login:` |
| HDA | **99%** (not 100%) |
| Open polish | TLS, leftover-DRAM persist across **host** HV reboot, guest VNC / console UI |

## Provenance

| Field | Value |
|-------|--------|
| Tag target (iron pin) | `f72b4276d198b5e90147e9be1037d0d0b7213a28` |
| Branch at close | `cursor/phase-b-skip-ovmf-facd` |
| CI run | [34552377351](https://github.com/vikkp/RayNu/actions/runs/34552377351) |
| CI EFI SHA256 | `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc` |
| COM2 fingerprint | `build: sha=f72b4276d198` |
| Iron evidence | `docs/evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md` (on phase-b tip) |

**Do not F11** `7f8dc0a9` / CI `34548550755` (Mac `curl: (7)`). Parent of this pin.

COM2 close order (abbrev):

```text
build: sha=f72b4276d198
RAYNU-V-M7-PHASE-B-SKIP-OVMF-OK
RAYNU-V-M7-PHASE-B-E4-CONTINUE-OK
HOST-NIC coexist listening on 10.99.99.145:8443
RAYNU-V-M7-HOST-NIC-HTTP-OK
boot: E4 SPA start — RayNu-F product ISO (Phase B; not SHELL; not ISO-INSTALL-OK)
RAYNU-V-M7-ISO-INSTALL-OK
RAYNU-V-RAYNU-F-DISK-BOOT-OK
… Linux root=UUID=814a97a0-… → login:
```

## Verify / remap

```bash
( cd releases/v0.1.0-everest-closed && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-everest-closed
```

Or download `r640-hypervisor.efi` from the GitHub release (SHA256
`e74460ff…`) and copy to Cruzer `EFI/BOOT/BOOTX64.EFI`. Keep APE PHY. Bind
LOM `:38`. Do not write PERC. Safe shutdown is iDRAC **Force Off**.

Mac-built iron floppy digests may differ by toolchain from this Linux CI
rebuild of the same source — same caveat as prior `v0.1.0-*` kits.

## Honesty

- Checkpoint only — **not** a GA / 1.0 product release. **HDA 99%**.
- Host/CI **never** print `RAYNU-V-M7-ISO-INSTALL-OK` (iron COM2 only).
- Does **not** claim TLS, leftover-disk persist across a hypervisor reboot,
  guest VNC, multi-distro, or 100%.
- Tag is the **iron pin** `f72b4276`, not later docs/site commits
  (`615055da` HDA, `cursor/everest-closed-m8-adr-facd` tip).
