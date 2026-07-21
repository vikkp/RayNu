# Runbook — USB / iDRAC virtual media (M7.0)

**Marker:** `RAYNU-V-M7-SHIP-OK`  
**Smoke:** `./tools/m7-ship-smoke.sh`  
**Package:** `./tools/package-release.sh`

## Story

M7.0 produces an **ops-trustable EFI release kit**: versioned `r640-hypervisor.efi`,
SHA256 sidecars, size gate (ADR-003), and this one-page deploy path for USB or
iDRAC virtual media. Closing the kit does **not** require a real R640 boot —
that is **M7.5** (`RAYNU-V-R640-BOOT-OK`).

## Build the kit

```bash
./tools/build.sh
./tools/check-size.sh
./tools/package-release.sh
# or: SKIP_BUILD=1 ./tools/package-release.sh   # if EFI already built
```

Output (example for `Cargo.toml` version `0.1.0`):

```text
dist/raynu-v-0.1.0/
  r640-hypervisor.efi
  r640-hypervisor.efi.sha256
  VERSION
  SHA256SUMS
  MANIFEST.txt
dist/raynu-v-0.1.0.tar.gz
dist/raynu-v-0.1.0.tar.gz.sha256
```

Verify:

```bash
cd dist/raynu-v-0.1.0
sha256sum -c r640-hypervisor.efi.sha256
sha256sum -c SHA256SUMS
```

## USB (FAT32 EFI System Partition)

1. Format a USB stick as **FAT32** with an EFI System Partition layout.  
2. Copy `r640-hypervisor.efi` to `\EFI\BOOT\BOOTX64.EFI`  
   (or `\EFI\BOOT\r640-hypervisor.efi` and select it in the boot manager).  
3. Insert USB into the PowerEdge; set one-time boot to USB in F11 boot menu / BIOS.  
4. Open the iDRAC **virtual console** (serial/COM1) before reboot.  
5. Expect the RayNu-V COM1 banner / boot markers on serial.

## iDRAC virtual media

1. Log into iDRAC (web).  
2. **Virtual Console** → **Virtual Media** → map the release directory or an ISO/img  
   that contains `r640-hypervisor.efi` as `\EFI\BOOT\BOOTX64.EFI`.  
3. Set next boot to virtual CD/USB (iDRAC / BIOS).  
4. Reboot; watch COM1 in the virtual console.  
5. Unmount virtual media after first light so production boots are not sticky.

## Checksums on the operator laptop

Always verify `r640-hypervisor.efi.sha256` (or the tarball `.sha256`) before
copying to USB / mapping as virtual media. Do not deploy an unverified binary.

## Limits

- This runbook ships the **binary**. Real R640 bring-up quirks (ACPI/APIC/timer)
  are tracked under **M7.5**.  
- Network Web UI and ISO install are **M7.1–M7.4**.  
- Secure Boot signing is not required for M7.0 (optional follow-on).
