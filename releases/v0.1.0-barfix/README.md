# v0.1.0-barfix — R640 virtio BAR / shell window

Stacks on COM2 SOL + EPT pool fixes. Caps the HV frame pool at **guest RAM
(256 MiB)** so `[256 MiB, 512 MiB)` stays free for virtio-blk/net BAR holes and
shell slabs (`pick_shell_slab_hpa`).

Prior R640 failure after eptfix:
```
boot: frame pool phys=0x1000000 pages=126976   # filled to 512MiB
boot: ERROR — no virtio-blk BAR hole above G0 guest RAM
```

```bash
( cd releases/v0.1.0-barfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-barfix
```

Expect: `frame pool clipped to guest RAM [1MiB,256MiB); BAR/shell window free`
then `M4.3 virtio-blk BAR=…` and VMLAUNCH progress.
