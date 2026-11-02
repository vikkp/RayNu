# 2026-10-31 — `f2828d95`: operator account (ADR-021 E6 lived)

**EFI:** `f2828d95d33d` (PR #427, CI run `41206547641`). COM2 `build: sha=f2828d95d33d run=41206547641`.
**Flash:** Lexar, label `RAYNUV`, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Rollback:** the wave 4 stick `11688cbfa078` / run `41203867999`. Older wave-2 pin remains `v0.1.0-m8-9-lan`.

## Save

Overview **Save account**, name `vikkp`. The password was not printed. COM2:

```text
boot: E6 operator wr lba=6190268907 match
RAYNU-V-AUDIT: OperatorAccountSet name_len=5
```

`name_len=5` is `vikkp`. The sector is `RAYNUOPR`, one LBA before the host-address sector. It holds a salt and SHA-256. The password is not on the spare.

## Next F11

Same stick. No second flash.

```text
boot: E6 operator rd lba=6190268907 set
boot: D9 host addr rd lba=6190268908 static
boot: D9 host addr static 10.99.99.146/24 gw=10.99.99.1 dns=0.0.0.0 name=
RAYNU-V-AUDIT: AuthDenied method_tag=1
boot: D10 ntp 162.159.200.123 unix=1793455341
boot: B5 autostart start off=19327352832
```

`1793455341` is 2026-10-31 09:02 CT. `AuthDenied method_tag=1` is a GET while the page still held `raynu-v-bringup`. Sign in then showed **Operator: vikkp**, the host answering, and guest 3 running. Ubuntu reached `raynusrv2 login:`.

The early ESP probe still printed `boot: E4 auth: bring-up token (lab; no auth.token)`. That file is absent. The standing page after the spare read is the password account.

**E6 lived.** Wave 5 stays open. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
