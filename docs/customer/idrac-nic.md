# iDRAC NIC Selection

Two Dell settings, both on `10.99.99.x`. No cable moves between them. The dedicated iDRAC cable, the LOM port 3 cable, and the LOM port 4 guest cable stay put. Port 3 is the jack Shared LOM uses, and the page cable is already there.

The running image for this matrix is `4e327820e039`, CI run `42384654720`. It already leaves the iDRAC PHY alone (`ape-nophylock=yes`). This test does not need a new EFI.

| NIC Selection | iDRAC answers on | Page answers on | Result |
|---------------|------------------|-----------------|--------|
| Dedicated | Dedicated iDRAC jack | LOM port 3, `https://10.99.99.201:8443/` | Passed. This is the setting the lab boots with. Guest 3 runs on LOM port 4. |
| Shared LOM | LOM port 3, same network `10.99.99.x` | LOM port 3, same cable | Not tested. |

Shared with Failover is not a row in this matrix.

## Dedicated

Record, without changing the setting:

1. iDRAC web page shows NIC Selection Dedicated.
2. The SPA shows `build 4e327820e039` and Guest 3 running.

## Shared LOM

1. Leave all three cables where they are.
2. iDRAC Settings, NIC Selection, Shared LOM. Not Shared with Failover. Apply. iDRAC restarts its network and stops answering on the dedicated jack.
3. The iDRAC web page comes back through LOM port 3. The SPA stays on `https://10.99.99.201:8443/` with Guest 3 running.
4. Set NIC Selection back to Dedicated. iDRAC answers on the dedicated jack again. The page stays up.

If the iDRAC web page does not come back, plug nothing new. The dedicated cable is still in. Wait for the BMC, then set Dedicated again from that jack or from the local console.

## Not this matrix

One cable for the page and the guests is a different row. So is choosing which LOM is the management port.
