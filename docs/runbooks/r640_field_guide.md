# RayNu-V — R640 first-light field guide (printable)

**Purpose:** Step-by-step order of action when a Dell PowerEdge **R640** is
racked and you want first light of `r640-hypervisor.efi`.  
**Gate:** `RAYNU-V-R640-BOOT-OK` (real iron only — a laptop smoke does **not**
count).  
**Related docs:** [`r640_iron_week.md`](r640_iron_week.md) (short checklist) ·
[`media_maker.md`](media_maker.md) (boot `.img` / `.iso` helper) ·
[`usb_idrac.md`](usb_idrac.md) · [`r640_boot.md`](r640_boot.md) ·
[`docs/evidence/r640/`](../evidence/r640/)

Print this page. Check boxes as you go. Write IPs / passwords / SHA256 in the
blanks — do not invent serial markers if the box did not print them.

### Plan at a glance (updated)

| Step | What you do | Tool |
|------|-------------|------|
| 1 | Get R640 alive (power, iDRAC, virtual console) | Hands + browser |
| 2 | Build + checksum the EFI kit on a **laptop** | `./tools/package-release.sh` |
| 3 | Pack `\EFI\BOOT\BOOTX64.EFI` into boot media | **`./tools/make-boot-media.sh`** (not hand-copy) |
| 4 | Attach media | **Prefer iDRAC Virtual Media** map of the `.img`; USB optional |
| 5 | Reboot, capture COM1, fill evidence, close in git | Field guide §§4–6 |

You do **not** need to manually format a FAT stick unless the media maker cannot
run on your laptop (fallback only).

---

## Before you start (what you need)

| Item | Notes / fill-in |
|------|-----------------|
| Dell PowerEdge R640 in a rack | Rails seated; service tag: _______________ |
| Power cords (redundant PSUs if present) | Both cords preferred |
| Network cable to **iDRAC** port | Dedicated Mgmt / iDRAC port on the rear |
| Operator laptop on the same mgmt network | Browser + ability to save a text log |
| iDRAC username / password | Default is often on the pull-tab; change if still factory |
| Laptop packages for media maker | `dosfstools` + `mtools` (`xorriso` optional for ISO) |
| USB stick (optional — only if not using iDRAC vMedia) | ≥ 64 MiB free; will be erased by `make-boot-usb.sh` |
| RayNu-V git checkout on a **build** machine | Not required on the R640 itself |

**Install media-maker deps once (laptop):**

```bash
# macOS
brew install dosfstools mtools xorriso

# Ubuntu/Debian
sudo apt install dosfstools mtools xorriso
```

**Rule:** Open the iDRAC **virtual console before** you reboot into RayNu-V, or
you will miss early COM1 markers.

---

## 1. Get the box alive

### a. Power on — what to do and what to check

1. Confirm the chassis is seated in the rack and ground/earthing is acceptable
   for your site.
2. Plug power:
   - Connect PSU1 (and PSU2 if present) to rack PDU outlets.
   - Prefer two different PDU circuits if you have them.
3. Press the front **power button** (or use iDRAC later once network is up).
4. Watch front LEDs:
   - Power / health should settle to a normal state (no hard fault blink that
     means “do not proceed”).
   - If the box will not power on: reseat power cords, try the other PSU, check
     PDU breakers — do not continue to software until the chassis powers.
5. Give firmware ~1–2 minutes after AC apply for iDRAC to finish booting (iDRAC
   can be up even when the host OS/firmware is still starting).
6. Record:
   - [ ] Power OK  
   - Date/time: _______________  
   - Who powered it: _______________

### b. Get on the iDRAC network — how to do this

iDRAC is the **out-of-band** management controller. You talk to it from a
browser; you do **not** need Windows/Linux installed on the R640.

1. Find the **iDRAC / Mgmt** Ethernet port on the rear of the R640 (labeled;
   separate from the big NIC ports used by the host OS).
2. Cable that port to your **management** switch / VLAN (not the public
   internet if you can avoid it).
