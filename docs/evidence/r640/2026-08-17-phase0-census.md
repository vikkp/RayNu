# R640 — ADR-013 Phase 0 PCI census (2026-08-17)

**Claim:** Phase 0 closed on iron. One management NIC chosen:

| Field | Value |
|-------|--------|
| PCI | `01:00.0` (func 0) |
| `vid:did` | **`14e4:165f`** (Broadcom NetXtreme **BCM5720**) |
| BAR0 | `0x92930000` |
| MSI-X | 17 table entries |
| Second port | `01:00.1` same `vid:did`, BAR0 `0x92900000` (do not bind both yet) |
| IOMMU | `ACPI DMAR=no` (print only; VT-d may be off in BIOS or missed by XSDT walk) |

**Not claimed:** `RAYNU-V-M7-HOST-NIC-HTTP-OK`. No native Device for this id yet.

## Media / path

Cruzer Micro, front USB 2. Replace-only `EFI/BOOT/BOOTX64.EFI` (Vignesh).  
PRE-EBS SNP residual still served SPA (`10.99.99.121:8443`) → `RAYNU-V-M7-UEFI-HTTP-OK`.  
Guest path green through M4 + `RAYNU-V-R640-BOOT-OK`. SNP idle WARN-only. **No RSOD.**

## Fingerprint (COM2)

```text
boot: PCI census nics=2
boot: PCI 01:00.00 vid:did=14e4:165f bar0=0x92930000 msix=17
boot: PCI 01:00.01 vid:did=14e4:165f bar0=0x92900000 msix=17
boot: IOMMU ACPI DMAR=no
boot: HOST-NIC census: no lab e1000; do not guess LOM (Phase D waits on this list)
…
RAYNU-V-R640-BOOT-OK
boot: WARN — POST-EBS SNP idle skipped; firmware SNP dead after EBS lease=10.99.99.121:8443
boot: HOST-NIC idle: no native Device for census vid:did=14e4:165f (Phase D waits on this id; do not guess LOM)
```

Full log: [`logs/2026-08-17-phase0-census-com2.txt`](logs/2026-08-17-phase0-census-com2.txt).

## Honesty

- Lab e1000 (`8086:100e`) is **not** on this box. QEMU `HOST-NIC-QEMU-OK` does not count.
- Firmware Tcp4 still absent; PRE-EBS SNP still works. That is E3, not E3b.
- Phase D is now unblocked **only** for `14e4:165f` (prefer `01:00.0`). Same `smoltcp::phy::Device` surface as e1000. Do not start X710/i40e.

Preserve kit: `releases/v0.1.0-adr013-baseline`.
