# R640 — ADR-013 Phase D / E3b closed (2026-08-20)

**Claim:** `RAYNU-V-M7-HOST-NIC-HTTP-OK` on real Dell PowerEdge R640 **after**
`RAYNU-V-R640-BOOT-OK`, on the host-owned BCM5720 (`14e4:165f` / `01:00.0` /
`:38`). Operator SPA loaded on `http://10.99.99.144:8443/` during the native
listen window.

**Not claimed:** Mount Everest. E4 polish (TLS/console) and a real distro
installer remain. PRE-EBS SNP HTTP does **not** count (timed out this boot).

## Closing kit

| Field | Value |
|-------|--------|
| Git HEAD (pin) | `ca198e2` |
| Feature | `005f25d` (`GRC_MODE_BSWAP_DATA`) |
| Branch | `cursor/m7-8-bcm-snp-mac-a623` (PR #162) |
| CI run | `32318819378` (success) |
| Artifact | `r640-hypervisor.efi` id **`9389057864`** (zip digest `sha256:7ec20ebc…3978`) |
| Media | Cruzer Micro, front USB 2, label `RAYNUV` |
| Lease | `10.99.99.144:8443` (parked SNP IPv4; native Device after EBS) |
| Station | `b0:26:28:5c:5a:38` (live LOM / Ubuntu `eno3`) |

Keep `ape-nophylock=yes`. Do **not** flash take-PHY / skip-CORECLK.

## What closed

| Gate | Marker | Evidence |
|------|--------|----------|
| M7.8 / E3b iron | `RAYNU-V-M7-HOST-NIC-HTTP-OK` | [`logs/2026-08-20-host-nic-http-ok-com2.txt`](logs/2026-08-20-host-nic-http-ok-com2.txt) |
| E2 (same boot) | `RAYNU-V-R640-BOOT-OK` | guest path through M4 + VMXOFF |
| E5 stamps (same boot) | `RAYNU-V-M7-ISO-BOOTED-FROM-DISK` | persist-detect still green |

## Path (honest)

1. PRE-EBS SNP residual leased `:38` / `10.99.99.144`. PRE-EBS HTTP window
   timed out (operator curled after `BOOT-OK`).
2. Post-EBS: inherit SNP analog, `CORECLK_RESET` for DMA, skip BMCR,
   `ape-nophylock=yes`, **`grc=bswap+wswap (Linux LE tg3)`**.
3. `link=up speed=1000 duplex=full`. Guest path green through SHELL / M4 / E2.
4. After `BOOT-OK`: firmware SNP idle skipped. Native BCM5720 reuse + listen.
5. First RX dumps show real ethertypes (`0800` / `86dd`) on multicast — endian
   closed. Then `HOST-NIC TCP accept`, `HOST-NIC HTTP exchange ok`,
   `RAYNU-V-M7-HOST-NIC-HTTP-OK`. `tx_prod`/`tx_cons` moved (16/16, 17/17).
6. Later exchange printed `RAYNU-V-AUDIT: AuthAllowed method_tag=1` (Bearer REST).

## Serial excerpt

```text
boot: HOST-NIC BCM5720 grc=bswap+wswap (Linux LE tg3)
boot: HOST-NIC BCM5720 link=up speed=1000 duplex=full
RAYNU-V-R640-BOOT-OK
boot: HOST-NIC idle listening on 10.99.99.144:8443 (after BOOT-OK BCM5720)
boot: CURL NOW → http://10.99.99.144:8443/  (native BCM5720; SNP is dead)
boot: HOST-NIC BCM5720 rx to=other etype=0800 hw=143 n=139 dst=01:00:5e:00:00:fb
boot: HOST-NIC TCP accept — client connected
boot: HOST-NIC HTTP exchange ok
RAYNU-V-M7-HOST-NIC-HTTP-OK
boot: HOST-NIC BCM5720 poll rx_prod=7 rx_cons=7 tx_prod=16 tx_cons=16 rx_ok=71
RAYNU-V-AUDIT: AuthAllowed method_tag=1
```

Full fingerprint: [`logs/2026-08-20-host-nic-http-ok-com2.txt`](logs/2026-08-20-host-nic-http-ok-com2.txt).

## Honesty

- QEMU `HOST-NIC-QEMU-OK` does not count.
- PRE-EBS `RAYNU-V-M7-UEFI-HTTP-OK` does not count.
- Phase F (native as primary beside VMX) closed later the same day. 72h soak
  stays later. Phase G closed 2026-08-21 as accepted-risk (shared LOM).
- Preserve kit for rollback: `releases/v0.1.0-adr013-baseline`.
