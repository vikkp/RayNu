# M7 Plan — Mount Everest (shippable single-host)

**Status:** **M7.5 + M7.6 + M7.7 stamp-persist + M7.8 / E3b + ADR-013 Stage 1 (Phases 0–G) + E4 SPA VMLAUNCH (P0-14) + E5 Stage 0–45 closed**. Stage 44 / P0-59 ATAPI closed on iron COM2 `bf696ca`. Stage 45 / P0-61 El Torito **CLOSED** on iron COM2 `0be7283` (`OVMF-ELTORITO-OK` `RN-ELT` n=197992). P0-60 G1 EPT **CLOSED** on iron COM2 after `5147222`. G0 VMCS relocate **CLOSED** (`E4 G0 VMCS relocated HPA=0x10a00000`; `M4-NVM-OK`). M4.3 virtio-blk host-slab **CLOSED** on iron COM2 after `22e28d0` (`M4-BLK-OK` `guest_code=0x10c00000`; then `M4-NET-OK` / `M4-SMP-OK` / `R640-BOOT-OK` / Phase F coexist). Phase G is the accepted-risk note (shared LOM). **P0-15**–**P0-61** are closed. Residual: Stage 46 `ISO-INSTALL-OK`, plus TLS/console. Optional: `VMRESUME` instead of VMLAUNCH-every-quantum.  
**Prior:** M7.4 closed on Latitude (`RAYNU-V-M7-UI-OK`); M7.3–M7.0 closed; M6 closed.  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M7 row) · ADR: [adr/ADR-009.md](adr/ADR-009.md) · E3 listen: [adr/ADR-012.md](adr/ADR-012.md) · E3b: [adr/ADR-013.md](adr/ADR-013.md) · ISO types: [adr/ADR-014.md](adr/ADR-014.md) · HDA: [hda.md](hda.md) · lived: [progress.md](progress.md)  
**Prior track:** [m6_plan.md](m6_plan.md)

**Mount Everest (product loop):**  
Ship EFI → boot via **iDRAC virtual media** on **real R640** → **network Web UI** → **deploy Linux ISO / install guest**.

M6 closed the production-ready *bar* (proof + ops harden + soak + external audit) on Latitude/QEMU.  
**M7** delivers the shippable **single-host** operator product. Cluster features (vMotion-like, DRS-like, hot-add) are **M8** — not M7 blockers.

---

## Strategy (accepted)

**Build software readiness before iron; close M7 only after real R640 boot; single-host first.**

- Do **not** claim M7 closed without `RAYNU-V-R640-BOOT-OK` (or equiv.) on real PowerEdge R640.
- Do **not** block M7 on Dell Tier‑2 OEM Redfish (ADR-005) — slip-ok.
- Do **not** pull HTTP/datastore/ISO into Proven Core without a new ADR (default **no**).
- Do **not** start product vMotion / DRS / hot-add on the M7 critical path (→ M8).
- Pre-iron order: **Ship kit → TLS/HTTP → datastore/ISO** (R640 racked ~1 month after plan open).

```
Track Ship:   M7.0 release kit + USB/iDRAC runbook
Track Net:    M7.1 TLS/HTTP codec + host TCP (closed); **M7.6 UEFI NIC listen (ADR-012)**
Track Store:  M7.2 datastore (images + ISOs)
Track ISO:    M7.3 ISO register + CD-ROM or extract-boot + virtio disk
Track UI:     M7.4 create-VM + media attach + basic console/log
Track Iron:   M7.5 real R640 boot (hard gate for M7 closed)

→ M7 closed when SHIP + HTTP + STORE + ISO + UI + R640-BOOT green
   (+ M7.6 UEFI listen required for honest E3 / network UI on iron)
         ║
         ╚══ M8 sketch: vMotion-like · DRS-like · hot-add
```

---

## Close rule

Do **not** claim a gate closed in docs/site until:

- Software gates: CI + documented host/QEMU smoke green (same culture as M6).
- **`RAYNU-V-R640-BOOT-OK`:** real PowerEdge R640 evidence only (Latitude/QEMU insufficient).

HDA + `site/hda.html` must stay fresh: update `docs/hda.md`, then `./tools/sync-hda-site.sh`.

---

## Gates

### M7.0 — EFI release kit — `RAYNU-V-M7-SHIP-OK`

**Status: closed** (Latitude `./tools/m7-ship-smoke.sh` → `RAYNU-V-M7-SHIP-OK`)

**Goal:** Ops-trustable ship artifact — not just `cargo build`.

**Deliverables:**

1. Versioned `r640-hypervisor.efi` packaging (tag or version stamp) + SHA256.
2. Size gate in release path (`tools/check-size.sh` / CI).
3. One-page USB + iDRAC virtual media runbook (`docs/runbooks/`).
4. Host gate + smoke → `RAYNU-V-M7-SHIP-OK`.
5. `GAP(CLOSED M7.0): EFI release kit`.

**Shipped (host):**

