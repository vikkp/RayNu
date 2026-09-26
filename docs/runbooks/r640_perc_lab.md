# R640 lab disks — read this before SSH, racadm, flash, or MegaRAID

**Lived:** 2026-09-26, operator `lsblk` on the lab PowerEdge after a PERC clear and an Ubuntu reinstall.  
**Service tag:** D0324Y2. **BIOS:** 2.2.11. **iDRAC:** 3.34.34.34 at `https://10.99.99.24`.  
**What this is:** the disk map future agents use. It is a hardware prerequisite. It is not `RAYNU-V-M8-PERC-LUN-OK`.

RayNu-V is **not** the running OS. Ubuntu 26.04 is the standing boot. The empty spare is for a future MegaRAID driver, and that driver has not been started.

Start at [`docs/m8_state.md`](../m8_state.md) for the persist / SPA history. This file is the disk map.

---

## How to tell the disks apart

Linux letters move. Match **size** (and the WWNs below). Do not script `/dev/sda`.

`lsblk` on 2026-09-26 (this boot’s letters):

| Letter that day | Size | What it is | Mount |
|-----------------|------|------------|-------|
| `sda` | 400G | PERC VD **UBUNTU0**, boot | `sda1` 1G vfat `/boot/efi`; `sda2` 398.9G ext4 `/` |
| `sdb` | 2.9T | PERC VD **RAYNU-SPARE** | none. No `FSTYPE`. |
| `sdc` | 3.8G | Cruzer / General_UDisk, front USB 2, label `RAYNUV` | not mounted |
| `sdd` | 298.1G | Toshiba Alpine persist (`0480:a004`) | not mounted. `sdd1` 48M vfat, `sdd2` 8G ext4 |

`findmnt` that day: `/` = the 400G ext4, `/boot/efi` = the 1G vfat. `fstab` lists root UUID `f5ddacc0-52a3-4260-bb52-634c87a3765b`, EFI `4F32-1D11`, and `/swap.img`. `fstab` does not name the 2.9T disk. The ext4 mount showed `stripe=192`.

Installer WWNs (the installer does not show PERC names):

| WWN | Size | VD |
|-----|------|----|
| `364cd98f082619900324a8bf37148b319` | 399.999G | UBUNTU0 |
| `364cd98f082619900324a8ce0d3477a66` | 2.882T | RAYNU-SPARE |

Safe read: `lsblk -o NAME,SIZE,FSTYPE,LABEL,UUID,MOUNTPOINT` and `findmnt`. Stop if the 400G disk is not the one mounted at `/`.

---

## Controllers

Two controllers appear in F2 **Device Settings**. Open only the first.

| Open? | Menu name | Role |
|-------|-----------|------|
| Yes | Integrated RAID Controller 1: **Dell PERC H740P Mini** | Lab disks. Ubuntu and the spare. |
| No | RAID Controller in Slot 1: **PERC H840 Adapter** | Not inventoried. Do not open it. Do not clear it. |

H740P Mini, from the controller dashboard the day the VDs were created:

| Field | Value |
|-------|-------|
| Serial | `97101GC` |
| PCI | `0000:18:00.0` |
| Vendor / device | `0x1000` / `0x0016` (MegaRAID SAS) |
| Package | 51.16.0-4076 |
| Firmware | 5.160.00-3530 |
| NVDATA | 5.1600.06-0003 |
| BBU | Yes |
| Mode | RAID |
| Slot | Integrated |
| Physical disks | 5, SAS |
| Bays | `01:00`–`01:04`, each ~1.091 TB / 1117.25 GB, 512-byte sectors |

---

## Virtual disks (Disk Group 0, RAID-6)

Both VDs share one disk group. Five drives, RAID-6: three data, two parity. Usable capacity before the carve was 3351.75 GB (the old 200 GB VD plus the old 3151.75 GB VD). Those old VDs are gone.

| VD | Name | Size | Boot | Contents |
|----|------|------|------|----------|
| 0 | **UBUNTU0** | 399.999 GB | **BOOT DEVICE** | Ubuntu 26.04. `/boot/efi` ~1G fat32, `/` ~399G ext4. |
| 1 | **RAYNU-SPARE** | 2.882 TB | not boot | Empty. No partition. No filesystem. No LVM. |

