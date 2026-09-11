# R640 iron evidence

**Status file:** [`STATUS`](STATUS) — `STATUS=closed` after real PowerEdge R640
first light (2026-08-15). See [`2026-08-15-r640-first-light.md`](2026-08-15-r640-first-light.md).

**M7.6 HTTP:** [`2026-08-16-uefi-http-ok.md`](2026-08-16-uefi-http-ok.md)  
**Tcp4 absent (Floppy root cause):** [`2026-08-16-uefi-tcp4-absent-root-cause.md`](2026-08-16-uefi-tcp4-absent-root-cause.md)  
**E4 SPA + install arm:** [`2026-08-16-e4-spa-install-arm.md`](2026-08-16-e4-spa-install-arm.md)  
**E5 persist write + detect (BLK fail):** [`2026-08-16-e5-persist-detect-blk-fail.md`](2026-08-16-e5-persist-detect-blk-fail.md)  
**E5 iron reboot-to-disk (close):** [`2026-08-16-e5-iso-install.md`](2026-08-16-e5-iso-install.md)  
**Preserve kit:** [`releases/v0.1.0-adr013-baseline/`](../../../releases/v0.1.0-adr013-baseline/) — WARN-only idle + ADR-013 Accepted (before native NIC)  
**Prior preserve:** [`releases/v0.1.0-e4-spa-arm/`](../../../releases/v0.1.0-e4-spa-arm/) — checkpoint before networking deep-dive  
**E5 ISO install:** [`STATUS-iso-install`](STATUS-iso-install) — **closed** (2026-08-16 stamp persist; 2026-09-10 real Alpine install)  
**E5 real distro install on iron (2026-09-10):** [`2026-09-10-59ac070-iso-install-ok.md`](2026-09-10-59ac070-iso-install-ok.md) — EFI `59ac070` / run `34425781629`; RayNu-F + Stage 46 product ISO; `setup-disk -m sys /dev/vda` → `Installation finished. No error reported.` → **`RAYNU-V-M7-ISO-INSTALL-OK`** on COM2; F7 relaunch then failed `VMCLEAR/VMPTRLD` (16 KiB host stack overflow into the VMCS) — iron `RAYNU-V-RAYNU-F-DISK-BOOT-OK` **not** claimed  
**E5 F7 relaunch on iron → installed GRUB menu (2026-09-10):** [`2026-09-10-975f8fc-f7-relaunch-grub-menu-exit-cap.md`](2026-09-10-975f8fc-f7-relaunch-grub-menu-exit-cap.md) — EFI `975f8fc` / run `34474850361`; `src=kbc` → relaunch → `GPT ESP lba=2048` → GRUB 2.12 from `vda` drew its menu (`executed automatically in 2s`), then `stop exit-cap exits=1048577` fired inside GRUB's timeout (menu poll loop ~2 exits/µs on R640) — fixed by the RayNu-F wall cap; iron `RAYNU-V-RAYNU-F-DISK-BOOT-OK` still **not** claimed  
**E5 reboot-to-disk CLOSED on iron (2026-09-10):** [`2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md`](2026-09-10-56a3ffd-e5-reboot-to-disk-disk-boot-ok.md) — EFI `56a3ffd` / run `34480107961`; install re-ran (`RAYNU-V-M7-ISO-INSTALL-OK`), `reboot` → F7 relaunch → `GPT ESP lba=2048` → installed GRUB 2.12 countdown `2s → 1s → 0s` → `Booting 'Alpine Linux v3.21, with Linux lts'` → **`RAYNU-V-RAYNU-F-DISK-BOOT-OK`** → `RAYNU-V-RAYNU-F-EBS-OK` → second `Linux version 6.12.13-0-lts` with `root=UUID=9af18543-…` → `EXT4-fs (vda2): mounted` → OpenRC → `login:` → `/proc/cmdline` from the installed system. **E5 Phase A whole loop closed on the real R640.**  
**Phase B CLOSED on iron (2026-09-11):** [`2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md`](2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md) — EFI `f72b4276` / run `34552377351`; no `--raynu-f`; coexist `HOST-NIC-HTTP-OK` on `10.99.99.145:8443` → SPA Start of RayNu-F ISO → `ISO-INSTALL-OK` → `DISK-BOOT-OK` → second Linux `root=UUID=814a97a0-…` → `login:`. Mount Everest product loop closed on iron. Residual: leftover persist / TLS / console  
**Post-EBS SNP dead:** [`2026-08-17-post-ebs-snp-dead.md`](2026-08-17-post-ebs-snp-dead.md) — firmware SNP hang + curl timeout + RSOD; do not claim `POST-EBS-HTTP-OK`  
**ADR-013 Phase 0 census:** [`2026-08-17-phase0-census.md`](2026-08-17-phase0-census.md) — iron pick **`14e4:165f`** BCM5720 dual-port  
**E3b / M7.8 iron close:** [`2026-08-20-e3b-host-nic-http-ok.md`](2026-08-20-e3b-host-nic-http-ok.md) — **`RAYNU-V-M7-HOST-NIC-HTTP-OK`** after `BOOT-OK` on `:38` / `10.99.99.144:8443`  
**Next architecture:** [`docs/adr/ADR-013.md`](../../adr/ADR-013.md) — management network (E3b, **Accepted**; Phase D **closed on iron** 2026-08-20)  
**E4 hang-fix Cruzer flash:** [`2026-08-21-e4-cruzer-flash-hangfix.md`](2026-08-21-e4-cruzer-flash-hangfix.md) — EFI `f413a9fc` on `RAYNUV` (superseded)  
**E4 G0-relocate Cruzer flash:** [`2026-08-21-e4-cruzer-flash-g0reloc.md`](2026-08-21-e4-cruzer-flash-g0reloc.md) — EFI `618e89e2` on `RAYNUV` (superseded; memcpy VMPTRLD loop)  
**E4 G0-clone Cruzer flash:** [`2026-08-21-e4-cruzer-flash-g0clone.md`](2026-08-21-e4-cruzer-flash-g0clone.md) — EFI `63cd694f` on `RAYNUV` (`CRUZER-FLASH-OK`)  
**E4 63cd694f clone then slot 1 error 11:** [`2026-08-21-e4-g0clone-spa-slot1-rev11.md`](2026-08-21-e4-g0clone-spa-slot1-rev11.md) — clone+marker+G0 VMLAUNCH OK; `VMPTRLD` slot 1 error 11; fail-soft G0; **not E4 closed**  
**E4 eb456eec VMCLEAR then SPA error 7:** [`2026-08-21-e4-vmclear-spa-entry7.md`](2026-08-21-e4-vmclear-spa-entry7.md) — error 11 gone; slot 1 re-entry `VMLAUNCH` error 7; idle; **not E4 closed**  
**E4 no-incoming-rewrite first SPA then zeros:** [`2026-08-21-e4-norewrite-spa-zeros.md`](2026-08-21-e4-norewrite-spa-zeros.md) — spec 201/start 200; first SPA `VMLAUNCH` OK; re-entry ctls all 0 / error 7; **not E4 closed** (superseded)
**E4 SPA VMLAUNCH + re-entry (close):** [`2026-08-21-e4-spa-shadow-reentry-ok.md`](2026-08-21-e4-spa-shadow-reentry-ok.md) — EFI `2b795a0`; spec 201/start 200; marker + shadow restore `fields=98`; G0↔SPA clear-state `VMLAUNCH`; **P0-14 closed on iron** (SHELL stub; not Everest)  
**E4 618e89e2 coexist listen:** [`2026-08-21-e4-g0reloc-coexist-listen.md`](2026-08-21-e4-g0reloc-coexist-listen.md) — `10.99.99.126:8443`; Mac curl `(28)` on first listen; later curls succeeded  
**E4 hang-fix iron boot:** [`2026-08-21-e4-hangfix-boot.md`](2026-08-21-e4-hangfix-boot.md) — `SLICE-G0` then slot 1; coexist HTTP-OK; GET-only paste  
**E4 SPA start then G0 VMPTRLD fail (hang-fix):** [`2026-08-21-e4-spa-launch-vmptrld-fail.md`](2026-08-21-e4-spa-launch-vmptrld-fail.md) — marker then VMXOFF; **not E4 closed**  
**E4 618e89e2 SPA then VMPTRLD loop:** [`2026-08-21-e4-g0reloc-spa-vmptrld-loop.md`](2026-08-21-e4-g0reloc-spa-vmptrld-loop.md) — marker + fail-soft (no VMXOFF); memcpy'd G0 VMCS at `0x10a00000` fails `VMPTRLD` every tick; **not E4 closed**

**COM2 serial archives (paper §6):** [`logs/`](logs/) — full operator pastes +
SHA256SUMS (keepconfix residual, xsavesfix close, confirming rebuild, and
literal `RAYNU-V-R640-BOOT-OK` marker capture + screenshot).

**Template:** [`TEMPLATE.md`](TEMPLATE.md) — copy to a dated file for future
campaigns / soak.

**Runbook:** [`docs/runbooks/r640_boot.md`](../../runbooks/r640_boot.md)  
**Living paper:** [`docs/paper/RayNu-V-Verification-Paper.md`](../../paper/RayNu-V-Verification-Paper.md) · site [`paper.html`](../../../site/paper.html)

Host scaffold smoke (`./tools/m7-r640-smoke.sh`) proves this directory and the
runbook exist; it does **not** print `RAYNU-V-R640-BOOT-OK`. That iron marker
is claimed only via filled evidence + `STATUS=closed`.
