# 2026-10-29 — `aeb2d79b`: one port 4 cable pull, the audit line reaches COM2 and Activity (ADR-021 D5 lived; wave 2 iron thread closed)

**EFI:** `aeb2d79bb179` (PR #418, CI push run `40687568692`, SHA256 `c55287cb25c989a7513fe9a062e54e8b6e20c039a5dfe2f333e04e5f6a419d6c`), the stick flashed for the D4 boot ([2026-10-28-aeb2d79b-wave-2-d4-discovered-port-ok.md](2026-10-28-aeb2d79b-wave-2-d4-discovered-port-ok.md)). The `build: sha=` line is not in this paste; the operator did not reflash between the two boots.
**Guest:** the 10 GiB tail on RAYNU-SPARE, a fresh boot (`Reached target multi-user.target`), vNIC Saved on the Guests row, Start. `:39` armed on the guest's first frame; at `t10s` `vtx=104 vrx=114 sent=104 held=0 fcs=0 wire=up lpa=45e1 macmode=19e04804`.
**Operator order:** `root` login on ttyS0; `test` typed into the Activity Send box (`RAYNU-V-M8-CONSOLE-OK`, getty `Login incorrect`); then the port 4 (LOM 4, `b0:26:28:5c:5a:39`) cable pulled and replugged; Refresh log on Activity.
**Rollback:** `v0.1.0-m8-9-lan`. Disks unchanged. No doorbell.

## What D5 asks

Physical link state of both LOM ports on the SPA, and an audit event on change. The page half lived on `8df4407d` (Overview Port 3 / Port 4 rows, change count). The audit half did not show there: `RAYNU-V-AUDIT: HostLinkChanged` was recorded but its COM1 mirror went through `write_str`, hushed once Linux owns the console. `f57bd265` moved the mirror to the no-wait writer; this is the first pull on an EFI that carries it.

## COM2

```
raynusrv2 login: RAYNU-V-AUDIT: HostLinkChanged func=1 up=0 mbps=0
boot: D5 link lom=4 func=1 down
boot: M8.9 egress :39 link=up speed=100 duplex=full
RAYNU-V-AUDIT: HostLinkChanged func=1 up=1 mbps=100
boot: D5 link lom=4 func=1 up mbps=100
```

One `down`, one `up`. The audit line leads its `boot: D5 link` line both times, in the order `report_link` writes them (`audit_log!` first, then the COM2 line). On the replug `egress_link_tick` set the MAC port mode (`egress :39 link=up speed=100 duplex=full`) before reporting the change, so that line sits between the two pairs. The partner on port 4 is 10/100 (`lpa=45e1`), so `mbps=100` is the wire.

## Activity page, `=== host UART ===` ring

The same five lines, in the same order, under the `RAYNU-V-M8-CONSOLE-OK` / `Login incorrect` / `login: timed out after 60 seconds` block from the console test. The ring is read over HTTP from the Mac, so the event is visible without COM2. Screenshot kept with PR #418.

**D5 lived.** With D6 (`8df4407d`) and D4 (`aeb2d79b`), the wave 2 iron thread is closed.

## Noted, not changed

- The Activity tile row read `GUEST — No guest yet` while the guest was at `login:` over `:39`. The tile reads a different source than the Guests row; a page nit for B20 (guest state and stop reasons readable on the page), not for this row.
- `login: timed out after 60 seconds` is getty giving up on the `test` line sent from the console box; expected.

## Not claimed

No new `RAYNU-V-*-OK` marker (D5 has none; the gate was the audit line on COM2 and Activity after a pull). Wave 2 stays open on B20 and A3 (host-first). Not ISO-INSTALL-OK, not 1.0, not GA. HDA stays 99%.