1. `tools/package-release.sh` → `dist/raynu-v-<version>/` + `.tar.gz` + SHA256 sidecars.
2. `mgmt/ship.rs` + `mgmt/m7_ship_gate.rs` + `tools/m7-ship-smoke.sh` + CI `m7-ship` (+ package step on `build-uefi`).
3. Runbook [`docs/runbooks/usb_idrac.md`](runbooks/usb_idrac.md).
4. `GAP(CLOSED M7.0): EFI release kit`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M7-SHIP-OK` (packaged `dist/raynu-v-0.1.0/`).

---

### M7.1 — Network TLS/HTTP mgmt plane — `RAYNU-V-M7-HTTP-OK`

**Status: closed** (Latitude `./tools/m7-http-smoke.sh` → `RAYNU-V-M7-HTTP-OK`)

**Goal:** Browser on operator LAN reaches SPA + REST (not in-process dispatch only).

**Deliverables:**

1. Minimal TLS (or HTTP) listener in-binary (size-boxed; ADR-003) serving embedded Web UI + REST.
2. Auth beyond bring-up toy token (reuse/extend M6.4 patterns).
3. QEMU/lab proof of reachability from a second host or user-net forward.
4. Host gate + smoke → `RAYNU-V-M7-HTTP-OK`.
5. `GAP(CLOSED M7.1): Network HTTPS/HTTP mgmt`.

**Shipped (host):**

1. `mgmt/http.rs` — HTTP/1.1 codec (SPA `GET /` + REST + `Authorization: Bearer`).
2. `mgmt/http_listen.rs` — UEFI listen stub (`UnsupportedOnFirmware`) + host `TcpListener` proof.
3. Lab **plaintext HTTP** (TLS deferred — ADR-003/009); SPA sends Bearer token.
4. Runbook [`docs/runbooks/mgmt_http.md`](runbooks/mgmt_http.md) + QEMU `hostfwd` sketch.
5. Gate + `tools/m7-http-smoke.sh` + CI `m7-http`.
6. `GAP(CLOSED M7.1): Network HTTPS/HTTP mgmt`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M7-HTTP-OK` (host TCP SPA + Bearer REST). UEFI NIC listen residual → **[ADR-012](adr/ADR-012.md) / M7.6**.

---

### M7.6 — UEFI NIC HTTP listen — `RAYNU-V-M7-UEFI-HTTP-OK`

**Status: closed on iron** (2026-08-16) — SNP residual path (Tcp4 absent on Virtual Floppy)

**Evidence:** [`docs/evidence/r640/2026-08-16-uefi-http-ok.md`](../evidence/r640/2026-08-16-uefi-http-ok.md) · COM2 [`logs/2026-08-16-uefi-http-ok-com2.txt`](../evidence/r640/logs/2026-08-16-uefi-http-ok-com2.txt)

**Scaffold (host/CI):** `RAYNU-V-M7-UEFI-HTTP-SCAFFOLD-OK`  
**OK marker:** `RAYNU-V-M7-UEFI-HTTP-OK`  
**Parent Everest criterion:** E3 (network UI) — MVP closed via iron serial bind + OK marker

**Goal:** Laptop on the management LAN reaches the already-shipped SPA/REST while
`r640-hypervisor.efi` runs on R640 (in-binary UEFI Tcp4; plaintext HTTP; TLS deferred).

**Deliverables:**

1. Replace `listen_mgmt_http_uefi` → `UnsupportedOnFirmware` with Tcp4 bind (SNP residual only if Tcp4 absent). **(scaffold: PRE-EBS Tcp4 + soft-fail)**
2. Serial prints bound port (or `MGMT_HTTP_DEFAULT_PORT`).
3. Reuse `handle_http_request` codec; no second HTTP stack; no helper binary ([Z]).
4. Size gate green (ADR-003).
5. `GAP(CLOSED M7.6): UEFI NIC HTTP listen`.

**Acceptance:** **Met on iron** — `RAYNU-V-M7-UEFI-HTTP-OK` on R640 COM2 (SNP residual PRE-EBS; see evidence). HDA E3 MVP closed; firmware SNP is dead after EBS (do not chase Tcp4 or post-EBS SNP).

---

### M7.2 — Datastore / image library — `RAYNU-V-M7-STORE-OK`

**Status: closed** (Latitude `./tools/m7-store-smoke.sh` → `RAYNU-V-M7-STORE-OK`)

**Goal:** Somewhere to put ISOs, disks, templates (ESP/NVMe-backed).

**Deliverables:**

1. Datastore abstraction (register/list/delete images) — `mgmt/datastore.rs`.
2. Persistence on ESP-shaped path (`EFI/RAYNU/images/catalog.txt`); host `std::fs`; UEFI stub.
3. API shapes for UI — REST `/images` + HTTP route; Bearer auth.
4. Host gate + smoke → `RAYNU-V-M7-STORE-OK`.
5. `GAP(CLOSED M7.2): Datastore`.

**Acceptance (met):** Latitude smoke + gate → `RAYNU-V-M7-STORE-OK` (host catalog + REST). UEFI SimpleFileSystem persist remains stubbed; ISO blobs → M7.3.

---

### M7.3 — ISO deploy path — `RAYNU-V-M7-ISO-OK`

**Status: closed** (Latitude `./tools/m7-iso-smoke.sh` → `RAYNU-V-M7-ISO-OK`)

**Goal:** Operator registers a distro ISO → VM can boot installer (CD-ROM **or** documented kernel-extract) with virtio-blk install target.

**Deliverables:**

