# 2026-10-28 — `8df4407d`: one cable pull on port 4 (ADR-021 D6 lived; D5 page half lived, audit mirror hushed; D4 discovery `match=yes`)

**EFI:** `8df4407d3630` (PR #418, CI push run `40612757544`, SHA256 `83d41c4dc36a12efa0740c1dbf1e1ec5bf8f544995d7ec5bc57215c7cce61f87`, 2,490,880 bytes).
**Flash:** Cruzer, F11 one-shot. Ubuntu 26.04 standing on UBUNTU0. Guest: the 10 GiB tail on RAYNU-SPARE (`c3a70e62` install, kernel `7.0.0-38`), vNIC Saved on the Guests row (`10.99.99.200/24`), then Start.
**Operator order:** login over `ssh vikkp@10.99.99.200` (so `:39` was armed and driven), a one-second `ip -br link` loop started in the guest, then the port 4 (LOM 4, `b0:26:28:5c:5a:39`) cable pulled for about twenty seconds and replugged.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell.

## COM2 (also in the SPA's `=== host UART ===` ring)

```
raynusrv2 login: boot: D5 link lom=4 func=1 down
boot: M8.9 egress :39 link=up speed=100 duplex=full
boot: D5 link lom=4 func=1 up mbps=100
```

The pull and the replug, both seen by the sampler after the link had been up. Iron `d7e2e24f`–`02e0acba` polled only while down; this is the first time a pulled cable was observed. The link partner on port 4 is 10/100 (`lpa=45e1`, same as `99f003c1`), so `mbps=100` is the wire, not a defect.

Earlier in the boot, at login (`t10s`): `wire=up bmsr=796d bmcr=1100 lpa=45e1 macmode=19e04804 … sent=103 … vrx=137`.

## Guest (D6)

```
vikkp@raynusrv2:~$ ip -br link
lo               UNKNOWN        00:00:00:00:00:00 <LOOPBACK,UP,LOWER_UP>
enp0s5           UP             02:52:4e:38:0b:0d <BROADCAST,MULTICAST,UP,LOWER_UP>
vikkp@raynusrv2:~$ setsid nohup bash -c 'while :; do echo "$(date +%T) $(ip -br link show enp0s5)"; sleep 1; done' > /tmp/link.log 2>&1 &
… cable out, cable in, SSH resumed …
vikkp@raynusrv2:~$ grep -c LOWER_UP /tmp/link.log; grep -v LOWER_UP /tmp/link.log | head -20
1968
11:29:36 enp0s5           DOWN           02:52:4e:38:0b:0d <NO-CARRIER,BROADCAST,MULTICAST,UP>
11:29:37 enp0s5           DOWN           02:52:4e:38:0b:0d <NO-CARRIER,BROADCAST,MULTICAST,UP>
… one line per second …
11:29:55 enp0s5           DOWN           02:52:4e:38:0b:0d <NO-CARRIER,BROADCAST,MULTICAST,UP>
```

`NO-CARRIER` for the whole pull, `LOWER_UP` before and after, and the SSH session over `:39` came back by itself. The device offered `VIRTIO_NET_F_STATUS`, the status word followed `host_link::guest_carrier`, and the config-change interrupt reached the driver. **D6 lived.**

## SPA Overview (D5, page half)

| Row | Text |
|---|---|
| Port 3 | SPA `b0:26:28:5c:5a:38` — `UP 100 FULL` — 0 changes |
| Port 4 | Guest LAN `b0:26:28:5c:5a:39` — `UP 100 FULL` — 2 changes |

Both LOM ports with role, MAC, speed, duplex, and the change count on the pull/replug port. The Guests page was unchanged.

## What did not show: the audit mirror

The `=== host UART ===` ring holds the two `boot: D5 link` lines and **no** `RAYNU-V-AUDIT: HostLinkChanged` line. `mirror_audit_to_com1` writes through `serial::write_str`, which returns early once a Linux guest owns the console (`linux_earlycon_share`); the `boot:` lines reached the ring because `egress_say` uses `write_str_nowait`. The event was appended to the audit ring (`record_event` does that before the mirror, unconditionally), but nothing on this boot could show it, and there is no HTTP reader for the ring. **D5 stays in flight**: the page half lived here; the "audit event on change" half needs the mirror on the no-wait path (fixed on the branch after this boot) and one more pull.

The same hush applies to every other audit mirror that fires under a running guest (`GuestNicDetached`, `VmStopped`, …). Not changed here; noted for the catalog.

## The D4 line (found later in the same COM2 session)

```
boot: M8.9 egress arm (no phylock, SPA DMA stays)
boot: M8.9 egress discovered pci=01:00.01 mac=b0:26:28:5c:5a:39 constant=:39 match=yes
boot: M8.9 egress :39 ape=yes ape-evt=sent (driver START on function 1; phylock stays with iDRAC)
boot: HOST-NIC BCM5720 reset…
boot: HOST-NIC BCM5720 ape-grc=yes pci-restore=64
boot: M8.9 egress :39 fw-magic=yes
boot: M8.9 egress :39 grc=bswap+wswap memarb=1 bufmgr=1
boot: M8.9 egress :39 phy bmsr=796d bmcr=1100 link=up
boot: M8.9 egress :39 link=up speed=100 duplex=full
boot: M8.9 dhcp discover leaves :39
boot: M8.9 egress rings armed pci=01:00.01 phylock=no
boot: M8.9 guest frame leaves :39 len=86
```

`guest_egress::discover` named the SPA's pair partner from the SPA's own PCI position (`01:00.0` → `01:00.1`), read its station MAC, and it matched the lab constant. **Discovery lived.** The function number prints as `.01` (two hex digits); cosmetic. **D4 stays in flight** until the arm and the port checks use the discovered port instead of `EGRESS_PHY_MAC` and one boot arms `:39` through that path.

## Not claimed

No new `RAYNU-V-*-OK` marker. D5 and D4 not lived. Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
