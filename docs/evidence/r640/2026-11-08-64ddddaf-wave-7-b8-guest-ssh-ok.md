# 2026-11-08 — `64ddddafeffa`: guest SSH returns after the I/O APIC switch

**EFI:** pull-request merge `64ddddafeffafd5df4066cc1b0f5a304df142987` of `9e3661fe` into `464a03d9`. COM2 `build: sha=64ddddafeffa run=42424856392`.
**Rollback:** `v0.1.6-wave6-closed`. The previous image `8f69297c` / run `42398077199` reached login and then dropped `ssh` to `10.99.99.200`.

## What iron showed

`ACPI: DSDT ... 00011E`. The previous boot's DSDT was `000106`. Linux loaded the AML and printed `ACPI: Using IOAPIC for interrupt routing`.

`virtio-pci 0000:00:05.0: enabling device (0000 -> 0002)`. The line `PCI INT A: no GSI` is absent. Disk MSI-X is still `boot: B8 virtio msi-x vec=21`, then `vda1 vda2 vda3`.

Queue-notify COM2 lines stop at `n=961`. The login prompt is not a stream of `off=0x300`.

`systemd-networkd-wait-online` finished. `ssh.service` started. From the operator Mac, `ssh vikkp@10.99.99.200` printed `Welcome to Ubuntu 26.04 LTS` and `vikkp@raynusrv2:~$`. The ten-second net sample has `vtx=109 vrx=178 sent=109 take11=0`.

**Guest NIC path confirmed.** B8 stays lived. Wave 7 stays open. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
