# 2026-11-02 — `87bee04b`: page once, reload refused during a FIN

**EFI:** `87bee04b44b5` (CI run `41512984706`). COM2 `build: sha=87bee04b44b5`.
**Do not F11 this file again.** The page loads on it. The reload does not.

## What the boot showed

Same spare image. The guest reached `raynusrv2 login:`. Firefox loaded the SPA once. The reload failed.

COM2 after the login prompt, and nothing else from the listener:

```text
boot: HOST-NIC TCP accept — client connected slot=4
boot: HOST-NIC TCP re-listen after HTTP slot=0
boot: HOST-NIC TCP accept — client connected slot=0
boot: HOST-NIC TCP re-listen after HTTP slot=1
```

`87bee04b` kept a normal SPA fetch on its TLS session, and closed the oldest finished session whenever none of the six sockets was listening. The reload's SYN arrived while that FIN was in flight. Nothing was listening, so the reload was refused.

## Change

smoltcp's listening socket becomes the connection. A browser keeps six persistent connections. The table is eight, so two stay in LISTEN. A live session is not closed to free a listener. `Connection: close`, a peer FIN, and the 10 minute idle still end that one socket.

**E4 stays in flight.** The listener is still TLS 1.2. HDA stays 99%.
