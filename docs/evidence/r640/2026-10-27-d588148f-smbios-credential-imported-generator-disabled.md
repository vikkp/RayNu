# 2026-10-27 — `d588148f`: SMBIOS Type 11 credential reached PID 1; Ubuntu's generator is disabled

**EFI:** `d588148f0058dc81e0cb5fa3f1f9ef5438c69381` (PR #416, CI `40507593718`, SHA256 `343f5cf643d2ff62593849a496f5e84f2332127c261f6e63f83e47da664e629b`).
**Flash:** Cruzer, F11 one-shot. Ubuntu 26.04 standing on UBUNTU0. Guest: the 10 GiB tail on RAYNU-SPARE.
**Operator order:** vNIC Save on the Ubuntu row (manual, `10.99.99.200/24`, gw `10.99.99.1`, DNS `1.1.1.1`), then Start. No `ip` command typed in the guest.

## COM2

```
boot: RayNu-F direct — OVMF leg bypassed at first exit (ADR-016; not ISO-INSTALL-OK)
boot: M8.9 smbios type11 network.network.10-raynu table=207 file=103 (this boot only; disk unchanged)
boot: RayNu-F BlockIo cd_bytes=0 cd_last_lba=0 disk_bytes=10737418240 disk_last_lba=20971519 (F4; not ISO-INSTALL-OK)
...
boot: M8.9 ip= on linux line (this boot only; disk unchanged)
```

`file=103` = `[Match]` + `MACAddress=` + `[Network]` + `Address=10.99.99.200/24` + `Gateway=10.99.99.1` + `DNS=1.1.1.1`. `table=207` = Type 11 (5) + string (194 + NUL) + NUL + Type 127 (6); 194 = 29 + 24 + 1 + base64(103) = 140.

## Guest (`vikkp@raynusrv2`, no address typed)

```
$ cat /proc/cmdline
BOOT_IMAGE=/vmlinuz-7.0.0-38-generic root=/dev/mapper/ubuntu--vg-ubuntu--lv ro console=ttyS0 ... cloud-init=disabled ...
$ ls /sys/firmware/dmi/entries
11-0  127-0
$ sudo systemd-creds --system list
NAME                     SECURE SIZE PATH
network.network.10-raynu secure 103B /run/credentials/@system/network.network.10-raynu
$ sudo systemd-creds --system cat network.network.10-raynu
[Match]
MACAddress=02:52:4e:38:0b:0d

[Network]
Address=10.99.99.200/24
Gateway=10.99.99.1
DNS=1.1.1.1
$ ls /run/systemd/network
ls: cannot access '/run/systemd/network': No such file or directory
$ networkctl status enp0s5
● 2: enp0s5   Network File: n/a   State: off (unmanaged)   Driver: virtio_net   Hardware Address: 02:52:4e:38:0b:0d
$ systemctl status systemd-network-generator.service
○ systemd-network-generator.service - Generate Network Units from Kernel Command Line
     Loaded: loaded (/usr/lib/systemd/system/systemd-network-generator.service; disabled; preset: enabled)
     Active: inactive (dead)
$ systemctl cat systemd-network-generator.service | grep -iE 'Credential|ExecStart|WantedBy'
ExecStart=/usr/lib/systemd/systemd-network-generator
ImportCredential=network.netdev.*
ImportCredential=network.link.*
ImportCredential=network.network.*
WantedBy=sysinit.target
$ journalctl -b -u systemd-network-generator
-- No entries --
$ systemctl is-active systemd-networkd
active
$ systemctl --version | head -1
systemd 259 (259.5-0ubuntu3.4)
```

## Reading

- The SMBIOS 3.0 table reached Linux through `EFI_SMBIOS3_TABLE_GUID` (`11-0`, `127-0`).
- PID 1 imported the credential from Type 11: 103 bytes, body identical to what RayNu-F built.
- `systemd-network-generator.service` is **disabled** on this Ubuntu (`preset: enabled`, netplan is their renderer). It never ran, so no `/run/systemd/network/10-raynu.network`, no match, `enp0s5` unmanaged and down.
- `/proc/cmdline` has no `ip=`: the old amend hit a `linux` line GRUB did not boot. That path is not the product path.
- `systemd-networkd` is active, so a `.network` is honored once written (proved below with a manual `systemctl start`).

## Diagnosis on the same boot (operator-typed; runtime only; not the close)

```
$ sudo systemctl start systemd-network-generator
$ ls /run/systemd/network
10-raynu.network
$ sudo networkctl reload
```

COM2 then printed the function-1 arm (`egress :39 ape=yes ape-evt=sent … fw-magic=yes … link=up speed=100 duplex=full`, `dhcp discover leaves :39`, `rings armed pci=01:00.01`, `guest frame leaves :39 len=42`), and the trace reached `txp=15/14 wrx=117/39827 vrx=133` at +10 s.

```
$ networkctl status enp0s5
  Network File: /run/systemd/network/10-raynu.network
  State: routable (configured)   Online state: online
  Address: 10.99.99.200   Gateway: 10.99.99.1   DNS: 1.1.1.1
  Connected To: eero (eero Pro 6E …) on port 2 (eth0)
  systemd-networkd[553]: enp0s5: Configuring with /run/systemd/network/10-raynu.network.
$ ip addr show enp0s5
  inet 10.99.99.200/24 brd 10.99.99.255 scope global enp0s5
```

Every hop after the generator works on this Ubuntu: the credential body is a valid `.network`, the MAC match selects `enp0s5`, networkd configures the saved address, gateway and DNS, and the guest sees the LAN switch over LLDP. The only missing piece is that the generator did not run at boot.

## Fix (next EFI)

A second Type 11 string, `io.systemd.credential.binary:systemd.unit-dropin.sysinit.target~10-raynu=<base64>` of `[Unit]\nWants=systemd-network-generator.service\n`. systemd 256+ writes `systemd.unit-dropin.*` credentials to `/run/systemd/system/<unit>.d/` before loading units, so `sysinit.target` wants the generator whatever its enable state. Runtime only; the guest disk is not written. COM2 will read `... file=103 + unit-dropin sysinit.target~10-raynu (this boot only; disk unchanged)`.

## Not claimed

Not D7. The address did not land by itself. Scores held. Rollback stays `v0.1.0-m8-8-ubuntu`.
