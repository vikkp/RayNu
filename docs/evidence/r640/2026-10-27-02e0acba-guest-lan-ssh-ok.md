# 2026-10-27 — `02e0acba`: guest frames leave and return on `:39`; SSH into the guest from the Mac

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-10-27 |
| Operator | vikkp |
| Host (service tag / hostname) | D0324Y2 / R640 lab |
| Boot method | USB (F11 one-shot, front USB 2, Lexar 116.1G `USB Flash Drive`) |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` |
| `r640-hypervisor.efi` SHA256 | `719e427a5cfd873d3abba167fe1fc1f05f186004831b8e116d02420efb7ec1ff` |
| EFI source | `02e0acba` (`cursor/m8-9-close-50bb`, PR #414) · CI `--run 40495122788` · COM2 `build: sha=02e0acba0e4a` |
| ESP contents | `EFI/BOOT/BOOTX64.EFI`, `EFI/RayNu/OVMF.fd`, `EFI/RayNu/installdisk.bin`; no `*.iso` |
| Serial channel | iDRAC SOL `console com2` |
| Other machine | `vikkp@Mac` on 10.99.99.0/24 |

## What this boot was for

Fourth F11 of the wave 1 D3 thread, the first with the function-1
bring-up from
[2026-10-27-99f003c1-lexar-login-39-rings-dark.md](2026-10-27-99f003c1-lexar-login-39-rings-dark.md):
`arm_egress` runs the tg3 `reset_hw` sequence on `01:00.1` through its own
APE window before the rings arm. Same path as before: bare Lexar, PERC
image latch, Ubuntu tail `choice=4` with the saved static NIC, login,
`ip link set enp0s5 up`.

## The arm, on COM2 for the first time

```text
boot: M8.9 egress arm (no phylock, SPA DMA stays)
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

Every line the fix promised, in order. The APE accepted a driver START
for function 1 (`ape-evt=sent`), the GRC lock was granted with function
1's bit, bootcode returned the magic, the core blocks came up.

## Frames leave and return (D3)

```text
boot: M8.9 net post-tx t0   ms=0     … vtx=1 vrx=0  sent=1 held=0 wire=up bmsr=796d lpa=45e1 macst=400000 macmode=e04804   wtx=2/389 wrx=0/0     txp=2/1 rcb=0
boot: M8.9 net post-tx t2s  ms=2048  … vtx=7 vrx=11 sent=7 held=0 wire=up bmsr=796d lpa=45e1 macst=400000 macmode=19e04804 wtx=6/536 wrx=11/1330 txp=8/7 rcb=11
boot: M8.9 net post-tx t10s ms=10003 … vtx=7 vrx=35 sent=7 held=0 wire=up bmsr=796d lpa=45e1 macst=400000 macmode=19e04804 wtx=0/0   wrx=24/2458 txp=8/7 rcb=3
```

Against `99f003c1` (`txp=8/0 wtx=0/0 rcb=0 vrx=0`):

- `txp=2/1` then `8/7`: the hardware consumer follows the producer. `wtx=2/389`
  then `6/536`: the MAC transmitted the guest's frames. Frames left `:39`.
- `wrx=11/1330`, `rcb=11`, `vrx=11` at +2 s and `vrx=35` at +10 s: received
  frames crossed the return ring into virtio-net. Frames returned.
- The firmware still sets the APE bits on `MAC_MODE` at +2 s
  (`e04804 → 19e04804`) and clears the wire statistics (`wtx=0/0` at +10 s
  with `vrx` still climbing). Traffic flows through it. Noted, not chased.

Guest side after `ip addr add 10.99.99.200/24 dev enp0s5` and
`ip route add default via 10.99.99.1`:

```text
vikkp@raynusrv2:~$ timeout 3 bash -c 'echo >/dev/tcp/10.99.99.1/22'; echo exit:$?
bash: connect: Connection refused
exit:1
vikkp@raynusrv2:~$ ip -s link show enp0s5
    RX:  bytes packets errors dropped  missed   mcast
        185166     947      0     194       0       0
    TX:  bytes packets errors dropped carrier collsns
           760      10      0       0       0       0
vikkp@raynusrv2:~$ ip neigh show dev enp0s5
10.99.99.1 lladdr 28:87:ba:a6:b6:04 STALE
```

ARP for the gateway resolved; the gateway answered a SYN with a RST. Then
the guest pulled the Ubuntu package index over `:39`:

