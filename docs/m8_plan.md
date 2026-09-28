# M8 Plan — operator product hardening (post-Everest)

**Status:** **OPEN — `8ad2ac89` lived both SPA paths. Choice 2 reinstalled the 8 GiB window (`d43dbf07-…`). Choice 1 then reached `localhost login:` from that UUID with `setup-disk` withheld. The guest disk is still that window.** (2026-09-28). **Read [m8_state.md](m8_state.md) first.** Disk map: [r640_perc_lab.md](runbooks/r640_perc_lab.md). Everest **CLOSED** (`f72b4276`). M8.0 persist **repeated** and A4s **lived** on `1f33eeda72f9`. A6 **lived once** (`abc` reached Alpine, `RAYNU-V-M8-CONSOLE-OK` in Activity, Host green). Ubuntu 26.04 is the standing OS on PERC VD **UBUNTU0** (~400 GB). **RAYNU-SPARE**'s first 8 GiB holds the image that reached `localhost login:` on `bda7a59b`, including after a chassis restart. That window is the guest disk. It is not the 2.9 TB VD. Do not format UBUNTU0. A5 is parked. **M8.7 identify lived** on `2dd2b412`: Mini `18:00.0` `1028:1fcd`, H840 `3b:00.0` not mapped, BAR `0x9d800000`, classic `outbound_msg_0` `raw=0` `allow=0`. That zero is the xscale register. **Fusion status lived** on `22ce3728`: scratch pad 0 `0xb73c0fed` is READY, max commands 4077, 128 reply queues, RDPQ, `mapped=1`. The `allow=0` on that line was the max-command bits, not a reset request. Iron `7577f934` posted IOC init and the LD list from frame-pool page `0x1000000`: `perc ioc status=0x00`, `perc ld status=0x00`, `n=2`, target 0 UBUNTU0 `429496467456` bytes, target 1 RAYNU-SPARE `3169417691136` bytes, `pick=1`. Doorbell was only loaded (`db=0x40000000`). Iron `f0a7aabb` posted one READ(16) of LBA 0 and, only if that status is 0, one READ(16) of the last LBA, then one WRITE(16) of that last LBA and a readback, then offered the spare as read-only virtio at `00:04.0`. A non-zero status stops that stage. It does not store the doorbell. COM2 printed `boot: perc write status=0x00`, `boot: perc write rd lba=6190268927 match`, and `RAYNU-V-M8-PERC-WRITE-OK`. LBA 0 stayed 16 zeros. `skip PERC` stays. Host pack remains `RAYNU-V-M8-PERC-HOST-OK`. Rollback kit [`v0.1.0-m8-a4s`](../releases/v0.1.0-m8-a4s/). `3f80abd0` is a probe; do not flash it. Do not open Firefox on `3e9ce45e`. Phase 0 stays. M8.2 host-ready. M8.3 keyboard lived once. M8.4 host-ready. A3 SKU DONE. Known-good and do-not-F11 tables live in `m8_state.md`, not here. **not VNC**.
**Parent:** [ADR-018](adr/ADR-018.md) · Everest: [ADR-009](adr/ADR-009.md) · lived: [progress.md](progress.md) · HDA: [hda.md](hda.md)  
**Prior track:** [m7_plan.md](m7_plan.md) (closed). Cluster / elasticity is **M9**, not this plan.

M8 is the polish table that used to read as “Everest residual.” It is **not** a claim that the product loop is unfinished. COM2 already printed `HOST-NIC-HTTP-OK` → SPA Start of RayNu-F → `ISO-INSTALL-OK` → `DISK-BOOT-OK` → `login:`.

---

## Iron rollback

Three pins. GitHub Latest stays the Everest loop. The current PERC rollback is the lived SPA choice. The standing-SPA pin is the older path back to `1f33eeda`. Do **not** treat an untagged tip as any of these pins.

### PERC SPA choice (`v0.1.0-m8-perc-spa`)

