# 2026-11-04 — function-0 listener health (not yet on iron)

Host change on `cursor/wave-5-e4-tls13-41d4`. No COM2 for this commit yet.

## What it does

`mgmt/listener_health.rs` decides from two snapshots of function 0:

- **Idle.** No baseline yet, or the MAC counters and the rings are flat, or a transmit descriptor is not sitting uncompleted. A quiet port does not recover.
- **Serving.** The return ring, the transmit consumer, or the MAC transmit counter moved.
- **Stalled.** Function 1 is armed, and the same miss has happened three times, one second apart. A miss is either "the MAC counted receive frames and the return ring did not take them" or "a transmit descriptor sat uncompleted with the MAC transmit counter flat."

The coexist tick (`listener_health_tick`) owns the clock. On Stalled it:

1. Halts function 0's MAC DMA and reprograms that port's rings and DMA engines. Software indices match the mailboxes it posts.
2. Aborts the ten smoltcp sockets and puts them back in LISTEN.
3. Prints one nowait line, `boot: HOST-NIC :38 recover n=… ok|fail rx=… rxp=… rxc=… tx=… txp=… txc=… frames=…`.
4. Records `AuditEvent::MgmtRestarted` kind 4.
5. Waits 8 seconds before the next sample. Four rebuilds cap the boot.

It does not write the PHY, take the phylock, pulse the core clock, or touch function 1. M8.9's egress arm is unchanged. Poll-mode stays.

## What iron has to show

After the guest reaches `network.target`, load `https://raynu-v.lab:8443`, sign in as `vikkp`, reload twice.

- Sign-in holds and both reloads load. E4 can be lived from that COM2, whether or not a recover line printed.
- `HOST-NIC :38 recover` prints and the page comes back. The counters on the line are the evidence.
- The line prints and the page stays dead, or the page dies with no line. The predicate or the rebuild is wrong. The counters, if the line printed, say which.

**E4 stays in flight.** Not lived. HDA stays 99%.
