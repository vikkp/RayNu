# Why firmware Tcp4 does not appear on R640 Virtual Floppy

**Status:** Investigation (not a gate close).  
**Does not claim:** `RAYNU-V-M7-ISO-INSTALL-OK` / Mount Everest.  
**Does not retract:** `RAYNU-V-M7-UEFI-HTTP-OK` (SNP + smoltcp residual still valid).

**Hardware:** Dell PowerEdge R640 · BIOS 2.2.11 · iDRAC Virtual Floppy · SNP residual lease `10.99.99.127:8443`.

## Verdict (current evidence)

Firmware **UNDI/SNP starts** after `ConnectController` (`snp=12`). The **NetworkPkg IPv4 stack never appears**: `mnp=0 ip4=0 dhcp4=0 tcp4=0` after PCI + SNP connect. Enabling “UEFI Network Stack” in BIOS did **not** change that (see `logs/2026-08-16-uefi-http-tcp4-absent-com2.txt`).

`ConnectController` can only **Start** drivers that are already **dispatched** (Driver Binding installed). It cannot conjure `Tcp4Dxe` if Dell BDS did not load NetworkPkg on this boot path.

**Working explanation:** Virtual Floppy boot uses a BDS path that starts NIC UNDI (SNP) but does **not** dispatch MnpDxe → Ip4Dxe → Dhcp4Dxe → Tcp4Dxe. Those DXEs are tied to **UEFI PXE / HTTP / iSCSI boot options**, not to “any boot that has a NIC.”

Until a later iron census shows `mnp>0` / `pxe>0` / `http>0` / loaded `Tcp4Dxe`, treat **firmware Tcp4 as unavailable on Virtual Floppy** and keep ADR-012 SNP residual.

---

## 1. Sequence we look for (and call)

Entry: `src/main.rs` → `run_pre_ebs_mgmt_listen()` → `net_probe_uefi::probe_and_print()` → `listen_mgmt_http_uefi(8443)`.

### Protocol census (`LocateHandleBuffer(ByProtocol)`)

| Layer | GUID | What we count | Iron (lived) |
|-------|------|----------------|--------------|
| SNP instance | `a19832b9-ac25-11d3-9a2d-0090273fc14d` | `SimpleNetwork` | 0 → **12** after PCI connect |
| MNP **service binding** | `f36ff770-a7e1-4cf9-9cba-e34b511d67b6` | NetworkPkg L2 mux | **0** |
| Ip4 **service binding** | `c51711eb-a9cf-46df-8e9e-23f19aa49611` | NetworkPkg IPv4 | **0** |
| Dhcp4 **service binding** | `9d9a39d8-bd06-45c5-aa0b-918bd6483b45` | NetworkPkg DHCP | **0** |
| Tcp4 **service binding** | `00720665-67eb-4a99-baf7-d3c33a1c7ce9` | `create_tcp4_child` | **0** |
| PCI I/O | `4cf5b200-68b8-4ca5-9eec-b23e3f50029a` | UNDI parent | **313** |
| Device Path | `09576e91-6d3f-11d2-8e39-00a0c969723b` | connect sweep | (not counted on COM2) |

**New extra census** (this change; next iron boot):

| Protocol | GUID | Why |
|----------|------|-----|
| NII / UNDI 3.1 | `e18541cd-f755-4f73-928d-643c8a79b229` | Confirms UNDI identifier vs software SNP |
| PXE Base Code | `03c4e603-ac28-11d3-9a2d-0090273fc14d` | PXE stack dispatched? |
| HTTP Service Binding | `bdc8e6af-d9bc-4379-a72a-e0c4e75dae1c` | HTTP boot DXE? |
| Ip4Config2 | `5b446ed1-e30b-4faa-871a-3654eca36080` | Ip4 config protocol without SB? |
| Driver Binding | `18a031ab-b443-4d1a-a5c0-0c09261e9f71` | Any dispatched UEFI drivers |

We do **not** use UEFI `EFI_HTTP_PROTOCOL` for the SPA — HTTP is the in-tree codec over TCP.

### `ConnectController` / `Start` / `OpenProtocol` order

