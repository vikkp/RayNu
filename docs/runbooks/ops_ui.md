# Runbook — Ops Web UI MVP (M7.4)

**Marker:** `RAYNU-V-M7-UI-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md)

## What this gate proves

M7.4 upgrades the embedded SPA from “demo create guest” to **create-VM with
fields** and media surfaces:

1. Form: guest id, CPU, RAM MiB, disk MiB, ISO id  
2. REST: `POST /vms/{id}/spec/{cpu}/{ram}/{disk}/{iso}`  
3. Media: `GET /images`, `POST /iso/{id}/deploy` (M7.2/M7.3)  
4. Start / stop unchanged  

## Host smoke

```bash
./tools/m7-ui-smoke.sh
```

Expect:

```text
RAYNU-V-M7-UI-OK
==> M7.4 Ops UI smoke PASSED
```

## Honesty / residuals

- **Console / serial log UI** — deferred (not in this gate).
- **TLS** — still plaintext lab HTTP (M7.1).
- **Firmware NIC listen** — still `UnsupportedOnFirmware`.
- **El Torito / CD-ROM** — still stubbed (M7.3).
- Host package smoke is fast unit tests; not a browser E2E on R640.

## Next

M7.5 real R640 boot (`RAYNU-V-R640-BOOT-OK`) — hard gate for M7 closed.
