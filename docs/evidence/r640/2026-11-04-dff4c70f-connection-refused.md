# 2026-11-04 — `dff4c70f` refused the page

EFI `dff4c70f4469472a3f687b2000221e456d743d54`, push run `41939240666`.

COM2 printed `listen=1` and `build: sha=dff4c70f4469 run=41939240666`. The boot reached `raynusrv2 login:`. The log has no `TCP accept`. Firefox at `https://raynu-v.lab:8443/` showed **Unable to connect**, `NS_ERROR_CONNECTION_REFUSED`, 0 bytes, 0 ms.

`listen=1` was only the arm line. The tick still called `listen()` on every closed slot, so the table did not stay the wave 4 socket. A SYN with no socket in LISTEN is a reset. That is the 0 ms refusal.

The wave 4 close (`11688cbf`) is one TCP socket for the boot. The page, the sign-in, and the later polls are more HTTP on that session. This note is not a new close. E4 stays lived on `9c3e660259de`. HDA stays 99%.
