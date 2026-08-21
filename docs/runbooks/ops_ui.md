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

## Hang-fix EFI on Cruzer (flash done 2026-08-21)

Cruzer `RAYNUV` (front USB 2, `/dev/sdc`) holds EFI `f413a9fc…` / size `1226240`
(artifact `9429378906`, commit `1a29e33`). Evidence:
[`docs/evidence/r640/2026-08-21-e4-cruzer-flash-hangfix.md`](../evidence/r640/2026-08-21-e4-cruzer-flash-hangfix.md).
Do **not** reflash hung `67b0acde` or Phase F `0d06297b`.

1. iDRAC SOL `console com2` before power.
2. One-time F11 boot Cruzer `RAYNUV`. BIOS order stays Ubuntu on PERC.
3. Ignore PRE-EBS SNP `CURL NOW` / 45s `mgmt HTTP accept timeout`.
4. After `RAYNU-V-M4-SLICE-G0`, COM2 **must** show `boot: sched switch → slot=00000001` (proves the hang fix). Then NVM / BLK / NET / SMP / `RAYNU-V-R640-BOOT-OK`.
5. Curl **only** after native coexist listen (lease from COM2, port **8443**):

```
HOST-NIC coexist listening on 10.99.99.149:8443 (VMX on; ADR-013 Phase F)
CURL NOW → http://10.99.99.149:8443/  (native BCM5720; G0 still scheduled; SNP is dead)
```

```bash
LEASE=10.99.99.149   # use the numeric lease COM2 printed
TOK='Authorization: Bearer raynu-v-bringup'
curl -sS -m 20 -D - -H "$TOK" -X POST "http://${LEASE}:8443/vms/1/spec/1/512/1024/0"
# Current Cruzer EFI (`f413a9fc`) has one TCP slot. After 201, wait for
# COM2 `HTTP exchange ok` then POST start (back-to-back start got curl 7 RST).
sleep 2
curl -sS -m 20 -D - -H "$TOK" -X POST "http://${LEASE}:8443/vms/1/start"
```

6. WANT COM2: `E4 SPA VMLAUNCH slot=1 private 2M EPT` then `RAYNU-V-M7-E4-SPA-LAUNCH-OK`.

Hang-fix iron boot (2026-08-21): `SLICE-G0` then `sched switch → slot=00000001`;
coexist HTTP-OK on `10.99.99.149:8443`. Evidence:
[`docs/evidence/r640/2026-08-21-e4-hangfix-boot.md`](../evidence/r640/2026-08-21-e4-hangfix-boot.md).
That paste is GET-only (`method_tag=1`). **Do not power off** — POST spec+start
on the same boot.

Coexist has **one** TCP listen slot. A SPA/browser (or aborted curl) that
prints `HOST-NIC TCP accept` without `HTTP exchange ok` holds the slot;
new `curl` SYNs time out (`curl: (28)`). Close every tab to `:8443` and retry
with `-m 20`. If COM2 is still stuck on that accept, iDRAC **Force Power Off**,
F11 the same Cruzer, and POST spec+start **immediately** after native
`CURL NOW` — do not open the SPA first.

Keep APE PHY. Bind LOM `:38`. Do not write PERC. Safe shutdown is iDRAC **Force Power Off**.

## Next

Iron COM2 `RAYNU-V-M7-E4-SPA-LAUNCH-OK`. Then distro installer on virtio-blk, then TLS / console / `auth.token` before wider-than-lab LAN.
