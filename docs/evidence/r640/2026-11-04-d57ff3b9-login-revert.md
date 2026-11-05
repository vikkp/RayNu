# 2026-11-04 — `d57ff3b9b08c`: sign-in dropped again; connection-shrinking reverted

**EFI:** `d57ff3b9b08c` (CI run `41886943984`). COM2 `build: sha=d57ff3b9b08c`.
Second iron boot of the wave-5 E4 connection work after `c3cdc6c1d7fe`.

## Boot

`boot: M8.9 egress :39 link=up`, the guest reached `raynusrv2 login:`, the
chassis stayed up. The pasted COM2 tail ends with two
`boot: HOST-NIC TCP accept — client connected slot=0` / `slot=1` and nothing
after them. The host-NIC `HTTP exchange ok` / `keep-alive` lines are
suppressed while the guest owns COM2 (`serial::linux_high_half()`), so the
log shows only the two accepts, not the exchanges on them.

## What Firefox showed

`https://raynu-v.lab:8443` loaded in one shot. `Operator: vikkp` painted.
Typing `vikkp` and the password then said **Account dropped** —
`POST /auth/login` threw twice with no HTTP status, same as `c3cdc6c1`.

## Regression and fix

Sign-in worked on `1cf827952140` (`2330f33b`) and failed on both
`c3cdc6c1d7fe` (`ce77b697`) and `d57ff3b9b08c`. The difference is the
connection-shrinking added after `2330f33b`:

- `ce77b697` — a reply on a non-kept session answers `Connection: close`
  when fewer than two sockets are listening, and the slot FINs.
- `d57ff3b9` — a finished handshake with no request FINs after 3 s, and
  only two sessions are kept at once.

Both make the browser open a **new** connection for the login POST. A
smoltcp coexist slot resets a SYN when no socket is in LISTEN, and the
coexist tick only runs on vmexits — during the Ubuntu guest, `skip_idle`
dominates, so the tick cannot re-arm a LISTEN slot between the browser
closing a connection and the login SYN. The SYN is reset, `fetch` throws,
the one retry hits the same window, and the SPA shows **Account dropped**.

This commit reverts the connection-shrinking to `3a7e1183`:
`mgmt/host_nic.rs`, `mgmt/host_nic_listen.rs`, `mgmt/http.rs`,
`mgmt/tls12.rs` (and the matching tests). The one-poll Overview
(`GET /host/overview`) and the burst re-listen that fixed the reload stay;
the 10-minute keep-alive is back; there is no `Connection: close`, no 3 s
spare FIN, no two-session cap. With connections kept, the login POST reuses
an established connection and needs no fresh SYN.

## Host verification

Real Firefox 159 over the reverted freestanding TLS 1.3 stack, operator
account armed: page load, sign-in, two reloads. The server log shows
`POST /auth/login -> HTTP/1.1 200 OK` and the polls flip from 401 to 200
(`/vms -> 200`), and both reloads served `GET / -> 200`. Host std TCP has a
kernel backlog, so it cannot reproduce the iron SYN-reset directly; the
host run proves the reverted codec/TLS serves the full login + reload flow.

**E4 stays in flight.** Not lived. HDA stays 99%. The iron close still
needs one boot that reads the operator certificate, finishes TLS 1.3,
signs in, and survives two reloads on COM2.
