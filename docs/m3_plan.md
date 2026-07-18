# M3 Plan — Unmodified Linux Guest

**Goal:** Unmodified Linux 6.x to a shell (or `init` marker) under RayNu-V.  
**Risk:** R04 — real kernels expose every emulation gap.  
**Proven Core:** Linux boot protocol and device emulation stay **outside** (ADR-002). EPT ownership / allocator / inject firewall stay **inside**.

Lived gates through M2.6: [progress.md](progress.md).

---

## Where we are

| Have | Need for Linux |
|------|----------------|
| VMLAUNCH / VMRESUME, HLT exiting | Continuous exit loop (not finish-after-timer) |
| 4 GiB identity EPT | Same OK for first shell; precise EPT later |
| Software inject + host LAPIC timer | Guest-visible timer (xAPIC MMIO or TSC) |
| Synthetic guest page (store/ISR) | bzImage + initrd + `boot_params` / e820 |
| Host COM1 only | Trap guest PIO → 16550 emu / passthrough |
| CPUID/MSR firewall stubs | Wired to VMEXIT handlers |
| `devices/` stub | Serial PIO first; virtio deferred |

---

## Target guest (first shell)

- **Kernel:** small x86_64 `bzImage` (defconfig trimmed or `tinyconfig` + serial)
- **Initrd:** busybox/`init` that prints `RAYNU-V-M3-SHELL-OK` (ash prompt optional)
- **Cmdline:**  
  `earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 acpi=off nokaslr maxcpus=1`
- **Entry:** 64-bit Linux boot protocol (HV already in long mode — skip real-mode)
- **Load:** ESP assets or PE `.assets.kernel` / `.assets.initrd` (ADR-003); **not** QEMU `-kernel` (that feeds the outer VM)

**Non-goals for first shell:** SMP, virtio-blk/net, full ACPI, >4 GiB RAM, precise EPT, multi-VM, PV clocks beyond “boots enough”.

---

## Subgates (work units)

Each gate = one branch `cursor/m3-N-…-a623`, marker, Latitude/QEMU or host test, docs touch.

### M3.0 — Guest I/O path — `RAYNU-V-M3-IO-OK`

**Why first:** Without console I/O, every later hang is silent.

- Unconditional I/O exiting (or bitmaps covering `0x3F8–0x3FF`)
- `devices/serial_pio.rs`: 16550 decode; passthrough → host COM1; magic latch
- Naked VMEXIT trampoline saves guest RAX; I/O exits VMRESUME into M2 phase machine
- Synthetic guest: after store/loop, `out 0x3f8, al` for `RAYNU-V-M3-IO`, then `hlt`
- Extend `tools/qemu-boot-test.sh`

Status: **in flight** (see PR). No kernel assets.

### M3.1 — CPUID filter — `RAYNU-V-M3-CPUID-OK`

- CPUID exiting; filter leaves (hide VMX from guest; sensible vendor/features)
- Wire / extend `sched/msr_firewall` patterns for CPUID policy (host tests)
- Guest smoke: CPUID then HLT; serial confirms filtered leaf

### M3.2 — Kernel load — `RAYNU-V-M3-LOAD-OK`

- Place bzImage + initrd + `boot_params` (setup header, e820, cmdline, ramdisk ptrs) in GPA
- Claim frames via `FrameAllocator` + ADR-004 ownership
- Host unit tests for boot-protocol packing
- QEMU: HV prints load addresses + setup magic (no entry yet)

### M3.3 — Early printk — `RAYNU-V-M3-EARLY-OK`

- Jump to 64-bit kernel entry with valid `boot_params`
- Continuous exit loop: I/O + CPUID (+ MSR stubs as needed)
- Serial shows Linux banner / earlyprintk
- **First real Linux signal on Latitude**

### M3.4 — Guest timer — `RAYNU-V-M3-GTIMER-OK`

- Guest-usable timer: trap xAPIC MMIO (or TSC deadline) + inject via `sched/interrupt`
- Distinct from M2.5 host LAPIC path
- Enough for jiffies / `init` scheduling

### M3.5 — Shell / init marker — `RAYNU-V-M3-SHELL-OK`

- `init` prints `RAYNU-V-M3-SHELL-OK` on COM1
- Full QEMU gate; Latitude required
- Docs / site celebrate M3 closed

---

## Parallel (does not gate shell)

- Verus L3 attempt on 4K single-guest EPT (ADR-004 M3 row)
- Harden Kani CI beyond soft-fail
- Precise EPT / drop identity scaffold (post-shell or late M3)

---

## How we work (same as M2)

| Where | Job |
|-------|-----|
| Mac `~/projects/raynu` | Code, `cargo test`, UEFI build |
| Latitude `~/raynu` | Nested KVM: `./tools/qemu-boot-test.sh` |
| GitHub | `cursor/m3-N-…-a623` → PR → merge after gate |

Day loop:

```bash
# Mac
git checkout main && git pull
git checkout -b cursor/m3-0-guest-io-a623
# …implement…
cargo test --no-default-features && ./tools/build.sh
git push -u origin HEAD

# Latitude
git fetch && git checkout cursor/m3-0-guest-io-a623
sudo ./tools/enable-nested-kvm.sh   # if needed
./tools/qemu-boot-test.sh
```

---

## Suggested start order

1. **This plan** lands in-tree (docs only).
2. **M3.0** — guest COM1 I/O (first code PR).
3. M3.1 → M3.2 (can overlap host load tests with I/O polish).
4. M3.3 is the schedule checkpoint (earlyprintk).
5. M3.4 → M3.5 close M3.

---

## File anchors

| Path | Role |
|------|------|
| `vmx/launch.rs` | Exit phase machine → grow into exit loop |
| `devices/mod.rs` | Device stubs → serial PIO |
| `memory/ept_hw.rs` | 4 GiB identity (keep for M3 bring-up) |
| `memory/ept.rs` | Ownership claims for kernel/initrd pages |
| `sched/interrupt.rs` | Inject firewall (reuse for guest timer) |
| `arch/apic.rs` | Host LAPIC; guest APIC trap is new |
| `sched/msr_firewall.rs` | Wire to MSR/CPUID exits |
| `docs/adr/ADR-003.md` | PE asset / size budget for kernel embed |
