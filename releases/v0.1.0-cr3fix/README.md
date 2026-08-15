# v0.1.0-cr3fix — R640 guest CR3 inside precise EPT

After BAR/pool fixes, VMLAUNCH still EPT-faulted because the guest shared
**host CR3** (`0x47e01000`, ~1150 MiB) which lies outside identity EPT
`[0,512MiB)`.

This kit builds a guest page-table identity map of `[0,512MiB)` in the HV pool
and sets `guest_cr3_phys` for G0 (and shell 2 MiB CR3 for G1–G3).

```bash
( cd releases/v0.1.0-cr3fix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-cr3fix
```

Expect: `boot: guest CR3 (precise identity)=0x…` with CR3 **< 0x10000000**, then
progress past `VMLAUNCH` (VMEXIT / later markers).