| Field | Value |
|-------|-------|
| GitHub | https://github.com/vikkp/RayNu/releases/tag/v0.1.0-m8-perc-spa (not Latest) |
| EFI source | `8ad2ac89125ee055d978f4ac1161770b97da40a2` |
| CI | `--run 36362775808` · COM2 `build: sha=8ad2ac89125e` |
| EFI SHA256 | `6d5dc7cbe088182b6dcd7896717f1cb204ba2e3604f477404ef0b809349c6849` |
| In-tree kit | `releases/v0.1.0-m8-perc-spa/` |
| Lived | Choice 2 wrote `d43dbf07-…`. A later boot, choice 1, reached `localhost login:` with `setup-disk` withheld. `5175b3da` then lived Guests Start of that same UUID |

Flash the kit file to leave the Guests-screen work. Do not rebuild it and expect the same COM2 stamp.

### Standing SPA + this install (`v0.1.0-m8-a4s`)

| Field | Value |
|-------|-------|
| GitHub | https://github.com/vikkp/RayNu/releases/tag/v0.1.0-m8-a4s (not Latest) |
| EFI source | `1f33eeda72f99e432204dc943386d99d3d341be8` |
| CI | `--run 36137732145` · COM2 `build: sha=1f33eeda72f9` |
| EFI SHA256 | `5539d83806e120eb6c331c11281407c0f902b121a770c7faa46d6a1a26280693` |
| In-tree kit | `releases/v0.1.0-m8-a4s/` |
| Lived | Sixth `login:` UUID `a0ad99ac-…`, lease `.154`, one TCP accept, `HTTP keep-alive`, Host green |

Flash the kit file. Do not rebuild it and expect the same COM2 stamp.

### Everest loop (M8.0 known-good, GitHub Latest)

Flash this to return to the original install loop on leftover DRAM. Do **not** treat a later tip as the Everest close.

