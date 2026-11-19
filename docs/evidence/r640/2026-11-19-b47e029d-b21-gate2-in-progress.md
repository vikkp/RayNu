# 2026-11-19 — `b47e029d` gate 2 in progress

EFI `b47e029d88aa`, push run `44622224710`. Gate 2 in progress. A login and `nproc` 2 are not the plan's pass. B21 stays in flight. vCPU 0 stayed on core 0.

COM2:

```
build: sha=b47e029d88aa run=44622224710
boot: B21 host cores online=16 of 16 (ADR-024; not ISO-INSTALL-OK)
boot: B7 guest slot 0 vcpus=2 (vCPUs are placed on host cores as they start; ADR-024; not ISO-INSTALL-OK)
boot: B7 vcpu 1 init
boot: B21 vcpu 1 on host core 1 (ADR-024; not ISO-INSTALL-OK)
boot: B7 vcpu 1 start page=0x99 (INIT then SIPI; ADR-023; not ISO-INSTALL-OK)
boot: B21 cores 0:[0.0] 1:[0.1] 2:[] 3:[] 4:[] 5:[] 6:[] 7:[] 8:[] 9:[] 10:[] 11:[] 12:[] 13:[] 14:[] 15:[] lock_wait_max_us=0 hold_max_us=18604038 kicks=0/0 recalls=0
boot: B7 vcpu 1 long mode
smp: Brought up 1 node, 2 CPUs
raynusrv2 login:
```

The page at `https://10.99.99.201:8443` loaded. `ssh` to `10.99.99.200` logged in. `nproc` printed `2`.

A later cores line on the same boot was `0:[0.0] 1:[0.1]` with `lock_wait_max_us=362043`.

Recorded on this boot: placement line, cores line, `Brought up 1 node, 2 CPUs`, `raynusrv2 login:`, SPA load, `ssh` to `10.99.99.200`, `nproc` printed `2`.

Not run, so gate 2 stays in progress: plan section 9 steps 2–5 and 7. Also not run: Shut down and Start with `recalls=` above 0, 60 s of two busy loops, a second guest beside this one, a `nokick.txt` boot, 4 vCPUs.

Gate 3 is not started. The row is lived at gate 3.
