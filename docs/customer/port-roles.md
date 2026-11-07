# Port roles

Shipment 1 uses fixed jacks on a PowerEdge R640. The page is port 3. Guests are port 4. iDRAC is either the dedicated jack or, when NIC Selection is Shared LOM, the same port 3 jack as the page. The operator does not pick a different split. That choice is outside this shipment.

Roles are untagged. There is no VLAN and no bond. The 10GbE jacks are unused.

The running image for this record is `4e327820e039`, CI run `42384654720`. The page is `https://10.99.99.201:8443/`. iDRAC is `10.99.99.24`.

| Role | Jack | What the page calls it | Lab identity |
|------|------|------------------------|--------------|
| iDRAC, Dedicated | Dedicated iDRAC jack | Not a page row | `10.99.99.24`, Active NIC Interface Dedicated, 1000 Mbps |
| iDRAC, Shared LOM | LOM port 3 | The page stays Port 3 | `LOM3 - [NIC.Integrated.1-3-1]`, Active NIC Interface LOM3, 100 Mbps, same iDRAC address |
| Management page | LOM port 3 | Port 3, SPA | BCM5720 function 0, PCI `01:00.0`, MAC `b0:26:28:5c:5a:38`, TCP 8443 |
| Guests | LOM port 4 | Port 4, Guest LAN | BCM5720 function 1, PCI `01:00.1`, MAC `b0:26:28:5c:5a:39` |

Overview shows Port 3 and Port 4 with link, speed, and duplex. On 2026-11-07 both read up. Guest 3 was running. A cable pull on port 4 shows the guest link down and then up. That is the guest jack, not the page jack.

## iDRAC on either setting

Dedicated keeps iDRAC on its own jack. Shared LOM moves iDRAC onto port 3, the jack that already has the page cable. Port 4 stays the guest jack in both settings. Failover was not used.

Both settings were tested on 2026-11-07 with no cable move. The page reloaded on `4e327820e039` with Guest 3 running while iDRAC was on LOM3, and again after NIC Selection was set back to Dedicated. The record of the clicks is [iDRAC NIC Selection](idrac-nic.md). Evidence `docs/evidence/r640/2026-11-07-4e327820e039-wave-6-f10-idrac-nic-ok.md`.

The install cabling is [Cabling](cabling.md). Dedicated is the setting a new site cables by default.

## What a guest uses

A guest NIC transmits on port 4. The lab guest address reached from another machine was `10.99.99.200` on that jack. The page address `10.99.99.201` is port 3 and is not the guest address.

## Not these roles

One cable carrying both the page and the guests is not Shipment 1. Choosing a different jack for the page is not Shipment 1. A VLAN trunk is not Shipment 1.