| Field | Value |
|-------|-------|
| GitHub | https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed (Latest) |
| Tag | `v0.1.0-everest-closed` → `f72b4276d198b5e90147e9be1037d0d0b7213a28` |
| CI | `--run 34552377351` · COM2 `build: sha=f72b4276d198` |
| EFI SHA256 | `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc` |
| In-tree kit | `releases/v0.1.0-everest-closed/` (PR #243, open) |
| Not this | 1.0/GA · TLS · leftover persist across HV reboot · F11 `34548550755` / `7f8dc0a9` |

Evidence: [`docs/evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md`](evidence/r640/2026-09-11-f72b4276-phase-b-spa-iso-install-disk-boot-ok.md). Cursor rule: [`.cursor/rules/iron-rollback.mdc`](../.cursor/rules/iron-rollback.mdc). Historical pre-native-NIC preserve (not this kit): `releases/v0.1.0-adr013-baseline`.

---

## Strategy

**Harden the single-host operator product before cluster features.**

- Do **not** reopen M7. Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.
- Do **not** pull TLS / persist / console into Proven Core without a new ADR (default **no**).
- Do **not** start vMotion / DRS / hot-add on this path (→ **M9**).
- Build **in order**. A later gate may be designed in parallel; it does not close before its predecessor without rewriting this plan. **M8.7** is the parallel Bar B track (host pack first). It does not close M8.4–M8.6.

```
M8.0  Persist install disk across HV reboot
      (file-backed nested / durable LUN iron / leftover DRAM fallback)
M8.1  TLS on coexist :8443
M8.2  Real operator auth (not bring-up token as product default)
M8.3  Guest console UI (web/VNC; serial already worked)
M8.4  ISO blob upload via SPA/REST
M8.5  UEFI catalog persist (SFS/NVMe write)
M8.6  Windows / multi-distro (ADR-014 later)
M8.7  PERC mailbox on RAYNU-SPARE (Fusion: status, IOC init, one last-LBA write lived)
      M8.4–M8.6 stay open. Bar B does not wait on them.

→ M9 sketch: vMotion-like · DRS-like · hot-add
```

---

## Close rule

Software gates: CI + host/QEMU smoke.  
Iron gates: real PowerEdge R640 COM2 (or documented HTTPS capture) — Latitude/QEMU insufficient when the claim is “survives a host reboot” or “browser TLS.”

HDA + `site/hda.html` stay fresh: update `docs/hda.md`, then `./tools/sync-hda-site.sh`.

---

## Gates

### M8.0 — Persist install disk across HV reboot

**Status: CLOSED on evidence** (`4af78b43` keep=1 + `DISK-BOOTX64` + `root=UUID=348005a9-…` after Force Off). Minted `RAYNU-V-M8-DISK-PERSIST-OK` not serial-printed. Backend **chosen**; host round-trip **exists**; **attach is persist-first**; nested File is QEMU **file-backed RAM**; iron USB 8 GiB slice persist is lived. Prototype EFI, not the Everest known-good.

**Goal:** The virtio install disk that `setup-disk` wrote still exists after the **hypervisor** reboots — not only after a **guest** F7 reset (ADR-017 `reset_keep_disk`). Leftover DRAM above PRECISE is the Everest attach; a RayNu-V reboot zeros that RAM.

**Why first:** Every other polish is wasted if the guest filesystem dies when RayNu-V restarts. Everest E5 did not require this.

**Backend choice (preferred — this is the real first commit):** virtio-blk HPA *is* a durable device. ADR-018 “NVMe/ESP-backed virtio,” not “hope DRAM is still there.” ADR-004 exclusive ownership: those HPAs are virtio-blk / BlockIo only — the guest never sees them as RAM.

| Role | Kind | What the HPA is | Survives HV reboot? |
|------|------|-----------------|---------------------|
| Nested QEMU | `File` | A QEMU disk file the hypervisor process re-opens | Yes (mechanism proof, **not** the iron close) |
| Iron | `DurableLun` | A USB partition or NVMe LUN that is **not** the R640 PERC (Ubuntu stays the standing boot order) | Yes — required for the iron marker |
| Fallback | `LeftoverDram` | Carved leftover above PRECISE (`carve_leftover_install_disk` / `take_leftover_install_disk`) when no persist media exists | **No.** Same as Everest. Guest F7 still keeps it (`reset_keep_disk`). |

**Rejected:** copy 1 GiB leftover DRAM ↔ ESP on every HV stop. Too slow, and this ESP is too small. ESP `installdisk.bin` is the 1 MiB LBA-stamp lab persist (M7.7), not the Alpine GPT/ESP/ext4 disk. The 4 GB UDisk already holds ~994 MiB alpine-extended; it cannot also hold a 1 GiB disk image next to the ISO.

**Markers:**

| Marker | Who prints it | Meaning |
|--------|---------------|---------|
| `RAYNU-V-M8-DISK-PERSIST-OK` | Real R640 COM2 only | Iron close: Force Off / reboot **RayNu-V** (not guest `reboot`), then the installed disk is back (`root=UUID=` + `\EFI\BOOT\BOOTX64.EFI`), `build: sha=` of the prototype |
| `RAYNU-V-M8-DISK-PERSIST-HOST-OK` | Host/CI | File-backed GPT+ESP+ext4 round-trip after an in-process “HV reboot.” Not nested. Not iron. |
| `RAYNU-V-M8-DISK-PERSIST-NESTED-OK` | Nested harness only (`tools/m8-persist-nested.sh`) | Kill/restart QEMU HV → second Linux without `setup-disk`. Not iron. Host cargo tests must never print this. |
| `RAYNU-V-M7-ISO-INSTALL-OK` | **Never** from host/CI/nested | Everest iron-only. Do not reuse. |

**Acceptance:**

1. **Host (closed):** round-trip a GPT+ESP+ext4 image through the `File` backend; after allocator reset / dropping the in-memory disk (and a real `std::fs` file surviving that drop), the restored virtio image still has `root=UUID=` and `\EFI\BOOT\BOOTX64.EFI`. Leftover-DRAM backend fails the same reboot. Print `RAYNU-V-M8-DISK-PERSIST-HOST-OK` only.
2. **Nested (CLOSED on `raynuvsrv1` `ce3d8a09`):** virtio attach prefers persist (`take_persist_install_disk`) then leftover DRAM then the pool. Empty persist uses `attach_disk` (zeros so the ISO wins). Installed persist uses `attach_disk_keep` / DurableLun `attach_lun` (`keep=1`). Nested QEMU `M8_PERSIST_IMG` backs initial RAM (`QEMU_MEM=3584M`, share=on) because distro `OVMF_CODE_4M.fd` ignores nvdimm and pc-dimm hotplug. Persist img size must equal `QEMU_MEM`. `MODE=full` on `raynuvsrv1`: boot1 leftover/File persist `hpa=0x20000000 bytes=536870912 keep=0` → Alpine `Installation is complete` → HV kill → persist file `EFI PART` → boot2 `keep=1` + `RAYNU-V-RAYNU-F-DISK-BOOT-OK` + no `setup-disk`. Harness printed `RAYNU-V-M8-DISK-PERSIST-NESTED-OK`. Evidence: [2026-09-13-ce3d8a09-m8-disk-persist-nested-ok.md](evidence/nested/2026-09-13-ce3d8a09-m8-disk-persist-nested-ok.md). Nested QEMU ≠ R640. Nested marker is harness-only (never host cargo, never iron, never `MODE=keep`/`lunkeep`/`usbkeep`). Mechanism proof, not the iron close.
3. **Iron (CLOSED on evidence `4af78b43`):** same Phase B loop as Everest, then **Force Off / reboot RayNu-V**. Lived COM2: peek `keep=1` → SPA `xhci diskprime` `image=DISK-BOOTX64` bytes=139264 → `DISK-BOOT-OK` → `root=UUID=348005a9-…` → `login: root`. Prototype `build: sha=4af78b4303fd`. Minted `RAYNU-V-M8-DISK-PERSIST-OK` **did not print** (const-only). One-time F11 the UDisk; Ubuntu-on-PERC stays the standing boot order. Do not format the PERC. DurableLun mapper: PCI census picks NVMe (class `01:08`) then USB ≥ 1 GiB; **refuses** PERC / MegaRAID and the ESP Cruzer (2–8 GiB window). **NVMe Identify + Read/Write** backs virtio when Identify succeeds (`MODE=lun`). **USB BOT/xHCI I/O** after ExitBootServices backs virtio when NVMe is absent (`MODE=usb` QEMU `qemu-xhci` + `usb-storage`). Intel PCH scratchpad ≤ 64 pages; USBLEGSUP OS owned; never `USBCMD.INTE`. COM2 always prints `xhci cap caplen=… scratch=…/64` on USB attempt. QEMU USB ≠ R640 xHCI. Iron `ac3b92cd` Cap `err=1` (16-page budget). Cap EFI `67db8a68`: `scratch=34/64`. Enum EFI `c6bdd671` / `53d1f7f7` (2026-09-14): HS U0 `sc=0xe03` then Address Device `cmd=3` `cmpl=0x11` on p10/p11/p14 (Slot Context DW1 Number of Ports was the port number). Slot DW1 EFI `3473a0b9`: Parameter Error gone; `cmpl=0x40` was MaxSlots after Address Device never posted (p11/p14 Enable Slot starved). Recover EFI `568e8696`: p10 `cmpl=0xff timeout`, p11 Toshiba `0480:a004` `usb I/O ready bytes=320072933376`, leftover skipped; peek `read-fail` and Alpine `vda`+`vdb` IOERR because ISO virtio used USB BOT. This EFI isolates ISO from LUN, BOT TUR/SENSE, 4Kn 512e. Do not `setup-disk` on leftover. Do not F11 until COM2 peek is honest and Alpine mounts `vdb`. Runbook: [m8_persist_iron.md](runbooks/m8_persist_iron.md).

**Not this gate:** TLS, VNC, Windows, Proven Core. Nested Alpine kill/restart is **closed on QEMU** (`RAYNU-V-M8-DISK-PERSIST-NESTED-OK`). Iron Force Off **CLOSED on evidence** (`4af78b43` keep=1 DISK-BOOT UUID); minted persist-OK not printed. DurableLun **NVMe I/O** is host-proven (`MODE=lun` QEMU Identify); **USB BOT** is TCG-proven (`MODE=usb` `usb I/O ready`). DurableLun `keep=1` after HV kill is **`MODE=lunkeep`/`usbkeep` TCG-proven** and now iron USB 8 GiB slice. If a persist prototype misbehaves, re-flash [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) (see [Iron rollback](#iron-rollback-m80-known-good)). HDA overall stays **99%** (TLS/console residual).

---

### M8.1 — TLS on the mgmt listen

**Status: CLOSED on iron** (`RAYNU-V-M8-TLS-OK`, EFI `928d6224`, 2026-09-19)

**Goal:** Coexist HTTP on BCM5720 (`10.99.99.x:8443`) becomes HTTPS. Plaintext remains a lab fallback. Size stays inside ADR-003.

**Acceptance:** Browser or `curl --cacert` on the operator LAN during the post-EBS native HTTPS window **before RayNu-F** (`PRE_RAYNUF_HTTPS_MS`). Lived: Mac `curl --tlsv1.2 --tls-max 1.2 --cacert` `--resolve raynu-v.lab` on `10.99.99.140:8443` returned SPA HTML; COM2 `boot: HOST-NIC HTTP exchange ok` then **`RAYNU-V-M8-TLS-OK`**. PRE-EBS SNP `http://` does not count. Evidence: [2026-09-19-928d6224-m8-tls-ok.md](evidence/r640/2026-09-19-928d6224-m8-tls-ok.md).

**Honesty:** ADR-009 already deferred TLS to close Everest. Host rustls (`RAYNU-V-M8-TLS-HOST-OK`) is not this close. rustls/ring cannot join `uefi-bin` (ring C needs `<assert.h>` on `x86_64-unknown-uefi`). CURL NOW on this EFI is `https://`. Lab millicert, TLS 1.2 ECDHE-RSA-AES128-GCM only. Closing M8.1 on COM2 does not rewrite Everest history. rustls is a **dev-dependency** (ADR-003); it is not in `uefi-bin`. Plaintext remains a lab fallback.

---

### M8.2 — Real operator auth

**Status: host-ready** (firmware still lab bring-up when no ESP `auth.token`; iron ESP-required default open)

**Goal:** Product default is not `raynu-v-bringup`. ESP `auth.token` (or successor) is the operator credential path.

**Honesty:** [`AuthMode::HostReady`](../mgmt/auth.rs) never accepts `raynu-v-bringup` (`RAYNU-V-M8-AUTH-HOST-OK`). Firmware HTTP still uses [`auth_allows`](../mgmt/api.rs) (`AuthMode::LabBringUp`) so iron REST does not 401 this flash. Host/CI never print `RAYNU-V-M8-AUTH-OK`. Nested QEMU ≠ R640.

**Depends on:** M8.1 is the natural pairing (TLS without auth is still a toy; auth without TLS leaks the token).

---

### M8.3 — Guest console UI

**Status: A4s lived; A6 lived once** (not VNC)

**Goal:** Operator types in the guest from the SPA (web/VNC or equivalent). iDRAC `console com2` remains the evidence channel, not the only keyboard.

**Honesty:** [`ConsoleMode::FirmwareSpaKeys`](../mgmt/console.rs) serves `POST /console/keys` into guest COM1 RBR. `GET /logs/guest` is the guest COM1 TX copy (not iDRAC SOL). Host tests still print `RAYNU-V-M8-CONSOLE-HOST-OK`. `GET /logs/serial` remains HV UART (not a guest console). SPA Activity has `g-keys` and polls `/logs/guest`. Standing HTTPS during RayNu-F is lived on `1f33eeda` (A4s). Millicert drops TLS 1.3 dummy CCS (RFC 8446 D.4) so Safari/Chrome ClientHello is not `ST_FAIL`. Host/CI never print `RAYNU-V-M8-CONSOLE-OK`. Nested QEMU ≠ R640. **not VNC**. Iron close is typing in Alpine from the SPA on BCM5720, then COM2. `maybe_print_iron_console_ok` prints `RAYNU-V-M8-CONSOLE-OK` with `write_line_nowait` so the line survives Linux earlycon share. That print lived once: Activity showed `abc`, `-sh: abc: not found`, and `RAYNU-V-M8-CONSOLE-OK`, Host green. Overview **Power off host** posts `/host/poweroff`. The 200 stays on the kept TLS session. After that reply drains, firmware does `VMXOFF` then `ResetSystem` SHUTDOWN. COM2 showed `boot: SPA host po` and iDRAC Power State went Off. A failed `VMXOFF` does not reset. On 2026-09-26 Ubuntu 26.04 is the standing OS. See [r640_perc_lab.md](runbooks/r640_perc_lab.md). **not VNC**.

**Honesty:** Serial auto-answer already installed Alpine on iron. This gate is UX, not E5.

---

### M8.4 — ISO blob upload via SPA

**Status: host-ready** (firmware ESP-staged `linux.iso` stays valid; iron network ISO PUT open)

**Goal:** Operator can PUT/POST an ISO through the network UI. ESP-staged `linux.iso` stays valid.

**Honesty:** [`UploadMode::HostReady`](../mgmt/iso_upload.rs) PUT/POST ISO bytes into a host datastore blob (`RAYNU-V-M8-ISO-UPLOAD-HOST-OK`). Firmware HTTP does not grow a coexist blob PUT. SPA has no upload widget (16 KiB). **ESP-staged stays valid**. Host/CI never print `RAYNU-V-M8-ISO-UPLOAD-OK`. Nested QEMU ≠ R640.

---

### M8.5 — UEFI catalog persist

**Status: open**

**Goal:** Replace `UnsupportedOnFirmware` with a real SFS/NVMe write of `EFI/RAYNU/images/catalog.txt`. Host `std::fs` path already closed M7.2.

---

### M8.6 — Windows / multi-distro (ADR-014)

**Status: open** (later)

**Goal:** Same UEFI+virtio product boot; typed `windows_iso` / additional Linux. Not a RayNu-F rewrite. Not WHQL.

---

### M8.7 — PERC mailbox on RAYNU-SPARE

**Status: Iron `bda7a59b` reached `localhost login:` from the 8 GiB spare image, then again after a chassis restart.** The second boot printed `boot: perc copy skip present alt=16777215` and `boot: perc virtio ro hidden`, launched `DISK-BOOTX64`, and printed `RAYNU-V-M8-PERC-BOOT-OK`. Linux saw `[vda] 8.00 GiB` and `[vdb]` and did not see `vdc`. `vda2` was clean (`6380/521216` files) and mounted `a0ad99ac`. The prompt was `localhost login: root`. The image was not rewritten. The Toshiba was still attached. The guest disk is that 8 GiB window. The rest of the 2.9 TB VD stays hidden. No doorbell. `8ad2ac89` then lived both buttons. Choice 2 reinstalled the window (`d43dbf07-…`). Choice 1 booted that UUID and withheld `setup-disk`. Leave that shell. Do not type `setup-alpine` there.

**Goal:** Post-EBS Fusion I/O for the H740P Mini (`1000:0016`, Linux Harpoon / Ventura), aimed at the empty ~2.9 TB VD. UBUNTU0 stays the boot disk. `pick_durable_lun` still prints `skip PERC`. Host marker `RAYNU-V-M8-PERC-HOST-OK`. The iron read marker lived on `9c37fcae`. The iron write marker lived on `f0a7aabb`. Host/CI never print either. A one-sector write is not a boot disk.

M8.4, M8.5, and M8.6 stay open. This milestone runs beside them because Bar B does not depend on ISO upload or Windows.

The H740P Mini is not a classic MFI doorbell device. Linux `megasas_set_adapter_type` marks `PCI_DEVICE_ID_LSI_HARPOON` as `VENTURA_SERIES` and uses `megasas_instance_template_fusion`. `megasas_read_fw_status_reg_fusion` reads `outbound_scratch_pad_0` (BAR + `0xB0`). The header comment about `outbound_msg_0` (BAR + `0x18`) is the xscale path. Iron `2dd2b412` loaded that xscale word and got `raw=0x00000000` `allow=0` `mse=0` at `18:00.0` BAR `0x9d800000`. That zero is not a reason to reset the adapter. The H840 at `3b:00.0` (`1028:1fc9`) was named and not mapped.

| Step | What | Close when |
|------|------|------------|
| M8.7 host | Pack `MR_DCMD_LD_GET_LIST` and one-block READ(16). Size fence. No BAR0. | `RAYNU-V-M8-PERC-HOST-OK` (landed) |
| M8.7 identify | One Dell H740P Mini (`1028:1fcd` or `1028:1fcf`). H840 BAR never mapped. | **Lived** `2dd2b412`: `cand` `18:00.0` `mini=1`, `3b:00.0` `mini=0`. |
| M8.7 status | Load scratch pads `0xB0`–`0xBC` on that Mini BAR. `allow` on the new line is the READY nibble. `post=0` until the IOC line. | **Lived** `22ce3728`: `s0=0xb73c0fed` `queues=128` `rdpq=1` `mapped=1`. Still `skip PERC`. |
| M8.7 ready | Iron is already READY. This image does not store the doorbell. OPERATIONAL, FAULT, or `mapped=0` does not post. The low 16 bits of scratch pad 0 are max commands, not a reset request. | Skip the doorbell store. Not OCR. |
| M8.7 IOC init | One `MFI_CMD_INIT` carrying MPI2 IOC INIT, posted as one 64-bit MFA store to the low inbound queue port (`0xC0`, not legacy `0x40`). One reply queue, RDPQ, poll mode, MSI-X off. DMA is eight frame-pool pages below 4 GiB. | **Lived** `7577f934`: `dma phys=0x1000000`, `perc ioc status=0x00`. |
| M8.7 list | One `MR_DCMD_LD_GET_LIST` on that same port, only after IOC status 0. | **Lived** `7577f934`: `n=2`, id 0 ubuntu `429496467456`, id 1 spare `3169417691136`, `pick=1`. |
| M8.7 read | READ(16) of LBA 0, then of the last LBA, each one 512-byte block, spare only. A non-zero status or a timeout stops the chain. | **Lived** `985495bef9fa`: `perc read status=0x00` sig 16 zeros, `perc read2 status=0x00` `lba=6190268927`. |
| M8.7 virtio | Read-only virtio-blk at `00:04.0` only after both statuses are 0. Install disk stays the Toshiba. `setup-disk` still withheld. | **Read marker lived** `9c37fcae`: `perc virtio rd lba=0 ok` and `RAYNU-V-M8-PERC-LUN-OK` on the `vdc` probe. Same boot `dd` of `/dev/vdc` returned 16 zero bytes. Read-only. `skip PERC` stays. |
| M8.7 write | One WRITE(16) of the spare last LBA, then READ(16) of that LBA. Payload is 16 bytes `RAYNU-SPARE-WR16` and 496 zeros. LBA 0 is not written. A non-zero status stops the write. Virtio stays read-only. Guest OUT stays rejected. | **Lived** `f0a7aabb`: `perc write status=0x00`, `perc write rd lba=6190268927 match`, `RAYNU-V-M8-PERC-WRITE-OK` via `write_line_nowait`. Host/CI never print it. |
| M8.7 image copy | Copy the 8 GiB Toshiba guest window onto RAYNU-SPARE with WRITE(16), eight sectors at a time, only after a USB peek shows `EFI PART`. Heartbeats are `boot: perc copy`. The mailbox probe LBA 0 is not written. Install-disk reads accept a byte range inside a sector (GPT entries are 128 bytes). A spare that already has a CRC-valid header and an `EFI PART` backup sector prints `boot: perc copy skip present` and is not rewritten. When that latch is set, `00:04.0` stays hidden so initramfs does not mount the read-only whole spare. UBUNTU0 is refused. After HTTPS is up, the firmware waits for `POST /perc/boot/installed` or `POST /perc/boot/reinstall`. Reinstall stages the ISO and may run `setup-disk` on that 8 GiB window only. Boot-as-is keeps the disk and withholds `setup-disk`. | **Both SPA paths lived** on `8ad2ac89`. Choice 2 wrote `d43dbf07-…`. Choice 1 then logged in from that UUID with `setup-disk` withheld. The guest disk is the 8 GiB window, not the 2.9 TB VD. Host/CI never print the marker. |

The fence accepts one LD of ~2.5–3.1 TiB and refuses ~300–512 GiB. Scratch pad 1's extended field (bits 21:14, plus one) is the Ventura reply-queue count. Bit `0x00800000` is RDPQ. Iron printed 128 queues and RDPQ. Those numbers size this IOC init (one queue, not 128). They are not a menu of registers to try next. If scratch pads 0–3 are all zero, COM2 prints `mapped=0` and `queues=na` and the IOC post is skipped. Adapter reset (`MFI_RESET_ADAPTER` to `inbound_msg_0`, diagnostic reset) is not an allowed command. The doorbell is not stored. Bus master is set on the Mini only when a descriptor is about to be posted. QEMU has no H740P. A nonzero IOC status or a 4-second poll timeout stops the image. A spare pick posts one READ(16) of LBA 0. If that status is 0, it posts one READ(16) of the last LBA. If that status is also 0, it arms read-only virtio at `00:04.0`. Ubuntu-sized targets do not. A nonzero status or a timeout stops that stage. After both reads return 0, one WRITE(16) of the last LBA is posted, then one READ(16) of that same LBA. LBA 0 is not written. There is no retry and no second target. A write miss still arms the read-only virtio device. Guest OUT on `00:04.0` stays rejected. Iron `0739edd0` ran the image copy: the 8 GiB Toshiba guest window, eight sectors per WRITE(16), after a USB peek showed `EFI PART`. COM2 printed `boot: perc copy` through `perc copy done` and `perc copy gpt ok`. The mailbox probe LBA 0 is not written. That copy is the Toshiba GPT at spare LBA 0. It is not `setup-disk`. UBUNTU0 is refused. The guest walk then failed `gpt_err=1` because partition entries are 128 bytes and the reader required a multiple of 512. This EFI copies a sector slice for those reads. If LBA 1 is a CRC-valid GPT header and the alternate LBA starts with `EFI PART`, COM2 prints `boot: perc copy skip present` and the USB rewrite does not run. A READ timeout prints `boot: perc copy present skip status` and does not start the rewrite. A failed copy still arms read-only virtio and the guest still boots the Toshiba. `RAYNU-V-M8-PERC-BOOT-OK` prints when the guest starts from the copied image. Host/CI never print it. Iron `40ec12fc` printed it at GRUB StartImage, then initramfs mounted read-only `vdc2`. Iron `bda7a59b` hides `00:04.0` and reached `localhost login:` from `vda2`. The whole VD stays hidden.

---

## M9 sketch (out of scope)

| Theme | Intent |
|-------|--------|
| vMotion-like | Live migrate running VM between hosts (builds on M6.3 proofs) |
| DRS-like | Placement / load-aware scheduling across hosts |
| Hot-add | CPU / RAM / disk add to running guest |

Do not pull M9 into M8 gate lists.

---

## First action

**M8.0 persist-first attach.** Choice remains **file-backed nested / durable LUN on iron / leftover DRAM as fallback**. Attach prefers persist (`attach_disk_keep` / `attach_lun` when the media looks installed). Nested `M8_PERSIST_IMG` backs QEMU RAM (distro OVMF ignores nvdimm/pc-dimm), default off. **NVMe I/O** (`MODE=lun`) Identify + Read/Write backs virtio when Identify succeeds; empty NVMe is not 1 GiB-zeroed. **USB BOT** (`MODE=usb`) is TCG-proven. **`MODE=lunkeep` TCG-proven:** plant GPT+ESP+ext4 at NVMe LUN offset 0, kill HV, second boot `keep=1`. NVMe I/O is post-EBS. **`MODE=usbkeep` TCG-proven:** same on qemu-xhci + usb-storage (header CRC + first ESP + FAT BPB + ext4; not 32 BOT array CRC). Iron marker is `RAYNU-V-M8-DISK-PERSIST-OK` (in-tree `uefi-bin` serial on keep=1 DurableLun DISK-BOOT; not on flashed `4af78b43`). Host marker is `RAYNU-V-M8-DISK-PERSIST-HOST-OK`. Keep ADR-004 exclusive ownership. Do not claim persist from nested QEMU alone. Do not copy 1 GiB through the Cruzer ESP. Do not format the PERC. Keep [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) as the flash rollback. A2 evidence close does not retire that pin.

**Next:** Leave the live `localhost:~#` on `d43dbf07-…`. Do not type `setup-alpine` there. Both SPA paths lived on `8ad2ac89`. A guest reboot while the reinstall choice was still set returned to the ISO; the following HV boot, with **Boot installed Alpine**, printed `DISK-BOOTX64` and withheld `setup-disk`. UBUNTU0 is not a target. The rest of the 2.9 TB VD stays hidden. LBA 0 is not written by the mailbox probe. Do not doorbell. Do not OCR. Census still prints `skip PERC`. Ubuntu 26.04 stays the standing OS on UBUNTU0. Do not format UBUNTU0. Do not flash the Toshiba. A6 lived once. **not VNC**. Nested QEMU ≠ R640.
