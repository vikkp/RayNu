# M8 Plan — operator product hardening (post-Everest)

**Status:** **OPEN** — M7 Mount Everest **CLOSED on iron** (`f72b4276` / `34552377351`, 2026-09-11).  
**Parent:** [ADR-018](adr/ADR-018.md) · queues: [ADR-020](adr/ADR-020.md) · fleet disks: [ADR-019](adr/ADR-019.md) · Everest: [ADR-009](adr/ADR-009.md) · lived: [progress.md](progress.md) · HDA: [hda.md](hda.md)  
**Sibling:** [gen1_plan.md](gen1_plan.md) (RayNu VM Generation 1 — unmodified media / firmware split).  
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

This plan is the **operator** queue (where disks and the operator live). RayNu VM Generation 1 (what the guest is allowed to be) is [gen1_plan.md](gen1_plan.md). [ADR-020](adr/ADR-020.md) serializes both. Cursor rule: [`.cursor/rules/m8-gen1-queues.mdc`](../.cursor/rules/m8-gen1-queues.mdc).

- Do **not** reopen M7. Host/CI never print `RAYNU-V-M7-ISO-INSTALL-OK`.
- Do **not** pull TLS / persist / console / RayNu-F into Proven Core without a new ADR (default **no**).
- Do **not** start vMotion / DRS / hot-add on this path (→ **M9**).
- Do **not** delete the Alpine ISO patcher / auto-answer before M8.0-mech COM2 (that is Gen-1 Phase 2).
- Do **not** treat Windows (M8.6) as a peer of TLS — it is Gen-1 Phase 5, after the Generation 1 freeze.
- Build **in order**. A later gate may be designed in parallel; it does not close before its predecessor without rewriting this plan **and** ADR-020.

