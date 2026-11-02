# 2026-11-02 — `1e7d44c9`: page loaded, Host card flipped

**EFI:** `1e7d44c9affb` (CI run `41508209986`). COM2 `build: sha=1e7d44c9affb`.
**Flash:** Lexar, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Do not F11 this file again.** The page loads on it. The Host card does not stay green, because every SPA reply closes the socket.

## What lived on this boot

Same spare image. `boot: E4 cert rd lba=6190268891 operator`. The guest reached `raynusrv2 login:`.

Firefox to `https://10.99.99.146:8443` signed in. Overview showed Host green ("Server is answering"), RayNu-V green, Guest 3 running, static `10.99.99.146/24`, Operator `vikkp`, Certificate `operator 790 bytes`. One reload of that page also loaded. A drained reply ends with FIN, so Firefox kept the body. On `d3faa4d7` the same reply was a TCP reset and Firefox said it could not connect.

## What did not

A second reload failed. COM2 then scrolled `boot: HOST-NIC TCP accept` and `boot: HOST-NIC TCP re-listen after HTTP` for as long as the page sat open. The Host card went red ("Cannot reach the server") while RayNu-V and Guest 3 stayed green. Those two tiles do not follow the guest-list fetch. The Host tile does: `listVms` sets it down when `fetch` throws, and the 2 s spare poll sets it back up when one request gets through.

`1e7d44c9` closed the socket after every SPA reply, including the five Overview polls that fire every 2 s. Each poll was a new TLS handshake. Six sockets, all mid-handshake, and the next SYN was refused. That refused fetch is the red Host card.

## Change

A SPA `fetch()` does not send `Connection: close`. That reply keeps the TLS session again, so the 2 s polls reuse the sockets they already have. `Connection: close`, a peer FIN, and the 10 minute idle still end with FIN. The library upload still keeps its connection.

When none of the six sockets is listening, the oldest session that has already finished a reply is FIN'd, so a reload still has a socket. A session still in its first handshake is left alone. One socket already on its way back to listen blocks a second close, so the pool is not torn down a slot per tick.

**E4 stays in flight.** The listener is still TLS 1.2. Not a new `ISO-INSTALL-OK`. HDA stays 99%.
