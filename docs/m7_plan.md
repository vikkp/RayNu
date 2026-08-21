# M7 Plan — Mount Everest (shippable single-host)

**Status:** **M7.5 + M7.6 + M7.7 stamp-persist + M7.8 / E3b + ADR-013 Phase F closed on iron**. Next: flash VMCLEAR+revision-rewrite EFI (iron `63cd694f` printed clone+marker+G0 VMLAUNCH then slot 1 error 11). Residual after a clean E4: TLS/console + distro installer.  
**Prior:** M7.4 closed on Latitude (`RAYNU-V-M7-UI-OK`); M7.3–M7.0 closed; M6 closed.  
**Parent roadmap:** [CLAUDE.md](../CLAUDE.md) (M7 row) · ADR: [adr/ADR-009.md](adr/ADR-009.md) · E3 listen: [adr/ADR-012.md](adr/ADR-012.md) · E3b: [adr/ADR-013.md](adr/ADR-013.md) · HDA: [hda.md](hda.md) · lived: [progress.md](progress.md)  
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

**Acceptance (met on Latitude host smoke):** marker + gate. **Residual:** El Torito/CD-ROM attach still stubbed; no full distro installer path on QEMU/iron yet.

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

**Honesty:** not a guest filesystem / distro ISO installer. El Torito residual.

**Acceptance:** **Met on iron** — persist-detect + `prefix_into=67108864` + `BOOTED-FROM-DISK` + `R640-BOOT-OK`. Mount Everest stays open (post-EBS HTTP + E4 polish + real distro installer).

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
Tcp4 stay dead after EBS. Keep `ape-nophylock=yes`. E4 SPA start now queues a real VMLAUNCH
(private 2 MiB EPT, slab VMCS) on the coexist quantum. Iron clone EFI `63cd694f` printed
`RAYNU-V-M7-E4-SPA-LAUNCH-OK` + G0 VMLAUNCH then slot 1 `VMPTRLD` error 11 — **not closed**.
The scheduler now `VMCLEAR`s the outgoing VMCS and rewrites the revision dword before
`VMPTRLD` of the incoming; first re-entry is `VMLAUNCH`. Guest is SHELL CPUID, not a
distro installer.

**Next:** Flash the VMCLEAR+revision-rewrite EFI **by SHA** (do not reuse `63cd694f`).
Force Off Ubuntu; F11 Cruzer. Mac spec → `sleep 2` → start using the **COM2 lease**
(not Ubuntu `.124`). WANT clone verify + marker + G0 VMLAUNCH **and** slot 1 without
error 11 (or one park HINT, quiet COM2, HTTP-OK). No repeating `VMPTRLD failed`, no
`VMXOFF`, no `boot gate failed`. Then TLS/console + distro.
Keep `NO_PHYLOCK` / skip BMCR when NCSI. Reject `42b42c99`, `ec08c00f`, `1404f055`, skip-CORECLK
`26573eb1`, and take-PHY (`ape-nophylock=no`). Preserve
`releases/v0.1.0-adr013-baseline`. Evidence:
[`docs/evidence/r640/2026-08-20-e3b-host-nic-http-ok.md`](evidence/r640/2026-08-20-e3b-host-nic-http-ok.md).