```
M8.0-mech  Persist across HV reboot on NVMe or USB (lab mechanism)  ← NOW
M8.0-perc  PERC RAID virtual disk as DurableLun (fleet SKU, ADR-019)
           [Gen-1 Phase 1 design may overlap: split guest_uefi.rs]
M8.1  TLS on coexist :8443          (do not close before M8.0-mech)
M8.2  Real operator auth
M8.3  Guest console UI (web/VNC; serial already worked)
M8.4  ISO blob upload via SPA/REST
M8.5  UEFI catalog persist (SFS/NVMe write)
M8.6  Windows / multi-distro (ADR-014) = Gen-1 Phase 5, after freeze

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

**Status: open** — backend **chosen**; host round-trip **exists**; **attach is persist-first** (`attach_disk_keep` when GPT ESP + BOOTX64 + ext4); nested File is QEMU **file-backed RAM** (distro OVMF ignores nvdimm/pc-dimm hotplug); Alpine kill/restart and iron COM2 are **not** this slice. Prototype, not the known-good.

**Goal:** The virtio install disk that `setup-disk` wrote still exists after the **hypervisor** reboots — not only after a **guest** F7 reset (ADR-017 `reset_keep_disk`). Leftover DRAM above PRECISE is the Everest attach; a RayNu-V reboot zeros that RAM.

**Why first:** Every other polish is wasted if the guest filesystem dies when RayNu-V restarts. Everest E5 did not require this.

**Backend choice (preferred — this is the real first commit):** virtio-blk HPA *is* a durable device. ADR-018 “NVMe/ESP-backed virtio,” not “hope DRAM is still there.” ADR-004 exclusive ownership: those HPAs are virtio-blk / BlockIo only — the guest never sees them as RAM.

| Role | Kind | What the HPA is | Survives HV reboot? |
|------|------|-----------------|---------------------|
| Nested QEMU | `File` | A QEMU disk file the hypervisor process re-opens | Yes (mechanism proof, **not** the iron close) |
| Iron lab | `DurableLun` USB/NVMe | USB ≥ 1 GiB or NVMe `01:08` — **not** the ESP Cruzer, **not** Ubuntu-on-PERC | Yes — **M8.0-mech** (`RAYNU-V-M8-DISK-PERSIST-OK`) |
| Iron fleet | `DurableLun` PERC VD | A MegaRAID **virtual disk** that is not the lab Ubuntu standing boot ([ADR-019](adr/ADR-019.md)) | Yes — **M8.0-perc** (`RAYNU-V-M8-PERC-LUN-OK`). USB persist does not close this. |
| Fallback | `LeftoverDram` | Carved leftover above PRECISE (`carve_leftover_install_disk` / `take_leftover_install_disk`) when no persist media exists | **No.** Same as Everest. Guest F7 still keeps it (`reset_keep_disk`). |

**Rejected:** copy 1 GiB leftover DRAM ↔ ESP on every HV stop. Too slow, and this ESP is too small. ESP `installdisk.bin` is the 1 MiB LBA-stamp lab persist (M7.7), not the Alpine GPT/ESP/ext4 disk. The 4 GB UDisk already holds ~994 MiB alpine-extended; it cannot also hold a 1 GiB disk image next to the ISO.

**Markers:**

| Marker | Who prints it | Meaning |
|--------|---------------|---------|
| `RAYNU-V-M8-DISK-PERSIST-OK` | Real R640 COM2 only | **M8.0-mech:** Force Off / reboot **RayNu-V**, installed disk back on **NVMe or USB** (`root=UUID=` + `\EFI\BOOT\BOOTX64.EFI`). Not PERC. Not the fleet SKU. |
| `RAYNU-V-M8-PERC-LUN-OK` | Real R640 COM2 only | **M8.0-perc (ADR-019):** same Force Off loop with virtio-blk on a **PERC virtual disk** that is not Ubuntu. Fleet SKU. Host/CI/nested never print this. |
| `RAYNU-V-M8-DISK-PERSIST-HOST-OK` | Host/CI | File-backed GPT+ESP+ext4 round-trip after an in-process “HV reboot.” Not nested. Not iron. |
| `RAYNU-V-M8-DISK-PERSIST-NESTED-OK` | Nested harness only (`tools/m8-persist-nested.sh`) | Kill/restart QEMU HV → second Linux without `setup-disk`. Not iron. Host cargo tests must never print this. |
| `RAYNU-V-M7-ISO-INSTALL-OK` | **Never** from host/CI/nested | Everest iron-only. Do not reuse. |

**Acceptance:**

1. **Host (closed):** round-trip a GPT+ESP+ext4 image through the `File` backend; after allocator reset / dropping the in-memory disk (and a real `std::fs` file surviving that drop), the restored virtio image still has `root=UUID=` and `\EFI\BOOT\BOOTX64.EFI`. Leftover-DRAM backend fails the same reboot. Print `RAYNU-V-M8-DISK-PERSIST-HOST-OK` only.
2. **Nested (CLOSED on `raynuvsrv1` `ce3d8a09`):** virtio attach prefers persist (`take_persist_install_disk`) then leftover DRAM then the pool. Empty persist uses `attach_disk` (zeros so the ISO wins). Installed persist uses `attach_disk_keep` / DurableLun `attach_lun` (`keep=1`). Nested QEMU `M8_PERSIST_IMG` backs initial RAM (`QEMU_MEM=3584M`, share=on) because distro `OVMF_CODE_4M.fd` ignores nvdimm and pc-dimm hotplug. Persist img size must equal `QEMU_MEM`. `MODE=full` on `raynuvsrv1`: boot1 leftover/File persist `hpa=0x20000000 bytes=536870912 keep=0` → Alpine `Installation is complete` → HV kill → persist file `EFI PART` → boot2 `keep=1` + `RAYNU-V-RAYNU-F-DISK-BOOT-OK` + no `setup-disk`. Harness printed `RAYNU-V-M8-DISK-PERSIST-NESTED-OK`. Evidence: [2026-09-13-ce3d8a09-m8-disk-persist-nested-ok.md](evidence/nested/2026-09-13-ce3d8a09-m8-disk-persist-nested-ok.md). Nested QEMU ≠ R640. Nested marker is harness-only (never host cargo, never iron, never `MODE=keep`/`lunkeep`/`usbkeep`). Mechanism proof, not the iron close.
3. **Iron M8.0-mech (USB/NVMe, still open):** same Phase B loop as Everest (`HOST-NIC-HTTP-OK` → SPA Start → install → F7 `DISK-BOOT-OK`), then **Force Off / reboot RayNu-V**. COM2 shows the installed disk again, prototype `build: sha=`, and `RAYNU-V-M8-DISK-PERSIST-OK`. One-time F11 the UDisk; Ubuntu-on-PERC stays the standing boot order. Do not format the PERC. DurableLun mapper **today** picks NVMe (class `01:08`) then USB ≥ 1 GiB; **refuses** PERC / MegaRAID and the ESP Cruzer (2–8 GiB window) — lab safety, not product policy ([ADR-019](adr/ADR-019.md)). **NVMe Identify + Read/Write** backs virtio when Identify succeeds (`MODE=lun`). **USB BOT/xHCI I/O** after ExitBootServices backs virtio when NVMe is absent. Intel PCH scratchpad ≤ 16 pages; USBLEGSUP OS owned; never `USBCMD.INTE`. QEMU USB ≠ R640 xHCI. Do not F11 until COM2 shows `I/O ready` + `virtio-blk … (durable LUN …)`. Runbook: [m8_persist_iron.md](runbooks/m8_persist_iron.md).
4. **Iron M8.0-perc (fleet, after mech):** post-EBS MegaRAID Read/Write of a **dedicated VD** (not Ubuntu) backing virtio. Marker `RAYNU-V-M8-PERC-LUN-OK`. Census `skip PERC` is not this close. PRE-EBS UEFI RAID BlockIo is not this close.

**Not this gate (M8.0-mech):** TLS, VNC, Windows, Proven Core, **PERC I/O**. Nested Alpine kill/restart is **closed on QEMU**. Iron USB/NVMe Force Off remains. If a persist prototype misbehaves, re-flash [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed). HDA overall stays **99%**. USB persist does not close the fleet SKU.

---

### M8.1 — TLS on the mgmt listen

**Status: open** (after M8.0, or overlapping design only)

**Goal:** Coexist HTTP on BCM5720 (`10.99.99.x:8443`) becomes HTTPS. Plaintext remains a lab fallback. Size stays inside ADR-003.

**Acceptance (draft):** Browser or `curl --cacert` (or documented equivalent) on the operator LAN after `BOOT-OK`. PRE-EBS SNP does not count.

**Honesty:** ADR-009 already deferred TLS to close Everest. Closing M8.1 does not rewrite that history.

---

### M8.2 — Real operator auth

**Status: open**

**Goal:** Product default is not `raynu-v-bringup`. ESP `auth.token` (or successor) is the operator credential path.

**Depends on:** M8.1 is the natural pairing (TLS without auth is still a toy; auth without TLS leaks the token).

---

### M8.3 — Guest console UI

**Status: open**

**Goal:** Operator types in the guest from the SPA (web/VNC or equivalent). iDRAC `console com2` remains the evidence channel, not the only keyboard.

**Honesty:** Serial auto-answer already installed Alpine on iron. This gate is UX, not E5.

---

### M8.4 — ISO blob upload via SPA

**Status: open**

**Goal:** Operator can PUT/POST an ISO through the network UI. ESP-staged `linux.iso` stays valid.

---

### M8.5 — UEFI catalog persist

**Status: open**

**Goal:** Replace `UnsupportedOnFirmware` with a real SFS/NVMe write of `EFI/RAYNU/images/catalog.txt`. Host `std::fs` path already closed M7.2.

---

### M8.6 — Windows / multi-distro (ADR-014 / ADR-020 Phase 5)

**Status: open** (later — **after** Gen-1 Phase 4 freeze)

**Goal:** Same UEFI+virtio product boot; typed `windows_iso` / additional Linux. Not a RayNu-F rewrite. Not WHQL. **Not a peer of M8.1 TLS.** Unmodified media is Gen-1; do not invent a Windows ISO patcher.

**Depends on:** [gen1_plan.md](gen1_plan.md) Phase 4 (`docs/raynu_vm_gen1.md` freeze). That freeze does not exist today.

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

**M8.0 persist-first attach.** Choice remains **file-backed nested / durable LUN on iron / leftover DRAM as fallback**. Attach prefers persist (`attach_disk_keep` / `attach_lun` when the media looks installed). Nested `M8_PERSIST_IMG` backs QEMU RAM (distro OVMF ignores nvdimm/pc-dimm), default off. **NVMe I/O** (`MODE=lun`) Identify + Read/Write backs virtio when Identify succeeds; empty NVMe is not 1 GiB-zeroed. **USB BOT** (`MODE=usb`) is TCG-proven. Iron **M8.0-mech** marker is `RAYNU-V-M8-DISK-PERSIST-OK`. Fleet **M8.0-perc** marker is `RAYNU-V-M8-PERC-LUN-OK` ([ADR-019](adr/ADR-019.md)). Host marker is `RAYNU-V-M8-DISK-PERSIST-HOST-OK`. Keep ADR-004 exclusive ownership. Do not claim persist from nested QEMU alone. Do not copy 1 GiB through the Cruzer ESP. Do not format the Ubuntu PERC. USB persist does not close the fleet SKU. Keep [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed) as the flash rollback until iron COM2 prints the persist marker.

**Next:** (1) iron M8.0-mech on USB/NVMe — [runbook](runbooks/m8_persist_iron.md). Do not F11 until COM2 `I/O ready` + virtio on the LUN. Keep patched Alpine + auto-answer. (2) M8.0-perc MegaRAID VD persist ([ADR-019](adr/ADR-019.md)) — fleet SKU; spare VD only; do not format Ubuntu. Do not start M8.1 TLS until M8.0-mech COM2 exists (design-only overlap is allowed). Do not delete the ISO patcher (Gen-1 Phase 2) before (1). PERC driver / `guest_uefi.rs` split design may overlap; they do not close before mech without rewriting this plan **and** [ADR-020](adr/ADR-020.md).