3. Discover the iDRAC IP (pick one method that matches your site):

   | Method | How |
   |--------|-----|
   | LCD / front panel | Scroll to network / iDRAC address if the bezel supports it |
   | DHCP reservation | Ask your network admin for the lease for this service tag |
   | Static from pull-tab / label | Some units ship with a default; still verify |
   | Laptop + USB/serial to iDRAC (advanced) | Only if your site uses that path |

4. From the operator laptop, ping the iDRAC IP. If no reply: check cable, VLAN,
   firewall, and that iDRAC finished booting after AC apply.
5. Open a browser to `https://<iDRAC-IP>/` (certificate warning is common on
   factory certs — use the site’s accepted process to continue).
6. Log in with iDRAC credentials. If this is a first bring-up, change the
   default password and store it in your password manager.
7. In iDRAC, note and write down:

   - [ ] iDRAC IP: _______________  
   - [ ] iDRAC firmware version: _______________  
   - [ ] Service tag: _______________  
   - [ ] Login works from this laptop

**Optional but useful before first RayNu-V boot**

- System → BIOS Settings (or similar): confirm **Boot Mode = UEFI** (not
  Legacy/BIOS-only).
- Enable virtualization (Intel VT-x; VT-d if you will use it later).
- Save and exit if you changed anything — then wait for the next controlled
  reboot with the console already open (step **c**).

### c. Open the virtual console — how to do this

The virtual console is how you **see COM1 / serial** and the remote KVM screen
without a physical monitor on the rack.

1. In the iDRAC web UI, open **Virtual Console** (sometimes under
   *Configuration → Virtual Console*, or a **Launch** button on the dashboard).
2. Allow the console app / HTML5 session if the browser prompts (HTML5 is
   preferred when available).
3. When the console window opens:
   - Confirm you can see the remote video (BIOS / boot logo / prior OS is fine).
   - Find the **serial / text** view if your iDRAC version separates it — RayNu-V
     prints markers on **COM1** (iDRAC virtual serial), which is what we need for
     the gate.
4. Prepare to **capture text**:
   - Prefer copy/paste from the serial pane into a file on the laptop, or
   - Enable any “serial log” / download feature your iDRAC offers, or
   - Have a notepad ready to paste continuously during boot.
5. **Do not reboot yet** for RayNu-V until media is ready (sections 2–3). Leave
   this console window open.

- [ ] Virtual console open and usable  
- [ ] Plan for saving the serial log ready  
- [ ] Console opened **before** the RayNu-V reboot

---

## 2. Build and verify the EFI kit (on a laptop / build machine)

Do this on a normal development machine with the RayNu-V repo — **not** on the
R640. This step only produces the **release kit**. Packing it onto boot media is
**section 3** (`make-boot-media`).

### a. Build the release kit

```bash
cd /path/to/RayNu
./tools/build.sh
./tools/check-size.sh
./tools/package-release.sh
```

If the EFI is already built:

```bash
SKIP_BUILD=1 ./tools/package-release.sh
```

Or run the interactive wrapper (can build the kit, then pack media):

```bash
./tools/media-maker.sh
```

### b. Verify checksums (mandatory)

```bash
cd dist/raynu-v-*    # the versioned kit dir — not the *-boot-media dir
sha256sum -c r640-hypervisor.efi.sha256
sha256sum -c SHA256SUMS
```

Both commands must report **OK**. If not, stop. Do not run `make-boot-media`
on a bad kit. (`make-boot-media.sh` also re-checks the EFI sidecar when present.)

### c. Write down kit identity (for evidence later)

- [ ] Kit version (`VERSION` file): _______________  
- [ ] `r640-hypervisor.efi` SHA256: _______________  
- [ ] Path to kit on this laptop: _______________

---

## 3. Prepare boot media with the media maker (default plan)

**Default plan:** do **not** hand-format a USB stick. Use the media maker to
build a FAT image that already contains `\EFI\BOOT\BOOTX64.EFI`, then attach
that image through **iDRAC Virtual Media** (preferred on a racked server).

Full tool docs: [`media_maker.md`](media_maker.md).

### a. Make the boot image on the laptop

