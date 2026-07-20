# Runbook — Mock HA failover (M6.6)

**Marker:** `RAYNU-V-M6-HA-OK`  
**Smoke:** `./tools/m6-ha-smoke.sh`

## Story

RayNu-V M6.6 provides a **single-host bring-up mock** of primary↔standby
failover over the management-plane `VmTable`. This is **not** cross-host live
migration (that path is M6.3 / Proven Core page transfer).

| Role | Table | Meaning |
|------|--------|---------|
| Primary | `HaPair.primary` | Active by default |
| Standby | `HaPair.standby` | Survivors land here after `failover_to_standby` |

## Operator path (host-testable)

1. Create + start guest(s) on the active table (`pair.active_table_mut()`).
2. Call `HaPair::failover_to_standby()` (or `POST /ha/failover` with
   `BRINGUP_AUTH_TOKEN`).
3. Source guests are stopped/destroyed; survivors recreate on standby.
4. Defined/Running guests restart as **Running** on the survivor (existing
   `VmTable::start` restart path).
5. Audit emits `HaFailoverStarted` then `HaFailoverCompleted { guest_count }`.
6. Optional reverse: `failover_to_primary()`.

## Security harden (gate checklist)

- REST lifecycle and HA routes require `BRINGUP_AUTH_TOKEN` (401 otherwise).
- No always-allow auth stub.
- Safe defaults: `guest_id == 0` rejected; destroy while Running → BadState.

## Limits

Bring-up mock only. Production secret rotation, multi-node fencing, and fault
injection are out of scope (see M6.7+).
