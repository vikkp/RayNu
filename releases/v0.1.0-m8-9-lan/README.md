# v0.1.0-m8-9-lan — M8.9 close rollback (guest on the customer LAN)

Flash this file to return to the EFI that closed M8.9 and ADR-021 wave 1.
It is **not** GitHub Latest. Latest stays `v0.1.0-everest-closed`
(`f72b4276`).

This is the rollback when a wave 2 EFI (link state, uplink discovery,
carrier) misbehaves. The older M8.8 pin `v0.1.0-m8-8-ubuntu` boots the same
Ubuntu disk but the guest has no LAN there.

## What this freezes

| Area | State |
|------|-------|
| Guest NIC | virtio-net on the Guests row, egress through the second BCM5720 function (`:39`, port 4), ADR-013 appendix C |
| Guest address | The address saved on the SPA reaches the guest as a systemd credential in SMBIOS Type 11 (`network.network.10-raynu`), plus a drop-in that pulls in `systemd-network-generator` |
| Guest side | Nothing typed. `/run/systemd/network/10-raynu.network` installed from the credential; `enp0s5` routable on `10.99.99.200/24`, gateway `10.99.99.1`, DNS `1.1.1.1` |
| LAN | SSH from a Mac on the LAN reached `raynusrv2` on `10.99.99.200` |
| Login | `vikkp@raynusrv2:~$` on Ubuntu 26.04, 10 GiB tail of RAYNU-SPARE |
| Untouched | UBUNTU0, the 8 GiB window, and the Ubuntu disk. The guest disk is not written by the hypervisor |

## Provenance

| Field | Value |
|-------|-------|
| EFI source | `0d865c7fcb0a53ef95039463c8d40ff1c7707db5` |
| CI | `40603110937` |
| EFI bytes | 2,485,760 |
| EFI SHA256 | `df6fc3465cecf1eaf058d5c8e675501827c16eb55e7b2c9758bc5e68326ad91b` |
| COM2 | `build: sha=0d865c7fcb0a` |
| Branch | `cursor/m8-9-d7-guest-address-50bb` (PR #416) |
| Evidence | `docs/evidence/r640/2026-10-27-0d865c7f-guest-address-from-spa-d7-ok.md` |

The bytes in this directory are the CI artifact. A later documentation
commit does not change them. A local rebuild will stamp a different
`build: sha=` and is not this rollback.

## Verify

```bash
( cd releases/v0.1.0-m8-9-lan && sha256sum -c r640-hypervisor.efi.sha256 )
```

Cruzer: replace only `EFI/BOOT/BOOTX64.EFI`. Leave `raynuf.txt`. Do not
write the Toshiba. Do not pass `--init-new-cruzer`.

After F11, the NIC record lives only in hypervisor memory: Save the vNIC on
the Guests row before Start, or the Start has no address to publish.
