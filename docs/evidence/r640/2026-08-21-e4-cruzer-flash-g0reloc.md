# R640 — E4 G0-VMCS-relocate EFI on Cruzer (2026-08-21)

**Claim:** Cruzer Micro `RAYNUV` now holds relocate+fail-soft `r640-hypervisor.efi`
(`618e89e2…`, size `1229312`) from artifact `9432035922` (commit `acba27b` /
relocate `7b750ab`). Helper printed `RAYNU-V-CRUZER-FLASH-OK`.
`installdisk.bin` (1024 bytes) and `auth.token` were left alone.

**Not claimed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK` as a closed iron gate. Flash is
not a boot. Not a distro installer. Not Mount Everest.

Do **not** reflash hang-fix `f413a9fc` (marker then `VMPTRLD slot=0` / VMXOFF)
or hung `67b0acde`.

## Kit

| Field | Value |
|-------|--------|
| Git HEAD (pin) | `acba27b` |
| Relocate feature | `7b750ab` (`VMCLEAR` G0 + host-only slab; fail-soft) |
| Branch | `cursor/e4-spa-launch-a623` (PR #169) |
| Artifact | `r640-hypervisor.efi` id **`9432035922`** (run `32439799435`) |
| EFI SHA256 | `618e89e2acf852e463c17dce0e33337e452caee6d43a6e3907cb18f392ff68b3` |
| EFI size | **1229312** |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Block device | `/dev/sdc` (`/dev/disk/by-label/RAYNUV`) |
| Serial | `200524441218e7503e33` |
| Host | `raynuvsrv1` Ubuntu `10.99.99.124` (PERC; not the HV lease) |
| Helper | `~/flash-cruzer-esp.sh` |

Keep `ape-nophylock=yes`. Bind LOM `:38` / `01:00.0`. Do **not** write PERC
`sda`/`sdb`. Do **not** flash `f413a9fc`, `67b0acde`, `0d06297b`, `c16cbffd`,
`9fc6a3c2`, `26573eb1`, `42b42c99`, `ec08c00f`, or `1404f055`.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Cruzer ESP write | **OK** | [`logs/2026-08-21-e4-cruzer-flash-g0reloc.txt`](logs/2026-08-21-e4-cruzer-flash-g0reloc.txt) |
| G0 VMCS relocate | in the flashed binary | `7b750ab` — not yet booted |
| Iron boot of `618e89e2` | **open** | F11 Cruzer still required |
| E4 SPA VMLAUNCH | **open** | do not claim from flash |

## Next on iron

1. iDRAC SOL `console com2` **before** power.
2. iDRAC **Force Power Off** (Ubuntu is still running). Leave Cruzer seated.
3. Power on → one-time **F11** Cruzer `RAYNUV`. BIOS order stays Ubuntu on PERC.
4. Ignore PRE-EBS SNP `CURL NOW` / 45s accept timeout. `.124` is Ubuntu, not HV.
5. Curl **from the Mac** only after native coexist listen (lease from COM2):

```text
HOST-NIC coexist listening on <LEASE>:8443 (VMX on; ADR-013 Phase F)
CURL NOW → http://<LEASE>:8443/
```

6. spec → `sleep 2` → start. No SPA.
7. WANT COM2: `E4 G0 VMCS relocated`, `RAYNU-V-M7-E4-SPA-LAUNCH-OK`, then more
   `HOST-NIC-HTTP-OK`. FAIL: `VMPTRLD failed slot=0` / `VMXOFF` / `boot gate failed`.
