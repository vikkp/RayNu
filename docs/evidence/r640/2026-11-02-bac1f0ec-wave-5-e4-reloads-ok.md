# 2026-11-02 — `bac1f0ec5139`: SPA and both reloads

**EFI:** `bac1f0ec5139` (CI run `41516590111`). COM2 `build: sha=bac1f0ec5139`.
**Leave this EFI booted.** The page and both reloads loaded. The listener is still TLS 1.2.

## What the boot showed

Same spare image. The guest reached `raynusrv2 login:`. Firefox at `https://10.99.99.146:8443` signed in. Overview: Host green ("Server is answering"), RayNu-V green, Guest 3 running, static `10.99.99.146/24`, Operator `vikkp`, Certificate `operator 790 bytes`, Port 3 `b0:26:28:5c:5a:38` UP 100 FULL. Both reloads of that page loaded. The Host card stayed green.

COM2 after the login prompt:

```text
boot: HOST-NIC TCP accept — client connected slot=5
boot: HOST-NIC TCP accept — client connected slot=6
boot: HOST-NIC TCP accept — client connected slot=7
```

Three accepts for the page and both reloads. No `re-listen after HTTP`. The other five sockets stayed in LISTEN. The 2 s polls reused the three sessions.

`+q6E616D65` on the guest line is the ASCII word `name` from the guest serial, not the listener. `reason=0x1e rip=0xffffffffb0fa858f` is a guest exit. It is not `ISO-INSTALL-OK`. Clock "not synced" is the NTP query with no reply on this boot.

**E4 stays in flight.** The listener is still TLS 1.2. HDA stays 99%.
