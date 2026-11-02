# 2026-11-02 — `12ffaff69fb9`: certificate read faulted before VMX

**EFI:** `12ffaff69fb9` (CI run `41209535454`). COM2 `build: sha=12ffaff69fb9 run=41209535454`.
**Flash:** Lexar, front USB 2. Ubuntu 26.04 standing on UBUNTU0. No reinstall. No doorbell. UBUNTU0 was not written.
**Do not F11 this file again.** The save on the previous boot of the same file stands. The read does not.

## Save (previous boot of this file)

Overview **Save certificate** after sign-in as `vikkp`. COM2:

```text
boot: E4 cert wr lba=6190268891 match
RAYNU-V-AUDIT: HostCertSaved source=1 cert=790
```

The page then showed **Certificate: operator 790 bytes** on `10.99.99.146/24`. That write is the spare image. This boot did not write it again.

## Read

Same stick. COM2 reached:

```text
boot: E6 operator rd lba=6190268907 set
```

and stopped. The video console was the BIOS red screen:

```text
Type: Invalid opcode (06) Source: Software (UEFI0004) on BSP
RIP=000000014002478F
-->RIP 14002478F r640_hypervisor-22fb1ba91daeed95.efi +02478F
```

`+02478F` is `vmxoff` in `reboot_after_dump`, immediately after the 59-byte line `boot: H2 reboot (saved guests start on the next RayNu boot)`. CR4.VMXE was clear, so VMXOFF is #UD. The CPU was still on the firmware exception handler. ExitBootServices had already returned. VMXON had not run.

The certificate read is the next call after the operator read. It parses the PKCS#8 key. `rsa` allocates. `mark_post_ebs` ran only after `leave_firmware` returned, so the allocator was still the UEFI pool, which returns null once boot services are gone. The null allocation panicked. The panic line uses the guest TX ring and did not reach SOL before the #UD.

The spare was not written on this boot. Operator account, host address, and the certificate image stay where the previous boot left them.

**E4 stays in flight.** Not a new `ISO-INSTALL-OK`. HDA stays 99%.
