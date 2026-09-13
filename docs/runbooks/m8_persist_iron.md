# Runbook — M8.0 iron DurableLun (PCI pick → post-EBS I/O → Phase B attach)

**Iron marker:** `RAYNU-V-M8-DISK-PERSIST-OK` (Force Off / reboot **RayNu-V** — **not this slice**)  
**Nested marker:** `RAYNU-V-M8-DISK-PERSIST-NESTED-OK` (QEMU File RAM — **not this runbook**)  
**Host marker:** `RAYNU-V-M8-DISK-PERSIST-HOST-OK` (cargo — **not this runbook**)  
**Never print:** `RAYNU-V-M7-ISO-INSTALL-OK`  
**M8.0 known-good flash:** GitHub Latest [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed). Do not F11 a persist prototype until COM2 shows I/O ready + virtio on the LUN.

Nested QEMU ≠ R640. QEMU NVMe/USB ≠ Intel PCH xHCI. Nested File RAM (`M8_PERSIST_IMG` `share=on`) does not exist on iron.

---

## Hardware (required before F11)

| Role | What | Notes |
|------|--------|--------|
| ESP | Cruzer / UDisk 2–8 GiB with alpine-extended | Boot stick. Mapper **refuses** this window as a LUN. |
| LUN | NVMe class `01:08`, **or** USB ≥ 1 GiB **outside** 2–8 GiB | 16 GiB+ stick is the usual USB pick. 1–2 GiB also eligible. never PERC. |
| Standing boot | Ubuntu on PERC | Do **not** format PERC. One-time F11 the ESP only. |
| Fleet (later) | Spare PERC **virtual disk** | [ADR-019](../adr/ADR-019.md). Not tonight. USB persist ≠ `RAYNU-V-M8-PERC-LUN-OK`. |

Without a LUN, COM2 prints `durable LUN none (leftover DRAM)` / `skip PERC` / `skip ESP Cruzer` plus `need NVMe class 01:08 or USB ≥1GiB outside Cruzer 2-8GiB`. Phase B then installs to leftover DRAM (dies on Force Off).

---

## COM2 needles (this slice)

Open iDRAC **SOL `console com2` first**.

### 1. PCI pick

Expect storage-class lines, then a pick or a reject:

- `durable LUN pci … nvme` → pick NVMe (`01:08`)
- `skip PERC` / `skip AHCI` / `skip ICH9 SATA` / `skip IDE` — not LUNs
- `durable LUN xhci … 8086:a1af` (Lewisburg) — USB I/O after EBS
- `durable LUN picked nvme|usb …`
- or `durable LUN none (leftover DRAM)` + the need-media line

### 2. Post-EBS I/O

After ExitBootServices:

- `durable LUN nvme I/O ready bytes=…` **or**
- `durable LUN usb I/O ready bytes=…` (ESP Cruzer window skipped; next port)
- `xhci caplen=… ports=… p1=0x…` (all ports through 16, not only 4)

Fail: `nvme I/O fail` / `usb I/O fail err=` + leftover DRAM. Intel PCH scratchpad ≤ 16 pages is supported; more is `err=1` Cap.

### 3. Phase B attach

Same Everest loop (`HOST-NIC-HTTP-OK` → SPA Start → install → F7 `DISK-BOOT-OK`), but virtio must be the LUN:

```
leftover install disk skip durable LUN
virtio-blk install disk bytes=… keep=0 (durable LUN nvme)
```

or `… (durable LUN usb)`.

**Not ready:** `leftover install disk hpa=… (leftover DRAM; durable LUN not ready)` then `virtio-blk … (leftover DRAM; durable LUN not ready)`. Do not Force Off expecting persist.

---

## Do not F11 until

1. A LUN the mapper will pick is seated.
2. COM2 shows `I/O ready` **and** `virtio-blk … (durable LUN nvme|usb)`.
3. Then one-time F11 this EFI (not `ce3d8a09` File-RAM). Ubuntu-on-PERC stays standing boot.

Force Off / reboot RayNu-V (not guest F7) is the iron close (`RAYNU-V-M8-DISK-PERSIST-OK`). That is **after** 1–3. If the prototype misbehaves, re-flash [`v0.1.0-everest-closed`](https://github.com/vikkp/RayNu/releases/tag/v0.1.0-everest-closed).

## Not this

- Nested `MODE=full` / File RAM
- M8.1 TLS
- Formatting the PERC
- Printing `ISO-INSTALL-OK` from host/CI/nested
