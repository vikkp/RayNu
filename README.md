# RayNu-V

**The world's first formally verified bare-metal hypervisor.**

One binary. Boots in 90 seconds. Knows your Dell hardware by name.
Imports your VMware VMs in one command. Passes your SOX audit with
mathematical proof — not just test results.

Memory isolation isn't tested. It's proved.

## Four Pillars

| Tag | Pillar | Priority |
|-----|--------|----------|
| **[V]** | Formally Verified Core | Long-term north star |
| **[Z]** | Zero-Config Single Binary | Near-term bet |
| **[D]** | Dell iDRAC-Native | Near-term bet |
| **[A]** | Audit-First / FinOps-Native | Medium-term bet |

Every change must advance at least one pillar. See [CLAUDE.md](CLAUDE.md) for the full governance document that controls architecture, coding standards, and the Proven Core boundary.

## Status

**M0 scaffolding** — project definition, ADRs, and a minimal UEFI binary that prints a boot banner. VMX/EPT and guest execution land in later milestones.

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
# Interactive: COM1 on stdio (guest exits via isa-debug-exit)
./tools/run-qemu.sh

# M0 CI gate: build + boot + require RAYNU-V-M0-BOOT-OK on serial
./tools/qemu-boot-test.sh
```

Requires `qemu-system-x86_64` and OVMF (`qemu-system-x86` + `ovmf` packages).

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
| [docs/risk_register.md](docs/risk_register.md) | Full risk register (R01–R14) |
| [docs/adr/](docs/adr/) | Architecture Decision Records (ADR-001–008) |

## What This Is Not

Not a Linux+KVM distro. Not a Xen/bhyve port. Not a general-purpose OS.
Production-targeted from M0 — fewer features than ESXi, every shipped feature auditable and (for the Proven Core) designed for mathematical verification.
