# 2026-11-04 — `c3cdc6c1d7fe`: page loaded, sign-in dropped

**EFI:** `c3cdc6c1d7fe` (CI run `41878369788`, pull-request merge of `ce77b697`).
COM2 `build: sha=c3cdc6c1d7fe run=41878369788`.
The merge is `ce77b697` into `344285bb`. That tree has `GET /host/overview`.

## Boot

`boot: E4 cert rd lba=6190268891 operator`. `boot: D9 host addr static 10.99.99.146/24`. `boot: E6 operator rd lba=6190268907 set`. `boot: HOST-NIC TLS wrap=tls13`. `HOST-NIC coexist listening on 10.99.99.146:8443` with `slots=10`. B5 autostart booted the tail. The guest reached `raynusrv2 login:`. The chassis stayed up. The pasted COM2 ends at that login. It has no `TCP accept` and no `HTTP exchange` line, so the browser session is not in the log.

## What Firefox showed

`https://raynu-v.lab:8443` painted on the first load. The Operator line read `Operator: vikkp`, which is not the HTML default (`Operator: checking`). That line is painted from `GET /auth/operator`, which needs no bearer, after `GET /host/overview` returns 401 to the lab token. Address stayed `This boot: DHCP`, the certificate stayed `Certificate: lab`, and the guest stayed `No guest yet`. Those three are the HTML defaults. They change only after an authenticated overview. The Host card stayed red because a 401 did not mark the server as answering.

Sign in then said **Account dropped**. That string is set only when `POST /auth/login` throws twice, with no HTTP status. A wrong password says **Sign-in refused**. Reload then did not load the page.

**E4 stays in flight.** Not lived. HDA stays 99%.
