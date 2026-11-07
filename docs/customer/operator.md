# Operator runbook

Day to day happens on the page, `https://<management-address>:8443/`. Reload the page until it answers. A serial console is not required to see that the host is up, that a guest is running, or that an update has been saved.

## Sign in

The Overview page takes the operator name and password. **Sign in** uses the account already stored. **Save account** stores a new one. A saved account replaces the bring-up login on the next boot.

The build line under the tiles is the running image: `build <sha> · CI <run>`. The run number links to that CI page.

## Guests

The Guests page lists the guests. Start and stop are on the guest row. A guest that is set to start after power-on comes back after the host powers on. On the lab R640 that guest is Guest 3, Ubuntu, and it was running while the hypervisor image was staged.

The guest disk is a window on the PERC array. Shipment 1 does not hand the guest the whole virtual disk.

## Update

The Overview update line is the record.

1. Choose the signed EFI. The stick file you boot is `EFI/BOOT/BOOTX64.EFI`.
2. Press **Stage update**. The status line stays on `Staging…` until the upload finishes.
3. Wait until the update line says `durable yes` and `saved`. It says `saving` until the last byte of the image is on the spare. The guest keeps running during that write.
4. Press **Keep this slot** only when the next boot should replace `EFI/BOOT/BOOTX64.EFI` and save the current file as `EFI/BOOT/BOOTX64.OLD`. A staged slot with no Keep does not change the stick.
5. Boot the stick. The first boot copies the new file. The boot after that runs it. The build line changes on that second boot.
6. **Roll back**, when the line says `old saved`, arms the following boot to copy `BOOTX64.OLD` back. Press it only when that is what you want.

On 2026-11-07, image `4e327820e039` (CI `42384654720`) went from `pending 0 · durable no · saving` to `pending 0 · durable yes · saved` while Guest 3 stayed running. Keep was not pressed. The stick was already that image.

## Power, address, certificate

**Power off host** and **Panic dump** are on Overview. After a power loss, a guest saved for the next boot starts on its own.

**Save address** stores the management address, prefix, gateway, and DNS. The new address is used on the next boot. This page stays on the address it already has.

**Save certificate** stores the TLS certificate for the next connection. This page stays on the certificate it already has. The certificate is the operator's. The lab page showed `Certificate: operator`.

## Audit

Activity has **Audit JSON** and **Audit CSV**. Those downloads are the audit ring. The same page has the guest console and the host log. The host log is there if you want it. The update line does not require it.
