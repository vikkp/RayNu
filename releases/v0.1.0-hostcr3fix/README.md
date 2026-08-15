# v0.1.0-hostcr3fix — keep host CR3 after VMEXIT

`v0.1.0-cr3fix` got past the guest-CR3 EPT fault: VMLAUNCH succeeded, guest
stored + HLT'd — then **silence** (no `VMEXIT phase=` / `RAYNU-V-M1-VMEXIT-OK`).

Cause: `setup_vmcs` wrote the **same** precise-window CR3 into both
`GUEST_CR3` and `HOST_CR3`. On HLT VMEXIT the host resumed with page tables
that only identity-map `[0,512MiB)`. UEFI code / IDT / stacks living above that
window → silent death (QEMU `-m 512M` hid this).

This kit keeps `HOST_CR3 = cpu::read_cr3()` (real UEFI CR3) and only overrides
`GUEST_CR3` when `guest_cr3_phys` is set.

```bash
( cd releases/v0.1.0-hostcr3fix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-hostcr3fix
```

Expect after `VMLAUNCH → guest store+loop+HLT…`:
`boot: VMEXIT phase=…` then `RAYNU-V-M1-VMEXIT-OK` (and later markers).
