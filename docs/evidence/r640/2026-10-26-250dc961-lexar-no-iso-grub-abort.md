# 2026-10-26 — `250dc961`: bare Lexar ESP, GRUB aborts for lack of RAM

| Field | Value |
|-------|-------|
| Date (UTC) | 2026-10-26 |
| Operator | vikkp |
| Host (service tag / hostname) | D0324Y2 / R640 lab |
| Boot method | USB (F11 one-shot, front USB 2, Lexar 116.1G `USB Flash Drive`) |
| EFI path on media | `\EFI\BOOT\BOOTX64.EFI` |
| `r640-hypervisor.efi` SHA256 | `25c12e9aa83371e914d8c63c816fd53ebb86701631aff6b435e5286fe91f3416` |
| EFI source | `250dc961` (main) · CI `--run 39329353308` · COM2 `build: sha=250dc961bcf4` |
| Flash tool | `./tools/flashcruzer.sh --init-new-cruzer` on `/dev/sdd` (first flash of the Lexar; `RAYNU-V-FLASHCRUZER-OK`) |
| Serial channel | iDRAC SOL `console com2` |

## What this boot was for

Wave 1 iron thread **D3** (guest frames leave and return on `:39` / port 4).
Port 4 was cabled before this boot. The Cruzer that carried
`EFI/RayNu/linux.iso` since September broke; the replacement Lexar was
formatted by `--init-new-cruzer` and holds only `BOOTX64.EFI`.

## What COM2 showed

Both LOM PHYs negotiated for the first time (`bmsr=796d` on `:39`):

```text
boot: Stage 46 no product ISO on ESP — lab El Torito stub
boot: HOST-NIC BCM5720 pre-EBS cand pci=01:00.00 MAC=b0:26:28:5c:5a:38 bmsr=796d
boot: HOST-NIC BCM5720 pre-EBS cand pci=01:00.01 MAC=b0:26:28:5c:5a:39 bmsr=796d
boot: mgmt HTTP listening on 10.99.99.155:8443 (PRE-EBS SNP window)
```

The PERC post read the spare and latched the image, but the leftover DRAM
walk that seeds report-RAM never ran (no `boot: report-RAM extra hpa=` line
between the latch and `RAYNU-V-M1-EBS-OK`):

```text
boot: conventional regions=9 pages_above_1MiB=16570941
boot: frame pool clipped to guest RAM [1MiB,256MiB); BAR/shell window free
boot: frame pool phys=0x1000000 pages=61440
boot: perc ld status=0x00 to=0 n=2 id=0 class=ubuntu bytes=429496467456 id=1 class=spare bytes=3169417691136 pick=1 (not PERC-LUN-OK)
boot: perc copy skip present alt=16777215 (not PERC-BOOT-OK)
boot: perc virtio ro hidden (vda is the 8 GiB image; not PERC-BOOT-OK)
boot: perc image latched — HTTPS will wait for SPA boot or reinstall (not PERC-BOOT-OK)
RAYNU-V-M1-EBS-OK
boot: perc SPA choice tail boot (free tail is vda; setup-disk withheld; 8 GiB window stays; not ISO-INSTALL-OK)
```

The standing SPA came up on the new lease and the operator started the
Ubuntu tail with its saved static NIC:

```text
boot: HOST-NIC coexist listening on 10.99.99.155:8443 (VMX on; ADR-013 Phase F)
RAYNU-V-M8-TLS-OK
RAYNU-V-M7-HOST-NIC-HTTP-OK
RAYNU-V-AUDIT: GuestNicSaved off=19327352832 mode=2
RAYNU-V-AUDIT: PercSpaChoice choice=4
```

The guest launch took the `iso=0` memory path — 32 report-RAM slots, no EPT
pre-map, and RayNu-F handed GRUB only the 20 MiB slab pool:

```text
boot: guest-UEFI report-RAM pool=32
boot: RayNu-F direct — OVMF leg bypassed at first exit (ADR-016; not ISO-INSTALL-OK)
boot: RayNu-F WARN no pre-mapped report-RAM; loader gets the slab pool only (F6-prep)
boot: RayNu-F GPT ESP lba=2048 sectors=1101824 part=1 (F7; not ISO-INSTALL-OK)
boot: RayNu-F found \EFI\ubuntu\grubx64.efi bytes=2828168 (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F grubcfg body=search.fs_uuid 6ded2137-de53-4881-93df-8bbdeb2b20ec root  set prefix=($root)'/grub' configfile $prefix/grub.cfg  (F7 disk; not ISO-INSTALL-OK)
boot: RayNu-F VMLAUNCH entry=0xdb5000 system_table=0x801000 relocs=2 image=DISK-BOOTX64 wall_cap_s=600 (F2b/F5; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-MEM-OK
boot: RayNu-F svc=AllocatePool id=0x205 status=0x8000000000000009 a1=0x2 a2=0x18 a3=0xa0fe48 a4=0xa0fe28 (ADR-016; not ISO-INSTALL-OK)
RAYNU-V-RAYNU-F-CONOUT-OK
memory to remember EFI allocation. Exiting GRUB won't free all memory.
could not allocate all requested memory: 4462 pages still required after iterating EFI memory map
Aborted. Press any key to exit.
```

`status=0x8000000000000009` is `EFI_OUT_OF_RESOURCES`. No `wire=` line
printed because Linux never started.

## Diagnosis

Three gates keyed report-RAM off `product_iso_retained_bytes().is_some()`
alone: the leftover DRAM seed in `boot/handoff.rs`, the 1008-slot allocation
in `report_ram_slots_alloc`, and the EPT pre-map before launch. The old
UDisk had carried the alpine-extended `linux.iso` since September, so every
earlier Ubuntu-tail boot (`c3a70e62`, `d7e2e24f`) was pre-mapped by accident.
With the spare image latched and no ESP ISO, the installed guest got 32 × 2
MiB lazily and GRUB's EFI allocator ran out after the 20 MiB slab pool.

Fix on `cursor/m8-9-close-50bb`: `product_guest_ram_armed()` =
ESP ISO retained **or** `perc_image_boot_latched()`. The latch is set by the
fusion post that runs before the leftover walk in the same handoff. The
frame-pool pick, which runs before the post, prefers the 512 MiB window on
an armed PERC post as well, so the layout matches the lived `c3a70e62`
Ubuntu-tail boots (`frame pool product-ISO iron [1MiB,512MiB)`). Host and
nested never arm or latch, so `iso=0` there still means the E4 SHELL lazy path.

Expected on the next F11 of this branch, in order: `frame pool product-ISO
iron [1MiB,512MiB)`, `report-RAM extra hpa=`, `report-RAM pool=<n> extra=<m>` with `n` well above 32,
`report-RAM EPT pre-map n=`, `RayNu-F high RAM base=0x2000000`, then GRUB
into Linux and the first `wire=` line.

## What this is

- First `bmsr=796d` on `01:00.1` (`:39`). Port 4 is cabled and negotiated.
- Standing SPA on `10.99.99.155:8443`; lease moved from `.145`.
- A product bug: the installed guest depended on a file on the USB stick.

## What this is not

- Not D3. No guest frame left `:39`. No `wire=` line.
- Not `RAYNU-V-M7-ISO-INSTALL-OK`, not `PERC-LUN-OK`. The 2.9 TB VD is still
  not the guest disk. No disk was formatted.
- Not a reason to F11 `250dc961` again on this stick without an ISO.