```bash
# From repo root (uses newest dist/raynu-v-* kit, or pass --kit)
./tools/make-boot-media.sh --kit dist/raynu-v-<ver>

# Or interactive (kit + media in one flow):
./tools/media-maker.sh
```

You get:

```text
dist/raynu-v-<ver>-boot-media/
  raynu-v-<ver>-uefi-boot.img    ← map as iDRAC Virtual Media *USB* (preferred)
  raynu-v-<ver>-uefi-boot.iso    ← map as iDRAC Virtual Media *CD* (if xorriso)
  MEDIA.txt
  *.sha256
```

- [ ] Boot `.img` built (script finished clean / printed boot-media marker)  
- [ ] Boot-image SHA256 (`*-uefi-boot.img.sha256`): _______________  
- [ ] Path to `*-boot-media/` on this laptop: _______________

### b. Attach media — Option A (preferred): iDRAC Virtual Media

No USB stick required at the rack.

1. Keep the iDRAC **virtual console** from §1c open.
2. iDRAC → **Virtual Console** → **Virtual Media**.
3. Map `*-uefi-boot.img` as a **virtual USB** stick (best match for
   `\EFI\BOOT\BOOTX64.EFI`).
4. If your iDRAC will not map `.img`, map `*-uefi-boot.iso` as a **virtual CD**.
5. Set **next boot** to that virtual USB/CD (iDRAC one-time boot, or F11).
6. - [ ] Virtual media mapped  
   - [ ] Method: vMedia USB `.img` / vMedia CD `.iso` (circle one)

### c. Attach media — Option B: physical USB (optional)

Use when you want a stick in the chassis, or vMedia is unavailable.

1. Build the `.img` as in **3a** (same file).
2. List disks carefully (`diskutil list` on macOS, `lsblk` on Linux).
3. Write the image (**erases the stick**):

   ```bash
   ./tools/make-boot-usb.sh \
     --img dist/raynu-v-<ver>-boot-media/raynu-v-<ver>-uefi-boot.img \
     --disk /dev/diskN
   ```

   Re-type the disk path when prompted. Wrong disk = data loss.
4. Plug the stick into the R640; note front/rear for evidence.
5. - [ ] USB written and inserted  
   - [ ] Port used: _______________

### d. Manual FAT copy (fallback only)

Only if `make-boot-media.sh` cannot run (missing packages / broken laptop tooling):

1. Format the stick as **FAT32**.
2. Create `EFI/BOOT` and copy `r640-hypervisor.efi` → `\EFI\BOOT\BOOTX64.EFI`.
3. See [`usb_idrac.md`](usb_idrac.md).

- [ ] Using fallback manual copy (note why): _______________

### e. Confirm next-boot choice before reboot

- [ ] Next boot = virtual USB / virtual CD / physical USB (circle)  
- [ ] Serial console still open (§1c)

---

## 4. First light — boot RayNu-V and capture serial

### a. Reboot with console already open

1. Confirm section **1c** console is still open.
2. Reboot the host (iDRAC **Power** → reboot/reset, or graceful reset from the
   console).
3. At the firmware boot menu (F11 if needed), select the **virtual USB / CD**
   or physical USB entry from section 3 (the media-maker image).
4. Watch COM1 / serial text in the virtual console.

### b. What “success” looks like on the wire

Look for these strings in the serial log (copy everything you see):

| Priority | Marker / signal | Meaning |
|----------|-----------------|--------|
| Required | `RAYNU-V-M0-BOOT-OK` | EFI started; COM1 path works |
| Strong | `RAYNU-V-M1-VMXON-OK` / `RAYNU-V-M1-VMEXIT-OK` | VT-x path alive |
| Strong | Later `RAYNU-V-M2-…` / `RAYNU-V-M3-…` markers | EPT / Linux path as far as iron reaches |
| Optional | Linux shell / earlyprintk banners | Best case for first light |

If you get M0 but stop short of shell: **that is still useful evidence** —
write the residual honestly. Do **not** type markers that did not appear.

### c. Save the log and clean up media

1. Paste / save the full serial session to a file on the laptop, e.g.
   `r640-serial-YYYY-MM-DD.txt`.
