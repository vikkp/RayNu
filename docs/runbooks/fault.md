# Runbook — Fault injection suite (M6.7)

**Marker:** `RAYNU-V-M6-FAULT-OK`  
**Smoke:** `./tools/m6-fault-smoke.sh`

## Story

Host-testable fault injection with **fail-closed** and **recover** criteria.
Not a 72-hr soak (M6.8). Not a real QEMU iron burn-in (optional demo later).

| Kind | Tag | Inject | Fail-closed | Recover |
|------|-----|--------|-------------|---------|
| KillVcpu | 0 | `Vcpu::tear_down` + `VmTable::stop` | Second tear_down rejected | New vCPU + `VmTable::start` |
| CorruptPage | 1 | Mark mapped GPA corrupt | `allow_access() == false` | `EptMap::unmap` + remap fresh frame |
| DropIrq | 2 | Arm `IrqDropLatch` | Inject rejected while armed | Disarm → `prepare_external_inject` ok |
| NetPartition | 3 | `VSwitch::set_partitioned(true)` | Unicast → `Ok(None)` | Clear partition → deliver |

## Audit

Each fault emits `FaultInjected` / `FaultFailClosed` / `FaultRecovered` as applicable
(`kind` = tag above).

## Limits

Bring-up host mocks only. Multi-host fencing, real page bit-flips, and soak
metrics are out of scope.
