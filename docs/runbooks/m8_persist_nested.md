# Runbook — M8.0 nested Alpine persist (`MODE=full`)

**Harness marker:** `RAYNU-V-M8-DISK-PERSIST-NESTED-OK` (`tools/m8-persist-nested.sh` only)  
**Iron marker:** `RAYNU-V-M8-DISK-PERSIST-OK` (real R640 COM2 — **not this runbook**)  
**Host marker:** `RAYNU-V-M8-DISK-PERSIST-HOST-OK` (cargo — **not this runbook**)  
**Never print:** `RAYNU-V-M7-ISO-INSTALL-OK`  
**M8.0 known-good flash:** GitHub Latest [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed). Do not F11.

## Story

`MODE=keep` / `lunkeep` / `usbkeep` plant a GPT fixture and prove `keep=1` after an HV kill. That is **not** nested-OK.

`MODE=full` is the nested close: real alpine-extended `setup-disk` → kill the **hypervisor** QEMU process (not guest F7) → second Linux without `setup-disk`. Virtio HPAs are leftover DRAM promoted to File persist; QEMU initial RAM is `M8_PERSIST_IMG` (`share=on`) because distro `OVMF_CODE_4M.fd` ignores nvdimm/pc-dimm.

Needs **nested KVM (VMLAUNCH)**. Cloud Agent VMs that log `kvm_spurious_fault` cannot close this gate. Run on **`raynuvsrv1`**. Nested QEMU ≠ R640.

## Prerequisites

1. `raynuvsrv1` (Ubuntu on the R640 PERC). Intel VT-x, `/dev/kvm` writable.  
2. `sudo ./tools/enable-nested-kvm.sh` → `nested=Y` and `enable_shadow_vmcs=0` (otherwise VMWRITE error 12).  
3. alpine-extended ISO (the harness fetches it). alpine-virt/standard lack `grub-efi`.  
4. Quit every other QEMU using KVM before the run.

## Procedure

```bash
cd ~/projects/raynu   # or the clone path
git fetch origin && git checkout <m8 persist tip with this runbook>
MODE=full ./tools/m8-persist-nested.sh
```

Expect:

1. Boot 1: leftover File persist reserve, VMLAUNCH (not `VMXON-SKIP`), Alpine `Installation is complete. Please reboot.`  
2. Harness SIGTERM + `sync` of `target/m8-persist.img`, then `EFI PART` at the persist HPA.  
3. Boot 2: `virtio-blk install disk bytes=… keep=1`, **no** `setup-disk`, `RAYNU-V-RAYNU-F-DISK-BOOT-OK` and/or `root=UUID=`.  
4. Harness prints `RAYNU-V-M8-DISK-PERSIST-NESTED-OK`. Serial must **not** print `RAYNU-V-M7-ISO-INSTALL-OK` or `RAYNU-V-M8-DISK-PERSIST-OK`.

Copy boot1/boot2 serial to `docs/evidence/` if you keep a nested evidence file. Do not treat this as iron persist.

## Not this

- TCG `MODE=smoke|keep|lun|usb|lunkeep|usbkeep`  
- Iron Force Off / `RAYNU-V-M8-DISK-PERSIST-OK`  
- M8.1 TLS  
- Formatting the PERC Ubuntu disk  
