# `main` merge order and named releases

Standing rule for landing the lived hypervisor on `main` and cutting kits
under `releases/`. Tracker numbers stay in [`hda.md`](hda.md). Product loop:
[`m7_plan.md`](m7_plan.md) · ADR-009.

Do **not** merge this document’s “do not merge” PRs. Do **not** print
`RAYNU-V-M7-ISO-INSTALL-OK` from host/CI. Latitude/QEMU ≠ R640.

---

## Why `main` is behind

Lived E5 is not on `main`. It is a PR **stack** that grew on iron branches.
`main` currently holds Latitude/QEMU M6 + site chrome + older E4/E5 stages.
The iron pin is `cursor/pit-during-apk-b7a8` (E5 Phase A, EFI `56a3ffd`,
Actions `34480107961`).

Walk the stack **or** open one release PR of the pin onto `main`. Do not
merge intermediate forks as standing product.

### Land in this order

| Step | What | Branch / PR | Onto | Ships when |
|------|--------|-------------|------|------------|
| 1 | **E5 Phase A** — ISO → RayNu-F installer → virtio-blk → reboot-to-disk | `cursor/pit-during-apk-b7a8` (this PR if retargeted) or `cursor/e5-phase-a-main-facd` | **`main`** | Merge + kit `v0.1.0-e5-phase-a` |
| 2 | **Phase B host wire** — SPA/REST product-ISO start queues RayNu-F | [#239](https://github.com/vikkp/RayNu/pull/239) `cursor/spa-raynu-f-start-facd` | the Phase A pin, then `main` | After step 1. Host tests only until COM2. |
| 3 | **Phase B iron** — SPA Start launches RayNu-F on COM2 | flash `9061ffca` / run `34527624141`, **no** `--raynu-f` | — | COM2 `E4 SPA start — RayNu-F product ISO`. Then kit `v0.1.0-e5-phase-b`. |
| 4 | **Mount Everest** | — | `main` | Phase B iron **and** ADR-009 UI bar. Do not tag Everest at step 1–3. |

The historical stack under #238 (`raynu-f-direct-iron` ← `iron-f7-4g-cruzer`
← Stage 46 ← P0-60 → El Torito) is already contained in the Phase A pin.
Prefer **one PR of the pin onto `main`** over merging each layer.

### Do not merge

| PR / pin | Why |
|---------|-----|
| #231 IdeBus / SCI | Parked (ADR-015). OVMF forcing family closed. |
| `975f8fc` / run `34474850361` | F7 reached GRUB; exit-cap ended it. Not a standing flash. |
| `59ac070` / run `34425781629` | Install closed; F7 VMCLEAR/VMPTRLD. Not a standing flash. |
| `2b795a0` as “latest” | E4 SHELL stub. Phase A pin is `56a3ffd`. |
| Site-only drafts that predate main chrome | #133 / stale HDA first-light. Chrome lives on `main`. |
| Superseded OVMF-forcing pins | ADR-016: do not mutate OVMF internals. |

Open PRs that already target `main` and are **not** this pin (#227 El Torito,
#217 ATAPI, #215 BOTH, #171 E4 quiet, #234 paper) are older slices. Close or
supersede them when the Phase A pin lands — do not merge them *after* the pin
or you rewind firmware.

---

## Named releases

Artifact path: `tools/package-release.sh` → `dist/raynu-v-<Cargo.toml version>/`
then copy into `releases/<tag>/` with a README (see `v0.1.0-e4-spa-launch`).

Iron pin = CI SHA matching COM2 `build: sha=`. Mac-built floppies may differ
by toolchain from the Linux CI EFI in the kit.

| Tag | Pin | Marker / evidence | Status |
|-----|-----|-------------------|--------|
| `v0.1.0-e4-spa-launch` | `2b795a0` / #169 | `RAYNU-V-M7-E4-SPA-LAUNCH-OK` | Already on `main` |
| **`v0.1.0-e5-phase-a`** | `56a3ffd` / `34480107961` | `ISO-INSTALL-OK` + `DISK-BOOT-OK` + second Linux `root=UUID=` + `login:` | Kit in this change. Land with the pin. |
| `v0.1.0-e5-phase-b` | `9061ffca` / `34527624141` (or later green) | COM2 SPA start → RayNu-F, preferably **without** `raynuf.txt` | Cut **after** iron. Host wire is #239, not this close. |
| `v0.1.0-everest` | — | E1–E6 per ADR-009 | Not yet. |

Preserve rollback kits: `releases/v0.1.0-adr013-baseline`,
`releases/v0.1.0-xsavesfix`.

Cut a kit:

```bash
# From a clone at the pin SHA, after CI has the EFI:
gh run download 34480107961 -n r640-hypervisor.efi -D /tmp/kit
# or: SKIP_BUILD=1 ./tools/package-release.sh   # local UEFI build
# Copy EFI + sidecars into releases/v0.1.0-e5-phase-a/ (see that README).
```

Do not GitHub-release “Mount Everest” until step 4.

---

## Website chrome (stop the format flip)

Cause: `pages.yml` publishes the whole `site/` tree from `main`. Iron
branches often carry a **stripped** `site/` (no assets, old `styles.css` /
`index.html`). HDA updates also rewrite `site/hda.html` notes. A conflict
resolved toward the iron file restores the old layout.

Rules:

1. `./tools/sync-hda-site.sh` writes **only** `site/hda.json`. Never CSS.
2. Before any PR whose base is `main`, run:
   ```bash
   ./tools/preserve-site-chrome.sh --restore --from origin/main
   ./tools/preserve-site-chrome.sh --check --from origin/main
   ```
   Then re-apply summit **notes** on `site/hda.html` (nav, og:image, favicons
   stay from `main`). Then `./tools/sync-hda-site.sh`.
3. Site-design PRs (#233 Worker lock, stories) merge to `main` **first**.
   Iron branches rebase or restore chrome; they do not commit a private
   `styles.css`.
4. One publish path: Cloudflare Worker **or** GitHub Pages, not both
   fighting. Keep #233 as the chrome lock.

`hda.html` summit copy is allowed to change with HDA. `styles.css`,
`index.html`, `assets/`, `404.html`, `stories.html` layout are not.

---

## Operator merge checklist (Phase A → `main`)

1. PR base = `main`, head = Phase A pin (+ this hygiene commit).
2. `./tools/preserve-site-chrome.sh --check --from origin/main`
3. `./tools/sync-hda-site.sh --check`
4. `cargo test --no-default-features -- --test-threads=1`
5. Merge **merge commit** (not squash of 500 iron SHAs if you want COM2
   `build: sha=` to stay a reachable git object — squash is OK only if the
   kit README still names `56a3ffd` / `34480107961`).
6. Do not enable auto-merge. Do not merge #231 / superseded F11 pins.
7. After merge: `git tag -a v0.1.0-e5-phase-a <merge or pin>`.
8. Then merge #239 (Phase B host wire). Flash Phase B separately; do not
   wait on `main` to F11.
