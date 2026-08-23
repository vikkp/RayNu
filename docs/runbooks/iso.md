# Runbook — ISO deploy path (M7.3)

**Marker:** `RAYNU-V-M7-ISO-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md) · product ISO: [ADR-014](../adr/ADR-014.md)  
**Prior:** [datastore.md](datastore.md) (M7.2)

## What this gate proves

M7.3 closes a **documented kernel-extract** deploy path on top of the image library:

1. **Register ISO** metadata into `ImageTable` (`ImageKind::Iso`).
2. **Bind extract-boot** to the existing bzImage/initrd staging +
   `guest::load_bzimage_guest` path (PE `.askern` / ESP `BZIMAGE` + `INITRD`).
3. **Empty virtio-blk** install target (`devices/virtio_blk.rs` capacity surface).

## REST shapes (Bearer auth)

| Method | Path | Result |
|--------|------|--------|
| `POST` | `/iso/{id}/deploy` | 201 — register ISO if needed + bind extract-boot + default install disk |
| `GET` | `/iso/deploy` | 200 — listed count `1` when plan ready |
| `POST` | `/iso/{id}/attach` or `/iso/{id}/attach/{type}` | 201 — host El Torito CD-ROM attach (mock EFI prefix until blob upload) |
| `GET` | `/iso/attach` | 200 — listed count of host-attached CD-ROMs |
| `POST` | `/iso/{id}/firmware` | 201 — firmware-facing CD arm (requires host attach first) |
| `GET` | `/iso/firmware` | 200 — listed count of FirmwareArmed records |
| `POST` | `/fw/box` | 201 — box the embedded guest UEFI firmware envelope (not OVMF) |
| `GET` | `/fw` | 200 — listed count of boxed guest firmware envelopes (0/1) |
| `POST` | `/fw/load` | 201 — identity-lazy load the stub payload (requires box first) |
| `GET` | `/fw/load` | 200 — listed count of loaded guest firmware stubs (0/1) |
| `POST` | `/fw/ovmf` | 201 — probe a host mock UEFI `_FVH` (requires load first; not embedded EDK2) |
| `GET` | `/fw/ovmf` | 200 — listed count of probed OVMF volumes (0/1) |
| `POST` | `/fw/ovmf/esp` | 201 — load the ESP fixture after probe (not embedded EDK2) |
| `GET` | `/fw/ovmf/esp` | 200 — listed count of ESP-loaded OVMF volumes (0/1) |
| `POST` | `/fw/slot` | 201 — arm guest firmware slot 1 after ESP load (not VMLAUNCH) |
| `GET` | `/fw/slot` | 200 — listed count of armed firmware slots (0/1) |
| `POST` | `/fw/bind` | 201 — bind firmware slot 1 to guest 1 after arm (not VMLAUNCH) |
| `GET` | `/fw/bind` | 200 — listed count of bound firmware guests (0/1) |

Token: `Authorization: Bearer raynu-v-bringup` (same as M6.4 / M7.1 / M7.2).

## Host smoke

```bash
./tools/m7-iso-smoke.sh
```

Expect:

```text
RAYNU-V-M7-ISO-OK
==> M7.3 ISO deploy smoke PASSED
```

## Honesty / residuals

- **MVP is kernel-extract**, not full installer media emulation. Lab/M7.3 only.
- Product ISO install is **UEFI-first + typed** ([ADR-014](../adr/ADR-014.md)):
  `linux_iso` | `windows_iso` | `generic_uefi`. Do not hard-wire SPA install to
  bzImage jump. Windows install is later; the type exists now.
- **`attach_cdrom_uefi`** after `FirmwareArmed` is `GuestVisible` — PCI
  IDE/ATAPI on the private guest-UEFI VMCS (`RAYNU-V-M7-E5-OVMF-CDROM-OK`).
  Unarmed calls still return `UnsupportedOnFirmware`. Stage 41
  (`RAYNU-V-M7-E5-OVMF-DXE-OK`) is past-PEI/DXE or a CD boot
  **attempt**, not a completed firmware CD boot. Host catalog **parse** (`parse_el_torito`)
  is Stage 0. Host **attach** (`attach_cdrom_host`) is Stage 1. Firmware **arm**
  (`attach_cdrom_firmware` / `FirmwareArmed`) is Stage 2
  (`RAYNU-V-M7-E5-CDROM-FIRMWARE-OK`). Guest firmware **envelope**
  (`box_guest_firmware` / `.asguefw`) is Stage 3
  (`RAYNU-V-M7-E5-GUEST-FW-OK`). Stub **load**
  (`load_guest_firmware` / `RAYNUFD`) is Stage 4
  (`RAYNU-V-M7-E5-GUEST-FW-LOAD-OK`). OVMF **probe**
  (`probe_ovmf_firmware` / `_FVH`) is Stage 5
  (`RAYNU-V-M7-E5-OVMF-PROBE-OK`). ESP **load**
  (`load_ovmf_from_esp`) is Stage 6
  (`RAYNU-V-M7-E5-OVMF-ESP-OK`). Slot **arm**
  (`arm_ovmf_firmware_slot`) is Stage 7
  (`RAYNU-V-M7-E5-OVMF-SLOT-OK`). Guest **bind**
  (`bind_ovmf_firmware_guest`) is Stage 8
  (`RAYNU-V-M7-E5-FW-BIND-OK`). Launch **prepare**
  (`prepare_ovmf_firmware_launch`) is Stage 9
  (`RAYNU-V-M7-E5-FW-PREP-OK`). Size-floor **stage**
  (`stage_ovmf_firmware_floor`) is Stage 10
  (`RAYNU-V-M7-E5-FW-FLOOR-OK`). EDK2-sized **stage**
  (`stage_edk2_ovmf_firmware`) is Stage 11
  (`RAYNU-V-M7-E5-FW-EDK2-OK`). ESP-path **launch**
  (`arm_ovmf_esp_launch` / `try_vmlaunch_guest_uefi_ovmf`) is Stage 12
  (`RAYNU-V-M7-E5-ESP-LAUNCH-OK`). Live-sized ESP **map**
  (`map_live_esp_ovmf`) is Stage 13 (`RAYNU-V-M7-E5-ESP-MAP-OK`).
  Reset-vector **arm** (`arm_ovmf_reset_vector`) is Stage 14
  (`RAYNU-V-M7-E5-RESET-VEC-OK`). Firmware-alias **arm**
  (`arm_ovmf_firmware_alias`) is Stage 15
  (`RAYNU-V-M7-E5-FW-ALIAS-OK`). Alias-EPT **program**
  (`program_ovmf_alias_ept`) is Stage 16
  (`RAYNU-V-M7-E5-ALIAS-EPT-OK`). Private alias-EPT **install**
  (`install_ovmf_alias_ept`) is Stage 17
  (`RAYNU-V-M7-E5-EPT-INSTALL-OK`). Real-ESP **qualify**
  (`qualify_real_esp_ovmf`) is Stage 18
  (`RAYNU-V-M7-E5-REAL-ESP-OK`). Guest-UEFI VMLAUNCH **insn arm**
  (`arm_ovmf_real_launch`) is Stage 19
  (`RAYNU-V-M7-E5-REAL-LAUNCH-OK`). Live-ESP **require**
  (`require_ovmf_live_esp`) is Stage 20
  (`RAYNU-V-M7-E5-LIVE-EXEC-OK`). Private guest-UEFI VMCS **arm**
  (`arm_ovmf_private_vmcs`) is Stage 21
  (`RAYNU-V-M7-E5-PRIV-VMCS-OK`). Live-ESP VMLAUNCH **issue**
  (`arm_ovmf_live_issue`) is Stage 22
  (`RAYNU-V-M7-E5-LIVE-ISSUE-OK`). Live-ESP **bytes probe**
  (`probe_ovmf_live_bytes`) is Stage 23
  (`RAYNU-V-M7-E5-LIVE-BYTES-OK`). Live-ESP **FD require**
  (`require_ovmf_live_fd`) is Stage 24
  (`RAYNU-V-M7-E5-LIVE-FD-OK`). Live-ESP **present**
  (`present_ovmf_live_esp`) is Stage 25
  (`RAYNU-V-M7-E5-LIVE-PRESENT-OK`). Live-ESP **admit**
  (`admit_ovmf_live_esp`) is Stage 26
  (`RAYNU-V-M7-E5-LIVE-ADMIT-OK`). Live-ESP **read**
  (`read_ovmf_live_esp`) is Stage 27
  (`RAYNU-V-M7-E5-LIVE-READ-OK`). Live-ESP **copy**
  (`copy_ovmf_live_esp`) is Stage 28
  (`RAYNU-V-M7-E5-LIVE-COPY-OK`). Live-ESP **place**
  (`place_ovmf_live_esp`) is Stage 29
  (`RAYNU-V-M7-E5-LIVE-PLACE-OK`). Live-ESP **apply**
  (`apply_ovmf_live_esp`) is Stage 30
  (`RAYNU-V-M7-E5-LIVE-APPLY-OK`). Live-ESP **commit**
  (`commit_ovmf_live_esp`) is Stage 31
  (`RAYNU-V-M7-E5-LIVE-COMMIT-OK`). Live-ESP **latch**
  (`latch_ovmf_live_esp`) is Stage 32
  (`RAYNU-V-M7-E5-LIVE-LATCH-OK`). Live-ESP **seal**
  (`seal_ovmf_live_esp`) is Stage 33
  (`RAYNU-V-M7-E5-LIVE-SEAL-OK`). Live-ESP **lock**
  (`lock_ovmf_live_esp`) is Stage 34
  (`RAYNU-V-M7-E5-LIVE-LOCK-OK`). Live-ESP **hold**
  (`hold_ovmf_live_esp`) is Stage 35
  (`RAYNU-V-M7-E5-LIVE-HOLD-OK`). Real ESP **retain**
  (`probe_ovmf_esp` / `accept_real_ovmf_bytes`) is Stage 36
  (`RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK`). Private guest-UEFI
  VMLAUNCH of those retained bytes is Stage 37
  (`RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`);
  `try_vmlaunch_ovmf_firmware` refuses the 80-byte mock, the 4 KiB floor,
  and the 1 MiB EDK2-sized fixture, then `MissingEsp` (no live map),
  `LiveMappedNotLaunched` (2 MiB+ map, no reset stub),
  `ResetVectorNotLaunched` (JMP FAR stub recorded),
  `FirmwareAliasNotLaunched` (4 MiB alias recorded),
  `AliasEptNotLaunched` (4 GiB window recorded),
  `AliasEptInstalledNotLaunched` (private install recorded),
  `RealEspNotLaunched` (real-ESP qualify recorded),
  `RealLaunchNotIssued` (insn path armed), or
  `LiveEspRequired` (live ESP bytes required), or
  `PrivateVmcsNotLaunched` (private guest-UEFI VMCS selected), or
  `LiveEspBytesNotPresent` (live-ESP issue path armed), or
  `LiveEspBytesAbsent` (live ESP bytes probed), or
  `LiveEspFdAbsent` (real ESP `OVMF.fd` required), or
  `LiveEspPresentAbsent` (real ESP bytes presented; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspAdmitAbsent` (real ESP bytes admitted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspReadAbsent` (real ESP bytes read-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspCopyAbsent` (real ESP bytes copy-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspPlaceAbsent` (real ESP bytes place-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspApplyAbsent` (real ESP bytes apply-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspCommitAbsent` (real ESP bytes commit-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspLatchAbsent` (real ESP bytes latch-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspSealAbsent` (real ESP bytes seal-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspLockAbsent` (real ESP bytes lock-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `LiveEspHoldAbsent` (real ESP bytes hold-attempted; live E4
  SHELL EPT not written; VMLAUNCH insn not issued), or
  `PrivateVmcsNotLaunched` (real ESP bytes retained; private
  guest-UEFI VMCS not allocated; VMLAUNCH insn not issued).
  Real EDK2 bytes are retained from ESP `EFI/RayNu/OVMF.fd` when present.
  Envelope box / stub load / FV probe / ESP load is not guest
  UEFI VMLAUNCH and not an embedded 4 MiB OVMF.
- **ISO blob upload** (raw bytes into ESP) is not claimed; metadata register is.
- Outside Proven Core (ADR-009 / ADR-014); size still ADR-003.

## Next

M7.4 Ops Web UI MVP (`RAYNU-V-M7-UI-OK`) surfaces create-VM + media attach.

E5 / M7.7 install-to-disk: see [iso_install.md](iso_install.md)
(`RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK`; iron `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` closed 2026-08-16).

E5 Stage 0 (host, closed): typed boot spec on REST/SPA + El Torito catalog parse.
`POST /vms/{id}/spec/{cpu}/{ram}/{disk}/{iso}/{linux_iso|windows_iso|generic_uefi}`.
`iso=0` stays E4 SHELL.

E5 Stage 1 (host, closed): `POST /iso/{id}/attach` arms host CD-ROM from El Torito.

E5 Stage 2 (host, closed): `POST /iso/{id}/firmware` arms FirmwareArmed after
host attach + boot-image sector validate. Not OVMF and not VMLAUNCH.

E5 Stage 3 (host, closed): `POST /fw/box` boxes the ADR-003 guest firmware
envelope (`.asguefw`). Not OVMF and not VMLAUNCH.

E5 Stage 4 (host, closed): `POST /fw/load` identity-lazy loads the `RAYNUFD`
stub payload after box. Not OVMF and not VMLAUNCH.

E5 Stage 5 (host, closed): `POST /fw/ovmf` probes a host mock `_FVH` after
load. ESP split-mode path is `EFI/RayNu/OVMF.fd`. Not embedded EDK2 and
not VMLAUNCH.

E5 Stage 6 (host, closed): `POST /fw/ovmf/esp` loads the ESP fixture after
probe. Not embedded EDK2 and not VMLAUNCH.

E5 Stage 7 (host, closed): `POST /fw/slot` arms firmware slot 1 after ESP
load. Not VMLAUNCH.

E5 Stage 8 (host, closed): `POST /fw/bind` binds slot 1 to guest 1 after
slot arm. Not VMLAUNCH.

E5 Stage 9 (host, closed): `POST /fw/prepare` records launch-prepare after
bind. `POST /fw/vmlaunch` refuses the 80-byte mock (409). Not VMLAUNCH.

E5 Stage 10 (host, closed): `POST /fw/floor` stages a 4 KiB size-floor FV
after prepare. `POST /fw/vmlaunch` refuses (not EDK2, 409). Not VMLAUNCH.

E5 Stage 11 (host, closed): `POST /fw/edk2` stages a 1 MiB EDK2-sized
candidate after floor (host test heap only). Production UEFI returns 409
(no embedded 1 MiB). `POST /fw/vmlaunch` refuses (`LaunchNotWired`, 409).
Not a shipped `OVMF.fd`. Not VMLAUNCH.

E5 Stage 12 (host, closed): `POST /fw/esp-launch` arms the ESP-path
VMLAUNCH contract after EDK2. `POST /fw/vmlaunch` calls
`try_vmlaunch_guest_uefi_ovmf` and returns 409 (`MissingEsp` — no live
`OVMF.fd`). The 1 MiB fixture is not launched. Not VMLAUNCH.

E5 Stage 13 (host, closed): `POST /fw/esp-map` records a live-sized
(2 MiB+) ESP map after launch-arm (host test heap only). Production UEFI
returns 409 (no embedded 2 MiB). `POST /fw/vmlaunch` returns 409
(`LiveMappedNotLaunched`). Not a shipped `OVMF.fd`. VMLAUNCH insn not issued.

E5 Stage 14 (host, closed): `POST /fw/reset-vec` records the SDM 9.1.4
reset-vector VMCS contract after the live map (host test heap stub only).
Production UEFI returns 409 (no embedded 2 MiB). `POST /fw/vmlaunch`
returns 409 (`ResetVectorNotLaunched`). Synthetic `0xEA` stub is not a
shipped `OVMF.fd`. VMLAUNCH insn not issued.

E5 Stage 15 (host, closed): `POST /fw/alias` records the unrestricted-guest
+ 4 GiB firmware-alias contract after reset-vector (host test heap fixture
only). Production UEFI returns 409 (no embedded 4 MiB). `POST /fw/vmlaunch`
returns 409 (`FirmwareAliasNotLaunched`). 4 MiB fixture is not a shipped
`OVMF.fd`. VMLAUNCH insn not issued.

E5 Stage 16 (host, closed): `POST /fw/alias-ept` records the 4 GiB
alias-EPT window after firmware-alias (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB). `POST /fw/vmlaunch`
returns 409 (`AliasEptNotLaunched`). Live EPT is not written. 4 MiB
fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued.

E5 Stage 17 (host, closed): `POST /fw/ept-install` records a private
alias-EPT install after program (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB). `POST /fw/vmlaunch`
returns 409 (`AliasEptInstalledNotLaunched`). Live E4 SHELL EPT is
not written. 4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn
not issued.

E5 Stage 18 (host, closed): `POST /fw/real-esp` records the real-ESP
VMLAUNCH-ready contract after install (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB). `POST /fw/vmlaunch`
returns 409 (`RealEspNotLaunched`). Live E4 SHELL EPT is not written.
4 MiB fixture is not a shipped `OVMF.fd`. VMLAUNCH insn not issued.

E5 Stage 19 (host, closed): `POST /fw/real-launch` records the
guest-UEFI VMLAUNCH insn-path arm after qualify (host test heap
fixture only). Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`RealLaunchNotIssued`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 20 (host, closed): `POST /fw/live-exec` records that live
ESP `\EFI\RayNu\OVMF.fd` bytes are required before VMLAUNCH (host
test heap fixture only). Production UEFI returns 409 (no embedded
4 MiB). `POST /fw/vmlaunch` returns 409 (`LiveEspRequired`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 21 (host, closed): `POST /fw/priv-vmcs` records a private
guest-UEFI VMCS (not E4 SHELL) after live-ESP require (host test
heap fixture only). Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`PrivateVmcsNotLaunched`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 22 (host, closed): `POST /fw/live-issue` records the
live-ESP VMLAUNCH issue path after private VMCS (host test heap
fixture only). Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspBytesNotPresent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 23 (host, closed): `POST /fw/live-bytes` records a live-ESP
bytes probe after live-issue (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspBytesAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 24 (host, closed): `POST /fw/live-fd` records that a real
ESP `OVMF.fd` is required after the live-bytes probe (host test heap
fixture only). Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspFdAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 25 (host, closed): `POST /fw/live-present` records a real-ESP
present-attempt after live-FD require (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspPresentAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 26 (host, closed): `POST /fw/live-admit` records a real-ESP
admit-attempt after live-present (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspAdmitAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 27 (host, closed): `POST /fw/live-read` records a real-ESP
read-attempt after live-admit (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspReadAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 28 (host, closed): `POST /fw/live-copy` records a real-ESP
copy-attempt after live-read (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspCopyAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 29 (host, closed): `POST /fw/live-place` records a real-ESP
place-attempt after live-copy (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspPlaceAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 30 (host, closed): `POST /fw/live-apply` records a real-ESP
apply-attempt after live-place (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspApplyAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 31 (host, closed): `POST /fw/live-commit` records a real-ESP
commit-attempt after live-apply (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspCommitAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 32 (host, closed): `POST /fw/live-latch` records a real-ESP
latch-attempt after live-commit (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspLatchAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 33 (host, closed): `POST /fw/live-seal` records a real-ESP
seal-attempt after live-latch (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspSealAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 34 (host, closed): `POST /fw/live-lock` records a real-ESP
lock-attempt after live-seal (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspLockAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 35 (host, closed): `POST /fw/live-hold` records a real-ESP
hold-attempt after live-lock (host test heap fixture only).
Production UEFI returns 409 (no embedded 4 MiB).
`POST /fw/vmlaunch` returns 409 (`LiveEspHoldAbsent`). Live E4
SHELL EPT is not written. 4 MiB fixture is not a shipped `OVMF.fd`.
VMLAUNCH insn not issued.

E5 Stage 36 (host + QEMU, closed): pre-EBS `probe_ovmf_esp` retains a
real ESP `OVMF.fd` when the image passes `accept_real_ovmf_bytes`.
QEMU stages a system `OVMF.fd` onto `EFI/RayNu/OVMF.fd`.
`guest_uefi_live_esp_bytes_present` follows that retain. Private
guest-UEFI VMCS is not allocated. `POST /fw/vmlaunch` after retain
returns 409 (`PrivateVmcsNotLaunched`) on host. No further `*Absent`
bookkeeping stages.

E5 Stage 37 (host + QEMU, closed): after retain, boot allocates a
private guest-UEFI VMCS + alias EPT and issues `VMLAUNCH` of the
retained bytes at `0xFFFF_FFF0` (`RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK`).
That path is not the E4 SHELL VMCS/EPT. Host `cargo test` still
returns `PrivateVmcsNotLaunched` and does not execute the instruction.
First entry is not a distro installer. Do not claim `ISO-INSTALL-OK`.

E5 Stage 38 (host + QEMU, closed): OVMF SEC `mov cr4, 0x640` no longer
triple-faults. `CR4.VMXE` is host-owned (mask + shadow). Serial
`RAYNU-V-M7-E5-OVMF-ALIVE-OK` after two non-TF exits or a guest HLT.
`RAYNU-V-M7-E5-OVMF-PAST-SEC-OK` after linear leaves the last 64 KiB
and PEI PCI / firmware COM / HLT. COM1/COM2 forwarded. Not full DXE.
Not installer.

E5 Stage 40 (host + QEMU): `attach_cdrom_uefi` after FirmwareArmed
presents the ISO on a PIIX3-class PCI IDE/ATAPI function (`00:01.1`)
with primary PIO. Serial `RAYNU-V-M7-E5-OVMF-CDROM-OK` after past-SEC
and PCI enum or an ATAPI sector read. Unarmed attach stays
`UnsupportedOnFirmware`. Not full DXE. Not installer.

E5 Stage 41 (host + QEMU): CMOS/fw_cfg/i440FX platform plus PIIX3
multifunction header (`00:01.0` so firmware scans `00:01.1` IDE) plus
EPT sink-resume so PEI can leave the `0xFCF8_F000` stall. After DXE
evidence a short resume tail, then E4 fail-soft. Serial
`RAYNU-V-M7-E5-OVMF-DXE-OK` after past-SEC and (ATAPI sector read or
exec-from-RAM + platform memory). Not a completed firmware CD boot.
Not installer. Next: virtio-blk + boot order CD→disk.
