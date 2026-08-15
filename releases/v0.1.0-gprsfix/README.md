# v0.1.0-gprsfix — RIP-relative guest GPR save/restore

`v0.1.0-hostcr3fix` cleared silence after VMLAUNCH and reached
`RAYNU-V-M2-TIMER-OK`, then failed:

- `guest COM1 I/O magic missing`
- `guest CPUID filter missing`
- `guest CPUID ECX store still has VMX`

Cause: `vmexit_landing` / `vmresume_with_gprs` used `mov [{sym}], reg`, which
LLVM lowers to **32-bit absolute** `[disp32]` (`0x4001ef70`). The UEFI image
lives at `0x140000000`, so saves missed the real statics. Handlers saw
RAX/AL=0; M2 memory verifies still passed.

Fix: RIP-relative `mov [rip + {sym}], reg`. Also stop setting primary bit 21
(Use TPR shadow) — that was misnamed as “CPUID exiting” (CPUID always exits).

```bash
( cd releases/v0.1.0-gprsfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-gprsfix
```

Expect after TIMER-OK: `RAYNU-V-M3-IO-OK`, `RAYNU-V-M3-CPUID-OK`, then
proto/Linux entry (not `boot gate failed`).
