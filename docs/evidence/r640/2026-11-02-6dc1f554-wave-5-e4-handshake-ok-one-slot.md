# 2026-11-02 — `6dc1f554`: operator handshake lived, one listen slot then wedged

**EFI:** `6dc1f554` (CI run `41489680281`). COM2 `build: sha=6dc1f554bc92 run=41489680281`.
**Flash:** Lexar, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Do not F11 this file again.** The handshake is lived on it. The page is not, because the listener has one TCP slot.

## Read and handshake

Same spare image the save on `12ffaff69fb9` wrote.

```text
boot: E6 operator rd lba=6190268907 set
boot: E4 cert rd lba=6190268891 operator
boot: HOST-NIC coexist listening on 10.99.99.146:8443 (VMX on; ADR-013 Phase F)
boot: B5 autostart start off=19327352832
```

The guest reached `raynusrv2 login:`. Firefox to `https://raynu-v.lab:8443` (hosts entry `10.99.99.146`) showed the certificate warning. That page is drawn only after the server certificate flight, so the operator certificate was presented and the chassis stayed up. On `0beeb871` this is where the box reset.

Accepting the warning opened a second connection. COM2:

```text
boot: HOST-NIC TCP accept — client connected
boot: HOST-NIC TCP re-listen after HTTP
boot: HOST-NIC TCP accept — client connected
```

The SPA then rendered (Overview, Power off host, Panic dump, Certificate line still on its placeholder). Sign-in made the page go away. After that, `ping 10.99.99.146` was 0% loss and there was no `boot: H2 panic` line, but `nc` and `curl` to `:8443` were `Connection refused` and COM2 never printed another `re-listen`. Quitting Firefox did not free the slot. The listener has one TCP socket. It had accepted the sign-in connection and was holding it (keep-alive, or a close whose TX queue never drained — that wait had no timeout). Every later SYN was reset.

## Change

1. Six listen sockets. smoltcp hands a SYN to the first socket still in `LISTEN`. One held connection no longer refuses the page's other probes.
2. A pending close that still has TX queued after 2 s is aborted and put back in `LISTEN`. The 8 ms drain for a queue that does empty is unchanged.
3. When none of the six is listening, the held connection idle the longest (at least 15 s) is aborted. An open page polls faster than that. The 10 minute keep-alive on a connection the page is still using is unchanged (`3e9ce45e` re-handshook at 30 s and the chassis went off).
4. The post-EBS heap frees on `dealloc` and coalesces. A handshake's `rsa` / `p256` temporaries (~73 KiB, measured) come back when those values drop. A rewind arena would also free the `EphemeralSecret` the session still holds. Host tests: a freed block is reused, neighbors coalesce, align 64, exhaustion then a free.

**E4 stays in flight.** The listener is still TLS 1.2. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
