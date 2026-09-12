# Iron COM2 — `59ac070` Alpine ISO installed to virtio disk; `RAYNU-V-M7-ISO-INSTALL-OK` on R640

- **Date:** 2026-09-10
- **Operator:** vikkp @ raynuvsrv1 (iDRAC `console com2`)
- **Platform:** Dell PowerEdge R640 (real iron; not Latitude, not QEMU, not nested KVM)
- **Boot:** one-time F11 LogiLink UDisk (front USB 2, ~3.8 G, `flashcruzer.sh --init-new-cruzer --allow-new-serial --any-cruzer-usb`)
- **EFI:** CI `--run 34425781629` (`59ac070`; COM2 second line `build: sha=59ac070…` matched)
- **Media:** Stage 46 product ISO `alpine-extended` (~994 MiB) retained from ESP; leftover-DRAM virtio install disk (1 GiB target)
- **Firmware:** RayNu-F (own UEFI tables; OVMF leg bypassed at first exit — ADR-016)
- **Claims:** **`RAYNU-V-M7-ISO-INSTALL-OK`** on iron (E5 install-to-disk)
- **Does not claim:** `RAYNU-V-RAYNU-F-DISK-BOOT-OK` on iron · reboot-to-disk · Everest E5 whole loop · E4 SPA-driven install (Phase B)

## What moved

The console-throughput model that `916af96` named as the link was fixed in
this pin (guest UART TX ring room + 250 µs line-rate pace + COM2 FIFO burst
+ kick throttle) and it held on iron:

- heartbeat `thre … room=3177..4096 pace=3360→5811 fifo=1 drained→97346 win→17488`
  (row 1 of the fix-pin table: pace and window both growing, FIFO bursts of 16)
- `Installing packages to root filesystem... (1/25) … (25/25)` streamed;
  `OK: 25 packages` in ~0.5 s — no `n_tty_write` stall, no stall dump
- `setup-disk -m sys /dev/vda`: GPT written, `grub-install --target=x86_64-efi`,
  initramfs built, **`Installation finished. No error reported.`**
- firmware printed **`RAYNU-V-M7-ISO-INSTALL-OK`** from the iron-only path
  (`guest_virtio_blk::take_iso_install_ok`: hypervisor CPUID bit clear,
  product queues armed, GPT/MBR header on `vda`, ≥ 512 B OUT)

This is the first real distro install on R640 through RayNu-V + RayNu-F.
The 2026-08-16 `BOOTED-FROM-DISK` stamp was an LBA-prefix persist test; this
is a filesystem + bootloader install by the guest's own installer.

## What did not

The auto-answer typed `reboot`:

- `restore host xcr0 … reason=0x1e` (I/O exit — i8042 `0x64 <- 0xFE`, `src=kbc`;
  the `guest reset requested src=` line itself never reached COM2 because the
  earlycon share was dropped with bytes still in the guest TX ring)
- `boot: RayNu-F relaunch after reset (F7)`
- `boot: RayNu-F launch failed: VMCLEAR/VMPTRLD (F2b)`
- leave_to_e4 hold; no `GPT ESP`, no `image=DISK-BOOTX64`, no second
  `Linux version`, no `RAYNU-V-RAYNU-F-DISK-BOOT-OK`

## Root cause (asm-proven, host-side)

`raynu_f_reset_relaunch` in the release `.efi` compiled to
`movl $81024,%eax; call __rust_probestack`: `RAYNU_F_STATE =
FirmwareState::new()` built the 79,704-byte `PagePool` (whole
`FirmwareState` = 85,632 B) on the stack and memcpy'd it into the static.
The guest-UEFI host stack was **4 pages** (16 KiB; `0x140c000..0x1410000`)
and the private VMCS (`0x140b000`) was the next frame down, so the frame
overwrote the VMCS revision dword and `VMPTRLD` answered VMfailValid.
Every VM exit re-enters at stack top, so only the deepest single exit
matters; F7 is the deepest and runs exactly once, after the install.
Nested KVM never tripped it (different frame layout under the L1 host).

No OVMF / foreign firmware state is involved; RayNu-F is ours (ADR-016).

## Fix pin (next green artifact of `cursor/pit-during-apk-b7a8`)

| Change | Where | COM2 tell |
|---|---|---|
| RayNu-F F7 template reset — memcpy from `.rdata` `RAYNU_F_STATE_TEMPLATE`; frame 81,024 → 40 bytes | `vmx/guest_uefi.rs` | none (no failure) |
| guest-UEFI host stack 4 → 32 pages + one `0x5A` guard page below | `vmx/guest_uefi.rs` | `boot: guest-UEFI host stack top=0x… pages=32 guard=0x…` at launch |
| split VMCLEAR / VMPTRLD diagnostics with `rev=` seen vs `want=` (`IA32_VMX_BASIC`) and guard state | `vmx/guest_uefi.rs` | `launch failed: vmptrld rev=… want=… guard=ok\|BREACHED` only on failure |
| flush guest TX ring before dropping the earlycon share | `boot/serial.rs` `flush_guest_tx` | `guest reset requested src=kbc` now visible |

Expected on the next flash after `RAYNU-V-M7-ISO-INSTALL-OK`:
`guest reset requested src=kbc` → `RayNu-F relaunch after reset (F7)` →
`GPT ESP` · `image=DISK-BOOTX64` → `RAYNU-V-RAYNU-F-DISK-BOOT-OK` → second
`Linux version 6.12.13-0-lts` with `root=UUID=` → `login:`.

## Serial excerpt (abridged, operator paste)

```text
build: sha=59ac070...
boot: Stage 46 product ISO retained from ESP ...
boot: RayNu-F direct — OVMF leg bypassed at first exit ... image=ISO-BOOTX64
Linux version 6.12.13-0-lts ... modules=loop,squashfs,virtio_pci,virtio_blk
linux virtio DRIVER_OK ... linux PIT resume paced
virtio stall dump thre ... room=3177 pace=3360 fifo=1 drained=... win=...
Installing packages to root filesystem...
(1/25) Installing alpine-baselayout-data ...
(25/25) Installing ...
OK: 25 packages
setup-disk -m sys /dev/vda
... GPT ... grub-install --target=x86_64-efi ... initramfs ...
Installation finished. No error reported.
RAYNU-V-M7-ISO-INSTALL-OK
virtio stall dump thre ... room=4096 pace=5811 fifo=1 drained=97346 win=17488
reboot
boot: restore host xcr0 ... reason=0x1e ...
boot: RayNu-F relaunch after reset (F7)
boot: RayNu-F launch failed: VMCLEAR/VMPTRLD (F2b)
... leave_to_e4 hold
```

Full SOL paste: operator archive (iDRAC `console com2`, 2026-09-10); add to
[`logs/`](logs/) with `SHA256SUMS` when copied off the workstation.

## Close claim

E5 **install-to-disk** on iron: closed by this run (marker printed on R640
COM2 by the firmware's iron-only path; installer reported no error).
`docs/evidence/r640/STATUS-iso-install` updated.

E5 **reboot-to-disk** on iron: **open** — `RAYNU-V-RAYNU-F-DISK-BOOT-OK`
has nested evidence only
([`2026-09-05-088ab25-f7-reset-src-kbc.md`](../nested/2026-09-05-088ab25-f7-reset-src-kbc.md)).
Do not F11 `34425781629` again expecting it.

Host/CI never prints `RAYNU-V-M7-ISO-INSTALL-OK`; the F7 host gate asserts
the literal is absent from `vmx/guest_uefi.rs`.
