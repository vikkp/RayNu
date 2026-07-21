# Runbook — R640 iron week to-do list (M7.5)

**Iron marker:** `RAYNU-V-R640-BOOT-OK` (real PowerEdge R640 only)  
**Scaffold smoke:** `./tools/m7-r640-smoke.sh` → `RAYNU-V-M7-R640-SCAFFOLD-OK`  
**First-light runbook:** [`r640_boot.md`](r640_boot.md)  
**Ship / media placement:** [`usb_idrac.md`](usb_idrac.md)  
**Evidence:** [`docs/evidence/r640/`](../evidence/r640/)

Software through M7.4 is closed on Latitude. This list is what you do when a
real PowerEdge R640 is racked, powered, and reachable on iDRAC. Host scaffold
smoke is **not** first light — only serial from this box closes
`RAYNU-V-R640-BOOT-OK`.

---

## 1. Rack basics

- [ ] Mount the R640, connect power, and bring iDRAC up on the management
      network (note IP, credentials, service tag).
- [ ] Confirm UEFI boot mode in BIOS; enable virtualization (VT-x / VT-d as
      required for your path).
- [ ] From a laptop, open the iDRAC web UI and start a **virtual console**
      session (COM1 / serial view ready **before** any reboot).
- [ ] Decide boot media: physical **USB** or **iDRAC virtual media** (both
      documented in [`usb_idrac.md`](usb_idrac.md)).

## 2. Build and verify the EFI kit

- [ ] On a build machine (not the R640):
      `./tools/build.sh` → `./tools/check-size.sh` → `./tools/package-release.sh`
- [ ] Verify checksums before you touch media:
      `sha256sum -c r640-hypervisor.efi.sha256` (inside `dist/raynu-v-*`)
- [ ] Keep the printed SHA256 — you will paste it into the evidence template

## 3. Use the runbooks (ship kit → first boot)

1. Place the binary per [`usb_idrac.md`](usb_idrac.md).  
2. Capture first light per [`r640_boot.md`](r640_boot.md).

- [ ] Copy `r640-hypervisor.efi` to media as `\EFI\BOOT\BOOTX64.EFI`
      (USB FAT32 ESP, or iDRAC Virtual Media map).
- [ ] Open iDRAC virtual console **before** reboot; prepare to save the serial
      log.
- [ ] One-time boot to USB / virtual media (F11 / iDRAC next-boot).
- [ ] On COM1, look for at least `RAYNU-V-M0-BOOT-OK`, then VMX markers
      (`RAYNU-V-M1-VMXON-OK` / `RAYNU-V-M1-VMEXIT-OK` when VT-x works), then as
      much EPT / Linux shell path as iron reaches.
- [ ] Save the full serial log. If you stop short of shell, write the residual
      down — do not invent markers.
- [ ] Unmount virtual media after the attempt so later boots are not sticky.

## 4. Fill the evidence template

Empty templates do **not** close M7.5.

- [ ] Read [`docs/evidence/r640/README.md`](../evidence/r640/README.md)
- [ ] Copy [`TEMPLATE.md`](../evidence/r640/TEMPLATE.md) to a dated file under
      `docs/evidence/r640/` (example: `2026-08-15-r640-first-light.md`)
- [ ] Fill every table field: date, operator, service tag, USB vs vMedia, EFI
      path, SHA256, kit version, iDRAC/BIOS notes, serial channel
- [ ] Check off required serial markers and paste a **real R640** COM1 excerpt
      (not Latitude/QEMU logs)
- [ ] Leave [`STATUS`](../evidence/r640/STATUS) as `STATUS=open` until the close
      PR — scaffold smoke requires that until iron is proven

## 5. Close M7.5 in git (only after iron)

- [ ] Open a PR that adds the filled dated evidence file and points `STATUS` to
      `STATUS=closed` with that path
- [ ] Flip the GAP to `GAP(CLOSED M7.5): Real R640 boot` and update
      `docs/progress.md` / HDA E2 / site
- [ ] Only then claim `RAYNU-V-R640-BOOT-OK`. Never print that marker from
      `./tools/m7-r640-smoke.sh` on a laptop — that script only prints
      `RAYNU-V-M7-R640-SCAFFOLD-OK`

---

## Honesty

Latitude + QEMU nested KVM already proved the software path. The R640 is the
judge for Mount Everest E2. If first light fails, keep evidence honest (what
you saw, what residual remains) and fix forward — do not close the gate from a
host smoke.