Policies on both, as created: stripe **256 KB**, Read Ahead, Write Back, **Disk Cache Disable**, Default Initialization **Fast**, Secure VD unchecked.

Disk Cache Disable is deliberate. The BBU covers controller Write Back. It does not cover the drives’ own cache. Fast init was required so the Ubuntu installer would not see the old LVM labels.

The ~100 GB the old Ubuntu looked like was a filesystem inside the old large VD. It was not free capacity on the controller. A byte range inside UBUNTU0 is still the Ubuntu virtual disk. A MegaRAID mailbox command addresses the controller. A future driver may use **only** RAYNU-SPARE, and it still has to refuse UBUNTU0.

---

## What you may do, and what you leave alone

**Leave alone**

- Do not partition, `mkfs`, mount, or LVM **RAYNU-SPARE**.
- Do not shrink, delete, or reformat **UBUNTU0**.
- Do not Clear Configuration, Set Factory Defaults, Import Foreign, or Auto Configure RAID 0. Those screens already ran once. Running them again destroys this Ubuntu.
- Do not open the H840.
- Do not format the Toshiba. Do not flash it. The 8 GiB virtio cap stays.
- Do not write a PERC volume from `flashcruzer.sh`. The script picks label `RAYNUV`. Disk letters are not a target.
- Do not `--init-new-cruzer`. Do not put `usbsoak.txt` back. `raynuf.txt` stays.
- Do not F11 `c4a41a17`, `e5cca2e0`, `36d3b559`, `fa6ce771`, `3e9ce45e`, or `3f80abd0`.
- Do not `setup-disk` by hand. Do not curl Start or `POST /vms/1/start` on a persist boot. One TCP slot.
- Do not type `normal` at `grub>`. Do not click Create or Start on the SPA while a persist guest is the point of the boot.
- Do not add rustls to `uefi-bin`. USB, xHCI, BOT, mgmt HTTP, and MegaRAID stay outside the Proven Core.
- Host, CI, and nested QEMU never `println` `RAYNU-V-M8-PERC-LUN-OK` (or the other iron `RAYNU-V-M8-*-OK` / `RAYNU-V-M7-ISO-INSTALL-OK` markers).

**In scope when the operator asks**

- Read-only `lsblk` / `findmnt` / `storcli` or MegaRAID inquiry that does not write.
- **M8.7 host** (`mgmt/megaraid.rs`) packs frames and refuses the 400 GB VD. It does not map BAR0. A doorbell is a later step, and only when asked.
- A5 (ESP `EFI/RayNu/auth.token`, reject `raynu-v-bringup`) is **parked**. The operator skipped it. The product rule that stays is: the password is not compiled into the EFI or the SPA. Do not start A5 unless they ask.

Scores stay put until iron I/O exists. `piece_perc_pct` is 15. Bar B is 18. HDA overall is 99, months 0.0. An empty virtual disk does not move them.

---

## How to reach the machine

| Path | Address | Notes |
|------|---------|-------|
| SSH (Ubuntu) | `vikkp@10.99.99.151` | DHCP. The lease can move. Prompt hostname is `raynusrv1`. Older notes say `raynuvsrv1`. Trust the IP and the disk sizes. |
| Host key after this reinstall | `SHA256:e8RgOlR34L9cvcIbhNRn12T1AVof8MiqlzmsZm6az1I` | Expected, because the OS was reinstalled. A changed-key warning against a **different** fingerprint is a stop. |
| iDRAC web / SSH | `https://10.99.99.24` | Dedicated iDRAC NIC. Dashboard hostname `USADDS19FP1.corpnet.ad.local`. |
| SOL | SSH to `10.99.99.24`, then `console com2` | Leave SOL with `Ctrl-\` before `racadm`. |
| RayNu HTTPS, only while the EFI is booted | LOM MAC `b0:26:28:5c:5a:38`, `https://raynu-v.lab:8443` | Lab CA: `assets/tls/lab-ca.crt.pem` (on BadaaMac, `/tmp/raynu-lab-ca.crt.pem`). PRE-EBS listen is plaintext `http://` and ends at ExitBootServices. |

iDRAC 3.34 **Storage → Overview → Virtual Disks** can Blink and Unblink. It cannot delete. **Configuration → Storage Configuration** returns RAC0503 (“no out-of-band capable or re-configurable controllers”) even with the host off. Virtual-disk create and delete on this firmware is **F2 System Setup → Device Settings → H740P Mini**.

