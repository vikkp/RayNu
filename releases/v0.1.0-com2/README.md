# v0.1.0-com2 — R640 SOL fix build

Same 0.1.0 tree plus **COM1+COM2 UART mirror** so Dell iDRAC
`console com2` shows post-M0 markers (not only ConOut / M0 freeze).

| File | Notes |
|------|--------|
| `r640-hypervisor.efi` | SHA256 in sidecar |
| Built UTC | see `VERSION` |

```bash
cd releases/v0.1.0-com2 && shasum -a 256 -c r640-hypervisor.efi.sha256
cd ../..
./tools/make-boot-media.sh --kit releases/v0.1.0-com2
```

Boot with SOL open (`ssh` → `console com2`) **before** reboot. Expect past M0:
`boot: M0 complete…`, `RAYNU-V-M3-ASSETS-OK`, `RAYNU-V-M1-EBS-OK`, then VMX markers.
