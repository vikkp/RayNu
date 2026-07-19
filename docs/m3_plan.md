# M3 Plan — Unmodified Linux Guest

**Goal:** Unmodified Linux 6.x to a shell (or `init` marker) under RayNu-V.  
**Risk:** R04 — real kernels expose every emulation gap.  
**Proven Core:** Linux boot protocol and device emulation stay **outside** (ADR-002). EPT ownership / allocator / inject firewall stay **inside**.

Lived gates through M3.5 (**synthetic M3 closed**): [progress.md](progress.md).

---

## Where we are

**Synthetic M3.0–M3.5 closed** on Latitude (proto-kernel / proto-init phase machine).

| Have (synthetic) | Need for real Linux |
|------------------|---------------------|
| Finite exit phases 0–7 → finish + VMXOFF | Continuous exit loop (unexpected exits must not abort) |
| 4 GiB identity EPT; guest shares host CR3 | Same OK for first shell; precise EPT later |
| Host LAPIC one-shot + software inject | Guest-safe timer (xAPIC MMIO trap and/or TSC); no host LAPIC clobber |
| Synthetic 1-page kernel/initrd + packed `boot_params` | Real bzImage + initrd load (ESP first; PE assets later) |
| COM1 data OUT + fake LSR; magic latches | Broader 16550 (DLAB/LCR enough for earlyprintk); RX later |
| CPUID filter hides VMX | MSR exiting + allow-list wired to VMEXIT |
| GPR save RAX–RDX + RSI | Full guest GPR save/restore for Linux |

---

## Target guest (first *real* shell)

- **Kernel:** small x86_64 `bzImage` (`tinyconfig` or trimmed defconfig + serial)
- **Initrd:** busybox/`init` that prints `RAYNU-V-M3-SHELL-OK` (ash optional)
- **Cmdline:**  
  `earlyprintk=serial,ttyS0,115200 console=ttyS0,115200 acpi=off nokaslr maxcpus=1`
- **Entry:** 64-bit Linux boot protocol (HV already in long mode — skip real-mode)
- **Load:** ESP `/EFI/BOOT` or `/assets/` first; PE `.assets.*` (ADR-003) when size/tooling ready  
  **not** QEMU `-kernel` (that feeds the outer VM)

**Non-goals for first real shell:** SMP, virtio-blk/net, full ACPI, >4 GiB RAM, precise EPT, multi-VM, interactive getty, PV clocks beyond “boots enough”.

---

## Subgates (work units)

Each gate = one branch `cursor/m3-N-…-a623`, marker, Latitude/QEMU or host test, docs touch.

### M3.0 — Guest I/O path — `RAYNU-V-M3-IO-OK`

**Why first:** Without console I/O, every later hang is silent.

- Unconditional I/O exiting (or bitmaps covering `0x3F8–0x3FF`)
- `devices/serial_pio.rs`: 16550 decode; passthrough → host COM1; magic latch
- Naked VMEXIT trampoline saves guest RAX; I/O exits VMRESUME into M2 phase machine
- Synthetic guest: after store/loop, `mov edx,0x3f8` / `out dx,al` for `RAYNU-V-M3-IO`, then `hlt`
- Extend `tools/qemu-boot-test.sh`

Status: **closed on Latitude** (`RAYNU-V-M3-IO-OK`). No kernel assets.

### M3.1 — CPUID filter — `RAYNU-V-M3-CPUID-OK`

- CPUID exiting; filter leaves (hide VMX from guest; sensible vendor/features)
- Wire / extend `sched/msr_firewall` patterns for CPUID policy (host tests)
- Guest smoke: CPUID then HLT; serial confirms filtered leaf

Status: **closed on Latitude** (`RAYNU-V-M3-CPUID-OK`).

### M3.2 — Kernel load — `RAYNU-V-M3-LOAD-OK`

- Place bzImage + initrd + `boot_params` (setup header, e820, cmdline, ramdisk ptrs) in GPA
- Claim frames via `FrameAllocator` + ADR-004 ownership
- Host unit tests for boot-protocol packing
- QEMU: HV prints load addresses + setup magic (no entry yet)

Status: **closed on Latitude** (`RAYNU-V-M3-LOAD-OK`). Synthetic stubs; real bzImage still M3.3+.

### M3.3 — Early printk — `RAYNU-V-M3-EARLY-OK`

- Jump to 64-bit kernel entry with valid `boot_params`
- Continuous exit loop: I/O + CPUID (+ MSR stubs as needed)
- Serial shows Linux banner / earlyprintk
- **First real Linux signal on Latitude**

Status: **closed on Latitude** (`RAYNU-V-M3-EARLY-OK`). Proto-kernel; real bzImage still deferred.

### M3.4 — Guest timer — `RAYNU-V-M3-GTIMER-OK`