1. ISO register into datastore — `register_iso` / REST deploy.
2. Documented **kernel-extract** MVP (`mgmt/iso.rs`); CD-ROM stub honest.
3. Empty virtio-blk install target size in deploy plan (M4.3 surface).
4. Host package smoke → `RAYNU-V-M7-ISO-OK` (fast unit tests; not a QEMU installer run).
5. `GAP(CLOSED M7.3): Linux ISO deploy path`.

**Acceptance (met on Latitude host smoke):** marker + gate. **Residual:** El Torito/CD-ROM attach still stubbed; no full distro installer path on QEMU/iron yet. Product installer is **UEFI-first + typed ISO** ([ADR-014](adr/ADR-014.md)); kernel-extract is lab MVP only — do not hard-wire SPA install to bzImage.

---

### M7.4 — Ops Web UI MVP — `RAYNU-V-M7-UI-OK`

**Status: closed** (Latitude `./tools/m7-ui-smoke.sh` → `RAYNU-V-M7-UI-OK`)

**Goal:** vSphere-*like* enough for single-host install — not full parity.

**Deliverables:**

1. Create-VM (CPU/RAM/disk/ISO) over network UI — SPA form + `POST /vms/{id}/spec/...`.
2. Attach media via images/ISO deploy buttons; start/stop; **console deferred**.
3. Surfaces datastore + ISO from M7.2/M7.3.
4. Host package smoke → `RAYNU-V-M7-UI-OK` (fast unit tests).
5. `GAP(CLOSED M7.4): Network create-VM + ISO UI`.

**Acceptance (met on Latitude host smoke):** marker + gate. **Residual:** console/serial UI, TLS, firmware NIC listen, El Torito/CD-ROM.

---

### M7.5 — Real R640 boot — `RAYNU-V-R640-BOOT-OK`

**Status: closed** (iron — 2026-08-15 COM2; scaffold remains host/CI)

**Scaffold marker (host/CI):** `RAYNU-V-M7-R640-SCAFFOLD-OK` via `./tools/m7-r640-smoke.sh`  
**Iron marker:** `RAYNU-V-R640-BOOT-OK` — claimed in [`docs/evidence/r640/`](evidence/r640/) (`STATUS=closed`)

**Goal:** First light on real PowerEdge R640 via USB or iDRAC vMedia.

**Deliverables:**

1. Boot `r640-hypervisor.efi` on R640; COM1/iDRAC serial works. **Done.**
2. VMX + EPT + Linux shell path observed on iron. **Done** (`SHELL-OK` + M4 chain).
3. Runbook evidence + marker `RAYNU-V-R640-BOOT-OK`. **Done.**
4. `GAP(CLOSED M7.5): Real R640 boot` — **Done.**

**Evidence:** [`docs/evidence/r640/2026-08-15-r640-first-light.md`](evidence/r640/2026-08-15-r640-first-light.md) · kit `releases/v0.1.0-xsavesfix/`

**Acceptance:** **Real R640 only.** Host scaffold smoke must not print the iron marker.

---

### M7.7 — ISO install-to-disk (E5) — `RAYNU-V-M7-ISO-BOOTED-FROM-DISK`

**Status: closed on iron** (2026-08-16) — LBA stamp persist two-boot on Cruzer Micro.
Firmware printed `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` (documented equivalent of
`RAYNU-V-M7-ISO-INSTALL-OK`). Host/CI still never prints the iron OK marker.

**Scaffold marker (host/CI):** `RAYNU-V-M7-ISO-INSTALL-SCAFFOLD-OK` via `./tools/m7-iso-install-smoke.sh`  
**Close evidence:** [`docs/evidence/r640/2026-08-16-e5-iso-install.md`](evidence/r640/2026-08-16-e5-iso-install.md) · `STATUS-iso-install=closed`

**Goal (met as stamp contract):** Extract-boot → virtio-blk LBA write → ESP persist → reboot-to-disk detect.

**Honesty:** not a guest filesystem / distro ISO installer. El Torito residual. Product path: [ADR-014](adr/ADR-014.md) (UEFI + virtio; `linux_iso` | `windows_iso` | `generic_uefi`).

**Acceptance:** **Met on iron** — persist-detect + `prefix_into=67108864` + `BOOTED-FROM-DISK` + `R640-BOOT-OK`. Mount Everest stays open (TLS/console + real distro installer).

---

## Milestone acceptance

**Critical for M7 closed:**

```text
RAYNU-V-M7-SHIP-OK
RAYNU-V-M7-HTTP-OK
RAYNU-V-M7-STORE-OK
RAYNU-V-M7-ISO-OK
RAYNU-V-M7-UI-OK
RAYNU-V-R640-BOOT-OK
==> Mount Everest single-host product loop PASSED
```

**Optional / follow-on (not required for M7 closed):** live Tier‑1 Redfish health, R640 soak (P0-10), multi-distro ISO matrix, Secure Boot signing.

**M7 closed ⇒** operator can iDRAC-boot RayNu-V on R640 and install a Linux guest from the network Web UI on that host.

---

## M8 sketch (out of scope for this plan)

| Theme | Intent |
|-------|--------|
| vMotion-like | Live migrate running VM between hosts (product ops; builds on M6.3 proofs) |
| DRS-like | Placement / load-aware scheduling across hosts |
| Hot-add | CPU / RAM / disk add to running guest |

Do not pull M8 into M7 gate lists.

---

## First action

