# Install RayNu-V

Put one signed `r640-hypervisor.efi` on a PowerEdge R640 and open the page. Shipment 1 is that file. There is no host operating system to install and no package to add on the server.

Cable the server first. The jacks are in [Cabling](cabling.md).

## What you need

- A PowerEdge R640. Shipment 1 is this chassis.
- A FAT32 USB stick. The boot file is `EFI/BOOT/BOOTX64.EFI`.
- The unsigned EFI from the release you are installing, and the RayNu-V Secure Boot key and certificate. The private key stays with you. It is not in the git tree and not in the EFI.
- A machine on the same network as the management jack, with a browser.

Secure Boot stays enabled. An unsigned EFI does not boot.

## 1. Sign the EFI

On the machine that holds the key:

```bash
umask 077
openssl pkey -in raynu-v-db.key -out /dev/shm/raynu-v-db.key
sbsign --key /dev/shm/raynu-v-db.key \
  --cert raynu-v-db.crt \
  --output r640-hypervisor-signed.efi \
  r640-hypervisor.efi
sbverify --cert raynu-v-db.crt r640-hypervisor-signed.efi
shred -u /dev/shm/raynu-v-db.key
sha256sum r640-hypervisor-signed.efi
```

`sbverify` prints `Signature verification OK`. Keep the signed hash. The stick copy has to match it.

## 2. Copy it to the stick

Mount the stick. Copy only `EFI/BOOT/BOOTX64.EFI`. Do not format any other disk.

```bash
sudo mount /dev/disk/by-label/YOUR-STICK /mnt/stick
sudo cp r640-hypervisor-signed.efi /mnt/stick/EFI/BOOT/BOOTX64.EFI
sync
sudo sha256sum /mnt/stick/EFI/BOOT/BOOTX64.EFI
sudo umount /mnt/stick
```

The sum matches the signed file from the previous step. Put `raynu-v-db.crt` on the stick as well if the BIOS does not already have it. The BIOS import reads a file.

## 3. Enroll the certificate

On the R640, System Setup:

1. F2, then System BIOS, then System Security, then Secure Boot.
2. Set Secure Boot Policy to Custom.
3. Key Management, Authorized Signature Database (`db`), append `raynu-v-db.crt`. Leave the certificates that are already there.
4. Set Secure Boot to Enabled. Save and exit.

Enable Secure Boot only after that certificate is in `db`. Otherwise the firmware rejects the EFI.

## 4. Boot

F11 and choose the stick. Open `https://<management-address>:8443/` and reload it until the page answers. The page coming up is the ready signal. The build line under the tiles shows the git SHA and the CI run.

The first guest exit line `reason=0x1c` is the normal first exit. It is not a fault.

## What the lab box did

On 2026-11-07 the stick was a Lexar labeled `RAYNUV`. Signed SHA256 `1101880eb670530ee3e23a5499cc9b0a2ac5579954c755dbe455fa9aa2b1995a`. F11 reached `build 4e327820e039`, CI run `42384654720`, at `https://10.99.99.201:8443/`. Guest 3 was already running. Those addresses are that lab.
