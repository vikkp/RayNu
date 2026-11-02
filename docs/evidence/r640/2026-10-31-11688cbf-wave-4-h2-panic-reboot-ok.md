# 2026-10-31 — `11688cbf`: panic dump, reboot, guest autostart (ADR-021 H2 lived)

**EFI:** `11688cbfa078` (PR #426, CI run `41203867999`). COM2 `build: sha=11688cbfa078 run=41203867999`.
**Flash:** Lexar, label `RAYNUV`, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Rollback:** the D9 stick `5d09631ec6d8` / run `41199806446`. Older wave-2 pin remains `v0.1.0-m8-9-lan`.

## Dump and reboot

The guest was at `raynusrv2 login:`. Overview **Panic dump** printed on the iDRAC SOL session:

```text
raynusrv2 login: boot: H2 panic sp
```

The rest of `spa` and `boot: H2 reboot` were cut off as the chassis reset. The operator reported the chassis rebooting. The cold reset returned to Ubuntu, because F11 is a one-shot on this lab.

## Next RayNu boot

The same stick, F11, no SPA click:

```text
boot: B5 autostart rd lba=6190268909 n=1
boot: D9 host addr rd lba=6190268908 static
boot: D9 host addr static 10.99.99.146/24 gw=10.99.99.1 dns=0.0.0.0 name=
boot: B5 autostart nic
boot: B5 autostart start off=19327352832
boot: D10 ntp 162.159.200.123 unix=1793453127
boot: D10 audit unix=1793453127
```

Ubuntu then reached `raynusrv2 login:`. `1793453127` is 2026-10-31 08:25 CT. The saved address and the autostart list were still on the spare.

**H2 lived. Wave 4 closed.** Not a new `ISO-INSTALL-OK`. HDA stays 99%.
