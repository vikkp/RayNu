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

## E4 SPA start on the coexist path (in-tree)

After ADR-013 Phase F, `POST /vms/{id}/start` is no longer table-only:

1. REST start (200) queues a flag (`note_spa_start`). It does **not** VMLAUNCH inside the HTTP tick.
2. The next credit-scheduler quantum (`schedule_preempt`, after `tick_native_coexist`) consumes the flag **once `M4_LADDER_DONE`**.
3. Slot 1 is relocated into the G1 2 MiB slab already punched out of G0 EPT: private **single 2 MiB** EPT, VMCS + host state in the slab (not the G0 identity pool Linux can scribble).
4. Iron marker (COM2, **not** host): `RAYNU-V-M7-E4-SPA-LAUNCH-OK` on SHELL CPUID.

This is a **SHELL CPUID** guest in the G1 slab, not a Linux distro installer and not TLS/`auth.token`. Stop parks slot 1 (`SPA_RUNNABLE = false`); G0 stays scheduled. Fail-soft resumes G0.

## Next

Iron COM2 `RAYNU-V-M7-E4-SPA-LAUNCH-OK`. Then distro installer on virtio-blk, then TLS / console / `auth.token` before wider-than-lab LAN.