**M7.4 closed** on Latitude (`RAYNU-V-M7-UI-OK` — host package smoke).  
**M7.5 + M7.6 closed on iron** (`RAYNU-V-R640-BOOT-OK`, `RAYNU-V-M7-UEFI-HTTP-OK`).  
**M7.7 stamp-persist closed on iron** (`RAYNU-V-M7-ISO-BOOTED-FROM-DISK`, 2026-08-16).  
**M7.8 / E3b closed on iron** (`RAYNU-V-M7-HOST-NIC-HTTP-OK`, 2026-08-20) — native BCM5720 after `BOOT-OK` on `:38` / `10.99.99.144:8443`.  
**Honesty:** E3 (PRE-EBS) and **E3b** (lifetime HTTP on host-owned NIC) are closed. Firmware SNP and
Tcp4 stay dead after EBS. Keep `ape-nophylock=yes`. **P0-14 closed on iron**
(EFI `2b795a0`, 2026-08-21): spec **201** + start **200** on `10.99.99.126:8443`;
first SPA `VMLAUNCH` printed `RAYNU-V-M7-E4-SPA-LAUNCH-OK`; G0↔SPA clear-state
re-entry restored 98 VMCS fields each quantum. Guest is SHELL CPUID, not a
distro installer. Switches are `VMLAUNCH` after `VMCLEAR`, not `VMRESUME`.
HTTP during the switch loop is not in the close paste. **ADR-013 Phase G
closed 2026-08-21** as accepted-risk: host HTTP and guest virtio-net share LOM
`:38` (Appendix B). Not VLAN / second NIC. Iron `2b795a0` logged every
scheduler quantum on COM2 (E4 bring-up debug). Next EFI logs the first G0
re-entry, first SPA re-entry, first restore per slot, then one HINT and stays
quiet except HTTP/WARN/markers.

