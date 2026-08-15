# v0.1.0-eptfix — R640 EPT pool + COM2 SOL

Includes COM1+COM2 serial mirror **and** HV frame-pool preference for
`[1MiB, 512MiB)` so identity EPT covers guest_code / kernel on large-RAM iron.

Prior R640 failure:
```
boot: ERROR — EPT violation GPA=0x47e01000
guest rip=0x1402fd000   # ~5 GiB HPA outside precise EPT
```

```bash
( cd releases/v0.1.0-eptfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-eptfix
# ssh idrac → console com2, map new .img, reboot
```

Look for: `frame pool clipped to precise EPT` then `RAYNU-V-M1-VMEXIT-OK` / shell markers.
