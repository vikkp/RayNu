# v0.1.0-keepconfix — keep earlyprintk; disable 8250 console

`v0.1.0-invpcidfix` reached `RAYNU-V-M3-GTIMER3-OK` / `RAYNU-V-M3-APIC-OK`
then went quiet after:

```text
printk: legacy bootconsole [earlyser0] disabled
```

Tinyconfig enables `SERIAL_8250_CONSOLE`. With `noapic` and no IRQ4 TX inject
(M3.19), the 8250 driver can claim COM1 and stall on TX IRQ before `/init`
runs the SHELL CPUID. Earlyprintk unregister also blinds iron debug.

Cmdline adds:
- `earlyprintk=serial,ttyS0,115200,keep`
- `8250.nr_uarts=0`

```bash
( cd releases/v0.1.0-keepconfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-keepconfix
```

Expect: serial stays alive past APIC-OK; `RAYNU-V-M3-SHELL-OK` (then NOIRQ / M4).
