# 2026-11-07 — `4e327820e039`: spare write finishes while Guest 3 stays up (ADR-021 A4 lived)

**Close page:** SPA `https://10.99.99.201:8443/` on `build 4e327820e039 · CI 42384654720`. Host answering, hypervisor running, Guest 3 running. Update line `active 1 · pending 0 · durable yes · saved`. Exit line `reason=0x1c rip=0xffffffe94` is the normal first guest exit. The serial console stayed closed. The operator reloaded the page until it answered, then staged from that page.

**Image:** push run `42384654720`, commit `4e327820e03918a1d530c18af2d6f74d68f3b5d7`. Unsigned SHA256 `e6a2ec783cf9228d307c282b458894616f4e2fc19209562ca5f007fe7f392baf`, 2,598,912 bytes. Signed SHA256 `1101880eb670530ee3e23a5499cc9b0a2ac5579954c755dbe455fa9aa2b1995a`. `sbverify` OK. Private key stayed in `~/raynu-keys`. It is not in git.

**Stick:** Lexar `RAYNUV`, front USB. `EFI/BOOT/BOOTX64.EFI` is that signed file. `EFI/BOOT/BOOTX64.B75A` is the previous image `b75acccce337`. UBUNTU0 was not formatted. The 2.9 TB PERC VD was not mounted.

## What iron showed

`b75acccce337` / run `42144210729` opened with the line ending in `saved` and Guest 3 running. Staging then stayed `pending 0 · durable no · saving`. That image wrote spare sectors only from the guest-off loop.

`4e327820e039` writes one spare sector on the standing SPA tick. After F11 the page showed `saved` with Guest 3 already running. Staging `r640-hypervisor-4e32-signed.efi` moved the line to `pending 0 · durable no · saving`, then to `pending 0 · durable yes · saved`, with the Guest 3 tile still running. **Keep this slot** was not pressed. The stick file is already this image, so the pending slot does not arm a copy.

The line stays on `saving` until the last sector. There is no byte counter on this page.

**A4 lived.** Wave 6 stays open. Next host-first row is H5. Not a new `ISO-INSTALL-OK`. HDA stays 99%. LOIHDA stays Bar A 94 / Bar B 72 / overall 80.