---

## How this layout was built (do not repeat the clear)

The five disks were already in one full RAID-6 group, so Free Capacity was empty and a third VD could not be added beside Ubuntu. The operator accepted losing the old Ubuntu.

1. F2 → Device Settings → **H740P Mini** (not the H840).
2. Virtual Disk Management said no virtual disks, and Controller Management showed Virtual Disk Count 0, while the iDRAC dashboard still counted 2. **Clear Configuration** on the H740P Mini was what actually freed the disks. Preserved cache was not offered on that page.
3. Create Virtual Disk from **Unconfigured Capacity**, all five Ready disks checked, RAID-6, name `UBUNTU0`, 400 GB, Disk Cache **Disable**, initialization **Fast**.
4. Second VD from **Free Capacity** on Disk Group 0, name `RAYNU-SPARE`, 2.882 TB, same policies.
5. Set Boot Device = **UBUNTU0**.
6. F11 one-shot UEFI boot → **Virtual Optical Drive** (`ubuntu-26.04-live-server-amd64.iso` as iDRAC virtual CD). The stale NVRAM entry named Ubuntu, front USB 2 (UDisk), and back USB 1 are the wrong row.
7. GRUB “Try or Install Ubuntu Server” does not choose the PERC disk. The storage screen does. Custom layout on the **399.999G** disk only: new fat32 `/boot/efi` (~1.049G) and new ext4 `/` (~398.947G). The 2.882T disk, the Toshiba, and the UDisk stayed unused.

After install, SSH host key changed. `ssh-keygen -R 10.99.99.151`, then accept the fingerprint above.

The clone at `~/projects/raynuv` and the Alpine ISO were on the old root. They are not guaranteed on this install until someone checks. Packages the flash path wants: `git`, `curl`, `ca-certificates`, `python3`, `gh`, `gdisk`, `dosfstools`, `ovmf`. Launcher: `tools/flashcruzer.sh --install-launcher`. Do not flash unless the operator asks. `--any-cruzer-usb` is for LogiLink `abcd:1234`. Identify the stick by label `RAYNUV`.

---

## EFI pins (RayNu is not booted)

GitHub Latest stays the Everest loop. Do not move Latest.

| Pin | When to use it | Identity |
|-----|----------------|----------|
| `v0.1.0-everest-closed` (Latest) | Original ISO loop on leftover DRAM | `f72b4276` / CI `34552377351` / SHA256 `e74460ff0e248a06d2e4ab546684d1006edd25855f50c8dc27dc202facab9cbc` / COM2 `build: sha=f72b4276d198` |
| `v0.1.0-m8-a4s` (not Latest) | Standing SPA rollback, no keyboard, no power button | `1f33eeda72f9` / CI `36137732145` / SHA256 `5539d83806e120eb6c331c11281407c0f902b121a770c7faa46d6a1a26280693` |
| `v0.1.0-m8-a6` (not Latest) | Last lived keyboard + host power-off | `fd2ca12e8eb9` / CI `36166108942` / EFI 2,066,944 bytes / SHA256 `be0df621ad0a7b03cd7817525b8122d61b7daaf23f29d5d2db67ef4d4c2bcba9` |

The A6 docs commit `f7a84e48` stamps `build: sha=f7a84e48cf1e`. The release binary is the earlier CI (`36166108942`), not that docs run. Do not F11 `34548550755` / `7f8dc0a9`.

---

## What is still open

- **M8.7 host** packs the LD-list frame and a one-block READ(16). `mgmt/durable_lun.rs` still skips the whole PERC (`skip PERC`). No doorbell has been sent. PRE-EBS UEFI RAID BlockIo dies at ExitBootServices. The iron close is still `RAYNU-V-M8-PERC-LUN-OK`, and host/CI never print it.
- USB persist on the Toshiba is a different close from PERC persist. Latitude and QEMU are a different close from this R640.
- A6 lived once (`abc` in Activity, `RAYNU-V-M8-CONSOLE-OK`, Host green), then Power off host left iDRAC Off. That history stays. The chassis was powered back on for this reinstall. **not VNC**.
- A5 stays parked.
