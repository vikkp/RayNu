# RayNu-V

**A commercially targeted, single-binary Type-1 hypervisor designed for formal verification from day one.**

One binary. Dell PowerEdge first (R640 / R650 / R660) — hardware we can own and test on.
Built toward VMware import, SOX-ready audit trails, and machine-checked memory isolation.

North star: *Memory isolation isn't tested. It's proved.* (roadmap — not a claim about today's binary.)

## Four Pillars

| Tag | Pillar | Priority |
|-----|--------|----------|
| **[V]** | Formally Verified Core | Long-term north star |
| **[Z]** | Zero-Config Single Binary | Near-term bet |
| **[D]** | Dell iDRAC-Native | Near-term bet |
| **[A]** | Audit-First / FinOps-Native | Medium-term bet |

Every change must advance at least one pillar. See [CLAUDE.md](CLAUDE.md) for the full governance document that controls architecture, coding standards, and the Proven Core boundary.

## Status

**M0 → M5.5 closed** — VMware import → `RAYNU-V-M5-MIGRATE-OK`. Active: **M5.7** (large-page L3) — [docs/m5_plan.md](docs/m5_plan.md). Lived: [docs/progress.md](docs/progress.md).

## Repository Layout

```
boot/      Early boot, firmware handoff
vmx/       VT-x / VMCS (Proven Core)
memory/    Frame allocator, EPT (Proven Core)
devices/   Emulated / passthrough devices
sched/     vCPU scheduling + Proven Core vCPU/IPI stubs
net/       Virtual switch
audit/     Audit ring + integrity (Proven Core)
mgmt/      Management plane
migrate/   VMware migration (ADR-007)
idrac/     iDRAC Redfish (ADR-005)
arch/      x86 / R640 helpers
docs/      Architecture notes + ADRs
tools/     Build, QEMU, size-check scripts
src/       UEFI entry + crate root
```

## Build

Requires a nightly Rust toolchain and the UEFI target:

```bash
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
rustup target add x86_64-unknown-uefi --toolchain nightly

./tools/build.sh
# → target/x86_64-unknown-uefi/release/r640-hypervisor.efi
```

## Test in QEMU

```bash
# Host prep for M1.1/M1.2 nested VT-x (once per boot; Latitude / Intel)
# Must print enable_shadow_vmcs=0 (or N). Quit QEMU first if reload fails.
sudo ./tools/enable-nested-kvm.sh

# Boot gate: M0 → M3.13 markers (requires KVM + nested VT-x for EPT/VMEXIT)
./tools/qemu-boot-test.sh

# Host verification gates (no QEMU):
cargo test --no-default-features   # includes RAYNU-V-M2-L2-OK + RAYNU-V-M3-L3-OK
./tools/verus-smoke.sh             # frozen pin → RAYNU-V-M3-VERUS-OK
./tools/verus-link-smoke.sh        # ept_model verus! → RAYNU-V-M3-L3-LINK-OK
./tools/verus-verify-smoke.sh      # true L3, no admit → RAYNU-V-M3-L3-VERIFY-OK
./tools/verus-refine-smoke.sh      # ghost↔exec refine → RAYNU-V-M3-L3-REFINE-OK
./tools/m5-idrac-smoke.sh          # Dell Tier-1 mock → RAYNU-V-M5-IDRAC-OK
./tools/verus-lpage-verify-smoke.sh # large-page L3 → RAYNU-V-M5-LPAGE-VERIFY-OK
./tools/verus-numa-smoke.sh        # NUMA ghost spec → RAYNU-V-M5-NUMA-OK
./tools/verus-alloc-refine-smoke.sh # alloc↔EPT refine → RAYNU-V-M5-ALLOC-REFINE-OK
./tools/verus-eptvio-smoke.sh      # EPT-violation exclusivity → RAYNU-V-M6-EPTVIO-OK
./tools/verus-hwpte-smoke.sh       # HW PTE bit-decode → RAYNU-V-M6-HWPTE-OK
./tools/verus-numa-l3-smoke.sh     # NUMA affinity L3 → RAYNU-V-M6-NUMA-L3-OK
./tools/verus-migrate-xfer-smoke.sh # migrate page transfer → RAYNU-V-M6-MIGRATE-XFER-OK
./tools/m6-auth-smoke.sh           # REST auth → RAYNU-V-M6-AUTH-OK
./tools/m6-pdf-smoke.sh            # PDF reports → RAYNU-V-M6-PDF-OK
./tools/m6-ha-smoke.sh             # HA failover + harden → RAYNU-V-M6-HA-OK
./tools/m6-fault-smoke.sh          # fault injection → RAYNU-V-M6-FAULT-OK
./tools/m6-soak-smoke.sh           # 72-hr soak thresholds → RAYNU-V-M6-SOAK-OK
./tools/m6-ext-smoke.sh            # external audit + spec review → RAYNU-V-M6-EXT-OK

# Interactive: COM1 on stdio (uses KVM when /dev/kvm exists)
./tools/run-qemu.sh
```

Requires `qemu-system-x86_64`, OVMF (`qemu-system-x86` + `ovmf`), and **KVM** (`sudo usermod -aG kvm $USER` then re-login).

**M1.2 nested prerequisite:** `kvm_intel.enable_shadow_vmcs=0` (the boot gate fails fast if shadow is still on). Run `sudo ./tools/enable-nested-kvm.sh` once per boot.

## Project site

GitHub Pages: [https://vikkp.github.io/RayNu/](https://vikkp.github.io/RayNu/)

Source lives in [`site/`](site/). On each push to `main`, CI publishes it to the `gh-pages` branch.

**Enable once (required):** repo **Settings → Pages → Build and deployment**

1. Source: **Deploy from a branch**
2. Branch: **`gh-pages`** / folder **`/` (root)**
3. Save

Then open https://vikkp.github.io/RayNu/ (may take a minute).

## Documentation

| Doc | Purpose |
|-----|---------|
| [CLAUDE.md](CLAUDE.md) | Governing rules for all code and reviews |
| [docs/architecture.md](docs/architecture.md) | Subsystem overview + Proven Core map |
| [docs/progress.md](docs/progress.md) | Closed gates + verification checkpoint |
| [docs/m3_plan.md](docs/m3_plan.md) | M3 Linux subgates (through first real shell) |
| [docs/m3_post_shell_plan.md](docs/m3_post_shell_plan.md) | Post-shell + true L3 + post-L3 (M3.11–M3.22) |
| [docs/m4_plan.md](docs/m4_plan.md) | M4 usable VM platform (platform spine → N-guest L3) |
| [docs/m5_plan.md](docs/m5_plan.md) | M5 operationally viable (mgmt → audit → Dell → proof) |
| [verus-version.toml](verus-version.toml) | Frozen Verus tag + commit + sha256 (ADR-008) |
| [docs/risk_register.md](docs/risk_register.md) | Full risk register (R01–R14) |
| [docs/adr/](docs/adr/) | Architecture Decision Records (ADR-001–008) |

## What This Is Not

Not a Linux+KVM distro. Not a Xen/bhyve port. Not a general-purpose OS.
Production-targeted from M0 — fewer features than ESXi, every shipped feature auditable and (for the Proven Core) designed for mathematical verification.
