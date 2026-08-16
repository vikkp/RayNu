# v0.1.0-e4-spa-arm — E4 SPA + install-arm checkpoint

**Preserve point** before networking deep-dive work (TLS / post-EBS listen /
Tcp4 path / residuals). Freezes tip `main` after E4 iron SPA evidence (#152/#153)
and E5 QEMU lab two-boot.

## What this freezes

| Area | State |
|------|-------|
| E2 | `RAYNU-V-R640-BOOT-OK` (closed) |
| E3 | `RAYNU-V-M7-UEFI-HTTP-OK` SNP residual PRE-EBS (closed) |
| E4 | Durable PRE-EBS mgmt + SPA create-VM + serial log + ESP auth (#151); iron SPA exercised |
| E5 | QEMU lab `BOOTED-FROM-DISK`; iron arm → `DISK-WRITTEN` / `LAB-OK` / `REBOOT-PENDING` |
| Open | `RAYNU-V-M7-ISO-INSTALL-OK` / Mount Everest |

## Provenance

| Field | Value |
|-------|-------|
| Git tip | `7b9a17b` — merge #153 (evidence checksums); EFI code tip `46090df` E4 |
| Path | SNP + smoltcp PRE-EBS (Tcp4 absent on Virtual Floppy) |
| Iron evidence | `docs/evidence/r640/2026-08-16-e4-spa-install-arm.md` |
| CI EFI SHA256 | `c9c213e092a7dece1545fa4ce5e4fcc0cb7370beb6130a00fa0b9dec585e1e24` |

COM2 fingerprint (E4 SPA + arm era):

```text
boot: CURL NOW → http://10.99.99.127:8443/
RAYNU-V-AUDIT: AuthAllowed method_tag=2
RAYNU-V-AUDIT: VmCreated guest_id=1
RAYNU-V-M7-UEFI-HTTP-OK
boot: E5 install-sized virtio-blk armed (PRE-EBS contract)
RAYNU-V-M7-ISO-DISK-WRITTEN
RAYNU-V-M7-ISO-INSTALL-LAB-OK
RAYNU-V-M7-ISO-REBOOT-PENDING
RAYNU-V-R640-BOOT-OK
```

## Verify / remap

```bash
( cd releases/v0.1.0-e4-spa-arm && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-e4-spa-arm
```

Mac-built iron floppy digests may differ by toolchain from this Linux CI rebuild
of the same source — same caveat as `v0.1.0-m76-snp-http`.

## Honesty

Does **not** claim Mount Everest or iron `RAYNU-V-M7-ISO-INSTALL-OK`.
Next Everest move after networking work: iron reboot-to-disk.