- Guest-usable timer: trap xAPIC MMIO (or TSC deadline) + inject via `sched/interrupt`
- Distinct from M2.5 host LAPIC path
- Enough for jiffies / `init` scheduling

Status: **closed on Latitude** (`RAYNU-V-M3-GTIMER-OK`).

### M3.5 — Shell / init marker — `RAYNU-V-M3-SHELL-OK`

- `init` prints `RAYNU-V-M3-SHELL-OK` on COM1
- Full QEMU gate; Latitude required
- Docs / site celebrate M3 closed

Status: **closed on Latitude** (`RAYNU-V-M3-SHELL-OK`). Proto-init (synthetic).

---

## Real Linux subgates (post-synthetic)

Same cadence: one branch `cursor/m3-N-…-a623`, marker, Latitude gate, docs touch.  
Keep synthetic path as fallback until each real gate replaces it.

### M3.6 — Continuous exit loop — `RAYNU-V-M3-LOOP-OK`

**Why first:** A real kernel will VMEXIT for reasons outside the phase machine; aborting kills bring-up.

- Replace finish-after-shell with a durable resume loop (I/O / CPUID / HLT / IRQ keep running)
- Unknown exit reasons: log + safe halt (or stub resume) instead of silent `finish_boot(false)` where possible
- Expand GPR save/restore beyond RAX–RDX+RSI
- Host test or QEMU marker that the loop survived N resumes after shell magic

Status: **closed on Latitude** (`RAYNU-V-M3-LOOP-OK`).

### M3.7 — Real bzImage load — `RAYNU-V-M3-BZIMAGE-OK`

- Build/obtain tiny x86_64 bzImage (+ optional initrd blob) as ESP assets
- Parse setup header; place kernel/initrd/`boot_params`/cmdline in GPA; claim frames (ADR-004)
- Jump to 64-bit entry (`code32_start` / handover entry) with RSI=`boot_params`
- Serial: load addresses + setup magic (HdrS) — entry may still fault until M3.8

Status: **closed on Latitude** (`RAYNU-V-M3-BZIMAGE-OK`). Minimal fixture; real tinyconfig next (M3.8).

### M3.8 — Real earlyprintk — `RAYNU-V-M3-LINUX-EARLY-OK`

- Latitude shows Linux early serial / banner (not proto-kernel)
- Fill 16550 gaps that block `earlyprintk=serial,ttyS0`
- Stub or handle first MSR / exception exits that appear in the log
- Marker distinct from synthetic `M3-EARLY-OK`

Status: **in flight** — relocatable tinyconfig bzImage + banner latch.
Known traps:
- Linux `startup_64` clears CR4.VMXE → #GP under VMX; host-own via
  `CR4_GUEST_HOST_MASK` / `CR4_READ_SHADOW` at Linux entry.
- Relocate window is `[align_up(load, 2MiB), align_up+init_size)` — reserve
  `init_size + kernel_alignment` and place the PM image on a 2 MiB boundary.

### M3.9 — Guest timer / MSR harden — `RAYNU-V-M3-GTIMER2-OK`

- Only as needed to reach `init`: MSR exit path + allow-list; guest APIC MMIO trap or TSC deadline
- Stop identity-mapping host LAPIC into the guest if that is the hang
- Reuse `sched/interrupt` inject firewall

Status: planned; may interleave with M3.8 if the kernel stalls before banner/`init`.

### M3.10 — Real shell / init — `RAYNU-V-M3-SHELL-OK` (real guest)

- busybox (or static) `init` on initrd prints the shell marker on COM1
- Same marker string as synthetic M3.5; gate proves **real** userspace
- Docs/site: “unmodified Linux to init marker”

Status: planned. Depends on M3.8 (and M3.9 if required).

---

## Parallel (does not gate first real shell)

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

1. ~~Plan / M3.0–M3.7~~ — done (through bzImage load).
2. **M3.8** — real earlyprintk on Latitude (tinyconfig bzImage).
3. **M3.9** — MSR / guest timer only as blockers appear.
4. **M3.10** — busybox/`init` → real `RAYNU-V-M3-SHELL-OK`.
5. Verus L3 / precise EPT / PE asset embed (parallel).

---

## File anchors

| Path | Role |
|------|------|
| `vmx/launch.rs` | Exit phase machine → **M3.6** continuous loop |
| `guest/linux_boot.rs` | Synthetic load today → **M3.7** real bzImage |

| `devices/mod.rs` | Device stubs → serial PIO |
| `memory/ept_hw.rs` | 4 GiB identity (keep for M3 bring-up) |
| `memory/ept.rs` | Ownership claims for kernel/initrd pages |
| `sched/interrupt.rs` | Inject firewall (reuse for guest timer) |
| `arch/apic.rs` | Host LAPIC; guest APIC trap is new |
| `sched/msr_firewall.rs` | Wire to MSR/CPUID exits |
| `docs/adr/ADR-003.md` | PE asset / size budget for kernel embed |
