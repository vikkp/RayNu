# R640 M4.3 — virtio-blk host-slab closed 2026-08-28

**Claim:** After G0 VMCS relocate / `M4-NVM-OK`, the M4.3 virtio-blk probe
ran from a host-only 2 MiB slab (`HPA=0x10c00000`, G0 EPTP, guest CR3 in
slab) and latched `RAYNU-V-M4-BLK-OK`. Net and SMP probes followed on
`0x10e00000` / `0x11000000`+`0x11200000`. `RAYNU-V-R640-BOOT-OK` and
Phase F coexist (`10.99.99.126:8443`, VMX on) held.

**Not claimed:** Stage 46 / `ISO-INSTALL-OK` / Everest E5. `ISO-BOOTED-FROM-DISK`
on this paste is persist-detect of the 1 KiB ESP stamp (M7.7 lab), not a
distro installer.

## Closing tip

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-08-28 |
| Platform | PowerEdge R640 |
| Boot method | Front USB 2: Cruzer Micro **RAYNUV** |
| Branch / tip | `cursor/p0-60-g1-ept-a623` `22e28d0` |
| Lease | `10.99.99.126:8443` (native BCM5720; SNP residual pre-EBS) |

## COM2 (this close)

```text
boot: M4.3 blk probe host slab HPA=0x0000000010c00000 (G0 EPTP; guest CR3 in slab)
boot: EPTP=0x000000000bbff01e guest_code=0x0000000010c00000
RAYNU-V-M4-BLK-OK
RAYNU-V-M7-ISO-BOOTED-FROM-DISK
boot: M4.3 complete — virtio-blk MMIO handshake + write/readback
RAYNU-V-M4-NET-OK
RAYNU-V-M4-SMP-OK
RAYNU-V-R640-BOOT-OK
boot: HOST-NIC coexist listening on 10.99.99.126:8443 (VMX on; ADR-013 Phase F)
```

Stage 45 El Torito (`RN-ELT` n=197992), P0-60 G1 `M4-SHELL-G1` /
`M4-2VM-OK`, and G0 relocate (`HPA=0x10a00000`, `M4-NVM-OK`) still held.

## Honesty / residuals

- Prior iron (`10e7984`) triple-faulted the probe at `guest_code=0xfc0f000`
  (`reason=0x02`) then `VMXOFF` / `boot gate failed`. That page sat in G0
  e820; Linux SHELL scribbled it.
- `ISO-BOOTED-FROM-DISK` is LBA stamp persist-detect, not a guest root
  filesystem and not a distro installer.
- Next: Stage 46 `ISO-INSTALL-OK`. Iron P0-14 stays `2b795a0`.
