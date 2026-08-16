# Runbook — Ops Web UI MVP (M7.4 / E4 polish)

**Marker:** `RAYNU-V-M7-UI-OK`  
**Plan:** [docs/m7_plan.md](../m7_plan.md) · ADR: [ADR-009](../adr/ADR-009.md) · listen: [ADR-012](../adr/ADR-012.md)

## What this gate proves

M7.4 / E4 upgrades the embedded SPA for operator MVP over lab HTTP:

1. Form: guest id, CPU, RAM MiB, disk MiB, ISO id  
2. REST: `POST /vms/{id}/spec/{cpu}/{ram}/{disk}/{iso}`  
3. Media: `GET /images`, `POST /iso/{id}/deploy` (extract-boot), `POST /iso/{id}/install`  
4. Start / stop  
5. **Host serial log:** `GET /logs/serial` + SPA panel (HV UART ring — not guest console)  
6. **Auth:** Bearer token field; ESP `EFI/RayNu/auth.token` overrides bring-up when present  

## Host smoke

```bash
./tools/m7-ui-smoke.sh
```

Expect:

```text
RAYNU-V-M7-UI-OK
==> M7.4 Ops UI smoke PASSED
```

## Iron PRE-EBS (R640)

During the M7.6 listen window (`RAYNU-V-M7-UEFI-HTTP-OK` path):

1. Optional: stage `EFI/RayNu/auth.token` on the boot media (disables bring-up token).  
2. Open `http://<lease-ip>:8443/` from the operator LAN.  
3. Paste Bearer token → Create guest → Deploy ISO → Refresh host serial log.  
4. Tables are **durable across exchanges** in the PRE-EBS window (shared mgmt state).

## Honesty / residuals

- **Guest console / VNC** — not claimed; host serial log only.  
- **TLS** — still plaintext lab HTTP (M7.1 / ADR-012).  
- **El Torito / CD-ROM** — still stubbed (M7.3).  
- **NIC attach** — JSON reports `nics:1` default; virtio-net attach residual.  
- Host package smoke is unit tests; iron proof is live curl/SPA during PRE-EBS.

## Next

E5 iron install-to-disk close (`RAYNU-V-M7-ISO-INSTALL-OK`) after E4 usable on iron.
