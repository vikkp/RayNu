# Secure Boot enrollment (ADR-021 A5)

**Status: lived 2026-11-05.** Signed EFI SHA256 `b78b0fa6…`, COM2 `build: sha=097f3c612c06 run=42062748313`, Secure Boot Enabled, `raynusrv2 login:`. Evidence [`docs/evidence/r640/2026-11-05-097f3c612c06-wave-6-a5-secure-boot-ok.md`](../evidence/r640/2026-11-05-097f3c612c06-wave-6-a5-secure-boot-ok.md). The guest line `secureboot: Secure boot disabled` is RayNu-F. It does not publish the host SecureBoot variable.

This is the ADR-021 catalog row A5 (signed EFI). It is not the parked
`auth.token` bring-up latch.

## Key decision (2026-11-05)

Shipment 1 signs with a **RayNu-V certificate** the customer enrolls in the
Secure Boot authorized signature database (`db`).

- The private key stays with the operator. It is not in git, not in CI, and
  not on the stick except as the public certificate.
- CI runs `./tools/sign-efi.sh --ephemeral`. That key is created in a temp
  directory and deleted before the step exits. The release kit from CI stays
  **unsigned**. Do not enroll that ephemeral certificate, and do not flash a
  binary signed with it.
- A Microsoft-signed **shim** is the alternative when a customer will not
  enroll a key. That shim is not built. It needs a Microsoft signature.
  Until it exists, Secure Boot on means "enroll the RayNu-V `db` certificate
  or the firmware will reject the EFI."

## Make a key (operator machine, not the repo)

```bash
umask 077
openssl req -x509 -newkey rsa:2048 \
  -keyout raynu-v-db.key -out raynu-v-db.crt \
  -days 3650 -subj "/CN=RayNu-V Secure Boot/"
```

`openssl` asks for a passphrase. Keep `raynu-v-db.key` off the git tree.
To sign, unwrap it into a temp file that is not the repository, then shred
it:

```bash
umask 077
openssl pkey -in raynu-v-db.key -out /dev/shm/raynu-v-db.key
RAYNU_SIGN_KEY=/dev/shm/raynu-v-db.key RAYNU_SIGN_CERT=raynu-v-db.crt \
  ./tools/package-release.sh
shred -u /dev/shm/raynu-v-db.key
```

The kit then contains the signed `r640-hypervisor.efi` and `raynu-v-db.crt`.
`MANIFEST.txt` says `Secure Boot: signed`. One of the two env vars without
the other is an error. Neither set leaves the kit unsigned
(`Secure Boot: unsigned`), which is what CI publishes and what the lab
flashes while Secure Boot is off.

The signature is a few kilobytes at the end of the PE. Re-run
`./tools/check-size.sh` against a tree copy if a signed file is what you
will flash; the ADR-003 hard limit stays 20 MB.

## Enroll on an R640 (lived 2026-11-05)

14th-gen System Setup. This path is the Dell BIOS screen. It has not been
walked on the lab box for a RayNu-V certificate.

1. Copy the signed EFI to `EFI/BOOT/BOOTX64.EFI` on the FAT32 stick. Put
   `raynu-v-db.crt` on that stick too. The BIOS import reads a file.
2. F2 → System BIOS → System Security → Secure Boot.
3. Set **Secure Boot Policy** to **Custom** so Key Management can append a
   certificate. Leave the factory certificates in `db` if other media should
   still boot.
4. Key Management → Authorized Signature Database (`db`) → append
   `raynu-v-db.crt`.
5. Set **Secure Boot** to **Enabled**. Save and exit.
6. Boot the stick. COM2 `build: sha=` with Secure Boot still enabled is the
   A5 close. Record the SHA, the CI run, and a photo or iDRAC export of
   System Security in `docs/evidence/r640/`.

Enabling Secure Boot before the certificate is in `db` makes the firmware
reject the EFI. Disable Secure Boot to boot an unsigned stick again. Do not
enable Secure Boot to try an unsigned CI artifact.

## What this does not do

- It does not put the private key in the binary, the image, or CI logs.
- The 2026-11-05 lab flash is the signed kit with Secure Boot Enabled. An
  unsigned CI artifact still must not be F11'd while Secure Boot is on.
- It does not open A7 (in-place update). That row is wave 6's iron thread
  and waits while wave 5's iron thread (E4) is open.
- It does not print `RAYNU-V-M7-ISO-INSTALL-OK` or any other iron marker.
