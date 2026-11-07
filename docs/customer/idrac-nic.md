# iDRAC NIC Selection

Two Dell settings, both on `10.99.99.x`. No cable moves between them. The dedicated iDRAC cable, the LOM port 3 cable, and the LOM port 4 guest cable stay put. Port 3 is the jack Shared LOM uses, and the page cable is already there.

The running image for this matrix is `4e327820e039`, CI run `42384654720`. It already leaves the iDRAC PHY alone (`ape-nophylock=yes`). This test did not need a new EFI.

Lived 2026-11-07 on that image. Evidence `docs/evidence/r640/2026-11-07-4e327820e039-wave-6-f10-idrac-nic-ok.md`.

| NIC Selection | iDRAC answers on | Page answers on | Result |
|---------------|------------------|-----------------|--------|
| Dedicated | Dedicated jack, `10.99.99.24` | LOM port 3, `https://10.99.99.201:8443/` | Passed. Active NIC Interface Dedicated, 1000 Mbps, full duplex. Failover Network None. Guest 3 on LOM port 4. |
| Shared LOM | LOM3 `NIC.Integrated.1-3-1`, same address `10.99.99.24` | LOM port 3, same cable | Passed. Active NIC Interface LOM3, 100 Mbps, full duplex. Failover Network None. The page reloaded on `4e327820e039` with Guest 3 running. |

Shared with Failover is not a row in this matrix.

The chassis was put back on Dedicated. Active NIC Interface read Dedicated again, the link returned to 1000 Mbps, and the page reloaded on the same build with Guest 3 running.

## What was clicked

1. iDRAC Settings, Connectivity, Network. NIC Selection was already Dedicated. The SPA showed `build 4e327820e039` and Guest 3 running.
2. NIC Selection was set to **LOM3 - [NIC.Integrated.1-3-1]**. Failover Network stayed None. Apply. iDRAC returned at `10.99.99.24` with Active NIC Interface LOM3. The SPA reload was the same screen.
3. NIC Selection was set back to Dedicated. Apply. Active NIC Interface read Dedicated. The SPA reload was the same screen.

## Not this matrix

One cable for the page and the guests is a different row. So is choosing which LOM is the management port. Dedicated stays the install setting this shipment cables by default.
