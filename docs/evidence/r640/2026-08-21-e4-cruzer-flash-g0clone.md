# R640 — E4 G0-VMCS-clone EFI on Cruzer (2026-08-21)

**Claim:** Cruzer Micro `RAYNUV` now holds VMREAD/VMWRITE-clone
`r640-hypervisor.efi` (`63cd694f…`, size `1231872`) from artifact `9461155533`
(commit `7d657b1` / clone `33155c1`). Helper printed `RAYNU-V-CRUZER-FLASH-OK`.
`installdisk.bin` (1024 bytes) and `auth.token` were left alone.

**Not claimed:** `RAYNU-V-M7-E4-SPA-LAUNCH-OK` as a closed iron gate. Flash is
not a boot. Not a distro installer. Not Mount Everest.

Do **not** reflash memcpy EFI `618e89e2` (VMPTRLD loop), hang-fix `f413a9fc`
(VMXOFF), or hung `67b0acde`.

## Kit

| Field | Value |
|-------|--------|
| Git HEAD (pin) | `7d657b1` |
| Clone feature | `33155c1` (VMREAD/VMWRITE G0 VMCS + sticky-park slot 0) |
| Branch | `cursor/e4-spa-launch-a623` (PR #169) |
| Artifact | `r640-hypervisor.efi` id **`9461155533`** (run `32522622376`) |
| EFI SHA256 | `63cd694f4f1f1bd8f8e11641df151811707bb063dd49676bb418b1d778348878` |
| EFI size | **1231872** |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Block device | `/dev/sdc` (`/dev/disk/by-label/RAYNUV`) |
| Serial | `200524441218e7503e33` |
| Host | `raynuvsrv1` Ubuntu `10.99.99.124` (PERC; not the HV lease) |
| Tree | `~/projects/raynu` |
| Helper | `./tools/flash-cruzer-esp.sh` |

Keep `ape-nophylock=yes`. Bind LOM `:38` / `01:00.0`. Do **not** write PERC
`sda`/`sdb`. Do **not** flash `618e89e2`, `f413a9fc`, `67b0acde`, `0d06297b`.

## What this proves

| Gate | Status | Evidence |
|------|--------|----------|
| Cruzer ESP write | **OK** | [`logs/2026-08-21-e4-cruzer-flash-g0clone.txt`](logs/2026-08-21-e4-cruzer-flash-g0clone.txt) |
| G0 VMCS clone | in the flashed binary | `33155c1` — not yet booted |
| Iron boot of `63cd694f` | **booted** | clone+marker+G0 VMLAUNCH; slot 1 error 11 — see [`2026-08-21-e4-g0clone-spa-slot1-rev11.md`](2026-08-21-e4-g0clone-spa-slot1-rev11.md) |
| E4 SPA VMLAUNCH | **open** | not a close |

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

```bash
LEASE=10.99.99.REPLACE_FROM_COM2
TOK='Authorization: Bearer raynu-v-bringup'
curl -4 --noproxy '*' -sS -m 20 -D - -o /tmp/raynu-e4-spec.body \
  -H "$TOK" -X POST "http://${LEASE}:8443/vms/1/spec/1/512/1024/0"
sleep 2
curl -4 --noproxy '*' -sS -m 20 -D - -o /tmp/raynu-e4-start.body \
  -H "$TOK" -X POST "http://${LEASE}:8443/vms/1/start"
```

WANT COM2: `E4 G0 VMCS clone fields=` … `VMPTRLD verify ok`, then
`RAYNU-V-M7-E4-SPA-LAUNCH-OK`, then either G0 `VMLAUNCH` or one park HINT.
FAIL: repeating `VMPTRLD failed slot=0`, `VMXOFF`, `boot gate failed`.