2. - [ ] Serial log saved path: _______________
3. **Unmount** iDRAC virtual media (if used) so the next reboot does not stick
   on the installer stick/image.
4. Remove the USB stick if you are done with this attempt (or leave it only if
   you intend another deliberate boot).

---

## 5. Fill the evidence template

Empty templates do **not** close the gate.

### a. Create a dated evidence file

1. On the build/laptop checkout:

   ```bash
   cp docs/evidence/r640/TEMPLATE.md \
      docs/evidence/r640/YYYY-MM-DD-r640-first-light.md
   ```

2. Open the new file and fill **every** table field:
   - Date (UTC), operator, service tag / hostname
   - Boot method: **iDRAC vMedia `.img`** / vMedia `.iso` / physical USB
     (from media maker — note which)
   - EFI path on media: `\EFI\BOOT\BOOTX64.EFI` (from `make-boot-media`)
   - `r640-hypervisor.efi` SHA256 (from section 2)
   - Boot-image SHA256 (`*-uefi-boot.img.sha256`) if used
   - Release kit version, iDRAC firmware, BIOS/UEFI mode
   - Serial channel used (iDRAC virtual COM1, etc.)

### b. Paste proof

1. Check off the marker list only for strings that appear in **your** log.
2. Paste a real serial excerpt into the template (from the R640 — not from
   Latitude/QEMU).
3. Leave `docs/evidence/r640/STATUS` as `STATUS=open` until the close PR.

- [ ] Dated evidence file filled  
- [ ] Serial excerpt is from this R640

---

## 6. Close M7.5 in git (only after real iron proof)

### a. Open a pull request

1. Commit the dated evidence file.
2. Set `docs/evidence/r640/STATUS` to `STATUS=closed` and point at that file.
3. Update docs that claim the gate (`GAP(CLOSED M7.5): Real R640 boot`,
   `docs/progress.md`, HDA E2, site) in the **same** change set when evidence
   is real.

### b. What you may claim

```text
RAYNU-V-R640-BOOT-OK
==> M7.5 real R640 boot PASSED
```

Only after the evidence PR is honest and complete.

### c. What you must not claim

- Do **not** treat `./tools/m7-r640-smoke.sh` on a laptop as first light — it
  only prints `RAYNU-V-M7-R640-SCAFFOLD-OK`.
- Do **not** use QEMU / Latitude serial logs as R640 evidence.

---

## Quick reference — order of action

1. Get box alive → power → iDRAC network → virtual console open  
2. `./tools/package-release.sh` (+ checksum) on a laptop  
3. `./tools/make-boot-media.sh` → map `*-uefi-boot.img` in iDRAC Virtual Media  
   (or `./tools/make-boot-usb.sh` for a physical stick)  
4. Reboot; capture COM1; save log; **unmap** vMedia  
5. Fill dated evidence from `TEMPLATE.md`  
6. Close in git only with real iron proof  

One-liner mental model: **kit → make-boot-media → iDRAC map → serial → evidence**.

---

## Troubleshooting (short)

| Symptom | Try |
|---------|-----|
| iDRAC page won’t load | Cable on Mgmt port? VLAN? Wait 2 min after AC; ping IP |
| Virtual console blank | Allow pop-ups / HTML5; try another browser; check iDRAC license features |
| `make-boot-media` missing `mkfs.vfat` / `mcopy` | Install deps (§ Before you start); see [`media_maker.md`](media_maker.md) |
| No `*-uefi-boot.img` | Did `package-release` create `dist/raynu-v-*`? Pass `--kit` explicitly |
| iDRAC won’t map `.img` | Try mapping `*-uefi-boot.iso` as CD, or use `make-boot-usb.sh` |
| No `RAYNU-V-M0-BOOT-OK` | Wrong boot device? Console opened too late? Remake media; re-verify SHA |
| M0 only, no VMX | Confirm VT-x enabled in BIOS; note residual in evidence |
| Sticky boot to USB/vMedia | Unmap virtual media; remove USB; clear one-time boot override |

---

*RayNu-V · M7.5 field guide · printable companion to `docs/runbooks/r640_iron_week.md` · media via `docs/runbooks/media_maker.md`*
