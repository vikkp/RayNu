# v0.1.0-xsavesfix — Enable XSAVES + naked SHELL CPUID /init

`v0.1.0-keepconfix` reached `Run /init as init process` then:

```text
BUG: TASK stack guard page was hit …
Kernel panic - not syncing: Fatal exception in interrupt
```

Iron log showed compacted XSAVE. Without secondary **Enable XSAVES/XRSTORS**
(bit 20), guest `xsaves`/`xrstors` #UD during irq/FPU paths → nested faults →
stack guard. Also harden `/init` so the first SHELL CPUIDs need no user stack
(gcc frame pushes previously ran before CPUID).

```bash
( cd releases/v0.1.0-xsavesfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-xsavesfix
```

Expect `secondary=` includes bit 20 (`0x100000` → e.g. `0x0010100a`); then
`RAYNU-V-M3-SHELL-OK` shortly after `Run /init`.
