# 2026-10-27 — `99f003c1`: bare Lexar to `login:`; `:39` negotiated, rings never moved

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-10-27 |
| Operator | vikkp |
| Host (service tag / hostname) | D0324Y2 / R640 lab |
| Boot method | USB (F11 one-shot, front USB 2, Lexar 116.1G `USB Flash Drive`) |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` |
| `r640-hypervisor.efi` SHA256 | `a9f9d757e0e9a4cb5c93c9065ab6c8ab63de28f57a33846feb155d029d80eaf1` |
| EFI source | `99f003c1` (`cursor/m8-9-close-50bb`, PR #414) · CI `--run 40486061222` · COM2 `build: sha=99f003c1850c` |
| ESP contents | `EFI/BOOT/BOOTX64.EFI`, `EFI/RayNu/OVMF.fd`, `EFI/RayNu/installdisk.bin`; no `*.iso` |
| Serial channel | iDRAC SOL `console com2` |

## What this boot was for

Third F11 of the wave 1 D3 thread on the bare Lexar, and the first with
port 4 (`:39`) cabled to the switch. `250dc961` ran GRUB out of RAM;
`ac1f8619` attached no disk. This EFI arms the product window and the
tail-disk attach on the PERC image latch, so the installed guest no longer
needs an ISO on the stick. The test was the D3 procedure: Start the Ubuntu
tail from Guests, bring `enp0s5` up in the guest, watch `wire=` and `wrx=`.

## What lived

The whole USB-independent path, through to a login on the installed disk:

```text
boot: HOST-NIC BCM5720 pre-EBS cand pci=01:00.00 MAC=b0:26:28:5c:5a:38 bmsr=796d
boot: HOST-NIC BCM5720 pre-EBS cand pci=01:00.01 MAC=b0:26:28:5c:5a:39 bmsr=796d
boot: mgmt HTTP listening on 10.99.99.146:8443 (PRE-EBS SNP window)
boot: perc ld status=0x00 to=0 n=2 id=0 class=ubuntu bytes=429496467456 id=1 class=spare bytes=3169417691136 pick=1
RAYNU-V-M8-TLS-OK
RAYNU-V-M8-PERC-WRITE-OK
RAYNU-V-M8-PERC-LUN-OK
RAYNU-V-AUDIT: GuestNicSaved off=19327352832 mode=2
RAYNU-V-AUDIT: PercSpaChoice choice=4
boot: Stage 46 virtio-blk install disk bytes=10737418240 keep=0 tail_off=19327352832 (free tail; 8 GiB window is not vda; not ISO-INSTALL-OK)
boot: guest-UEFI report-RAM pool=1008 extra=814 no-zero
boot: guest-UEFI report-RAM EPT pre-map n=1008
boot: guest-UEFI CD GuestVisible iso=1 bytes=73728
boot: guest-UEFI product guest from the PERC image (no ESP ISO; lab stub CD; not ISO-INSTALL-OK)
boot: RayNu-F high RAM base=0x2000000 bytes=2113929216 tail=1845493760 premapped=2113929216 (F6-prep; not ISO-INSTALL-OK)
boot: RayNu-F BlockIo cd_bytes=0 cd_last_lba=0 disk_bytes=10737418240 disk_last_lba=20971519 (F4; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\ubuntu\grubx64.efi bytes=2828168 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xdb5000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 wall_cap_s=600
RAYNU-V-RAYNU-F-MEM-OK
boot: M8.9 ip= on linux line (this boot only; disk unchanged)
[    0.000000] Linux version 7.0.0-14-generic (buildd@lcy02-amd64-043) … Ubuntu 7.0.0-14
boot: M8.9 virtio-net MMIO (guest keeps running; not ISO-INSTALL-OK)
Welcome to Ubuntu 26.04 LTS!
[FAILED] Failed to start systemd-networkd-wait-online.service - Wait for Network to be Online.
raynusrv2 login: vikkp
Welcome to Ubuntu 26.04 LTS (GNU/Linux 7.0.0-14-generic x86_64)
```

Both ESP-ISO dependencies from the two earlier boots are closed on iron.
The 10 GiB tail is `vda`, the 8 GiB window stays, no disk was formatted.

## Layer 1 on `:39` is closed

Pre-EBS `bmsr=796d` on `01:00.1`. Once the guest sent its first frame the
trace read the PHY on the wire: `lpa=45e1`, which is a link partner
advertising 10/100 half and full duplex with pause. The switch is
negotiating with port 4. The earlier iDRAC "Link Down / AN disabled" on
port 4 is gone.

## What stopped (D3 fails on the host send ring)

In the guest, `enp0s5` existed with the saved static MAC and `state DOWN`
(the installed netplan predates the NIC). `sudo ip link set enp0s5 up`
produced the three trace lines; the prompt had already been printed
before the first one.

```text
boot: M8.9 net post-tx t0   ms=0     … txring=338 vtx=1 vrx=0 sent=1 held=0 wire=up bmsr=796d bmcr=1100 lpa=45e1 macst=401000 macmode=e04804   wtx=0/0 wrx=3460/657559 fcs=0 txp=2/0 rcb=0
boot: M8.9 net post-tx t2s  ms=2026  … txring=351 vtx=3 vrx=0 sent=3 held=0 wire=up bmsr=7949 bmcr=1100 lpa=0    macst=401000 macmode=19e04804 wtx=0/0 wrx=0/0       fcs=0 txp=4/0 rcb=0
boot: M8.9 net post-tx t10s ms=10032 … txring=372 vtx=7 vrx=0 sent=7 held=0 wire=up bmsr=796d bmcr=1100 lpa=45e1 macst=401000 macmode=19e04804 wtx=0/0 wrx=32/4889   fcs=0 txp=8/0 rcb=0
```

Read left to right:

- `vtx=7 sent=7 held=0`: the guest's virtio-net TX works and every frame
  reached `egress_send_uefi`.
- `txp=8/0`: eight send BDs were posted; the hardware consumer never moved.
  `wtx=0/0`: the MAC transmitted nothing. Not one guest frame left `:39`.
- `wrx=3460/657559` at `t0`: the MAC did receive from the wire (switch
  broadcast on a live port). `rcb=0`: not one received frame reached the
  return ring. The RX DMA path is as dark as TX.
- `macmode e04804 → 19e04804` between `t0` and `t2s`, with the wire
  counters cleared and the PHY bounced (`bmsr=7949 lpa=0`) and back: the
  firmware (APE / bootcode) set `APE_TX_EN | APE_RX_EN | KEEP_FRAME_IN_WOL`
  and re-took the port two seconds after we started using it.
- No `boot: M8.9 egress arm …`, `dhcp discover leaves :39` or `rings armed`
  line anywhere on COM2: the arm ran inside a Linux VM exit, where
  `serial::write_str` is hushed for the earlycon share, so its lines were
  dropped.

In the guest after that: `sudo ip addr add 10.99.99.200/24 dev enp0s5` and
`sudo ip route add default via 10.99.99.1` were accepted; `ping` is not on
the minimized image (`-bash: ping: command not found`). The guest's own
view agrees with the host trace:

```text
vikkp@raynusrv2:~$ timeout 3 bash -c 'echo >/dev/tcp/10.99.99.1/22'; echo exit:$?
bash: connect: No route to host
exit:1
vikkp@raynusrv2:~$ ip -s link show enp0s5
2: enp0s5: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP mode DEFAULT group default qlen 1000
    link/ether 02:52:4e:38:0b:0d brd ff:ff:ff:ff:ff:ff
    RX:  bytes packets errors dropped  missed   mcast
             0       0      0       0       0       0
    TX:  bytes packets errors dropped carrier collsns
          1202      17      0       0       0       0
vikkp@raynusrv2:~$ ip neigh show dev enp0s5
10.99.99.1 FAILED
```

`TX 17 packets`, `RX 0`, ARP for the gateway `FAILED`: seventeen frames
handed to virtio, none answered. Stop from the SPA Guests screen:

```text
boot: RayNu-F stop spa-stop exits=124083 wall_ms=2255402 svc=1414 svc_err=605 conout_ok=1 blk_rd=605 blk_wr=1 allocs=23 free_pages=477455 (F2b; not ISO-INSTALL-OK)
boot: M8.9 net tx=17 rx=0 sent=17 held=0 (not ISO-INSTALL-OK)
boot: M8.9 net post-tx stop ms=1boot: guest-UEFI restore host xcr0=0x1 osxsave=0 reason=0x1e rip=0xffffffff859a6103
boot: SPA stop idle — guest off; Start on that row boots it again (not ISO-INSTALL-OK)
```

`tx=17 sent=17 held=0`: every guest frame was handed to `egress_send_uefi`
and none was held. The `post-tx stop` line was cut after `ms=1`: the trace
is written through the guest UART ring and no VM exit drains it once the
guest is off. Fixed alongside (`flush_guest_tx()` after the Stop trace).
The guest ran 37.6 minutes (`wall_ms=2255402`) and stopped clean.

## Diagnosis

`arm_egress` programmed the function-1 rings on whatever the UEFI UNDI
driver left at ExitBootServices. It skipped everything `hw_init` does for
function 0 (Linux `tg3_reset_hw`): no APE driver START for function 1, no
`MISC_HOST_CTRL`, no `CORECLK_RESET` under the GRC lock with the bootcode
magic, no memory arbiter, no `GRC_MODE` (byte/word swap and
`HOST_SENDBDS`, without which the chip does not read send BDs from host
memory), no buffer manager, no FTQ reset. A send ring on that state does
not advance, and a firmware that was never told a driver owns function 1
keeps managing it.

Fix on `cursor/m8-9-close-50bb`: `arm_egress` runs the function-0 sequence
on `01:00.1` through function 1's own APE window (`ApeCtx`): driver START,
`MISC_HOST_CTRL`, halt, `chip_reset_at` (GRC lock requested with function
1's bit), `wait_fw_magic`, `enable_core_blocks`, bus master, rings, MAC,
DMA engines, HOSTCC, completion blocks, mailboxes, then the PHY kick. The
PHY lock still stays with iDRAC. Every COM2 line of the arm goes through
the nowait path so it is not dropped under the earlycon share.

Expected on the next F11, after `ip link set enp0s5 up`:
`boot: M8.9 egress :39 ape=yes ape-evt=sent`, `reset…`, `ape-grc=yes
pci-restore=64`, `fw-magic=yes`, `grc=bswap+wswap memarb=1 bufmgr=1`,
`egress :39 phy … link=up`, `dhcp discover leaves :39`, `rings armed
pci=01:00.01 phylock=no`; then `net post-tx` with `txp=N/N`, `wtx>0`,
`wrx>0`, `rcb>0`, `vrx>0`, and `macmode` holding `e04804` (or the
`e04808` GMII form) at `t2s` and `t10s`.

## Also seen

- `boot: M8.9 ip= on linux line` printed, but the Ubuntu 7.0 kernel has no
  `CONFIG_IP_PNP` and the dracut initrd carries no network module, so the
  M8.9.5 `ip=` injection is inert on this disk. `enp0s5` had no address
  until the operator added one. D7 cannot be automatic on this disk until
  the address reaches the guest another way.
- `boot: M8.9 virtio-net MMIO` printed; no `boot: M8.9 virtio-net mac=`
  line did. To check in the config-read path.
- The Stop trace line is truncated after `ms=1` (ring not drained after the
  guest is off). Fixed on the branch with the egress change.

## What this is

- The bare-Lexar product path lived end to end: PERC image latch → tail
  disk attach → GRUB → Ubuntu `login:`, with no ISO on the stick.
- `:39` layer 1 closed on iron: `bmsr=796d`, `lpa=45e1`.
- The D3 fault is named and on the host side: function 1 never left the
  UNDI state and the firmware re-took it.

## What this is not

- Not D3. `wtx=0/0`, `rcb=0`, `vrx=0`. No guest frame left `:39` and none
  came back.
- Not D7 (no address reached the guest by itself) and not D8.
- Not `RAYNU-V-M7-ISO-INSTALL-OK`. `PERC-LUN-OK` and `PERC-WRITE-OK` are
  the standing M8.7 markers, not new. No disk formatted.
