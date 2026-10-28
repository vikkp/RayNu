# 2026-10-27 — `0d865c7f`: the SPA-saved address reached the guest by itself (ADR-021 D7 lived; wave 1 closed)

**EFI:** `0d865c7fcb0a` (PR #416, CI `40603110937`, SHA256 `df6fc3465cecf1eaf058d5c8e675501827c16eb55e7b2c9758bc5e68326ad91b`).
**Flash:** Cruzer, F11 one-shot. Ubuntu 26.04 standing on UBUNTU0. Guest: the 10 GiB tail on RAYNU-SPARE (`c3a70e62` install, kernel `7.0.0-38`).
**Operator order:** vNIC Save on the Ubuntu row (manual, `10.99.99.200/24`, gw `10.99.99.1`, DNS `1.1.1.1`), then Start. **Nothing was typed in the guest before the readings below.**

## COM2

```
boot: RayNu-F direct — OVMF leg bypassed at first exit (ADR-016; not ISO-INSTALL-OK)
boot: M8.9 smbios type11 network.network.10-raynu table=345 file=103 + unit-dropin sysinit.target~10-raynu (this boot only; disk unchanged)
boot: RayNu-F BlockIo cd_bytes=0 cd_last_lba=0 disk_bytes=10737418240 disk_last_lba=20971519 (F4; not ISO-INSTALL-OK)
```

`345 = 207 + 138`: the `.network` credential (194 + NUL) and the drop-in credential (29 + 43 + 1 + 64 base64 + NUL) in one Type 11, then Type 127.

## Guest, untouched

```
$ ls /run/systemd/system/sysinit.target.d/
ls: cannot access '/run/systemd/system/sysinit.target.d/': No such file or directory
$ systemctl status systemd-network-generator --no-pager
● systemd-network-generator.service - Generate Network Units from Kernel Command Line
     Loaded: loaded (/usr/lib/systemd/system/systemd-network-generator.service; disabled; preset: enabled)
     Active: active (exited) since Sun 2026-08-16 12:00:09 UTC
    Process: 403 ExecStart=/usr/lib/systemd/systemd-network-generator (code=exited, status=0/SUCCESS)
Aug 16 12:00:09 raynusrv2 systemd-network-generator[403]: Installed /run/systemd/network/10-raynu.network from credential.
$ systemctl show sysinit.target -p Wants -p DropInPaths
Wants=… systemd-network-generator.service …
DropInPaths=/run/systemd/generator.early/sysinit.target.d/10-raynu.conf
$ systemctl show systemd-network-generator -p WantedBy -p DropInPaths -p ActiveEnterTimestamp
WantedBy=sysinit.target
ActiveEnterTimestamp=Sun 2026-08-16 12:00:09 UTC
$ ls /run/systemd/network
10-raynu.network
$ networkctl status enp0s5
● 2: enp0s5
                Network File: /run/systemd/network/10-raynu.network
                       State: routable (configured)
                Online state: online
                      Driver: virtio_net
            Hardware Address: 02:52:4e:38:0b:0d
                     Address: 10.99.99.200
                     Gateway: 10.99.99.1
                         DNS: 1.1.1.1
                Connected To: eero (eero Pro 6E GGB1UD22409407M6) on port 2 (eth0)
Aug 16 12:00:16 raynusrv2 systemd-networkd[555]: enp0s5: Configuring with /run/systemd/network/10-raynu.network.
Aug 16 12:00:16 raynusrv2 systemd-networkd[555]: enp0s5: Link UP
Aug 16 12:00:16 raynusrv2 systemd-networkd[555]: enp0s5: Gained carrier
$ ip addr show enp0s5
2: enp0s5: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc fq_codel state UP
    inet 10.99.99.200/24 brd 10.99.99.255 scope global enp0s5
```

## From the Mac

```
vikkp@Mac ~ % ssh vikkp@10.99.99.200 'hostname; networkctl status enp0s5 | grep -E "Network File|Address: 10"'
raynusrv2
                Network File: /run/systemd/network/10-raynu.network
                     Address: 10.99.99.200
```

## Reading

- The two credentials rode SMBIOS Type 11 through `EFI_SMBIOS3_TABLE_GUID`. PID 1 wrote the `systemd.unit-dropin.sysinit.target~10-raynu` credential to `/run/systemd/generator.early/sysinit.target.d/10-raynu.conf` (the early generator directory, not `/run/systemd/system/`), so `sysinit.target` wanted `systemd-network-generator.service` although Ubuntu ships it disabled.
- The generator installed `/run/systemd/network/10-raynu.network` from the `network.network.10-raynu` credential at `12:00:09`; networkd configured `enp0s5` from it at `12:00:16`. The address, gateway and DNS are the ones saved on the SPA. No operator command in the guest.
- SSH from the Mac to that address reached `raynusrv2`.
- The guest disk was not written. The guest clock is still 71 days behind (known residual).

## Closes

- **ADR-021 D7** (guest address on the customer LAN from the SPA) — lived.
- **M8.9.5** address in the guest — lived; **M8.9** complete with M8.9.6 (`02e0acba`).
- **Wave 1 (Close M8.9)** — D3, D7, D8 all lived. Current wave advances to 2.

## Not claimed

Not a DHCP client in the guest: the saved address is static for both NIC modes (ADR-013). Not auth (A5 parked). Not PERC fleet. Not GA. HDA stays 99. Rollback for an M8.9-era flash: this EFI is a candidate pin; until tagged, `v0.1.0-m8-8-ubuntu`.
