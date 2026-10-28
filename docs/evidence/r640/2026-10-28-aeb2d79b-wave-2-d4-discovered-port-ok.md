# 2026-10-28 — `aeb2d79b`: one quiet boot, the guest's LAN over the discovered port (ADR-021 D4 lived)

**EFI:** `aeb2d79bb179` (PR #418, CI push run `40687568692`, artifact `r640-hypervisor.efi` SHA256 `c55287cb25c989a7513fe9a062e54e8b6e20c039a5dfe2f333e04e5f6a419d6c`, 2,491,904 bytes).
**Flash:** Cruzer, F11 one-shot. COM2 `build: sha=aeb2d79bb179`. Ubuntu 26.04 standing on UBUNTU0. Guest: the 10 GiB tail on RAYNU-SPARE (`c3a70e62` install, kernel `7.0.0-38`), vNIC Saved on the Guests row (`RAYNU-V-AUDIT: GuestNicSaved off=19327352832 mode=2`), `PercSpaChoice choice=4`, Start.
**Operator order:** nothing after Start. No cable pull. Then `ssh vikkp@10.99.99.200` from the Mac.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell.

## What D4 asks

The egress port is the other function of the SPA's BCM5720, found on the box, not a MAC constant compiled in. On this EFI `EGRESS_PHY_MAC` is a mint-collision guard and a test fixture; nothing on the arm path reads it. `init_bcm5720` runs `guest_egress::discover` from the SPA's own PCI position and keeps the answer (`note_discovered`); `arm_egress` reads `discovered()` back, checks the BAR and the station MAC against that record, and programs `port.phy_mac`. `port_ok`, `handoff`, `preview_offer`, `StartNic.egress_phy`, `ip_token` / `network_file` and `guest_close::judge` take the same discovered MAC.

## COM2 — discovery at NIC init (post-EBS, before any guest)

```
boot: M8.9 egress discovered pci=01:00.01 mac=b0:26:28:5c:5a:39 (SPA partner; no constant)
```

## COM2 — the arm, at guest `[   16.9]` during `systemd-networkd.service`

```
boot: M8.9 egress arm (no phylock, SPA DMA stays)
boot: M8.9 egress discovered pci=01:00.01 mac=b0:26:28:5c:5a:39 (SPA partner; arm)
boot: M8.9 egress :39 ape=yes ape-evt=sent (driver START on function 1; phylock stays with iDRAC)
boot: HOST-NIC BCM5720 reset…
boot: HOST-NIC BCM5720 ape-grc=yes pci-restore=64
boot: M8.9 egress :39 fw-magic=yes
boot: M8.9 egress :39 grc=bswap+wswap memarb=1 bufmgr=1
boot: M8.9 egress :39 phy bmsr=796d bmcr=1100 link=up
boot: M8.9 egress :39 link=up speed=100 duplex=full
boot: M8.9 dhcp discover leaves :39
boot: M8.9 egress rings armed pci=01:00.01 phylock=no
boot: M8.9 guest frame leaves :39 len=90
```

The same record both times (`pci=01:00.01 mac=b0:26:28:5c:5a:39`); the arm sequence is the lived one from `0d865c7f` / `8df4407d`. The `constant=:39 match=` print from `8df4407d` is gone because there is no constant to match.

Traffic over that port, from the guest's side of the ring:

```
t0   vtx=1  vrx=0  sent=1  held=0 wtx=2/389 wrx=0/0    txp=2/1   rcb=0  wire=up lpa=45e1 macmode=e04804
t2s  vtx=1  vrx=8  sent=1  held=0 wtx=0/0   wrx=8/873  txp=2/1   rcb=8  wire=up           macmode=19e04804
t10s vtx=10 vrx=50 sent=10 held=0 wtx=9/818 wrx=42/9956 fcs=0 txp=11/10 rcb=18 wire=up    macmode=19e04804
```

`systemd-networkd-wait-online.service` finished, `network-online.target` reached, `ssh.service` started, `raynusrv2 login:` on ttyS0.

## Guest, over port 4 from the Mac

```
vikkp@Mac ~ % ssh vikkp@10.99.99.200
…
Welcome to Ubuntu 26.04 LTS (GNU/Linux 7.0.0-38-generic x86_64)
vikkp@raynusrv2:~$ hostname
raynusrv2
vikkp@raynusrv2:~$ ip -br addr show enp0s5; ip neigh show 10.99.99.1
enp0s5           UP             10.99.99.200/24 fd8a:f4fd:96f9:fc05:52:4eff:fe38:b0d/64 fe80::52:4eff:fe38:b0d/64
10.99.99.1 dev enp0s5 lladdr 28:87:ba:a6:b6:04 REACHABLE
```

The address is the SPA-saved one (D7 path, credential `network.network.10-raynu`), the gateway's ARP entry is `REACHABLE`, and the SSH session is the proof that frames crossed the discovered port both ways. **D4 lived.**

## A side check: guest memory

The kernel's `Memory:` line on this boot read `451660K/2091552K available … 1633084K reserved`, where `8df4407d` read `1621660K available … 463676K reserved` on the same e820 map and the same `high RAM … tail=1845493760`. From inside the guest:

```
vikkp@raynusrv2:~$ free -m
               total        used        free      shared  buff/cache   available
Mem:            1637         146        1219           0         297        1491
Swap:           1376           0        1376
```

`total 1637` MiB on a 2 GiB guest with `crashkernel` holding 320 MiB is the same figure as the earlier boots. The `Memory:` line counted memory the kernel had not yet released at the moment it printed; not a change in what the guest is given. Nothing on this diff touches guest memory. No action.

## Not claimed

No new `RAYNU-V-*-OK` marker (D4 has none; the gate was the lease over the discovered port and it was met). D5 not lived: no cable pull on this boot, so the `HostLinkChanged` mirror on the no-wait path was not exercised; `f57bd265` is in this EFI and waits for one pull. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
