# Releases and support

This is what a Shipment 1 customer gets for a year. It is a proposal until the terms are accepted. It does not add a feature on the page.

## A release

A supported release is a git tag. The tag names one signed `r640-hypervisor.efi`, its SHA256, and the build line the page shows: `build <sha> · CI <run>`.

The tags a customer can be handed back to today are:

| Tag | Build |
|-----|--------|
| `v0.1.0-everest-closed` | `f72b4276` |
| `v0.1.0-m8-8-ubuntu` | `c3a70e62` |
| `v0.1.0-m8-9-lan` | `0d865c7f` |

A CI build that has no tag is not a supported release. The lab box is running `4e327820e039`. That image is how the current rows were proved. It is not itself one of the tags above.

You install a release from the page. **Keep this slot** arms the next boot to save the file you are running as `EFI/BOOT/BOOTX64.OLD` and to boot the new file after that. **Roll back** puts `BOOTX64.OLD` back on the boot after that. That pair is the rollback. There is no separate patch file.

## Cadence

A new tag is cut when a fix for a lived Shipment 1 row is ready to support, and that image has booted on our R640. We do not ship on a calendar month. We do not ask a customer to follow every commit.

## The year

The year is twelve months from the date of the tag you install.

During that year you get:

- A fix that makes a lived Shipment 1 row work again on the [support matrix](support-matrix.md), delivered as the next tagged image.
- The install, cabling, port-role, operator, and support-matrix documents kept true for that tag.
- The rollback above, to the previous signed file.

During that year you do not get:

- A promise that a later wave ships inside the year. New rows ship when they are lived, as a new tag, and that tag starts its own year when you install it.
- Hardware the support matrix does not list. Today that includes a PowerEdge R650 or R660, a Windows guest, a cluster, and one cable for the page and the guests.
- A clock on a reply, a night-and-weekend rotation, or a visit to the rack.
- Someone else to press Keep, Roll back, or change iDRAC NIC Selection for you.

## How to ask

Write to [namaskaar@raynuv.com](mailto:namaskaar@raynuv.com). Include the build line from the page, the support-matrix row, and what the page showed. We answer as ourselves, on that mailbox.

## After the year

The image you installed keeps running. The stick still has the previous file if you kept one. Fixes after the year are a new tag. Installing that tag starts a new twelve months.
