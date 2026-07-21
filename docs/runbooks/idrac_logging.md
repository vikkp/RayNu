# Runbook — Logging on iDRAC after RayNu-V boots

**Goal:** Keep a durable record of what happens once `r640-hypervisor.efi` is
running on a PowerEdge R640 with iDRAC watching.

## Three channels (do not conflate them)

| Channel | What it captures | Tool / path |
|---------|------------------|-------------|
| **A. HV → COM1** | Everything RayNu-V prints: gate markers + mirrored audit lines | Firmware COM1 (`boot/serial.rs`); visible in iDRAC Virtual Console / SOL |
| **B. Laptop capture** | Saves channel A to a file for evidence | `./tools/capture-idrac-serial.sh` |
| **C. BMC Redfish logs** | iDRAC SEL / Lifecycle Controller entries (power, sensors, firmware) | `./tools/capture-idrac-serial.sh redfish …` |

RayNu-V **cannot** turn iDRAC into a full session recorder by itself. The
hypervisor writes to **COM1**; **you** (or SOL) must be attached to save it.
iDRAC’s own SEL is separate — pull it with Redfish when credentials work.

## Operator recipe (R640 first light)

1. **Before reboot**, start a capture:

   ```bash
   # Easiest: tee while Virtual Console serial is open (paste or pipe)
   ./tools/capture-idrac-serial.sh tee \
     --out docs/evidence/r640/$(date -u +%Y-%m-%d)-r640-serial.txt
   ```

   Or, if SOL is enabled on the BMC:

   ```bash
   IDRAC_PASS='…' ./tools/capture-idrac-serial.sh sol \
     --host <idrac-ip> --user root \
     --out docs/evidence/r640/$(date -u +%Y-%m-%d)-r640-serial-sol.txt
   ```

2. Boot RayNu-V (media maker + Virtual Media / USB — field guide §§2–4).

3. Confirm the transcript contains at least `RAYNU-V-M0-BOOT-OK` and later
   `RAYNU-V-AUDIT:…` lines for management/audit events (UEFI builds mirror the
   audit ring to COM1).

4. Optional BMC snapshot after the attempt:

   ```bash
   IDRAC_PASS='…' ./tools/capture-idrac-serial.sh redfish \
     --host <idrac-ip> --user root \
     --out-dir docs/evidence/r640/$(date -u +%Y-%m-%d)-idrac-logs
   ```

5. Attach the serial file (and Redfish dir if any) to the dated evidence under
   `docs/evidence/r640/`.

## What “logged” means in RayNu-V after boot

On UEFI iron, `audit_log!(…)` still fills the in-memory hash-chained ring
(`audit/integrity.rs`) **and** emits a one-line COM1 record:

```text
RAYNU-V-AUDIT: VmStarted guest_id=1
```

High-churn frame alloc/free events are **not** mirrored to COM1 (they would
flood the console); they remain in the ring only.

## Limits

- Virtual Console HTML5 “record session” varies by iDRAC version — prefer
  `tee` / `sol` so the file lives in git evidence.
- Live Redfish paths differ across iDRAC 8/9; `redfish` mode is best-effort and
  may 404 — COM1 serial remains the **primary** RayNu-V proof.
- QEMU lab capture stays `tools/qemu-boot-test.sh` / `SERIAL_CHARDEV=file:…`.

## Smoke

```bash
./tools/capture-idrac-serial-smoke.sh   # → RAYNU-V-M7-SERIAL-CAPTURE-OK
```
