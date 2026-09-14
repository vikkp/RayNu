# RayNu VM Generation 1 — living plan

**Status:** **OPEN** — Everest (M7) **CLOSED on iron**. This is the **guest/firmware** queue, not the operator persist/TLS queue.  
**Parent:** [ADR-020](adr/ADR-020.md) · firmware: [ADR-016](adr/ADR-016.md) · typed ISO: [ADR-014](adr/ADR-014.md) · F7: [ADR-017](adr/ADR-017.md)  
**Sibling:** [m8_plan.md](m8_plan.md) (operator). Cluster is **M9**, not this plan.

RayNu-F is **already** the product path (`raynu_f/` on lived `main`). Iron printed `RAYNU-V-M7-ISO-INSTALL-OK` and `RAYNU-V-RAYNU-F-DISK-BOOT-OK`. Nested persist nested-OK is closed. **This plan does not reopen Everest.**

The product is **one specified VM** (Hyper-V Generation 2 style: UEFI-only, no legacy BIOS, synthetic devices only). Unmodified media. Reject guests that need more; do not patch them as the product path.

---

## Honesty (Phase 0 is half-closed)

| Claim | Lived |
|-------|--------|
| RayNu-F exists / is the E5 path | **True.** Do not paste “`raynu_f/` is not present.” |
| Iron ISO install + reboot-to-disk | **Closed** (`f72b4276` / `34552377351`). Host/CI never print `ISO-INSTALL-OK`. |
| Nested reboot-to-disk | **Landed** (Phase 0). Nested QEMU ≠ R640. |
| Unmodified Alpine ISO | **Open** (Phase 2). Iron still uses `patch_iso_linux_serial_console` + serial auto-answer. |
| Runtime services | **Open** (Phase 3). `efi=noruntime` is lab. |
| `docs/raynu_vm_gen1.md` freeze | **Does not exist yet** (Phase 4). Do not claim it. |

---

## Close order vs M8

[ADR-020](adr/ADR-020.md) serializes both queues. **Do not delete the ISO patcher before M8.0-mech** (`RAYNU-V-M8-DISK-PERSIST-OK` on COM2). Keep patched Alpine + auto-answer for the USB/NVMe Force Off loop.

Design of Phase 1 (split `guest_uefi.rs`) may overlap M8.0-mech. Closing Phase 2 (delete patcher) before that COM2 line is a queue collision.

---

## Phases

### Phase 0 — nested reboot-to-disk

**Status: landed** (nested). Iron Everest path already rebooted to disk with the patcher.

Not a license to delete the patcher.

### Phase 1 — split the firmware crate

**Status: open** (after M8.0-mech)

**Goal:** Split `vmx/guest_uefi.rs` so the VMX exit loop, Gen-1 device model, and `raynu_f/` are separable. OVMF remains a `tools/` oracle, not the product firmware.

**Stop:** OVMF puppeting as the product path (ADR-016: no third-party firmware state mutation).

### Phase 2 — unmodified Alpine

**Status: open** (after M8.0-mech; prefer after Phase 1)

**Goal:** Stock alpine-extended ISO → install → reboot → login. **Delete** `mgmt/iso_install.rs` `patch_iso_linux_serial_console` and `devices/guest_serial_answer.rs` (and the GRUB linux-line grow that exists only to carry `efi=noruntime` / virtio knobs).

**Conformance:** zero guest-side product patches; **zero** auto-answer.

**Stop:** new per-ISO stanzas; claiming a distro until unmodified.

### Phase 3 — Runtime services v1.0

**Status: open** (after Phase 2)

**Goal:** Guest UEFI runtime services that survive `ExitBootServices` (GVA + `SetVirtualAddressMap`). Unmodified Ubuntu/Debian. `efi=noruntime` is **lab**, not supported.

Runtime is **v1.0**, not “later.”

### Phase 4 — freeze Generation 1

**Status: not started**

**Goal:** Write and freeze `docs/raynu_vm_gen1.md`. Wire `generation = 1` on the create-VM spec. CI matrix of **unmodified** ISOs that must install.

Do not pretend this file or freeze exists until this phase closes.

### Phase 5 — Windows / Secure Boot / vTPM

**Status: later** (after Phase 4)

Same UEFI+virtio model ([ADR-014](adr/ADR-014.md)). Not WHQL. **Not a peer of M8.1 TLS.** Operator-facing name remains M8.6 in [m8_plan.md](m8_plan.md).

---

## Proven Core

RayNu-F stays **outside** Proven Core (ADR-002 / ADR-016). Promotion only by a new ADR into the ~3k LOC buffer (default **no**). EPT exclusive ownership (ADR-004) still applies: guest disks are virtio/BlockIo HPAs, never RAM.

---

## First action on this queue

Do **not** start Phase 2 (delete patcher) while M8.0-mech iron is the NOW item. If firmware work is in flight, it is Phase 1 **design** only, or wait for COM2 `RAYNU-V-M8-DISK-PERSIST-OK`.

Agent rule: [`.cursor/rules/m8-gen1-queues.mdc`](../.cursor/rules/m8-gen1-queues.mdc).
