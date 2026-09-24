# M8 Plan — operator product hardening (post-Everest)

**Status:** **OPEN — M8 recovery** (2026-09-24). **Read [m8_state.md](m8_state.md) first.** Everest **CLOSED** (`f72b4276`). M8.0 persist **CLOSED on evidence** and marker printed (`928d6224`, also M8.1 `RAYNU-V-M8-TLS-OK`). `e5cca2e0` soak printed `RAYNU-V-USBSOAK-DONE` (239/239). `c4a41a17` reached installed GRUB and stopped at `grub>`; the one-shot line was the ESP BPB. This EFI prints `grubcfg=yes` or `grubcfg=no` on that walk. Phase 0 (never ISO after `EFI PART`, `setup-disk` withheld) stays. M8.2 host-ready. M8.3 standing SPA / console keys firmware-in-tree, iron OPEN. M8.4 host-ready. A3 SKU DONE. Known-good and do-not-F11 tables live in `m8_state.md`, not here.
**Parent:** [ADR-018](adr/ADR-018.md) · Everest: [ADR-009](adr/ADR-009.md) · lived: [progress.md](progress.md) · HDA: [hda.md](hda.md)  
**Prior track:** [m7_plan.md](m7_plan.md) (closed). Cluster / elasticity is **M9**, not this plan.

M8 is the polish table that used to read as “Everest residual.” It is **not** a claim that the product loop is unfinished. COM2 already printed `HOST-NIC-HTTP-OK` → SPA Start of RayNu-F → `ISO-INSTALL-OK` → `DISK-BOOT-OK` → `login:`.

---

## Iron rollback (M8.0 known-good)

Flash this if an M8 persist prototype misbehaves. Do **not** treat a later tip as the Everest close.

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
- Build **in order**. A later gate may be designed in parallel; it does not close before its predecessor without rewriting this plan.

```
M8.0  Persist install disk across HV reboot
      (file-backed nested / durable LUN iron / leftover DRAM fallback)
M8.1  TLS on coexist :8443
M8.2  Real operator auth (not bring-up token as product default)
M8.3  Guest console UI (web/VNC; serial already worked)
M8.4  ISO blob upload via SPA/REST
M8.5  UEFI catalog persist (SFS/NVMe write)
M8.6  Windows / multi-distro (ADR-014 later)

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

**Status: firmware SPA keys in-tree** (iron SPA keyboard / VNC open)

**Goal:** Operator types in the guest from the SPA (web/VNC or equivalent). iDRAC `console com2` remains the evidence channel, not the only keyboard.

**Honesty:** [`ConsoleMode::FirmwareSpaKeys`](../mgmt/console.rs) serves `POST /console/keys` into guest COM1 RBR. `GET /logs/guest` is the guest COM1 TX copy (not iDRAC SOL). Host tests still print `RAYNU-V-M8-CONSOLE-HOST-OK`. `GET /logs/serial` remains HV UART (not a guest console). SPA Activity has `g-keys` and polls `/logs/guest`. Standing HTTPS during RayNu-F is firmware-in-tree (A4s). Millicert drops TLS 1.3 dummy CCS (RFC 8446 D.4) so Safari/Chrome ClientHello is not `ST_FAIL`; idle abort is 15 s; handshake fail re-listens. Host/CI never print `RAYNU-V-M8-CONSOLE-OK`. Nested QEMU ≠ R640. **not VNC**. Iron close is typing in Alpine from the SPA on BCM5720, then COM2.

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

**Next:** **M8.3 console iron.** Firmware SPA `POST /console/keys` is in-tree. A4 TLS is **CLOSED on COM2** (`928d6224` `RAYNU-V-M8-TLS-OK`). Persist-OK also printed on keep=1 DISK-BOOT. Iron close is typing in the guest from the SPA (`RAYNU-V-M8-CONSOLE-OK`). Not VNC. Sit at `localhost:~#`. Do not `setup-disk`. Do not flash Toshiba `/dev/sdc`. Nested QEMU ≠ R640.
