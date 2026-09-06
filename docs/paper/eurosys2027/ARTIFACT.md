# Optional artifact (reviewers may ignore)

EuroSys allows a supplementary file. The paper must stand alone. Do **not**
upload the full git history.

## Minimum artifact (recommended)

A zip that reproduces the host-only L3 crate under the frozen pin:

```
isolo-ept-artifact/
  README.txt              # how to verify; no identity
  verus-version.toml      # pin only (already has no author)
  ept_model/              # crate as in tree
  Cargo.toml              # workspace stub that builds ept_model
```

Command of record (once Verus is installed at that pin):

```
cargo verus verify -p ept_model
```

Expected: `80 verified, 0 errors`.

## README.txt contents (anonymous)

```
Isolon ept_model artifact (EuroSys 2027 fall, anonymous).

This crate is host-only. It is not linked into the UEFI binary.
Install Verus release/0.2026.07.12.0b42f4c (see verus-version.toml
sha256_linux). Then:

  cargo verus verify -p ept_model

Expected last line: 80 verified, 0 errors.
Live executable EPT is not in this zip and is not claimed as L3.
```

## Do not include

- `.git/`
- `site/`, `docs/hda.md`, living paper with author block
- iDRAC passwords, COM2 logs that print `RAYNU-V-*` if you can avoid them
- `CLAUDE.md` / ADR files that name the product

Iron serial logs are optional and deanonymizing; leave them out of the
zip. The paper already summarizes them.

## Building the zip (operator)

From a clean clone, something like:

```bash
mkdir -p /tmp/isolo-ept-artifact
cp -a ept_model /tmp/isolo-ept-artifact/
cp verus-version.toml /tmp/isolo-ept-artifact/
# write anonymous README + stub Cargo.toml by hand
# strip RAYNU-V from ept_model comments if you have time
(cd /tmp && zip -r isolo-ept-artifact.zip isolo-ept-artifact)
```

Upload as HotCRP supplementary material, not as the 12-page PDF.
