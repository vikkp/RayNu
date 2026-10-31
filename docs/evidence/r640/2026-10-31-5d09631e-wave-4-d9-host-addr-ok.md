# 2026-10-31 — `5d09631e`: host management address persisted (ADR-021 D9 lived)

**EFI:** `5d09631ec6d8` (PR #425, CI run `41199806446`). COM2 `build: sha=5d09631ec6d8 run=41199806446`.
**Flash:** Lexar, label `RAYNUV`, front USB 2. Ubuntu 26.04 standing on UBUNTU0. Boot next was still saved, so each F11 started the Ubuntu tail by itself. No reinstall. No doorbell. UBUNTU0 was not written.
**Rollback:** `v0.1.0-m8-9-lan`. The B4 stick is `621654369b68` / run `40958321431`.

## First boot of this stick

The host-address sector was empty. COM2 printed `boot: D9 host addr dhcp` at the PERC read and again at the standing bind. PRE-EBS SNP and the standing SPA both used DHCP `10.99.99.146/24`. Overview showed `This boot: DHCP 10.99.99.146/24` with gateway `10.99.99.1`. Hostname and DNS were left empty so the Mac hosts name `raynu-v.lab` stayed the certificate name.

**Save address** then printed:

```text
boot: D9 host addr wr lba=6190268908 match
RAYNU-V-AUDIT: HostAddrSaved mode=1 prefix=24
```

That click did not move the listener. The page stayed on `.146`.

## Next F11

Same stick. PRE-EBS SNP still took a DHCP lease, and this boot the lease was `.146` again. After ExitBootServices the spare sector came back:

```text
boot: D9 host addr rd lba=6190268908 static
boot: D9 host addr static 10.99.99.146/24 gw=10.99.99.1 dns=0.0.0.0 name=
boot: HOST-NIC coexist listening on 10.99.99.146:8443 (VMX on; ADR-013 Phase F)
```

`dns=0.0.0.0` and an empty `name=` are the blank DNS and hostname fields from the save. The standing SPA bound the saved address and kept MAC `b0:26:28:5c:5a:38`. Autostart still started the Ubuntu tail (`boot: B5 autostart start off=19327352832`).

**D9 lived.** Wave 4 stays open on D10 and H2. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