```text
1. snapshot()                          # probe
2. ConnectController(PCI I/O, recursive=true)
3. ConnectController(Device Path, recursive=true)
4. snapshot()                          # after-pci   ← iron: snp=12, tcp4=0
5. ConnectController(SNP, recursive=true)   # if snp>0
6. snapshot()                          # after-snp
7. ConnectController(MNP/Ip4/Dhcp4/Tcp4 SB) # no-op if 0 handles
8. ConnectController(AllHandles take 256, recursive=true)
9. snapshot() + extra census           # after-all
10. listen: LocateHandle(Tcp4 SB) → create_child → OpenProtocol(Tcp4)
    else: OpenProtocol(SNP, GetProtocol) → snp.start() + initialize()
          → smoltcp DHCP + TCP
```

`connect_controller(handle, None, None, true)` = recursive Start of every Driver Binding that `Supported()` the handle. Spec expectation: UNDI → SNP → **MnpDxe** → **Ip4Dxe** → **Tcp4Dxe**. Step 5–8 only work if those DXEs are already in the driver list.

SNP residual uses **`GetProtocol`**, not Exclusive — so we do not kick a hypothetical MNP `BY_DRIVER` open. (On this iron, MNP never opened SNP.)

### What we were missing (now fixed in probe only)

| Gap | Effect |
|-----|--------|
| All-handles pass **skipped** once `snp>0` | Could hide a late-dispatched DXE on a non-PCI handle |
| Never `ConnectController` on MNP/Ip4/Dhcp4/Tcp4 SB | Harmless if counts are 0; required if SB exists but children are not started |
| No NII / PXE / HTTP / Ip4Config2 census | Could not distinguish “UNDI only” vs “NetworkPkg present but unbound” |

Listen path is unchanged: Tcp4 first, SNP residual on `NoTcp4Stack`.

---

## 2. Missing OpenProtocol / Connect steps?

**Unlikely to be a missed `OpenProtocol` on Tcp4.** `create_tcp4_child` already `LocateHandleBuffer(Tcp4ServiceBinding)`. Empty buffer → no stack. You cannot Open what firmware never installed.

**Possible missed Connect (now attempted):**

1. All-handles after SNP (previously skipped).
2. Explicit SB connect if any NetworkPkg SB appears.

**Not missing, and must not do on the SNP residual:**

- `OpenProtocol(SNP, Exclusive)` — would `DisconnectController` MNP if it ever appeared (GRUB/edk2 lesson). We use `GetProtocol`.
- Starting our own TCP while firmware Tcp4 is live on the same SNP — ADR-012: one stack per path.

**Cannot be fixed by more ConnectController:** loading `Tcp4Dxe` from a firmware volume or from our FV. Connect does not `LoadImage`.

---

## 3. Dell / R640 / Virtual Floppy quirks

Dell 14G (R640) BIOS *Network Settings* (UEFI mode only):

| Menu | What it actually does |
|------|------------------------|
| **UEFI PXE Settings → PXE Device n** | Creates a **PXE boot option**. BDS connects NetworkPkg when **that option** is selected. |
| **UEFI HTTP Settings → HTTP Device n** | Same for HTTP boot (needs HttpDxe + Tcp4). |
| **UEFI iSCSI Settings** | Same for iSCSI boot. |
| Device Settings / NIC option ROM | Legacy UNDI; not the full IPv4 stack. |

Lived: operator enabled “UEFI Network Stack” and **still** got `Tcp4 stack absent` on Virtual Floppy (`tcp4-absent` COM2). That matches Dell’s model: the toggle **creates boot options**; it does **not** promise Tcp4 for Floppy/USB BDS.

**Virtual Floppy / iDRAC vMedia:**

- BDS path is USB/floppy, not Network.
- UNDI often still starts (PCI connect → `snp=12`).
- PXE/HTTP DXEs typically stay **undispatched** until a network boot option runs.
- Floppy is a short PRE-EBS window; we already cap all-handles at 256.

**BIOS 2.2.11** is old relative to current 14G (2.22.x). A BIOS update is a valid experiment; do not assume it alone enables Tcp4 on Floppy.

**Host NIC vs iDRAC:** SNP DHCP is on the LOM/host LAN (`10.99.99.127`), not the iDRAC BMC IP. Unrelated to Tcp4 absence.

