# Cabling

Shipment 1 uses three jacks on a PowerEdge R640. The management page and the guests do not share a cable.

| Jack | What it carries | Cable |
|------|-----------------|-------|
| Dedicated iDRAC | iDRAC web interface and virtual media | Yes. iDRAC NIC Selection is Dedicated. |
| LOM port 3 | The RayNu-V page, TCP 8443 | Yes. Untagged. This is the first BCM5720 jack, PCI `01:00.0`. |
| LOM port 4 | Guest traffic | Yes, once a guest has a NIC. Untagged. Second BCM5720 jack, PCI `01:00.1`. |
| 10GbE ports | Nothing in Shipment 1 | No |

Plug iDRAC before you change the host. The page does not come up on the dedicated iDRAC jack.

The lab page address on port 3 is `10.99.99.201/24`. A new site sets its own address on the page. That address is used on the next boot. The page you already have stays on the address it opened with.

## Not this guide

iDRAC NIC Selection Shared LOM is not the install path here. Dedicated is the setting this shipment boots with. Both settings are in [iDRAC NIC Selection](idrac-nic.md). Shared LOM is not tested yet.

One cable for both the page and the guests is not Shipment 1. Neither is a VLAN, a bond, or a 10GbE uplink.
