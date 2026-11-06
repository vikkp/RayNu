# 2026-11-05 — `fa3f4860c6a7`: in-place update and rollback (ADR-021 A7 lived)

**Close boot:** COM2 `build: sha=fa3f4860c6a7 run=42096983983`. No `boot: A7 stick restore` and no `boot: A7 stick wrote` on that boot. SPA `https://10.99.99.201:8443/` showed the same build, Guest 3 running, and `active 1 · pending none · durable yes`.
**Image that restored the stick:** `build: sha=318c6617c99b run=42133864880`. Signed file SHA256 `7cca349c07b7a435ec665d97db4718c6865a749ceaa93c1193a3b4e144000095`. Private key stayed in `~/raynu-keys`. It is not in git.
**File put back:** `fa3f4860c6a7` signed SHA256 `3c2a633d9360a3eb6d57f2afa830d6d15fa72a86741603be6769fdb12ec0a478`, 2,599,968 bytes. That is `EFI/BOOT/BOOTX64.OLD` from the copy, written back over `EFI/BOOT/BOOTX64.EFI`.
**Stick:** Lexar, front USB 2. Guest disk on the PERC was not the stick. UBUNTU0 was not formatted.

## What iron showed

From `fa3f4860c6a7`, Keep armed slot 1. The next boot of that same image printed `boot: A7 stick read slot=1` and `boot: A7 stick wrote bytes=2599968`, then `boot: A7 stick clear` and `boot: A7 wr lba=6181880318 match` before the guest started.

The following boot was `build: sha=318c6617c99b run=42133864880`. It did not write the stick again. `boot: A7 rd lba=6181880318 slot=1 pending=none`. Overview: `active 1 · pending none · durable yes · old saved`.

The guest was stopped. Roll back changed the line to `active 1 · pending none · durable yes · restore old`. COM2 then printed `boot: A7 wr lba=6181880318 match`.

The next boot was still `318c6617c99b` and printed `boot: A7 stick restore` and `boot: A7 stick restored bytes=2599968`, then `boot: A7 stick restore clear` and `boot: A7 wr lba=6181880318 match` before autostart. The guest reached `raynusrv2 login:`.

The F11 after that was `build: sha=fa3f4860c6a7 run=42096983983` with the stick left alone. The SPA showed Guest 3 running.

An earlier pass also copied `1f74a8e8ec45` / run `42093029301` (`boot: A7 stick wrote bytes=2598432`, signed SHA256 `adfc0e2cf35b1bfde7a6583506514e370bbea49b31a3ad9614adbc5152bbaa24`) and booted it. That image does not set `swapped`, so Roll back on the page had nothing to arm. The close used `318c6617c99b`, which does.

`fa3f4860c6a7` and `318c6617c99b` still wait until the guest is stopped before the spare header is written. `d0904bf8` writes one spare sector between guest VM exits. That commit was not this F11.

**A7 lived.** Wave 6 stays open on the rows after A7. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
