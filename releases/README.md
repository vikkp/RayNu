# RayNu-V release kits (git-tracked)

Built EFI kits live here so a normal `git pull` on an operator laptop
downloads them. Build outputs under `/dist/` and `/target/` stay gitignored.

## Latest

| Version | EFI | SHA256 |
|---------|-----|--------|
| **v0.1.0** | [`v0.1.0/r640-hypervisor.efi`](v0.1.0/r640-hypervisor.efi) | see `v0.1.0/r640-hypervisor.efi.sha256` |

```bash
cd /path/to/raynu
git pull
sha256sum -c releases/v0.1.0/r640-hypervisor.efi.sha256
```

## Boot media (not in git)

FAT `.img` / El Torito `.iso` are large; regenerate on the laptop from this kit:

```bash
./tools/make-boot-media.sh --kit releases/v0.1.0
# → dist/raynu-v-0.1.0-boot-media/*-uefi-boot.img
```

Then map the `.img` in iDRAC Virtual Media (see `docs/runbooks/r640_field_guide.md`).

## Rebuild

```bash
./tools/build.sh
./tools/package-release.sh
# copy/refresh releases/vX.Y.Z/ from dist/raynu-v-X.Y.Z/ when shipping
```
