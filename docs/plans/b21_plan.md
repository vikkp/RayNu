# B21 execution plan: guest vCPUs on real host cores

Repo `vikkp/RayNu`. Written against the B7 branch head `49fdad36`
(`origin/cursor/wave-7-b7-smp-25ca`, PR #440, B7 lived on iron `505ef6bdbc73`).
Line numbers are from that commit. They drift. Find by symbol name.

Read `CLAUDE.md`, `.cursor/rules/*.mdc`, `docs/adr/ADR-021.md` (row B21, wave 7),
`docs/adr/ADR-022.md` and `docs/adr/ADR-023.md` (all of it, and the section
"What B21 changes") before the first edit. Everything below assumes you have.

The owner's rule for this row: **do it slow, do it right**. Three iron gates, in order.
A failed gate stops the line. Do not merge. Do not tag.

---

## 0. What B21 is

Today (B7) every vCPU of every guest is time-sliced on **one** host core. The other
cores of the R640 sit where the firmware parked them.

B21 starts the other host cores, puts each one in VMX root, and runs vCPUs on them.
A guest with 2 vCPUs then has two cores of capacity, and two guests no longer share
one core. A guest cannot be given more vCPUs than there are host cores online.

The row closes when, on the R640:

1. COM2 names the host cores online and each is in VMX root (gate 1).
2. An Ubuntu guest with 2 vCPUs runs two CPU-bound jobs in about the time of one
   (gate 2).
3. Two guests that are both busy each keep a full core (gate 3).
4. Everything B6 and B7 lived still works on the same stick.

### The benchmark: how VMware ESXi does it

| ESXi | B21 | Same? |
|---|---|---|
| Every host logical CPU runs the scheduler; a vCPU (world) runs on any of them | A run queue per host core; a vCPU is placed on a core when it comes up | Yes, without later rebalancing (B22) |
| A VM cannot have more vCPUs than host logical CPUs | `cpu_max = min(4, host cores online)`; Save and Start refuse more | Yes |
| A halted vCPU gives up its CPU; an idle CPU sleeps | A halted vCPU's core waits outside the hypervisor; a core with no vCPU sleeps in MWAIT | Partly: a core whose vCPU is halted spins (B22) |
| A vCPU on another CPU is interrupted with an IPI | Host IPI on a fixed vector forces the VM exit | Yes |
| Side-channel-aware scheduler: no two VMs on sibling threads | One thread per physical core is started; the sibling thread is never used | Yes (the strict form) |
| VMkernel locks are fine-grained; many CPUs are in the kernel at once | **One monitor lock**: guests run in parallel, hypervisor work is one core at a time | No. Stated. B22 splits the lock. |
| Co-stop bounds how far sibling vCPUs drift apart | Not built. With one vCPU per core there is no drift to bound | Not needed at 1:1; a limit when overcommitted |

---

## 1. Decisions already made (do not redesign)

| # | Decision | Why |
|---|---|---|
| D1 | **One monitor lock.** After a core is online, every instruction it runs in VMX root that touches shared state runs under one fair ticket spinlock. Taken in the VM-exit landing stub before any guest register is stored. Released by the VM-entry stubs after the last register is loaded. | `vmx/guest_uefi.rs` alone has about 225 statics written for one CPU, and the exit stub stores guest registers in globals. Under the lock all of that stays correct by construction. ADR-023 expected one lock per guest; that is B22, split with measurements from this row. |
| D2 | **Identity rule.** With one host core online, behaviour is B7 as lived. The only instructions that differ are the uncontended lock take and release and one COM2 summary line. | B7 is the regression base. |
| D3 | **Core 0 is the home core.** Boot, the E4 idle loops, Start, the RayNu-F firmware phase, reset, leave, guest INIT/SIPI service and the management tick run only on core 0. The other cores run vCPUs and nothing else. Core 0 holds at least one vCPU whenever any guest is on. | Every lifecycle path is lived on core 0 and uses core 0's original stack (`E4_RSP`). None of it is rewritten. |
| D4 | **A run queue per core, the lived `pick` on each.** Each vCPU (a *unit*, `slot * MAX_VCPUS + vcpu`) is assigned to exactly one core. Placement happens when the vCPU comes up: the core with the fewest units, other cores before core 0 on a tie. It is revisited only when core 0 switches a unit out (offload) and when core 0 recalls a unit. No move between two non-zero cores. | The scheduler stays the lived one. Rebalancing after a guest leaves is B22. |
| D5 | **A VMCS has a home core.** Only the home core may load, read, write or clear it. Moving a unit: the home core VMCLEARs it; the new core VMPTRLDs it, writes `HOST_RSP` for its own stack, and enters with VMLAUNCH. | SDM: a VMCS must not be active on two logical processors. `HOST_RSP` is the only host-state field that differs per core. |
| D6 | **Only core 0 asks another core for a unit and waits** (recall). A non-zero core never waits on another core for a unit. Any core may wait for other cores to take a pending INVEPT; that wait is only on cores that are in guest mode. | No wait cycle is possible. |
| D7 | **The kick is a host IPI on vector `0xF2`.** `PIN_BASED_EXTERNAL_INTERRUPT_EXITING` and `VM_EXIT_ACK_INTERRUPT_ON_EXIT` are already set, and host IF is always 0, so a kick is a VM exit and never a host interrupt. **Correctness never depends on a kick**: a running vCPU exits at least every 2 ms (lived `clamp_ticks`), a parked one is polled every 200 µs. `EFI\RayNu\nokick.txt` turns kicks off for diagnosis. | The kick is latency only. A lost kick costs time, not state. |
| D8 | **Idle.** A core with no unit sleeps in MWAIT (C1) on its mailbox word, PAUSE when CPUID has no MWAIT. A core whose units are all halted leaves the monitor and polls every 200 µs. Core 0 is unchanged (it spins through exits and that is what ticks the SPA). | Seven or fifteen cores spinning at 100% is not acceptable on a server. |
| D9 | **One thread per physical core.** Hyperthread siblings are never started. At most 16 cores (`MAX_UNITS`). | Skylake-SP is affected by L1TF and MDS. Two guests on sibling threads is the case ESXi's side-channel-aware scheduler forbids. |
| D10 | **Shared host tables.** GDT, TSS, IDT, CR3 and the control registers are core 0's. Per core: a 1 MiB host stack and a VMXON region. | The host takes no interrupts and has no IST stacks, so a per-core TSS would have no reader. This deviates from the ADR-023 table and ADR-024 says so. The row that installs a host IDT brings the per-core TSS. |
| D11 | **The cap is ADR-023 D13.** `guest_cpu::cpu_max()` returns `min(MAX_VCPUS, host cores online)`. Save refuses a larger count. Start of a row whose saved count is larger is refused with a sentence that names both numbers. `EFI\RayNu\onecore.txt` starts no other core and keeps the whole B7 model, cap 4. | ESXi refuses power-on. `onecore.txt` is the on-stick way back to B7 behaviour. |
| D12 | **Platform timers stay on vCPU 0's exits** (ADR-023 D8), on whichever core vCPU 0 runs. | A host timer is B9, the next row. |
| D13 | **Three iron gates, one branch.** Gate 1: cores online, guests still on core 0. Gate 2: vCPUs 1..N on other cores. Gate 3: a guest's vCPU 0 on another core. The row is lived at gate 3. | One variable per flash. |
| D14 | **No Proven Core file is edited.** `VmxLifecycle` is instantiated once per core. `AuditEvent::VmxEnabled { vcpu_id: core }` is reused. `msr_firewall` is used through `shadow_read` / `shadow_write`. | ADR-002. |
| D15 | **Topology comes from EFI MP Services before ExitBootServices.** Cores are started after ExitBootServices with INIT, SIPI, SIPI. No MADT parse. Protocol absent ⇒ one core and a COM2 line. | Present on the R640 and on OVMF. |
| D16 | **A panic keeps the monitor.** The panicking core does not release the lock, so every other core stops at its next exit. | Panic over silent corruption. |
| D17 | **Single fetch from guest memory.** A value read from guest memory that decides a host-side bound, index or address is read once into a local and validated there. | With real cores the guest can change memory while the hypervisor reads it. On one core it could not. |

**Words used below.** *Unit*: one vCPU of one guest. *Home*: the core on which the
unit's VMCS is active. *Assigned*: the core whose run queue holds the unit.
*Monitor*: the hypervisor while it holds the lock. *Park*: a non-zero core that has
left the monitor and waits.

---

## 2. Rules that fail the work if broken

- Never mark B21 `lived` without R640 COM2 and SSH evidence for gate 3. Nested QEMU
  and CI never do. Host, CI and nested never print an iron marker.
- A core that is not core 0 never prints, never touches the audit ring, the frame
  allocator or any `static` outside its own per-core entries **unless it holds the
  monitor**. During bring-up it reports through atomics and core 0 prints for it.
- A core that is not core 0 never executes `leave_to_e4`, `raynu_f_reset_relaunch`,
  `smp_collapse`, `on_guest_left`, `ap_start`, `ap_init`, `prepare_start`, `launch`,
  `launch_slot_guest`, `raynu_f_launch_on_stopped_vmcs`, `launch_uefi`,
  `guest_start_waiting` or `maybe_tick_standing_spa`. The first two hand the work to
  core 0 (WP5). The others get `host_smp::assert_core0("name")` as their first line.
- Never send INIT to a core that has executed VMXON. Bring-up happens once per boot.
- The lock is dropped inside a handler only through `yield_monitor_until` (WP5).
  Nothing else calls `release`.
- `vmx/launch.rs`, `vmx/lifecycle.rs`, `vmx/hardware.rs`, `vmx/ops.rs`,
  `sched/msr_firewall.rs`, `sched/ipi.rs`, `audit/integrity.rs`, `memory/ept_hw.rs`
  are not edited.
- No third-party firmware state mutation (ADR-016). Host cores are started only with
  the architected INIT and startup IPIs.
- No per-exit COM2 `inb` on the exit path. New lines on the exit path use
  `serial::write_*_nowait`.
- Every `unsafe` block: `// SAFETY:` and `// KANI-TARGET:`. Every new `static`: one
  line saying who writes it (its own core, or the monitor holder).
- About 1,100 host tests assert on source text with `include_str!(..).contains("…")`.
  **Add code; do not rename or reflow existing calls.** Where a lived line must behave
  differently, prepare state before it so the same line does the right thing. When a
  gate fails, restore the string it wants. Do not edit the gate.
- `gh` is read-only. PR through the PR tool. Do not merge. No force-push. Kill
  processes by PID only.
- Scores do not move: HDA months 0.0 / overall 99, LOIHDA Bar A 94 / Bar B 72 /
  overall 80.
- Lab disks: do not format UBUNTU0, do not partition RAYNU-SPARE, do not open the H840.

---

## 3. Code map (what exists at `49fdad36`)

| What | Where | Fact that matters |
|---|---|---|
| Exit landing | `vmx/guest_uefi.rs` `guest_uefi_vmexit_landing` (6378) | Naked. First instruction stores RAX into the global `SAVED_RAX`. Then RBX..R15, XMM0–15 into `SAVED_XMM`, then `jmp guest_uefi_vmexit`. One landing for every guest-UEFI VMCS. |
| Exit handler | `guest_uefi_vmexit` (7335) | First line `LAUNCH_ENTERED.store` is a per-guest static, so the current slot must be right before it. Calls `maybe_tick_standing_spa`, `take_guest_stop`, `b4_service`, NIC poll, then the reason switch. Tail (8026–8051): `smp_service`, `try_inject_guest_irq`, `arm_preempt_on_resume`, `guest_sched::resume(idle)`. |
| Entry points | `enter_guest` (6440) → `guest_uefi_vmresume` (6450) or `guest_uefi_vmlaunch` (6517); `raynu_f_vmlaunch` (10044, called at 8711); `ops::vmlaunch()` in `launch_uefi` (6005) | These four are the only ways into a guest. Each naked stub ends `vmresume` / `vmlaunch`, then `jmp {fail}`. `enter_guest` is called only from `guest_sched`. |
| Entry failure | `guest_uefi_resume_failed` (6507), `guest_uefi_launch_failed` (6574), `raynu_f_launch_failed` (10083) | Read `VM_INSTRUCTION_ERROR` with VMREAD (not flags), print, `leave_to_e4()`. |
| Host stack | `reserve_guest_uefi_host_stack` (7313), `HOST_RSP_SHARED` (4821) | One 1 MiB stack plus 1 MiB canary. Every VMCS names it. `host_rsp = top & !0xF`. |
| VMCS host state | `setup_guest_uefi_vmcs` (6028) | Reads CR0/CR3/CR4, EFER, PAT, IDTR, selectors, FS/GS base from the live CPU; GDT/TR from `install_host_tss`. Pin controls already have external-interrupt exiting; exit controls already have ack-on-exit. |
| B7 vCPU start | `smp_service` (6806), `ap_init` (6668), `ap_start` (6695), `smp_collapse` (6638) | `ap_start` loads the new VMCS, sets it up with `HOST_RSP_SHARED`, loads the sender's VMCS again. `smp_collapse` and `ap_init` VMCLEAR other vCPUs' VMCSes directly. |
| Leave and reset | `leave_to_e4` (17276), `raynu_f_reset_relaunch` (8958), `raynu_f_stop` (10094) | `leave_to_e4`: `smp_collapse`, `on_guest_left`, VMCLEAR, jump to `E4_RSP`/`E4_RESUME` (core 0's first stack). 17 call sites. `raynu_f_stop` ends in it. |
| Scheduler | `vmx/guest_sched.rs`: `pick` (88), `hw::resume` (756), `switch_out`, `switch_in`, `other` (874), `on_guest_left` (920), `clamp_ticks` (380), `SLICE_START` | `pick` is pure and walks `MAX_UNITS`. `other(u)` says whether a unit is on, halted, has work. `switch_in` sets current slot and vCPU, VMPTRLDs, restores registers, MSRs, XSAVE, the `msr_firewall` shadows. `resume` enters directly when `!slot::cpu_shared()`. |
| Current words | `guest/slot.rs`: `CURRENT`, `CUR_VCPU[slot]` | One word and one word per slot. Every `PerGuest` / `PerVcpu` bank is indexed through them. |
| Wake seam | `guest/slot.rs` `wake`, `wake_vcpu`; `devices/lapic_virt.rs` `post_remote` | ADR-023 D12. IOAPIC pins are different: each vCPU pulls its own pins in `guest_irq` on its resume; a raise wakes nobody. |
| INVEPT | `memory/ept_hw.rs` `invept_global`; eight call sites in `guest_uefi.rs` (5832, 15976, 16018, 16058, 16091, 16147, 16207, 17712) | Local core only. |
| Shadow MSRs | `sched/msr_firewall.rs` `shadow_read` / `shadow_write` | One bank for SPEC_CTRL and APIC_BASE. `switch_out` / `switch_in` already save and load it per unit. |
| VMXON | `vmx/lifecycle.rs` `VmxLifecycle::new().enable(region)`; `src/main.rs` `run_m1_vmx` (165) | Per-CPU by construction. `hardware::vmxon` does feature control, CR0/CR4 fix-up, the revision write. |
| Host LAPIC | `arch/apic.rs` | `probe()` knows xAPIC base or x2APIC. `eoi()`. No ICR code. |
| Boot order | `src/main.rs` `main` (45) | Pre-EBS probes → `leave_firmware()` → frame allocator → `run_m1_vmx` → `run_m2_ept_launch` → ladder → E4 idle. |
| ESP lab flags | `boot/raynu_f_flag.rs` `probe` (143) | Pre-EBS file probes (`twoguest.txt`, `vcpuN.txt`). Also calibrates `tsc_hz()`. |
| Memory at handoff | `boot/handoff.rs` `leave_firmware` | The pool takes `CONVENTIONAL` only. Pages allocated before EBS as `LOADER_DATA` are never in it. Host page tables are the firmware's identity map and are never changed after boot. |
| vCPU count | `mgmt/guest_cpu.rs` `cpu_max` (86), `save` (187), `on_accepted_start` (243); accept sites `mgmt/guest_catalog.rs` 923, 963, 1051 and `mgmt/perc_boot_choice.rs` 574; `apply_vcpus_for_start` (`guest_uefi.rs` 6616) | `cpu_max` is `MAX_VCPUS`. `GET /perc/guests` already carries `cpu_max`. |
| Page text | `assets/webui.html` 294 | "They share one host core in this build." |
| Panic | `mgmt/host_panic.rs` `firmware_panic` | Dump, VMXOFF on this core, cold reset. |
| Nested runner | `tools/run-qemu.sh` | No `-smp`. Trailing `"$@"` goes to QEMU. |

There is no host AP bring-up anywhere in the tree. `sched/smp_probe.rs` is the old
two-VMCS probe on one core; leave it.

Names in the skeletons below that are not in this table are new and yours to write:
everything in `host_smp::`, plus `mark_needs_launch`, `vmcs_of(u)` /
`saved_vmcs_of(u)` (the per-vCPU form of the lived `slot_vmcs()`), `stack_top_of`,
`unit_has_work`, `us_to_tsc` arguments as `resume` already passes them. The skeletons
show the logic; they are not compiled.

---

## 4. Architecture in one page

```
core 0 (home)                         core 1..N (runners)
─────────────                         ───────────────────
boot, E4 idle, Start, firmware        ap_idle: MWAIT on MAILBOX[core]
  │                                     │ woken: take monitor, switch_in a unit
  ▼                                     ▼
guest entry stub ── release ──►  guest runs  ◄── release ── guest entry stub
  ▲                                     ▲
VM exit landing ── take ──┐       ┌── take ── VM exit landing
                          ▼       ▼
                    ONE MONITOR LOCK
        (exit handler, devices, scheduler, serial, audit)
```

State kinds:

1. **Everything in software** (every `static`, every bank, EPT tables, device state,
   serial, audit): owned by whoever holds the monitor.
2. **A unit's hardware state** (its VMCS, and while loaded its CR2, DRs, MSRs, XSAVE):
   owned by its home core. Nobody else touches that VMCS.
3. **Per-core words** (`CORE_UNIT`, `CORE_SHADOW`, `IN_GUEST`, `MAILBOX`, slice
   start): written by their own core, or by the monitor holder where noted.
4. **Guest memory**: shared with guest code running on other cores. D17.

The "current" words (`slot::CURRENT`, `CUR_VCPU`, the `msr_firewall` shadows, the
`SAVED_*` register image) are one copy. Each core **saves what it needs when it
leaves the monitor and puts it back when it enters**:

- `monitor_leave()` runs just before every guest entry: records
  `CORE_UNIT[core] = current unit` and the two shadows.
- `monitor_enter()` is the first line of the exit handler: sets the current slot and
  vCPU and the shadows from those words.
- The register image needs neither: the landing stub writes it after taking the lock
  and the entry stub reads it before releasing.

---

## 5. Work packages

One commit per WP unless noted. Each commit builds and passes
`cargo test --no-default-features -- --test-threads=1`.

Stages: **WP0–WP4 = gate 1.** **WP5–WP9 = gate 2.** **WP10 = gate 3.** Push when a
stage's nested rows are green; that push run is the stage's flash. You may keep
coding the next stage while the operator runs a gate, but a gate's flash contains
only its own stage and earlier.

### WP0. Environment and baseline

```bash
cd /agent/repos/raynu
git fetch origin main cursor/wave-7-b7-smp-25ca
# If PR #440 is merged, branch from origin/main and use base main.
# If not, branch from the B7 head and open the PR with base cursor/wave-7-b7-smp-25ca;
# retarget to main after #440 merges.
git checkout -b cursor/wave-7-b21-host-cores-<suffix> origin/cursor/wave-7-b7-smp-25ca
./tools/sync-sku-site.sh --next          # expect: wave 7, pick up B21 first
cargo test --no-default-features -- --test-threads=1 2>&1 | tail -5
./tools/build.sh && ./tools/check-size.sh
ls -l /dev/kvm && grep -cw vmx /proc/cpuinfo && nproc    # nested KVM; nproc >= 6 wanted
```

Create `/tmp/nest/run.sh` (not in the repo; do not commit it) and `chmod +x` it. It
is the B7 helper unchanged:

```bash
#!/usr/bin/env bash
# usage: run.sh lab|product NAME [stop-regex] [stop-count] [timeout]
set -uo pipefail
cd /agent/repos/raynu
mode="$1"; name="$2"; pat="${3:-}"; cnt="${4:-1}"; tmo="${5:-900}"
log=/tmp/nest/$name.log
rm -rf /tmp/nest/esp-$name; rm -f "$log"; : > "$log"
if [[ "$mode" == lab ]]; then
  env RAYNU_F=1 QEMU_ACCEL=kvm ESP=/tmp/nest/esp-$name SERIAL_CHARDEV=file:$log ${EXTRA_ENV:-} \
    timeout --signal=KILL "$tmo" ./tools/run-qemu.sh >/tmp/nest/$name.out 2>/tmp/nest/$name.err &
else
  env PRODUCT_ISO=$PWD/target/alpine-extended-3.21.3-x86_64.iso RAYNU_F=1 QEMU_ACCEL=kvm \
    QEMU_MEM="${QEMU_MEM:-4096M}" ESP=/tmp/nest/esp-$name SERIAL_CHARDEV=file:$log ${EXTRA_ENV:-} \
    timeout --signal=KILL "$tmo" ./tools/run-qemu.sh >/tmp/nest/$name.out 2>/tmp/nest/$name.err &
fi
wrap=$!; start=$(date +%s)
while kill -0 $wrap 2>/dev/null; do
  if [[ -n "$pat" ]]; then
    n=$(grep -c -E "$pat" "$log" 2>/dev/null || true)
    if (( n >= cnt )); then sleep 3; break; fi
  fi
  sleep 2
done
for p in $(pgrep -P $wrap 2>/dev/null); do
  for q in $(pgrep -P $p 2>/dev/null); do kill -KILL $q 2>/dev/null; done
  kill -KILL $p 2>/dev/null
done
kill -KILL $wrap 2>/dev/null; wait $wrap 2>/dev/null
echo "run $name: $(( $(date +%s) - start ))s lines=$(wc -l <"$log")"
```

Run the B7 baselines on the untouched branch head and keep the logs. They are what
"unchanged" means:

```bash
/tmp/nest/run.sh lab base-lab "RAYNU-V-RAYNU-F-DISK-BOOT-OK|RayNu-F stop" 3 600
QEMU_MEM=4096M /tmp/nest/run.sh product base-prod "localhost:~# cat /proc/cmdline" 1 600
EXTRA_ENV="TWO_GUESTS=1" QEMU_MEM=8192M /tmp/nest/run.sh product base-two "guest\(s\) still on|is started again" 2 900
EXTRA_ENV="VCPUS=2" QEMU_MEM=4096M /tmp/nest/run.sh product base-smp2 "^2\r?$" 2 900
EXTRA_ENV="TWO_GUESTS=1 VCPUS=2" QEMU_MEM=8192M /tmp/nest/run.sh product base-two-smp "guest\(s\) still on|is started again" 2 1200
```

Nested note: the monitor lock is a ticket lock. If the test machine has fewer idle
CPUs than `HOST_SMP`, the outer host can deschedule the core whose turn it is and
everything waits. Keep `HOST_SMP` ≤ `nproc − 1`.

No commit.

### WP1. ADR-024 and the catalog

1. `docs/adr/ADR-024.md`: "Guest vCPUs on host cores: one monitor lock, a run queue
   per core". Sections: Context (B7, the 225 statics, the global register image),
   Benchmark (the table in section 0), Decision (D1–D17 as a table), Deviations from
   the ADR-023 table (one lock not per-guest locks; shared GDT/TSS; placement not
   rebalanced; no co-stop; timers still on vCPU 0), Known limits (section 11 of this
   plan), Consequences, References. Status: Accepted, not built.
2. `docs/adr/ADR-023.md`: one line under "What B21 changes": "Built as ADR-024. Where
   this table and ADR-024 differ, ADR-024 is the decision." Do not rewrite the table.
3. `CLAUDE.md` ADR table: add the ADR-024 row.
4. `docs/adr/ADR-021.md`:
   - B21: Status `in flight`, Ref `ADR-024`, Note names the branch.
   - New row **B22**: "Hypervisor work runs on more than one host core at a time
     (per-guest locks in place of the monitor lock); vCPUs rebalance across cores; an
     idle vCPU's core sleeps" · `planned` · `Final` · Ref `ADR-024` · Note "B21 runs
     guests in parallel and hypervisor work one core at a time; this row removes that
     limit, with the lock-wait numbers B21 prints."
   - Put `B22` in wave 11 (after `B17`). Total rows +1. The S1 count stays 68.
5. `./tools/sync-sku-site.sh` and `--check`. Stage `site/sku.json`.

Commit: `docs(adr-024): vCPUs on host cores; B21 in flight; B22 planned (Final, wave 11)`.

### WP2. `arch/host_smp.rs`: topology, memory, flags (no core is started)

New files `arch/host_smp.rs` and `arch/host_smp_test.rs`; add to `arch/mod.rs`.
Header: Pillar [D] [Z], Proven Core **outside**, ADR-024.

Constants:

```rust
pub const MAX_CORES: usize = 16;          // == slot::MAX_UNITS
pub const NO_CORE: u8 = 0xFF;
pub const KICK_VECTOR: u8 = 0xF2;
pub const BLOCK_PAGES: usize = 512;       // 2 MiB per non-zero core
// Block layout, low to high: [0,4K) VMXON region · [4K,1 MiB) canary 0x5A · [1 MiB,2 MiB) host stack
```

Pure functions (host-tested, no hardware):

```rust
pub struct CpuInfo { pub apic_id: u32, pub package: u32, pub core: u32, pub thread: u32,
                     pub enabled: bool, pub healthy: bool, pub bsp: bool }
/// One thread per (package, core). The BSP's core is first and its sibling is skipped.
/// Disabled or unhealthy CPUs are skipped. In xAPIC mode an APIC ID above 254 is skipped.
/// At most MAX_CORES. Returns the APIC IDs in core-index order (index 0 = BSP).
pub fn select_cores(cpus: &[CpuInfo], x2apic: bool, out: &mut [u32; MAX_CORES]) -> usize;
/// Core index from a stack pointer: 0 unless rsp is inside the block array.
pub fn core_of_rsp(rsp: u64, ap_base: u64, n_ap: usize) -> usize;
pub fn stack_top(core: usize, ap_base: u64, core0_top: u64) -> u64;   // (block end) & !0xF
pub fn cpu_cap(max_vcpus: u8, cores_online: usize, onecore_flag: bool) -> u8;
/// Fewest units wins; on a tie a non-zero core wins over core 0, then the lowest index.
pub fn pick_core(loads: &[u8], online: usize) -> usize;
```

`pub fn this_core() -> usize`: on UEFI read RSP and call `core_of_rsp`; on host tests
return a thread-local override, default 0. Core 0 runs on three different stacks (the
firmware's, `HOST_RSP_SHARED`, E4), none inside the block array, so "not in a block"
is core 0. No VMCS field and no MSR is involved, so it cannot be wrong after a move.

`pub unsafe fn probe_pre_ebs()` (UEFI only), called in `main` right after
`probe_host_nic_lab_flag()`:

1. `uefi::boot::get_handle_for_protocol::<uefi::proto::pi::mp::MpServices>()` then
   `open_protocol_exclusive`. `get_number_of_processors()`, then
   `get_processor_info(i)` for each: `processor_id` (APIC ID), `status_flag`
   (BSP / ENABLED / HEALTH bits), `location.{package, core, thread}`. Check the names
   against the vendored `uefi` 0.33 source. If the wrapper is missing, the protocol
   GUID is `3fdda605-a76e-4f46-ad29-12f4531b3d08`. Any error ⇒ one core.
2. `select_cores`. If `onecore.txt` is present ⇒ one core.
3. Refuse (one core, with the reason) when CR4.LA57 is set or `tsc_hz()` is 0.
4. If more than one core: allocate with `boot::allocate_pages`, type `LOADER_DATA`:
   - `AllocateType::MaxAddress(0x9_F000)`, 4 pages: the low block (page 0 = trampoline,
     pages 1–3 = boot page tables). If the address returned is 0, allocate again and
     free the first.
   - `AllocateType::AnyPages`, `(n − 1) * BLOCK_PAGES` pages: the block array. Zero
     it, fill each canary with `0x5A`.
   If either fails ⇒ one core.
5. Record in statics (written here, before any other core exists): `AP_BASE`, `N_AP`,
   `APIC_IDS[]`, `LOW_BASE`, `X2APIC`, the counts for the summary line.
6. One COM2 line:
   `boot: B21 host cpus logical=<L> enabled=<E> cores=<C> packages=<P> threads_skipped=<T> x2apic=<0|1> use=<n> (ADR-024)`.

Flags, in `boot/raynu_f_flag.rs` `probe` beside the `vcpuN.txt` reads:
`EFI\RayNu\onecore.txt` and `EFI\RayNu\nokick.txt`, each with a note line and a getter
(`host_onecore()`, `host_nokick()`). `tools/run-qemu.sh`: `ONECORE=1` and `NOKICK=1`
stage them (remove stale files first, as the `vcpuN.txt` block does);
`HOST_SMP=<n or full -smp string>` adds `-smp "$HOST_SMP"`.

Host tests: `select_cores` (16 threads on 8 cores gives 8; BSP not thread 0; a disabled
CPU; 40 cores capped at 16; xAPIC with ID 300), `core_of_rsp` at every block edge,
`stack_top` alignment, `cpu_cap`, `pick_core` ties.

Commit: `feat(arch): host core topology from MP Services, per-core blocks (B21)`.

### WP3. Start the cores: trampoline, long mode, VMXON, sleep

Still in `arch/host_smp.rs`, plus `arch/host_smp_tramp.rs` for the assembly.

**Trampoline** (`core::arch::global_asm!`, copied to `LOW_BASE`). COFF has no 16-bit
relocations: use only label differences, and patch absolute values into data slots
when copying. Skeleton (Intel syntax; end the block with `.code64`):

```
raynu_ap_tramp_start:
.code16
    cli
    cld
    xor  ebx, ebx
    mov  bx, cs
    shl  ebx, 4                         ; ebx = linear address of this page
    mov  ax, cs
    mov  ds, ax
    lgdt [T_GDTR - raynu_ap_tramp_start] ; base is below 1 MiB, 24 bits are enough
    mov  eax, cr0
    or   eax, 1
    mov  cr0, eax
    .byte 0x66, 0xEA                    ; jmp far 0x08:tramp32
T_JMP32: .long 0                        ; patched: LOW_BASE + (tramp32 - start)
    .word 0x08
.code32
tramp32:
    mov  ax, 0x10
    mov  ds, ax
    mov  es, ax
    mov  ss, ax
    mov  eax, 0x20                      ; CR4.PAE only; the rest is set in Rust
    mov  cr4, eax
    mov  eax, [ebx + T_CR3 - raynu_ap_tramp_start]
    mov  cr3, eax                       ; boot PML4, below 1 MiB
    mov  ecx, 0xC0000080
    rdmsr
    or   eax, [ebx + T_EFER - raynu_ap_tramp_start]   ; LME, and NXE when core 0 has it
    wrmsr
    mov  eax, cr0
    or   eax, 0x80000001                ; PG | PE
    mov  cr0, eax
    .byte 0xEA                          ; jmp far 0x18:tramp64
T_JMP64: .long 0                        ; patched: LOW_BASE + (tramp64 - start)
    .word 0x18
.code64
tramp64:
    mov  rsp, [rip + T_STACK]
    mov  rdi, [rip + T_CORE]
    mov  rax, [rip + T_ENTRY]
    jmp  rax
.balign 8
T_GDT:   .quad 0
         .quad 0x00CF9A000000FFFF       ; 0x08 code32
         .quad 0x00CF92000000FFFF       ; 0x10 data
         .quad 0x00AF9A000000FFFF       ; 0x18 code64
T_GDTR:  .word 31
         .long 0                        ; patched: LOW_BASE + (T_GDT - start)
.balign 8
T_CR3:   .long 0
T_EFER:  .long 0
T_STACK: .quad 0
T_CORE:  .quad 0
T_ENTRY: .quad 0
raynu_ap_tramp_end:
```

Export each slot's offset as a label difference. A host test checks: the blob is
under 1024 bytes, byte 0 is `0xFA`, every slot offset is inside the blob. After the
first build, disassemble the copied bytes once
(`objdump -D -b binary -m i8086` / `i386` / `i386:x86-64` on the three parts) and
keep the listing in `/opt/cursor/artifacts/b21-trampoline.txt`.

**Boot page tables.** The trampoline cannot load a 64-bit CR3 in 32-bit mode, and the
firmware may map low memory no-execute. Clone the path to the first 2 MiB and share
everything else with core 0:

```rust
/// `read(pa)` returns the 512 entries of core 0's table at `pa`. None when PML4[0] is absent,
/// or PDPT[0] is a 1 GiB page whose base is not 0.
pub fn build_boot_tables(bsp_cr3: u64, pdpt_pa: u64, pd_pa: u64,
    read: impl Fn(u64) -> [u64; 512]) -> Option<([u64; 512], [u64; 512], [u64; 512])>;
// pml4 = copy of core 0's PML4;          pml4[0] = pdpt_pa | P | RW
// pdpt = copy of core 0's PDPT for [0,512 GiB);  pdpt[0] = pd_pa | P | RW
// pd   = copy of core 0's PD for [0,1 GiB) (or 512 identity 2 MiB entries when that was a 1 GiB page)
// pd[0] = 0 | P | RW | PS                 // [0, 2 MiB) identity, executable
```

Bit 63 is clear on the three replaced entries. Host-test it with a fake `read`.

**Entry stub** in the image (naked): `mov rax, [rip + BSP_CR3]; mov cr3, rax;
xor ebp, ebp; call {ap_main}`. The switch to core 0's own CR3 happens here, executing
from the image, which both sets of tables map the same way.

**`extern "C" fn ap_main(core: usize) -> !`**. No lock, no prints, no shared statics.
Each step that can fail stores a code in `AP_STATE[core]` and spins forever:

1. `AP_STATE[core] = STARTED`.
2. `lidt` core 0's IDTR (captured with `cpu::sidt()` on core 0).
3. `write_cr0(bsp_cr0)`; `write_cr4(bsp_cr4 & !CR4_VMXE)`; `wrmsr(EFER, bsp_efer)`;
   `wrmsr(IA32_PAT, bsp_pat)`; `xsetbv(0, bsp_xcr0)`; `fninit`.
4. Host LAPIC: if core 0 is x2APIC and this core is not, set EXTD. SVR = enable |
   `0xFF`. TPR = 0. Read the APIC ID; it must equal `APIC_IDS[core]` (code `BAD_APIC`).
5. TSC check. Read `IA32_TSC_ADJUST` when CPUID.7.0:EBX[1] says it exists; it must
   equal core 0's (code `BAD_TSC`). Then the handshake: core 0 stores its TSC in
   `T0`, this core stores its TSC in `T1`, core 0 reads `T2`; require `T0 ≤ T1 ≤ T2`
   in three rounds (core 0 decides; a failure marks the core unusable).
6. `VmxLifecycle::new().enable(block_vmxon_region(core))` (code `NO_VMX` on error).
   Keep the `VmxLifecycle` on this core's stack for good.
7. `AP_STATE[core] = ONLINE`, then `ap_idle(core)` (below).

**`pub unsafe fn start_cores()`** on core 0, called in `run_m1_vmx` right after
`M1_VMXON_OK_MARKER` and its audit event, before `run_m2_ept_launch`. Nothing to do
when `N_AP == 0`. Capture core 0's CR0, CR3, CR4, EFER, PAT, XCR0, IDTR,
`IA32_TSC_ADJUST`. Build and write the boot tables, copy and patch the trampoline.
Then **one core at a time**:

- patch `T_STACK = stack_top(core)`, `T_CORE = core`;
- ICR: INIT (`0x4500`), wait 10 ms; SIPI (`0x4600 | (LOW_BASE >> 12)`), wait 200 µs;
  SIPI again. x2APIC: `wrmsr(0x830, (apic_id << 32) | cmd)`. xAPIC: write `0x310`
  then `0x300`, then wait for delivery-status bit 12 to clear (bounded).
  Waits are TSC loops on `tsc_hz()`;
- wait up to 100 ms for `STARTED`, then up to 200 ms for `ONLINE` or a failure code;
- print `boot: B21 host core <i> apic=0x<id> online (VMXON ok)` or
  `boot: B21 host core <i> apic=0x<id> not used: <no answer|apic id|tsc|vmxon>`;
- on `ONLINE`, `record_event(AuditEvent::VmxEnabled { vcpu_id: i as u32 })` from core 0.

A core that fails is never used; later cores keep their index. Every wait has a
bound. Then set `CORES_ONLINE` and print
`boot: B21 host cores online=<n> of <use> (ADR-024; not ISO-INSTALL-OK)`.
With one core that line is the only B21 output of the boot.

**`ap_idle(core) -> !`** (a core with no unit). It does not hold the monitor:

```rust
loop {
    while MAILBOX[core].load(Acquire) == 0 {
        if HAS_MWAIT { monitor(&MAILBOX[core]); if MAILBOX[core].load(Acquire) == 0 { mwait(0, 0) } }
        else { core::hint::spin_loop() }
    }
    // WP5 fills this in: take the monitor, pick an assigned unit, switch_in, enter_guest.
    MAILBOX[core].store(0, Release);
}
```

`MAILBOX` entries are `#[repr(align(64))]` so a store to one does not wake another
core. `HAS_MWAIT` = CPUID.01H:ECX[3]. In gate 1 nothing ever writes a mailbox.

Nested: `HOST_SMP=4` on `lab`, `product` and `TWO_GUESTS=1 VCPUS=2`. Expect four
`B21 host core` lines, `online=4 of 4`, and everything else equal to the baselines.
Also `HOST_SMP=4,sockets=1,cores=2,threads=2` (expect `threads_skipped=2`,
`online=2`), `ONECORE=1 HOST_SMP=4` (expect `online=1`), and
`./tools/run-qemu.sh -overcommit cpu-pm=on` once to run the MWAIT path.

Commit: `feat(arch): start host cores, VMXON on each, sleep until used (B21)`.

### WP4. The monitor lock and the per-core context

`arch/host_smp.rs`:

```rust
/// Ticket lock. Core 0 holds ticket 0 from the first instruction of the boot.
pub static MON_NEXT: AtomicU32 = AtomicU32::new(1);
pub static MON_OWNER: AtomicU32 = AtomicU32::new(0);
/// TSC at the last VM-exit landing, written under the lock by the landing stub.
pub static LAND_TSC: AtomicU64 = AtomicU64::new(0);
/// Core inside the monitor, NO_CORE when none. Checked, not relied on.
static MON_HOLDER: AtomicU8 = AtomicU8::new(0);

pub fn acquire() { let t = MON_NEXT.fetch_add(1, AcqRel); while MON_OWNER.load(Acquire) != t { spin_loop() } }
pub fn release() { MON_OWNER.fetch_add(1, Release); }
```

Host test with real threads: 8 threads, 100,000 `acquire`/`release` each around a
plain counter; the total is exact and `MON_NEXT − MON_OWNER == 1` at the end.

**Landing stub.** Put this before the existing first line of
`guest_uefi_vmexit_landing`; change nothing after it:

```
"push rax",
"push rdx",
"rdtsc",
"shl rdx, 32",
"or rax, rdx",
"push rax",
"mov eax, 1",
"lock xadd dword ptr [rip + {mon_next}], eax",
"2:",
"cmp dword ptr [rip + {mon_owner}], eax",
"je 3f",
"pause",
"jmp 2b",
"3:",
"pop rax",
"mov [rip + {land_tsc}], rax",
"pop rdx",
"pop rax",
```

The stack is the core's own (`HOST_RSP`). Guest RFLAGS are already in the VMCS.

**Entry stubs** (`guest_uefi_vmresume`, `guest_uefi_vmlaunch`, `raynu_f_vmlaunch`).
Immediately before the `vmresume` / `vmlaunch` line, after the last load from
`SAVED_*` and `SAVED_XMM`:

```
"lock inc dword ptr [rip + {mon_owner}]",
```

and between the entry instruction and the existing `jmp {fail}`:

```
"mov eax, 1",
"lock xadd dword ptr [rip + {mon_next}], eax",
"2:",
"cmp dword ptr [rip + {mon_owner}], eax",
"je 3f",
"pause",
"jmp 2b",
"3:",
```

The failure handlers read the error with VMREAD, so losing RAX and flags is fine.
Make `host_smp::monitor_reenter()` the first line of `guest_uefi_resume_failed`,
`guest_uefi_launch_failed` and `raynu_f_launch_failed`.

**`launch_uefi`** (6005): `host_smp::monitor_leave(); host_smp::release();`
immediately before `match ops::vmlaunch()`, and
`host_smp::acquire(); host_smp::monitor_reenter();` as the first statements of both
arms.

**Leave and enter:**

```rust
/// Last Rust before a guest entry. The entry stub releases MON_OWNER.
pub unsafe fn monitor_leave() {
    let core = this_core();
    if cores_online() > 1 {
        if INVEPT_PENDING[core].swap(false, AcqRel) { ept_hw::invept_global(); }      // WP7
        CORE_UNIT[core].store(slot::unit(slot::current(), slot::current_vcpu()) as u8, Release);
        CORE_SHADOW[core] = (shadow_read(MSR_SPEC_CTRL), shadow_read(MSR_APIC_BASE));
        // Two cores on one host stack is the worst bug this row can have.
        if ops::vmread(HOST_RSP) != Ok(stack_top_of(core)) { wrong_stack(core) }       // print, hand home or leave
    }
    note_hold(core);                                // max and sum of TSC spent in the monitor
    IN_GUEST[core].store(true, Release);
    if MON_HOLDER.swap(NO_CORE, AcqRel) != core as u8 { two_in_monitor() }
}

/// First line of guest_uefi_vmexit.
pub unsafe fn monitor_enter() {
    let core = this_core();
    if MON_HOLDER.swap(core as u8, AcqRel) != NO_CORE { two_in_monitor() }
    IN_GUEST[core].store(false, Release);
    note_wait(core, cpu::rdtsc().wrapping_sub(LAND_TSC.load(Acquire)));
    if cores_online() == 1 { return; }
    let u = CORE_UNIT[core].load(Acquire) as usize;
    slot::set_current(slot::unit_slot(u));
    slot::set_current_vcpu(slot::unit_vcpu(u));
    shadow_write(MSR_SPEC_CTRL, CORE_SHADOW[core].0);
    shadow_write(MSR_APIC_BASE, CORE_SHADOW[core].1);
    if INVEPT_PENDING[core].swap(false, AcqRel) { ept_hw::invept_global(); }           // WP7
    if ops::vmptrst() != Ok(saved_vmcs_of(u)) { wrong_vmcs(core, u) }                  // print and stop this core
}
```

`monitor_reenter()` is `monitor_enter()` without the wait note. `two_in_monitor()`
prints one line with `write_line_nowait` and spins forever holding what it holds.
`MON_HOLDER` starts as 0 (core 0 is in the monitor from boot).

Call sites: `monitor_enter()` as the **first** statement of `guest_uefi_vmexit`
(before `LAUNCH_ENTERED.store`); `monitor_leave()` as the first statement of
`enter_guest()`, and before `raynu_f_vmlaunch()` at 8711.

`vmx/launch.rs` is not touched: its lab guests neither release nor take, so core 0
simply keeps the monitor while it runs them. No other core has a unit then.

**Kick vector EOI.** At the top of `guest_uefi_vmexit`, after `monitor_enter()`:
when the basic reason is external interrupt and `VM_EXIT_INTR_INFO` is valid with
vector `KICK_VECTOR`, call `crate::arch::apic::eoi()` and count it. Nothing else
changes for that exit; both existing arms already resume.

**Per-core slice start.** In `vmx/guest_sched.rs` make `SLICE_START` a small type
with `load` and `store` that index `[AtomicU64; MAX_CORES]` by `this_core()`. The
call sites keep their text.

**Bank comments.** In `guest/slot.rs` add to the `PerGuestCell` / `PerVcpuCell`
safety text: "With more than one host core, the writer is the core that holds the
monitor lock (ADR-024)." Add; do not replace the B7 sentence.

**Status line.** Next to `beat()` in `guest_sched.rs`, core 0 only, every `BEAT_US`
while `cores_online() > 1` and a guest is on:
`boot: B21 cores 0:[0.0 1.0] 1:[] 2:[] lock_wait_max_us=<n> hold_max_us=<n> kicks=<sent>/<taken> recalls=<n>`
(`s.v` per unit assigned to the core), `write_*_nowait`.

**Panic.** In `mgmt/host_panic.rs` add the comment that the panicking core keeps the
monitor (D16) and confirm the path calls nothing that yields.

Source-text tests (their style): the landing contains `lock xadd` before
`slot_rax`; each entry stub has `lock inc` directly before its entry instruction;
`guest_uefi_vmexit` starts with `monitor_enter()`; `enter_guest` starts with
`monitor_leave()`.

Nested: all WP0 baselines with `HOST_SMP` unset and with `HOST_SMP=4`. Milestones
equal the baselines. This is the gate 1 build.

Commit: `feat(vmx): one monitor lock around VMX root, per-core context (ADR-024)`.

**→ Gate 1** (sections 6–9).

### WP5. A run queue per core

State in `arch/host_smp.rs` (written by the monitor holder):

```rust
static ASSIGNED: [AtomicU8; MAX_UNITS]    // core whose queue holds the unit; 0 by default
static HOME:     [AtomicU8; MAX_UNITS]    // core where its VMCS is active; 0 by default; NO_CORE when cleared for a move
static RECALL:   [AtomicBool; MAX_UNITS]  // core 0 wants this unit back
static WAKE_CORE:[AtomicU8; MAX_CORES]    // something changed for this core (cache-line aligned)
static HOME_REQ: [AtomicU8; MAX_GUESTS]   // 0 none, 1 Leave, 2 Reset: work core 0 must do for the slot
```

The defaults (assigned 0, home 0) are exactly B7: every VMCS is built and run on
core 0.

Invariants. Put them in the module doc and assert them where cheap:

- I1. A core switches in unit `u` only when `ASSIGNED[u] == core` and
  `HOME[u] ∈ {core, NO_CORE}`.
- I2. Only core `HOME[u]` executes VMPTRLD, VMREAD, VMWRITE or VMCLEAR on `u`'s VMCS.
- I3. `HOME[u]` changes only by: its home core VMCLEARs it and stores `NO_CORE`; a
  core that switches it in stores itself.
- I4. Core 0 has at least one assigned, started unit whenever a slot is on.
- I5. A slot with `HOME_REQ != 0` is not picked by any core.

**5a. Scheduler filter** (`guest_sched.rs`). First lines of `other(u, now)`:

```rust
if host_smp::assigned(u) != host_smp::this_core() || host_smp::home_req_pending(slot::unit_slot(u)) {
    return Other::default();
}
```

`clamp_ticks` and the `!slot::cpu_shared()` test in `resume` stay as they are: with
two or more units on anywhere, every vCPU keeps the lived 2 ms preemption bound.
That bound is what makes D7 true.

**5b. `switch_in`.** After the successful `vmptrld`, add `host_smp::on_loaded(next)`:

```rust
if cores_online() > 1 {
    let me = this_core();
    // I1 check; a violation prints and returns false from switch_in
    if HOME[next] != me { HOME[next] = me; guest_uefi::mark_needs_launch(s, v); }  // sets AP_NEEDS_LAUNCH
    ops::vmwrite(HOST_RSP, stack_top_of(me));
}
```

`AP_NEEDS_LAUNCH` already makes `enter_guest` use VMLAUNCH. It works for vCPU 0 too.

**5c. `yield_monitor_until`** in `guest_sched.rs` `mod hw` (it needs `saved_gprs`):

```rust
/// Give the monitor to other cores until `done()`, then take it back with this core's
/// context as it was. The only place a handler drops the lock.
pub(crate) unsafe fn yield_monitor_until(mut done: impl FnMut() -> bool) {
    let s = slot::current(); let v = slot::current_vcpu();
    let gpr = saved_gprs(); let xmm = saved_xmm();
    let spec = shadow_read(MSR_SPEC_CTRL); let apic = shadow_read(MSR_APIC_BASE);
    host_smp::holder_clear(); host_smp::release();
    while !done() { core::hint::spin_loop(); }
    host_smp::acquire(); host_smp::holder_set();
    slot::set_current(s); slot::set_current_vcpu(v);
    load_saved_gprs(&gpr); load_saved_xmm(&xmm);
    shadow_write(MSR_SPEC_CTRL, spec); shadow_write(MSR_APIC_BASE, apic);
    host_smp::take_pending_invept();
}
```

If a wait passes 2 s, print once
`boot: B21 core <n> waited 2 s for core <m> (ADR-024; not ISO-INSTALL-OK)` when the
monitor is next held, and keep waiting. Do not proceed without the answer.

**5d. Non-zero cores.** First lines of `resume(idle)`:
`let core = host_smp::this_core(); if core != 0 { ap_resume(core, idle) }`.

```rust
unsafe fn ap_resume(core: usize, idle: bool) -> ! {
    let mut parked = false;
    loop {
        let now = cpu::rdtsc(); let cu = cur_unit();
        if serve_recalls(core, cu, idle, now) { ap_next(core) }      // cu was given back
        match pick(cu, idle, used, quantum, starve, |u| other(u, now)) {
            Pick::Switch(next) => {
                switch_out(cu, idle, now);
                if !switch_in(next, now) { ap_hand_home(HomeReq::Leave) }
                super::super::enter_guest()
            }
            Pick::Stay if idle && !unit_has_work(cu, now) => {
                // Every unit here is halted. Wait outside the monitor.
                let until = now + us_to_tsc(PARK_POLL_US, hz, fb);   // PARK_POLL_US = 200
                yield_monitor_until(|| host_smp::woke(core) || cpu::rdtsc() >= until);
                parked = true;
                continue;
            }
            Pick::Stay => {
                if parked { super::super::arm_wake_exit() } else { super::super::clamp_firmware_preempt() }
                super::super::enter_guest()
            }
        }
    }
}
```

`unit_has_work(u, now)` is the expression `other()` already uses:
`take_wake_vcpu(s, v) || with_vcpu(s, v, || (v == 0 && stop_pending()) || guest_irq_waiting(now))`.
A unit woken from a park must not wait a quantum for its interrupt. The injection
lives in the resume tail of `guest_uefi_vmexit`, which a park wake does not pass
through. So the wake entry is: `PREEMPT_UART_PACED.store(true)`, write
`VMX_PREEMPTION_TIMER_VALUE = 1`, `enter_guest()`. The guest exits at once, and that
exit runs the whole lived path and injects. A paced exit steps no HPET and raises no
PIT (ADR-022), so this is safe on vCPU 0 too. Never write an unpaced 1 there; it
would step the platform timers. Put it in one helper in `guest_uefi.rs`
(`arm_wake_exit()`) and use the same helper after a `switch_in` of a halted unit
that has work, if `rearm_preempt_on_switch_in` leaves it a full quantum (read it;
B7 accepted up to 2 ms there, B21 should not).

`serve_recalls`: for every `u` with `RECALL[u]` and `HOME[u] == core`: if `u == cu`,
`switch_out(cu, idle, now)`; `vmclear`; `HOME[u] = NO_CORE`; `ASSIGNED[u] = 0`;
`RECALL[u] = false`. Returns whether `cu` went.

`ap_next(core) -> !`: the first assigned unit that is on and up (one with work
first); `switch_in`, `enter_guest`. None ⇒ `host_smp::holder_clear(); release();
ap_idle(core)`.

`ap_idle` (WP3): after the mailbox wakes it, `acquire()`, `holder_set()`, print
`boot: B21 vcpu <s>.<v> on host core <c>` (nowait) the first time a unit lands on
the core, then `ap_next(core)`.

**5e. Placement.** `host_smp::place(u)` on core 0, called at the end of `ap_start`
after `slot::set_vcpu_up`:

```rust
if cores_online() == 1 { return; }
let c = pick_core(&loads(), cores_online());      // loads[c] = started units of on slots assigned to c
if c != 0 {
    ops::vmclear(vmcs_of(u));                      // ap_start left it active on core 0
    HOME[u] = NO_CORE; ASSIGNED[u] = c; poke_core(c);
}
```

`poke_core(c)`: `WAKE_CORE[c] = 1`, `MAILBOX[c] = 1` (the kick is added in WP6).

**5f. Offload at a core-0 switch.** In `resume`, in `Pick::Switch(next)`, directly
after `switch_out(cu, idle, now);` add `host_smp::offload(cu);`: when `cu` is
eligible and the best non-zero core `c` has `loads[c] + 1 < loads[0]`: VMCLEAR,
`HOME = NO_CORE`, `ASSIGNED = c`, `poke_core(c)`. Eligible in this stage: vCPU
index ≥ 1.

**5g. Core-0-only gates.**

- `smp_service()`: first line `if host_smp::this_core() != 0 { return; }`.
- `maybe_tick_standing_spa()` (`mgmt/host_nic_coexist.rs` 38): the same first line
  (UEFI cfg).
- `ap_init(v)`: before its `vmclear`, `host_smp::recall_unit(slot::unit(s, v))`;
  after it, reset assigned and home to 0.
- `smp_collapse()`: first line `host_smp::recall_slot(slot::current())` (before the
  early return); last line resets assigned and home to 0 for the slot's vCPUs ≥ 1.
- `on_guest_left()`: before the lived `switch_in(slot::unit(next, …current_vcpu…))`,
  call `host_smp::home_unit_for(next)`. It sets `CUR_VCPU[next]` to a started vCPU of
  `next` that is assigned to core 0. In this stage vCPU 0 always is.

`recall_unit(u)` / `recall_slot(s)` (core 0 only, `assert_core0`): for each unit with
`HOME ∉ {0, NO_CORE}`: `RECALL = true`, `poke_core(home)`. For each unit assigned
away with `HOME == NO_CORE` (placed, never loaded): `ASSIGNED = 0`. Then
`yield_monitor_until(|| every recalled unit has HOME == NO_CORE)`.

**5h. Leave and reset from another core.** First line of `leave_to_e4()`:
`if host_smp::this_core() != 0 { host_smp::ap_hand_home(HomeReq::Leave) }`.
First line of `raynu_f_reset_relaunch(src)`: the same with `HomeReq::Reset`; store
`src` in a per-guest cell for core 0.

`ap_hand_home(req) -> !`: `HOME_REQ[s] = max(HOME_REQ[s], req)` with Leave above
Reset; `switch_out(cu, false, now)`; VMCLEAR; `HOME = NO_CORE`; `ASSIGNED = 0`;
`WAKE_CORE[0] = 1`; `ap_next(core)`.

On core 0, in `resume`, after the `core != 0` branch and before the pending-start
test:

```rust
if let Some((s, req)) = host_smp::take_home_req() {
    let now = cpu::rdtsc();
    switch_out(cu, idle, now);
    host_smp::recall_slot(s);
    if !switch_in(slot::unit(s, 0), now) { /* print; fall through to leave */ }
    match req { HomeReq::Leave => super::super::leave_to_e4(),
                HomeReq::Reset => super::super::raynu_f_reset_relaunch(reset_src(s)) }
}
```

Both then run exactly the lived code, on core 0, with that slot current.

`raynu_f_stop`, `b4_service` and `take_guest_stop` need no change: on another core
they run under the monitor up to `leave_to_e4` and are caught there.

Host tests: `pick_core` with loads; a table-driven model of
assigned/home/recall transitions that checks I1–I3 after every step; `other()`
returns default for a unit assigned elsewhere; source-text checks for every first
line named above.

Commit: `feat(sched): a run queue per host core; vCPUs 1..N placed on other cores (B21)`.

### WP6. The kick

`arch/apic.rs`: `pub unsafe fn send_ipi(apic_id: u32, vector: u8)` (x2APIC: MSR
`0x830`; xAPIC: `0x310` then `0x300`, wait for bit 12).

`host_smp::poke_core(c)` gains: `if c != this_core() && IN_GUEST[c] && !host_nokick()
{ send_ipi(APIC_IDS[c], KICK_VECTOR); KICKS_SENT += 1 }`. Add
`host_smp::poke_unit(u) = poke_core(ASSIGNED[u])`.

Hooks (UEFI cfg, no-ops when one core is online):

- `guest/slot.rs` `wake_vcpu(slot, v)`: after the `fetch_or`, `poke_unit(unit(slot, v))`.
- `guest/slot.rs` `wake(slot)`: after the `fetch_or`, `poke_unit` for each started vCPU.
- `devices/guest_irq.rs`, where an IOAPIC pin becomes pending (`raise_ioapic_gsi`
  and the functions that end there): find the target vCPU with the same
  `lapic_virt::pick_target` call `deliverable` uses; when it is not the current vCPU,
  `poke_unit`.

A kick that arrives while the target is in VMX root stays pending (host IF is 0) and
becomes one extra exit at its next entry. The EOI in WP4 handles it.

Known edge, record in ADR-024: a kick left pending on core 0 after the last guest
stops is taken by the next guest entry on core 0, which is always a guest-UEFI entry
on the product path.

Commit: `feat(arch): host IPI kick for a vCPU on another core (B21)`.

### WP7. INVEPT on every core

`host_smp::invept_everywhere()`:

```rust
ept_hw::invept_global();
if cores_online() == 1 { return; }
let me = this_core(); let mut wait = false;
for c in online cores except me { INVEPT_PENDING[c] = true; if IN_GUEST[c] { poke_core(c); wait = true; } }
if wait { guest_sched::yield_monitor_until(|| no core has IN_GUEST && INVEPT_PENDING) }
```

A core takes its pending flush in `monitor_enter`, `monitor_leave` and after a yield
(WP4, WP5), so a core that is not in a guest flushes before it next enters one.

Replace the eight `ept_hw::invept_global()` calls in `vmx/guest_uefi.rs` with
`host_smp::invept_everywhere()`. If a source gate wants the old string at a site,
keep the old call and add the new one after it. Leave `vmx/launch.rs` alone.

A mapping that goes from absent to present needs no remote flush, but do not try to
tell the cases apart. Flush everywhere at all eight.

Commit: `feat(ept): an EPT change is flushed on every host core (B21)`.

### WP8. Guest memory: single fetch (D17)

Before any vCPU runs beside another. Review every hypervisor read of guest memory
that decides something on the host side:

```bash
rg -n "read_unaligned|read_volatile|from_raw_parts|\*\(.* as \*const" devices/guest_virtio_blk.rs \
  devices/guest_virtio_net.rs devices/guest_virtio_console.rs devices/ide_cdrom.rs devices/guest_msix.rs
rg -n "fn read_guest_pte|dump_low_ram_insn|fn .*decode|guest_uefi_gpa_to_hpa" vmx/guest_uefi.rs
```

For each: (a) ring indices, descriptor `addr`/`len`/`flags`/`next`, request headers
and page-table entries are read once into a local, checked on the local, used from
the local; (b) descriptor chains have a bound on length; (c) instruction bytes for
MMIO decode are copied once (at most 15 bytes) and decoded from the copy; (d) no
`&[u8]` over guest memory is validated and then read again for a decision. Payload
copies are fine.

Fix what fails the rule. Write the list of sites reviewed, and each fix, into
ADR-024 ("Guest memory review"). If nothing needed a fix, the list still goes in.

Commit: `fix(devices): single fetch from guest memory before real cores (B21)`, or
`docs(adr-024): guest memory review` when there is no code change.

### WP9. The cap, the page, nested bring-up of stage 2

1. `mgmt/guest_cpu.rs`: `cpu_max()` returns
   `host_smp::cpu_cap(MAX_VCPUS, cores_online, onecore)`. `save` refuses
   `n > cpu_max()`. Add `pub fn start_refusal(spare_off) -> Option<(u8, u8)>`
   (saved count, cap) when the saved count is above the cap.
2. Accept sites (`guest_catalog.rs` 923, 963, 1051): before the choice is applied,
   on a refusal return the site's existing "not accepted" shape with
   `"error":"needs N host cores, M online"`. Autostart
   (`perc_boot_choice.rs` 574): print
   `boot: B21 autostart skipped: guest needs <N> host cores, <M> online (ADR-024)`
   and do not start it.
3. `apply_vcpus_for_start`: clamp `n` to `cpu_max()`; when it clamps (lab flag
   only), print `boot: B21 guest slot <s> asked for <N> vCPUs; <M> host cores online`.
4. `write_cpu_json`: add `"host_cores":<n>`. `docs/openapi/v1.yaml`, `mgmt/openapi.rs`.
5. `assets/webui.html` line 294: build the sentence from `host_cores`. One core:
   the existing sentence. More: "Each vCPU runs on its own host core while there are
   enough (N online)." Keep the function boundaries as they are; B7 broke the Guests
   row once by closing a loop in the wrong place. Extend `mgmt/webui_test.rs`.
6. Nested bring-up: section 6 rows 6–15 until green. This is where the time goes.
   Use the failure table in section 10.

Commits: `feat(mgmt): vCPU count capped at host cores online (B21)`, then fixes as
separate commits. This is the gate 2 build.

**→ Gate 2.**

### WP10. vCPU 0 on another core (stage 3)

1. Eligibility in `host_smp::offload`: also vCPU 0 when
   `RAYNU_F_LINUX_HANDOFF && !RAYNU_F_MODE` for its slot, no INIT or SIPI is latched
   for the slot, and no home request is pending. Offload still happens only at a
   core-0 `Pick::Switch`, so core 0 always keeps the unit it switches to (I4).
2. `devices/lapic_virt.rs`: `pub fn ap_request_pending() -> bool` (reads the INIT
   and SIPI latches of the current slot, takes nothing).
3. In the resume tail, where `smp_service()` is called: on a non-zero core, when the
   current vCPU is 0 and `ap_request_pending()`, call
   `host_smp::ap_hand_home(HomeReq::Service)`. `Service` = 3 and is the lowest
   priority. Core 0's handler does nothing for it beyond clearing it: the unit is in
   core 0's queue again, and its next exit there runs `smp_service`.
4. `host_smp::home_unit_for(next)` (from 5g): when no started vCPU of `next` is
   assigned to core 0, `recall_unit(unit(next, 0))`, then name vCPU 0. Print
   `boot: B21 vcpu <s>.0 back on host core 0`.
5. Check by reading, and add a nested row for each: SPA Stop, page Shut down
   (B4 SCI), guest `reboot`, guest `poweroff`, each for a guest whose vCPU 0 is on a
   non-zero core, both running and halted.

Nested: section 6 rows 16–19. Commit:
`feat(sched): a guest's vCPU 0 may run on another host core (B21)`. This is the
gate 3 build.

**→ Gate 3.**

### WP11. Docs, PR, CI

Section 8, at each stage. CI: in `.github/workflows/ci.yml`, in the KVM step that
already runs `./tools/qemu-boot-test.sh`, add a second run with `HOST_SMP=4` that
greps the log for `B21 host cores online=4`. It is skipped the same way when the
runner has no VMX.

---

## 6. Nested QEMU test matrix

All in tmux. Kill QEMU by PID after each. `EXTRA_ENV` is word-split on purpose.
"As baseline" means the `boot:` milestone diff from the B7 plan shows timing
differences only.

**Stage 1 (after WP4)**

| # | Run | Must show |
|---|---|---|
| 1 | The five WP0 baselines, `HOST_SMP` unset | As baseline. One B21 line: `host cores online=1 of 1`. |
| 2 | The same five with `EXTRA_ENV="HOST_SMP=4 …"` | `online=4 of 4`, four core lines. Everything else as baseline: vCPUs still share core 0. |
| 3 | `HOST_SMP=4,sockets=1,cores=2,threads=2` product | `threads_skipped=2`, `online=2`. |
| 4 | `ONECORE=1 HOST_SMP=4 VCPUS=2` product | `online=1`; as `base-smp2`. |
| 5 | Row 2's product run with `-overcommit cpu-pm=on` | Same result (MWAIT path). |

**Stage 2 (after WP9)**

| # | Run | Must show |
|---|---|---|
| 6 | Rows 1 and 4 again | Unchanged. |
| 7 | `HOST_SMP=4 VCPUS=2` product | `boot: B21 vcpu 0.1 on host core 1`. `nproc` prints 2. The parallel test below passes. |
| 8 | `HOST_SMP=4 VCPUS=4` product | vCPUs 1, 2, 3 on cores 1, 2, 3. `nproc` prints 4. |
| 9 | `HOST_SMP=4 TWO_GUESTS=1 VCPUS=2`, 8192M | `0.1` and `1.1` on two different cores. One guest powers off and is started again (recall, then placement again); the other keeps running. |
| 10 | Row 7 through the installed-disk reboot | After the reboot: a second `B7 vcpu 1 start` and a second `B21 vcpu 0.1 on host core`. |
| 11 | `HOST_SMP=2 TWO_GUESTS=1 VCPUS=2` | Four units on two cores; both guests print 2; the status line shows two units per core. |
| 12 | `HOST_SMP=2 VCPUS=4` | `asked for 4 vCPUs; 2 host cores online`; `nproc` prints 2. Unit test covers Save and Start refusal. |
| 13 | `NOKICK=1 HOST_SMP=4 VCPUS=2` | Correct, `kicks=0/0`, slower IPIs only. |
| 14 | Row 7, then `POST /host/panic` while both vCPUs are busy | The dump lines, then the reset. No hang. |
| 15 | Row 9 left running 30 minutes with the fork loop below in both guests | No `waited 2 s`, no `two cores`, no `wrong stack`, no guest stall lines. |

**Stage 3 (after WP10)**

| # | Run | Must show |
|---|---|---|
| 16 | `HOST_SMP=4 TWO_GUESTS=1` (one vCPU each) | After the second guest's hand-off: `boot: B21 vcpu 1.0 on host core <n>`. Both reach their prompt. |
| 17 | Row 16; power off the guest on core 0 | `boot: B21 vcpu 1.0 back on host core 0`; the other keeps running; the first is started again. |
| 18 | Row 16; Stop, then Shut down, then `reboot`, then `poweroff` of the guest on the other core | Each ends as on B7: the slot is off or boots again; the guest on core 0 never stalls. |
| 19 | `HOST_SMP=4 TWO_GUESTS=1 VCPUS=2` 30 minutes with both loops | As row 15. |

Parallel test in the guest (Alpine has busybox `time`, `taskset`, `sha256sum`):

```sh
time sh -c 'head -c 1500m /dev/zero | sha256sum'                       # T1
time sh -c 'taskset -c 0 sh -c "head -c 1500m /dev/zero | sha256sum" & \
            taskset -c 1 sh -c "head -c 1500m /dev/zero | sha256sum" & wait'   # T2
```

Pass: `T2 < 1.4 × T1` with real cores. On `base-smp2` (one core) `T2` is about
`2 × T1`; record both numbers.

Fork loop (reschedule IPIs and TLB shootdowns):
`i=0; while [ $i -lt 20000 ]; do /bin/true; i=$((i+1)); done &` twice, pinned to
different CPUs.

In every SMP guest's dmesg these must be absent: `TSC warp`, `Marking clocksource
'tsc' as unstable`, `APIC ID mismatch`, `failed to report alive`, `rcu: INFO:
rcu_sched detected stalls`, `watchdog: BUG: soft lockup`, `smp_call_function`
timeouts.

---

## 7. Gates before asking for each F11

1. `cargo test --no-default-features -- --test-threads=1` green. Save to
   `/opt/cursor/artifacts/b21-gate<N>-host-tests.log`.
2. `./tools/build.sh` and `./tools/check-size.sh`.
3. The stage's matrix rows green, and every earlier stage's rows again. Logs in
   `/opt/cursor/artifacts/`.
4. `./tools/sync-sku-site.sh --check`, `./tools/sync-hda-site.sh --check`,
   `./tools/sync-loihda-site.sh --check`, `./tools/check-site-chrome.sh`,
   `./tools/check-h5-docs.sh`, `./tools/check-f10-docs.sh`, `./tools/check-d11-docs.sh`,
   `./tools/check-h8-docs.sh`, `./tools/check-openapi.sh`.
5. Every CI job green on the **push** run of the stage's head.
6. `git diff --stat <base>..HEAD` shows none of the files named in section 2 as not
   edited.
7. Write `/opt/cursor/artifacts/code-tour-author-handoff.md` (`<decision>`, `<risk>`,
   `<verified>`). Do not mention it in the PR body or the summary.

---

## 8. Docs procedure (same change as the code)

While in flight (first push, after WP4):

- `docs/adr/ADR-021.md` B21: `in flight`, branch and PR, "nested: N host cores online;
  not lived". `./tools/sync-sku-site.sh`, stage `site/sku.json`.
- `docs/hda.md`: new top changelog row, this-commit delta, `last_updated`; scores held.
  `./tools/sync-hda-site.sh`, stage `site/hda.json`. The row says B22 was added as
  Final in wave 11 and that wave 7 is still the current wave.
- `docs/loihda.md`: new changelog row; scores held. `./tools/sync-loihda-site.sh`,
  stage `site/loi.json`.
- PR body: what changed; the identity rule; nested evidence; "not lived"; the
  sentence "Guests run in parallel; hypervisor work runs one core at a time
  (ADR-024); B22 removes that"; a **Proven Core** paragraph: "No Proven Core file is
  edited. `VmxLifecycle` is instantiated once per host core; `AuditEvent::VmxEnabled`
  is reused with the core index"; the known limits of section 11; the footers
  `HDA: months 0.0→0.0 · overall 99%→99% · ETA unchanged` and
  `LOIHDA: Bar A 94%→94% · Bar B 72%→72% · months unchanged`.

After gate 1 and after gate 2 pass on iron (one change each):

- Evidence doc `docs/evidence/r640/<date>-<sha8>-wave-7-b21-gate<N>.md` from
  `TEMPLATE.md`: every `boot: B21` line, the guest output, what was not run.
- B21 stays `in flight`. Note gains "gate N on iron `<sha12>` / run `<id>`: …".
- One HDA and one LOIHDA changelog row per iron boot; scores held.

After gate 3 passes, in one change:

- B21 → `lived`, Note = iron SHA12, CI run, signed EFI SHA256, the three evidence
  docs, and "guests run in parallel; hypervisor work one core at a time; B22".
- `site/index.html`, `docs/sku.md`, `docs/progress.md`, `docs/m8_plan.md`, `README.md`,
  `CLAUDE.md`: `rg -n "one host core|share one|B21|one core of capacity"` and patch
  the strings **in place**. Never rewrite `site/index.html`.
- `./tools/sync-sku-site.sh --next` and quote what it prints (wave 7 stays open: B9
  next). Do not start B9 without its own plan.
- No tag and no release unless the owner asks.

---

## 9. Iron

Rollback if a stick misbehaves: the B7 stick `505ef6bdbc73` / run `44336344309`,
and behind it GitHub Latest `v0.1.7-b6`. On the stick itself, an empty file
`EFI\RayNu\onecore.txt` makes this EFI start no other core.

Flash and sign block to give the operator. Fill in the branch and the green **push**
run of the stage. Three stages, three run ids:

```bash
cd ~/projects/raynu
git fetch origin <branch>
git checkout -B <branch> origin/<branch>
./tools/flashcruzer.sh --no-git --run <RUN_ID> --require-head --allow-uefi-only --any-cruzer-usb --download-only
umask 077
openssl pkey -in ~/raynu-keys/raynu-v-db.key -out /dev/shm/raynu-v-db.key
./tools/sign-efi.sh --key /dev/shm/raynu-v-db.key --cert ~/raynu-keys/raynu-v-db.crt --in "$HOME/r640-hypervisor.efi" --out "$HOME/r640-hypervisor-signed.efi"
sbverify --cert ~/raynu-keys/raynu-v-db.crt "$HOME/r640-hypervisor-signed.efi"
shred -u /dev/shm/raynu-v-db.key
sha256sum "$HOME/r640-hypervisor.efi" "$HOME/r640-hypervisor-signed.efi"
sudo mount /dev/disk/by-label/RAYNUV /mnt
sudo cp "$HOME/r640-hypervisor-signed.efi" /mnt/EFI/BOOT/BOOTX64.EFI
sync
sudo sha256sum /mnt/EFI/BOOT/BOOTX64.EFI
sudo umount /mnt
```

`--require-head` means the stage's head must be the branch head when the operator
flashes. Do not push the next stage's commits until the operator has the stick
written, or give the run id with the commit checked out instead of the branch.

Ask the operator for the full COM2 text of each boot and the SSH output of each step.

**Gate 1** (cores online, guests as B7)

1. F11. Guests page shows `build <sha12> · CI <run>`. COM2: the `host cpus` line,
   one line per core, `host cores online=<n> of <n>`. Write the numbers down: this is
   the first time the R640's topology, APIC mode and MWAIT answer are on record.
2. Start guest 3 (Ubuntu) with 2 vCPUs. `ssh 10.99.99.200`, `nproc` prints 2. Start
   guest 2 (Alpine) beside it; `localhost login:`. The SSH session stays alive.
3. In Ubuntu `sudo reboot`; it comes back with 2 CPUs. Stop Alpine from the page.
4. Leave it 10 minutes. Read the iDRAC power figure and note it.

Pass: steps 1–3. Any core `not used` is recorded with its reason and does not fail
the gate if at least 4 cores are online.

**Gate 2** (vCPUs 1..N on other cores)

1. F11. COM2 as gate 1.
2. Start guest 3 with 2 vCPUs. COM2: `boot: B21 vcpu 0.1 on host core <n>`. Over SSH:
   ```
   nproc
   time sh -c 'head -c 3000m /dev/zero | sha256sum'
   time sh -c 'taskset -c 0 sh -c "head -c 3000m /dev/zero | sha256sum" & taskset -c 1 sh -c "head -c 3000m /dev/zero | sha256sum" & wait'
   grep -E "LOC|RES|CAL|TLB" /proc/interrupts
   sudo dmesg | grep -iE "smp|Brought up|clocksource|tsc|APIC|stall|lockup" | head -40
   date; sleep 60; date
   ```
   Expect 2; the second time under 1.4 × the first; both CPU columns counting; no
   stall or TSC line; 60 s between the dates.
3. Start guest 2 (Alpine) with 2 vCPUs beside it. COM2: `boot: B21 vcpu 1.1 on host
   core <m>`. `nproc` prints 2. Ubuntu's SSH session stays alive. Copy one
   `boot: B21 cores …` status line.
4. In Ubuntu `sudo reboot` (back with 2 CPUs, a new `B21 vcpu 0.1` line), then
   `sudo poweroff` (the slot is off), then Start again.
5. Stop Alpine from the page.
6. Set guest 3 to 4 vCPUs. Start. `nproc` prints 4; three `B21 vcpu 0.x` lines.
7. Last: the host panic button on the page while Ubuntu is busy. The dump prints and
   the host resets.

Pass: steps 2–5 and 7. Step 6 is recorded whichever way it goes.

**Gate 3** (a guest's vCPU 0 on another core)

1. F11. Set both guests to 1 vCPU. Start guest 3, then guest 2. COM2 after the
   second one's kernel is up: `boot: B21 vcpu 1.0 on host core <n>`.
2. Run the single `time sh -c 'head -c … | sha256sum'` in both at once, then in each
   alone. Each "together" time is within 1.2 × its "alone" time.
3. Shut down guest 3 from the page (the one on core 0). COM2:
   `boot: B21 vcpu 1.0 back on host core 0`. Alpine keeps answering. Start guest 3.
4. On whichever guest COM2 says is on a non-zero core: Stop from the page; Start;
   Shut down from the page; Start; `reboot` inside it; `poweroff` inside it; Start.
5. Set guest 3 to 2 vCPUs, Start, repeat gate 2 step 2.
6. Power-cycle the hypervisor. Autostart brings the guest back; the `B21 vcpu` lines
   print again.
7. Wave 0 regression as recorded in the B7 evidence doc.

B21 is lived when steps 1–5 and 7 pass. Step 6 is recorded whichever way it goes.

---

## 10. Failure signatures

| Symptom | Look at |
|---|---|
| `host core N … not used: no answer` | Trampoline. Check the patched `T_JMP32`, `T_GDTR` base, `T_CR3`, the SIPI page number. TCG with `-d int,cpu_reset` shows a triple fault. Boot tables: `pd[0]` must be an executable 2 MiB page. |
| `not used: vmxon` | `IA32_FEATURE_CONTROL` on that core, CR4 written with VMXE, the VMXON page revision. |
| `not used: tsc` | Real on a two-socket box if the firmware left `IA32_TSC_ADJUST` different. Do not use the core. Report it. |
| Core 0 stops after `B21 host cpus` | A wait without a bound, or xAPIC delivery status never clearing. |
| Reset the instant a vCPU first enters on another core | `HOST_RSP` not rewritten in `switch_in` (the `wrong stack` check should have caught it). |
| `VMLAUNCH failed insn_error=4` | VMLAUNCH on a VMCS that was not cleared: a move without VMCLEAR on the home core (I3). |
| `VMRESUME failed insn_error=5` | VMRESUME on a cleared VMCS: `mark_needs_launch` missing after a move. |
| `VMPTRLD refused` | The VMCS is still active on another core (I2), or a wrong pointer. |
| Guest stuck at `smp: Bringing up secondary CPUs` | INIT/SIPI never served (the `smp_service` gate; in stage 3, `ap_request_pending`), or the unit was placed and the core's mailbox never written. |
| Everything stops, COM2 silent | The lock. An entry path without `lock inc`, or a second release. `two cores in the monitor` prints for the second case. Check `MON_NEXT − MON_OWNER`. |
| `waited 2 s for core N` | The core never reached `serve_recalls`: it is in a path that skips `guest_sched::resume`, or it waits for something core 0 holds. |
| Guest dmesg `TSC warp` | Host TSCs differ, or a vCPU's `VMCS_TSC_OFFSET` differs from vCPU 0's. |
| Soft lockup on a vCPU while core 0 starts a guest or serves a large upload | Lock held long by core 0. Expected class; read `hold_max_us`. Over 5 s is a finding for ADR-024. |
| IPIs slow, `kicks=N/1` | The EOI for `KICK_VECTOR` is missing: the first kick is never acknowledged. |
| A storm of external-interrupt exits on one core | Something pokes it on every exit. Kicks must go only to a core that is in a guest. |
| SSH to a guest whose vCPU 0 is parked stalls | A wake without a poke: `slot::wake` on NIC delivery, or the IOAPIC raise hook. |
| Virtio MMIO misbehaves only with more than one core | A stale EPT translation: an `invept_global` site not converted. |
| Rare guest disk or network corruption only with real cores | D17. A value fetched twice from guest memory. |
| Iron only | x2APIC IDs, two packages, MWAIT, low memory mapped no-execute by the firmware (the boot tables cover it). The gate 1 COM2 lines say which. |

---

## 11. Known limits (say them in ADR-024, the PR and the B21 note)

- Hypervisor work runs on one core at a time. A long operation on core 0 (guest
  Start, RAM zeroing, a TLS handshake, PERC I/O) delays every other vCPU's exits for
  that long. B7 had the same stall for every guest; B21 does not make it worse. B22.
- Placement is decided when a vCPU comes up. After a guest leaves, units are not
  rebalanced between non-zero cores. With more units than cores, cores are shared
  with the 2 ms quantum and there is no co-stop.
- A core whose vCPUs are all halted spins outside the monitor. Core 0 always spins.
  Deeper C-states are not used.
- Hyperthread siblings are not used.
- GDT, TSS and IDT are shared; a host fault is fatal, as today.
- Platform timers are still stepped by vCPU 0's exits. B9.
- `guest_tsc()` (the virtio stall watchdog's clock) over-counts time away for a guest
  whose cores are shared.
- Not more than 16 host cores. No MADT fallback when MP Services is absent.
- Only core 0 panics are exercised by the page button.

---

## 12. Out of scope

- Per-guest or finer locks (B22). Rebalancing, co-stop, C-states (B22).
- A host IDT, host timer interrupts, per-core TSS.
- NUMA placement and pinning (B17). Hot-add (B19).
- A stable guest clock (B9). x2APIC for the guest.
- More than 4 vCPUs per guest or more than 4 guests.
- Any change to `vmx/launch.rs` lab guests.
- A tag, a release, or a merge.