---

## 4. Loading NetworkPkg / Tcp4Dxe ourselves

| Option | Feasible? | Size | Risk |
|--------|-----------|------|------|
| **A. Rely on firmware** (status quo + better census) | Yes | 0 | None to SNP residual |
| **B. BIOS: enable PXE/HTTP Device + still boot Floppy** | Experiment | 0 | May dispatch NetworkPkg at BDS even for Floppy |
| **C. `LoadImage` Tcp4Dxe from Dell FV** | Maybe | 0 | Need FV walk + DEPEX; Dell may not expose NetworkPkg files |
| **D. Embed EDK2 NetworkPkg DXEs in our EFI/ESP** | Yes in principle | ~300–800 KiB uncompressed (Mnp+Arp+Ip4+Ip4Config2+Udp4+Dhcp4+Tcp4); still ≪ 15 MB ADR-003 | Driver/UNDI mismatch; two-stack policy; maintenance; must Start **before** SNP residual opens the NIC |

**Recommendation:** do **not** embed NetworkPkg yet. Size is safe; complexity and “two stacks” are not. Prove first (next iron extra-census + BIOS experiments) that firmware DXEs are absent. If B or C produces `tcp4>0`, use firmware Tcp4 and leave smoltcp as residual. If they stay 0, document Virtual Floppy as a **platform limitation** and keep SNP residual.

We do **not** `LoadImage`/`StartImage` any DXE today.

---

## 5. Next iron experiments (ordered)

Do these on the **tip that prints `extra` / `after-all` / `stack_ok`**. SNP residual must still print `CURL NOW`.

| # | Experiment | How | Success look like |
|---|------------|-----|-------------------|
| 1 | **Extra census on current Floppy path** | Remap tip media; SOL `console com2` | `extra nii=… pxe=… http=…`; `after-all` still `tcp4=0` **or** surprise `tcp4>0` |
| 2 | **PXE Device 1 Enabled**, still boot Virtual Floppy | F2 → Network Settings → UEFI PXE → Device 1 Enabled; one-shot Floppy | If `pxe>0` or `mnp>0`: firmware stack **exists**, Floppy BDS was the gap |
| 3 | **HTTP Device 1 Enabled**, Floppy boot | Same menu, HTTP Device | `http>0` / `tcp4>0` would mean HttpDxe pulled Tcp4 |
| 4 | **USB ESP** (not iDRAC Floppy) | Same `.img` on FAT USB | Isolates vMedia vs any removable boot |
| 5 | **BIOS 2.2.11 → current 14G** | iDRAC firmware update (maintenance window) | Repeat 1–3; note version on evidence |
| 6 | **One-shot PXE boot** (optional) | F11 → PXE Device — **do not expect our EFI** | Only proves firmware can run PXE; not our listen |

**Stop rule:** if experiment 1 shows `nii>0` (or `snp>0`) and `pxe=0 http=0 mnp=0 ip4=0 tcp4=0` after `after-all`, that is enough to file **platform limitation: Virtual Floppy does not dispatch NetworkPkg**. Experiments 2–5 are then optional confirmation, not blockers.

**Do not:** rewrite SNP residual; Exclusive-open SNP; claim Tcp4 from host/QEMU.

---

## Lived COM2 fingerprints (prior)

```text
boot: uefi-net probe snp=0 mnp=0 ip4=0 dhcp4=0 tcp4=0 pci=313
boot: uefi-net after-pci snp=12 mnp=0 ip4=0 dhcp4=0 tcp4=0 pci=313
```

Then SNP residual: `CURL NOW` → `AuthAllowed` / `VmCreated` → `RAYNU-V-M7-UEFI-HTTP-OK`.

Archives: `logs/2026-08-16-uefi-http-net-probe-zero-com2.txt`, `…-tcp4-absent-com2.txt`, `…-snp12-after-pci-com2.txt`, `…-uefi-http-ok-com2.txt`.

## Reproduce (diagnostics tip)

```bash
git checkout main && git pull
./tools/rebuild-uefi-http-boot-media.sh
# remap Virtual Floppy; SOL console com2
# look for: extra / after-snp / stack_ok / after-all / extra-after
```
