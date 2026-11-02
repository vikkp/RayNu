# 2026-11-02 — `0beeb871`: certificate read lived, first browser connection reset the chassis

**EFI:** `0beeb871` (CI run `41480366600`). COM2 `build: sha=0beeb871…`.
**Flash:** Lexar, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Do not F11 this file again.** The read is lived on it. The handshake is not.

## Read (lived)

Same spare image the save on `12ffaff69fb9` wrote. COM2:

```text
boot: E6 operator rd lba=6190268907 set
boot: E4 cert rd lba=6190268891 operator
```

The bump allocator was selected before the spare walk, so the PKCS#8 parse in the read had a heap. The boot went on to `HOST-NIC coexist listening` on `10.99.99.146:8443`, B5 autostart, and the Ubuntu guest reached `raynusrv2 login:`.

## Handshake (reset)

The Mac opened `https://10.99.99.146:8443`. COM2:

```text
raynusrv2 login: boot: HOST-NIC TCP accept — client connected
boot: HOST-NIC TC
```

and the chassis cold-reset to Ubuntu. No `boot: H2 panic` line. The second line is cut at 17 bytes: one COM2 FIFO burst. During Linux earlycon share every hypervisor line is queued nowait in the guest TX ring and a VM exit drains one 16-byte burst, so the panic dump that followed never reached SOL before `ResetSystem`.

## Diagnosis (host measurement, not COM2)

A counting allocator on the host, same `rsa` / `p256` crates and the same key sizes as the firmware:

| Step | Bytes handed out |
|---|---:|
| `RsaPrivateKey::from_pkcs8_der` (one parse) | 99,663 |
| `Tls12Listen::reset()` on `0beeb871` | 99,663 |
| `Tls12Listen::reset()` after this change | 0 |
| one ECDHE-RSA handshake (server side) | 72,654 |
| wrapped HTTP exchange | 0 |

The post-EBS heap is a leaky bump (`boot/uefi_alloc.rs`): nothing is freed. It was 512 KiB. E4 on `12ffaff6` made `reset()` call `apply_identity()`, which re-parses the key, and `reset()` runs on every accept, every peer close, and every re-listen. The first browser connection to an unknown certificate is accept → handshake → alert/close → re-listen, so by the second connection the bump was gone. `alloc` returned null, the alloc-error panic ran, and the reboot came before the dump drained. Before E4, `reset()` kept the parsed key (`9c1e6b34` `self.key.take()` … put back), which is why the same path lived on wave 4.

## Change

1. `Tls12Listen::reset()` keeps the parsed key. `host_cert::identity_generation()` moves on restore, Save, and Use lab certificate; `reset()` re-parses only when it moved. Host test `reset_keeps_the_parsed_key_until_the_identity_moves` counts parses.
2. `BUMP_N` 512 KiB → 4 MiB. `.bss`, so the EFI file size does not change. About fifty handshakes per boot instead of six. The handshake leak itself (~73 KiB) is not fixed here.
3. `reboot_after_dump` prints `boot: H2 heap used=N of M (post-EBS bump active)` after the panic line and calls `serial::flush_guest_tx()` before `VMXOFF` / `ResetSystem`, so the next panic names itself on COM2.

## Next F11

Expect `boot: E4 cert rd lba=6190268891 operator`, then the browser connection: `TCP accept`, the certificate warning or the sign-in page, `HOST-NIC HTTP exchange ok`. If the chassis still resets, COM2 now carries `boot: H2 panic …` and the heap line.

**E4 stays in flight.** The listener is still TLS 1.2. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
