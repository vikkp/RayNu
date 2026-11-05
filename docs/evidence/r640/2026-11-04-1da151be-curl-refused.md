# 2026-11-04 — `1da151be06e0`: curl Connection refused

**EFI:** `1da151be06e0` (CI run `41539812411`, pull-request merge of `07d61078`).
COM2 `build: sha=1da151be06e0 run=41539812411`.
**Leave this EFI.** Do not treat the Back/Forward page as a fresh load.

## What the Mac showed

Firefox at `https://raynu-v.lab:8443` (`hosts` entry `10.99.99.146`) did not finish a direct load or a refresh. Back, then Forward, showed the page already painted. Sign-in on that document said **Account dropped**.

From the same Mac, `10.6.0.6`:

```text
curl -vk --http1.1 --max-time 20 https://10.99.99.146:8443/
connect to 10.99.99.146 port 8443 from 10.6.0.6 port 58456 failed: Connection refused
curl: (7) Failed to connect to 10.99.99.146 port 8443 after 19 ms
```

19 ms is a TCP reset, not a timeout. The address answered. smoltcp sends that reset when no slot is in LISTEN. The same shape lived on `6dc1f554` with one slot. This boot has ten, and all ten were established.

COM2 on the boot itself had `boot: E4 cert rd lba=6190268891 operator`, `boot: HOST-NIC TLS wrap=tls13`, and `HOST-NIC coexist listening on 10.99.99.146:8443`. The guest reached `raynusrv2 login:`. The curl was two days later. No new `TCP accept` line belongs with a reset.

**E4 stays in flight.** Not `ISO-INSTALL-OK`. HDA stays 99%.