**First action (Stage 46 `ISO-INSTALL-OK` — Everest E5, OPEN):**
Sixty-third slice (this EFI): iron COM2 `virtio-blk install disk bytes=1048576`
because greedy 2 MiB report-RAM ate the `[1MiB,256MiB)` pool. Reserve the
install disk **before** report-RAM so 64 MiB (REST default) fits; leftover
2 MiB slots still back CMOS 2 GiB. Do not invent HPA. Nested stays 1 MiB.
Sixty-second slice (this EFI): iron COM2 `xAPIC MMIO decode fail
gpa=0xfee000f0 insn=` at `rip=0xfffcfc86` (OVMF flash). Identity peek
only covered 32 MiB RAM; copy instruction bytes from the private flash
HPA. Product ISO hold, not E4 SHELL. SSE MOVUPS still in this EFI.
Sixty-first slice (this EFI): guest-UEFI trampoline `movdqu` saves XMM0–15
into 16-byte-aligned `SAVED_XMM`; MMIO decode covers MOVUPS/MOVUPD/MOVSS/MOVSD
(`0F 10`/`11`), MOVDQU (`F3 0F 6F`/`7F`), MOVDQA (`66 0F 6F`/`7F`),
MOVAPS/MOVAPD (`0F 28`/`29`). Do not hide SSE2. Iron stick already has
`e3f56aa`; this is the next EFI after COM2 or CI.
Sixtieth slice (this EFI): iron `fsck.vfat` proved the 64 MiB FAT is
healthy (`131072` sectors, `26 files`, `17063/129022` clusters). Retry
without `--refat-cruzer` still fails (`free=57323008 need=67108864`).
Skip remount/fsck when FAT size < need. Whole-disk `mkfs.vfat -I`.
Fifty-ninth slice (this EFI): Cruzer FAT is a 64 MiB image (`131072`
sectors) on 977.5 MiB media, so alpine-virt `linux.iso` (63 MiB) cannot
fit with EFI+OVMF. `--refat-cruzer` copies `installdisk.bin`/`auth.token`
off, `mkfs.vfat -I -F 32 -n RAYNUV` on the identified stick, restores keep
files. Never PERC. Never `sda`/`sdb`.
Fifty-eighth slice (this EFI): Cruzer flash remounts and `fsck.vfat -a`
to reclaim stale FAT32 FSInfo / orphaned clusters after ENOSPC (`df`
showed 54 MiB free while `du` was 8.4 MiB on a 977.5 MiB stick). Never
`mkfs` / format. Keep `installdisk.bin` / `auth.token`.
Fifty-seventh slice (this EFI): Cruzer ESP flash prunes leftover/partial
`*.iso` then `df`-checks before staging alpine-virt `linux.iso` (~63 MiB
on the 977.5 MiB RAYNUV stick). Keep `installdisk.bin` / `auth.token`.
Fifty-sixth slice (this EFI): Alpine auto-answer waits for `/dev/vda`
(`mdev -s` each second, up to 5s) so a slow virtio probe is a block
device before `setup-disk` opens it. `mdev -s` then `sleep 1` left the
node missing if the driver bound during the sleep. Fifty-fifth slice (this EFI): Alpine auto-answer `modprobe isofs` and
`mount -t iso9660` so BusyBox mounts ISO9660 on virtio-blk `/dev/vdb` (and
ATAPI `/dev/sr0`) instead of probing a disk without `iso9660` in
`/proc/filesystems`. Fifty-fourth slice (this EFI): Alpine auto-answer `modprobe sr_mod` so `/dev/sr0`
exists when the live image booted from virtio-iso. Fifty-third slice (this EFI): Alpine auto-answer `mount /dev/vdb ... || mount /dev/sr0`
so apk still sees ISO9660 when virtio-iso is not ready. Fifty-second slice (this EFI): MMIO CMPS/SCAS (`A6`/`A7`/`AE`/`AF`, F3 REPE /
F2 REPNE) so memcmp/memchr of virtio/IOAPIC/xAPIC sets RFLAGS instead of
decode-fail spinning. RAM GPA miss does not invent HPA. Fifty-first slice (this EFI): Alpine auto-answer `sleep 1` after `mdev -s` so a
slow virtio probe is visible before `setup-disk` `find_disks`. Fiftieth slice (this EFI): Alpine auto-answer `modprobe virtio_pci` and
`mdev -s` before `setup-disk` so `find_disks` sees `/sys/block/vda` (otherwise
`No disks available` answers n and the installer exits). Forty-ninth slice (this EFI): Alpine auto-answer **overwrites** `/etc/apk/repositories`
with `/media/cdrom/apks` (does not append) so `apk update` does not hang on
network mirrors, and answers `sys` on `How would you like` (not the shorter
`like to use`, which also matches `Which disk(s) would you like to use?`).
Forty-eighth slice (this EFI): Alpine auto-answer appends `/media/cdrom/apks` to
apk repos and `apk update` before `setup-disk`, and answers `sys` to
`like to use` when `-m sys` did not stick. MMIO near CALL/JMP r/m (`FF /2`,
`FF /4`) so a BAR `call`/`jmp` sets RIP (CALL pushes RIP+len; long mode
defaults to 64-bit like PUSH; far CALLF/JMPF stay decode-fail; stack GPA
miss does not invent HPA). Forty-seventh slice (this EFI): Alpine auto-answer `setup-disk -s 0` (no swap),
`Which disk` → `/dev/vda`, and `No disks available` answers `n` to the
following boot-media `(y/n)` instead of `y`. Virtqueue/stack/MOVS GPA
translate lazy-maps report-RAM 2 MiB (same pool as an EPT miss; does not
invent a non-pool HPA). Forty-sixth slice (this EFI): MMIO MOVS/STOS/LODS (`A4`/`A5`/`AA`/`AB`/`AC`/`AD`,
optional F3 REP) so memcpy/memset of virtio/IOAPIC/xAPIC writes one
element per EPT (RAM GPA miss does not invent HPA; REP with RCX left
keeps RIP). CMPS/SCAS stay decode-fail. Forty-fifth slice (this EFI): MMIO PUSH/POP r/m (`FF /6`, `8F /0`) so a BAR
`push`/`pop` writes the stack (long mode defaults to 64-bit even without
REX.W; 66h is 16-bit) instead of decode-fail spinning. Stack GPA miss
does not invent HPA (virtio/xAPIC retry the EPT; IOAPIC skips).
Forty-fourth slice (this EFI): MMIO TZCNT/LZCNT/POPCNT (`F3 0F BC`/`BD`/`B8`)
so BMI1 `tzcnt`/`lzcnt` and `popcnt` of virtio/IOAPIC/xAPIC write the
count into the GPR (src 0 writes bitwidth and CF for TZCNT/LZCNT) instead
of decoding as BSF/BSR. Forty-third slice (this EFI): MMIO CMPXCHG8B (`0F C7 /1`) compares
EDX:EAX to the 64-bit BAR and stores ECX:EBX on match so Linux
`cmpxchg8b` of virtio/IOAPIC/xAPIC does not spin. CMPXCHG16B (REX.W)
is not emulated. Forty-second slice (this EFI): MMIO SHLD/SHRD (`0F A4`/`A5`/`AC`/`AD`)
writes the double-precision shift into the BAR (fill from the GPR, count
imm8 or CL) so Linux `shld`/`shrd` of virtio/IOAPIC/xAPIC does not spin.
Forty-first slice (this EFI): an armed product ISO uses the 16 777 216
resume cap on nested KVM too (`PRODUCT_ISO=` QEMU can pass OVMF
StartImage). Lab 72 KiB / `iso=0` nested stays 65536 so CI E4 SHELL
is unchanged. Nested still never prints `ISO-INSTALL-OK`.
Fortieth slice (this EFI): MMIO DIV/IDIV (`F6`/`F7` /6 /7) writes AX or
DX:AX so Linux `div`/`idiv` of virtio/IOAPIC/xAPIC does not spin; divisor 0
or quotient overflow injects #DE at the faulting RIP (no skip). MOVNTI
(`0F C3`) stores 32/64-bit GPR to the BAR (no 16-bit form).
Thirty-ninth slice (this EFI): MMIO one-operand MUL/IMUL (`F6`/`F7` /4 /5)
writes AX or DX:AX so Linux `mul`/`imul` of virtio/IOAPIC/xAPIC does not
spin. Thirty-eighth slice (this EFI): MMIO IMUL (`0F AF` r, r/m and `69`/`6B`
r, r/m, imm) so Linux signed multiply of virtio/IOAPIC/xAPIC does not
spin. Thirty-seventh slice (this EFI): Alpine auto-answer `mkdir -p /media/cdrom`
before `mount /dev/vdb` so apk still sees ISO9660 when nlplug never created
the mountpoint (virtio-iso, not ATAPI). Reply queue is 160 bytes.
Thirty-sixth slice (this EFI): MMIO PREFETCH (`0F 18`/`0F 0D`), multi-byte
NOP (`0F 1F`/`0F 19`), and CLFLUSH (`0F AE` /7) skip without touching the
BAR so a compiler hint on virtio/IOAPIC/xAPIC does not spin; BSF/BSR
(`0F BC`/`0F BD`) write the bit index into the GPR and set ZF.
Thirty-fifth slice (this EFI): MMIO CMOVcc (`0F 40`–`4F`) and SETcc
(`0F 90`–`9F`) so Linux conditional moves/sets on virtio/IOAPIC/xAPIC
do not spin. Thirty-fourth slice (this EFI): MMIO group-2 shifts (`C0`/`C1` imm8,
`D0`/`D1` 1, `D2`/`D3` CL) — SHL/SHR/SAR/ROL/ROR/RCL/RCR — so Linux
bitfield ops on virtio/IOAPIC/xAPIC do not spin. Thirty-third slice (this EFI): MMIO ADC (`10`/`11`/`12`/`13`, group-1 `/2`)
and SBB (`18`/`19`/`1A`/`1B`, group-1 `/3`) consume RFLAGS.CF so Linux
`adc`/`sbb` on virtio/IOAPIC/xAPIC does not spin. Thirty-second slice (this EFI): guest-UEFI CR8-load/store exiting so
Linux `mov cr8` (TPR) syncs `lapic_virt` after `nolapic` was dropped.
No VMCS GUEST_CR8; store writes `APIC_TPR = (val & 0xF) << 4`. E4 SHELL
VMCS does not request CR8 exiting. Thirty-first slice (this EFI): MMIO CMPXCHG (`0F B0`/`B1`) and XADD
(`0F C0`/`C1`) so Linux `lock cmpxchg` on virtio/IOAPIC/xAPIC does not
spin. Thirtieth slice (this EFI): MMIO BT/BTS/BTR/BTC (`0F BA` /4–7 imm8 and
`0F A3`/`AB`/`B3`/`BB`) so Linux `lock bts` on virtio/IOAPIC/xAPIC does
not spin; CF = old bit. Twenty-ninth slice (this EFI): product ISO IOAPIC vectors latch into
`lapic_virt` IRR and inject IRR→ISR so Linux `ack_APIC_irq` EOI matches
(M3.12: bare VM-entry inject with empty ISR is `Fatal exception in
interrupt`); remote IRR + level-triggered retry after EOI while the line
is still high. PIC stays a direct inject for noapic/early 8259.
Twenty-eighth slice (this EFI): MMIO dest-reg ALU (`02`/`03` ADD r, r/m,
`0A`/`0B` OR, `22`/`23` AND, `2A`/`2B` SUB, `32`/`33` XOR) writes the GPR
and updates RFLAGS; INC/DEC (`FE`/`FF`) and NOT/NEG (`F6`/`F7` /2 /3)
RMW the BAR so a decode-fail does not spin. Twenty-seventh slice (this EFI): auto-answer mounts virtio-iso `/dev/vdb` on
`/media/cdrom` before `setup-disk`; MMIO CMP `3A`/`3B` is `reg - mem`;
group-1 / register-form SUB. Twenty-sixth slice (this EFI): MMIO TEST/CMP update RFLAGS (virtio ISR poll
does not spin); serial auto-answer stays in CONFIRM after `[y/N]` so a later
`bootloader?` still gets `grub`. Twenty-fifth slice (this EFI): product ISO xAPIC 4 KiB EPT trap + `lapic_virt`
CUR_COUNT/EOI (IRR inject on preempt/HLT); same-length ISO patch puts
`virtio_blk` in `modules=` (`squashfs,sd-mod,usb-storage quiet` →
`squashfs,virtio_blk console=ttyS0`) and drops `nolapic` now that CUR_COUNT
moves; `terminal_output console` → `serial` when present; MMIO decode adds
register-form AND/OR/XOR/ADD and group-1 ADD. Twenty-fourth slice (this EFI): i8253 lo/hi access (`0x34`) returns lo then hi
on unlatched `inb 0x40`; 8-bit MMIO without REX uses AH/CH/DH/BH not SPL.
Twenty-third slice (this EFI): virtio/IOAPIC MMIO decode adds group-1
AND/OR/XOR (`80/81/83`) so a RMW does not spin on decode-fail. Twenty-second slice (this EFI): virtio/IOAPIC MMIO decode adds XCHG, MOVSX,
and moffs (decode-fail spins virtio); GRUB `insmod all_video` → `serial`
when present. Twenty-first slice: serial auto-answer matches alpine-conf
`confirm_erase` `[y/N]: ` (not only `(y/n)`), so an ISO that does not skip
via `ERASE_DISKS` still gets `y`. Twentieth slice: Alpine auto-answer exports `USE_EFI=1` with
`BOOTLOADER=grub` so UEFI `setup-disk` does not try syslinux/MBR; same-length
ISO patch `set timeout=1` → `set timeout=0` (after `timeout=10`) and
`insmod efi_gop` / `insmod efi_uga` → `insmod serial` when present so GRUB
EFI does not wait on GOP. Nineteenth slice: i8253 channel 0 is a 16-bit lo/hi + latch
counter so Linux `nolapic` `inb 0x40` sees a real count; `raise_pit` steps it.
The old stub wrote `val | 0x00FF` and never returned a high byte. Eighteenth slice: same-length ISO patch keeps `squashfs` in
`modules=` (`squashfs,sd-mod,usb-storage quiet` → `squashfs console=ttyS0 nolapic  `;
twenty-fifth slice swapped in `virtio_blk` and dropped `nolapic` after the xAPIC trap)
so Alpine mkinitfs can mount the live root; optional `console=tty0` → `noapic`;
MMIO insn fetch loops across 4 KiB pages; decode skips segment prefixes and
zero-extends MOVZX / 32-bit MOV into r8–r15. Seventeenth slice: Alpine
auto-answer sets `BOOTLOADER=grub` on `setup-disk` and replies `grub` to a
`bootloader?` prompt so the picker cannot stall the install. Sixteenth slice:
ISO patch also sets `nolapic` so Linux does not program the guest-UEFI static
xAPIC page (CUR_COUNT never moves) and then disable PIT. Fifteenth slice:
product ISO PIT IRQ 0 on HLT/preemption so Linux jiffies advance and HLT
wakes; UART/virtio PIC still beat the timer. Fourteenth slice: same-length ISO
patch added `console=ttyS0` / `noapic` (later restored squashfs in slice 18);
`alpine_dev=cdrom` → `alpine_dev=vdb` when present so alpine-virt
`nlplug-findfs` looks at virtio-iso `/dev/vdb` (0 hits OK).
Thirteenth slice: virtio INTx also raises IOAPIC pin 11 (PCI
interrupt line) so Linux without ACPI `_PRT` can complete virtio-blk.
Twelfth slice: virtio GPA copies stop at 4 KiB so lazy report-RAM
2 MiB slots (non-contiguous HPA) are not overrun. Eleventh slice: product ISO window reveals a read-only virtio-blk
at `00:03.0` (`/dev/vdb`) backed by the ISO bytes so alpine-virt (virtio
initramfs, often without `ata_piix`) can find ISO9660 media; packed
common-cfg 32-bit read at `0x10` includes `num_queues`; slot-3 INTA is GSI 18.
Tenth slice: virtio-blk OUT walks every data descriptor in the
chain (Linux blk-mq bio_vec), not only the first. Ninth slice: ATAPI PIO DRQ is 31 CD sectors so Linux `sr` READ(10)
is not completed short at 4; IDENTIFY is PIO-only; nIEN masks IRQ 14.
Eighth slice: ISO patch loads `ata_piix` + `sr-mod` + `console=ttyS0`
(squashfs stays in initramfs), GRUB `gfxterm` → `serial` when present, BusyBox
`/ # ` auto-answer, 64-bit virtqueue GPA writes keep the high half. Seventh slice: virtio-blk OUT copies the full request (not a 4 KiB
cap), ISO patch loads `sr-mod` + `console=ttyS0` and zeros GRUB timeout, virtio
PCI INTA line is IRQ 11. Sixth slice: Alpine serial auto-answer — `login:` → `root`, `~# `
→ `setup-disk -m sys /dev/vda` (with virtio modprobe). Fifth slice: host COM2 (iDRAC SOL) then COM1 RX is copied into
guest COM1 RBR so the installer can take serial input. 16550 loopback so
Linux 8250 autoconfig can bind. Fourth slice: product ISO 16550 (scratch/FIFO, COM1 GSI 4) so
Linux 8250 can bind `ttyS0`, plus a same-length ISO cmdline patch
(`sd-mod,usb-storage quiet` → `sd-mod console=ttyS0`). Lab UART stays
stub. Third slice: product ISO PIC + IOAPIC (ATA GSI 14, virtio slot-2
INTA GSI 17) and VM-entry inject so a Linux installer can complete virtio-blk
/ ATAPI instead of polling. Lab 8259 stays RAZ/WI. PRE-EBS ESP probe for
`\EFI\RayNu\linux.iso` (then `\linux.iso`, `\EFI\RayNu\install.iso`) copies a
window-sized ISO into `LOADER_DATA`. Guest-UEFI presents that ISO, arms
virtio-pci queues only when the window is armed, and **holds** instead of
packed-bzImage E4. Lab 72 KiB stub / `iso=0` still enum-only and fail-softs
to E4 `LINUX-EARLY`. Product resume cap is 16 777 216 on iron **and** nested
when the window is armed (lab-stub nested still 65536). Not `ISO-INSTALL-OK`. Keep
`windows_iso` / `generic_uefi`. `ISO-BOOTED-FROM-DISK` is persist-detect.
M4.3 host-slab closed on iron COM2 after `22e28d0`: `M4.3 blk probe host
slab HPA=0x10c00000`; `guest_code=0x10c00000`; `RAYNU-V-M4-BLK-OK`; then
`M4-NET-OK` / `M4-SMP-OK` / `R640-BOOT-OK` / Phase F coexist on
`10.99.99.126:8443`. Stage 45, P0-60, and G0 relocate stay CLOSED.
Accepted sequence ([ADR-014](adr/ADR-014.md)): Stage 45 (closed) → P0-60
(closed) → G0 VMCS relocate (closed) → M4.3 blk host-slab (closed) → Stage 46 `ISO-INSTALL-OK`.
Do not number G1 as Stage 46.
Stage 44 closed on iron COM2 `bf696ca`: `RAYNU-V-M7-E5-OVMF-ATAPI-OK`
`sectors=1` `packet=9` `scsi=0x28` `ata=0xa0` `ataio=982` stop n=30769
`pci_ide=1 virtio=1`; BOTH-OK n=12411 virtio `00:02.0` + IDE `00:00.1`;
`00:00.0` stays i440FX `0x1237` (no AcpiTimerLib ASSERT). Keep FIX WB hold.
Do not skip `ebecc9c3`. Do not move virtio to `00:00.0`. Do not fake
`sectors`. After ATAPI: E4 SHELL then M4.2 G1 EPT `GPA=0x10403000`
fail-soft is not Stage 44 residual. Not installer. Do not claim Everest E5 /
`ISO-INSTALL-OK`. `iso=0` E4 SHELL start stays valid.
Do **not** VMLAUNCH the 80-byte mock, the 4 KiB size-floor, the 1 MiB
EDK2 fixture, the 2 MiB live-map `_FVH`, a synthetic `0xEA` reset stub,
or a 4 MiB firmware-alias / alias-EPT / private-install / real-ESP /
insn-arm / live-exec / private-VMCS / live-issue / live-bytes / live-FD /
live-present / live-admit / live-read / live-copy / live-place / live-apply /
live-commit / live-latch / live-seal / live-lock / live-hold fixture.

