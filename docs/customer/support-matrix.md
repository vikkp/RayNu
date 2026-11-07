# Support matrix

What Shipment 1 runs on. A row marked supported has booted on the lab R640. Anything else is outside this shipment, not a promise for a later chassis.

## Supported in Shipment 1

| Piece | What shipped | Lab evidence |
|-------|----------------|--------------|
| Server | Dell PowerEdge R640 | Boot and the page on this chassis |
| Management NIC | Broadcom BCM5720, LOM port 3, PCI `01:00.0` | Page on TCP 8443 |
| Guest NIC | Broadcom BCM5720, LOM port 4, PCI `01:00.1` | Guest traffic on that jack |
| Storage | Dell PERC H740P Mini | Guest disk on a window of the array. The H840 is not used |
| Firmware | One signed `r640-hypervisor.efi` | Secure Boot custom `db`, certificate `CN=RayNu-V Secure Boot` |
| Guest | Ubuntu, one guest | Ubuntu reached a login on the PERC window. The lab guest is Guest 3 |
| Management | The page on TCP 8443 | Operator sign-in, address, certificate, audit export |

The running lab image on 2026-11-07 is `4e327820e039`, CI run `42384654720`.

## Not in this shipment

| Piece | Where it stands |
|-------|-----------------|
| PowerEdge R650, R660 | Not booted |
| Broadcom BCM57416, Intel X710 | Not booted |
| PERC H750, H840, NVMe as the guest disk | Not the Shipment 1 disk |
| iDRAC NIC Selection Shared LOM | Dedicated is the setting this shipment uses |
| 10GbE, VLAN, bond, one cable for page and guests | Not the cabling in [Cabling](cabling.md) |
| Windows guests | Not booted |
| More than one guest as a supported count | The lab runs one guest |
| A cluster, live migration, or a second controller | Not Shipment 1 |

Guest Linux prints `secureboot: Secure boot disabled`. That line is the guest firmware. It does not mean the host Secure Boot switch is off.
