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

**M0 → M2.1** — EFI boots, VMXON, **EPT identity map** (4 GiB), guest **store + loop + HLT** under EPT, one **VMEXIT**. Markers: `RAYNU-V-M2-EPT-OK`, `RAYNU-V-M2-GUEST-OK`. QEMU needs **KVM**; TCG prints `RAYNU-V-M1-VMXON-SKIP`. Latitude: BIOS VT-x on, `sudo ./tools/enable-nested-kvm.sh`, `./tools/qemu-boot-test.sh`. Next: EPT ownership asserts (ADR-004) / allocator (M2 continue).

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

# Boot gate: M0 → M2.0 markers (requires KVM + nested VT-x for EPT/VMEXIT)
./tools/qemu-boot-test.sh

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
| [docs/risk_register.md](docs/risk_register.md) | Full risk register (R01–R14) |
| [docs/adr/](docs/adr/) | Architecture Decision Records (ADR-001–008) |

## What This Is Not

Not a Linux+KVM distro. Not a Xen/bhyve port. Not a general-purpose OS.
Production-targeted from M0 — fewer features than ESXi, every shipped feature auditable and (for the Proven Core) designed for mathematical verification.