**Closed host + QEMU:** Stage 0–35 as before · Stage 36
`RAYNU-V-M7-E5-LIVE-BYTES-PRESENT-OK` · Stage 37
`RAYNU-V-M7-E5-OVMF-VMLAUNCH-OK` · Stage 38
`RAYNU-V-M7-E5-OVMF-ALIVE-OK` (CR4.VMXE host-owned so OVMF SEC
`mov cr4, 0x640` does not triple-fault; short resume; not full OVMF) ·
Stage 39 `RAYNU-V-M7-E5-OVMF-PAST-SEC-OK` (left last 64 KiB + PEI PCI /
firmware COM / HLT; COM1/COM2 forwarded; not full DXE) ·
Stage 40 `RAYNU-V-M7-E5-OVMF-CDROM-OK` (`attach_cdrom_uefi` →
GuestVisible; PCI IDE/ATAPI on the private VMCS; not full DXE) ·
Stage 41 `RAYNU-V-M7-E5-OVMF-DXE-OK` (**closed** nested VT-x: CMOS/fw_cfg
+ i440FX at `00:08.0` + IDE at `00:00.0` PEI DID `0x7010`;
`OVMF-CDROM-OK` pci_ide=1 sectors=0; post-DXE tail then E4; not
installer) ·
Stage 42 `RAYNU-V-M7-E5-OVMF-VIRTIO-OK` (**closed** nested VT-x: PEI DID
`00:00.0` `val=0x1042`; `OVMF-VIRTIO-OK` pci=1; CD GuestVisible;
`pci_ide=0` sectors=0; stop n=115 virtio=1; not installer) ·
Stage 43 `RAYNU-V-M7-E5-OVMF-BOTH-OK` (**closed** nested VT-x `1b07692`:
`pci select 00:00.01` `val=0x70108086`; `OVMF-BOTH-OK`; stop n=1111
`pci_ide=1 virtio=1` `sectors=0` `spin=1`; E4 Linux #DF fail-soft).
Stage 44 `RAYNU-V-M7-E5-OVMF-ATAPI-OK` (**closed** iron COM2 `bf696ca`:
`sectors=1` `packet=9` `scsi=0x28` stop n=30769 `pci_ide=1 virtio=1`;
BOTH-OK n=12411 virtio `00:02.0` + IDE `00:00.1`; no AcpiTimerLib ASSERT).
Stage 45 `RAYNU-V-M7-E5-OVMF-ELTORITO-OK` (**closed** iron COM2 `0be7283`:
`RN-ELT` n=197992 catalog=1 bootimg=1 magic=1 sectors=183 elt=1 packet=533
scsi=0x28 port=0x3f8; 2048-byte FAT + ISO9660 BOOTX64; 262144-exit cap).

**Next after Stage 45 + P0-60 + G0 relocate + M4.3:** Stage 46
`ISO-INSTALL-OK` (OPEN; PIC/IOAPIC inject + ESP retain + virtio-pci queues +
read-only ISO virtio at `00:03.0` + hold when the product window is armed;
lab stub still E4; not `sectors>0`
alone; G1 is not Stage 46).
Product ISO is
[ADR-014](adr/ADR-014.md) (UEFI+virtio, typed; not bzImage-only). Optional: skip
`VMCLEAR` when launch-state is launched and `VMRESUME` instead. Keep
`NO_PHYLOCK` / skip BMCR when NCSI. Reject `42b42c99`, `ec08c00f`, `1404f055`, skip-CORECLK
`26573eb1`, hung E4 prefixes, and take-PHY (`ape-nophylock=no`). Preserve
`releases/v0.1.0-adr013-baseline`. Evidence:
[`docs/evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md`](evidence/r640/2026-08-21-e4-spa-shadow-reentry-ok.md).