```text
vikkp@raynusrv2:~$ resolvectl query archive.ubuntu.com
archive.ubuntu.com: 91.189.91.83 -- link: enp0s5 …
-- Information acquired via protocol DNS in 106.0ms.
vikkp@raynusrv2:~$ sudo apt-get update && sudo apt-get install -y openssh-server
Get:1 http://security.ubuntu.com/ubuntu resolute-security InRelease [137 kB]
Get:2 http://archive.ubuntu.com/ubuntu resolute InRelease [136 kB]
…
Get:11 http://archive.ubuntu.com/ubuntu resolute/universe amd64 Packages [16.0 MB]
Get:12 http://archive.ubuntu.com/ubuntu resolute/universe Translation-en [6329 kB]
Get:13 http://archive.ubuntu.com/ubuntu resolute/universe amd64 Components [4556 kB]
…
Reading package lists... Done
E: Release file for … resolute-security/InRelease is not valid yet (invalid for another 71d 4h 49min 34s).
```

About 30 MB of TCP through the guest NIC. `unattended-upgrades` then woke
on the new network and installed a kernel update (`7.0.0-38-generic`) on
the tail disk before `openssh-server` could be installed; the next boot of
this disk comes up on that kernel.

## SSH from another machine (D8)

```text
vikkp@Mac ~ % ssh vikkp@10.99.99.200
The authenticity of host '10.99.99.200 (10.99.99.200)' can't be established.
ED25519 key fingerprint is: SHA256:/VYRvnatWWN985g2jZewWnL1CDsBPSXPEqsyy41y9rk
…
vikkp@10.99.99.200's password:
Welcome to Ubuntu 26.04 LTS (GNU/Linux 7.0.0-14-generic x86_64)
vikkp@raynusrv2:~$ hostname; ip -s link show enp0s5
raynusrv2
2: enp0s5: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP mode DEFAULT group default qlen 1000
    link/ether 02:52:4e:38:0b:0d brd ff:ff:ff:ff:ff:ff
    RX:  bytes packets errors dropped  missed   mcast
     378671067  258048      0    1512       0       0
    TX:  bytes packets errors dropped carrier collsns
       2663586   29868      0       0       0       0
```

A login from the Mac into the guest running on RayNu, through the switch,
port 4, function 1, the host send/return rings and virtio-net. The
counters, read over that SSH session: 378 MB / 258,048 packets in and
2.6 MB / 29,868 packets out with zero errors — the package indexes and the
kernel update, all through the guest NIC. The first
`ssh` attempt had returned `Connection refused` from the guest because
`openssh-server` was not on the minimized image; that RST was itself a
full round trip.

## Not closed here (D7)

The address `10.99.99.200/24` was typed into the guest by the operator.
The SPA saved a static NIC (`GuestNicSaved mode=2`) and `boot: M8.9 ip=
on linux line` printed, but this Ubuntu kernel has no `CONFIG_IP_PNP` and
the dracut initrd has no network module, so the `ip=` token does nothing.
No DHCP client ran in the guest. D7 ("DHCP from their server or static
from the SPA") stays open: the address must reach the guest another way
(netplan on the disk, a cloud-init seed, or a guest agent).

## Also seen

- Guest clock was about 71 days behind (apt "not valid yet"), so the
  guest is not getting the host wall clock. RTC/CMOS path to look at.
- No `boot: M8.9 virtio-net mac=` line again (the `virtio-net MMIO` line
  did print). Config-read path to check.
- The firmware APE bits on `MAC_MODE` at +2 s and the cleared wire stats.

## What this is

- **D3 lived**: guest frames leave and return on `:39` (`txp=8/7`,
  `wtx=6/536`, `rcb=11`, `vrx=35`; guest `RX 378671067 bytes / 258048
  packets, 0 errors`; ARP resolved; apt and a kernel update pulled).
- **D8 lived**: SSH from the Mac into the guest at `10.99.99.200`.
- The function-1 bring-up (driver START, `chip_reset_at`,
  `enable_core_blocks`) is what moved the rings; `99f003c1` without it had
  `txp=8/0`.

## What this is not

- Not D7. The address was set by hand. Wave 1 is not closed.
- Not `RAYNU-V-M7-ISO-INSTALL-OK`. No disk formatted. No PERC marker is
  new. The APE phylock stayed with iDRAC.
