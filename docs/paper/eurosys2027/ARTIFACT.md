# Optional artifact (reviewers may ignore)

EuroSys allows a supplementary file. The paper must stand alone.

## Minimum artifact

```
isolo-ept-artifact/
  README.txt              # how to verify; no identity
  verus-version.toml      # pin + sha256_linux
  ept_model/              # crate as in tree
  Cargo.toml              # workspace stub that builds ept_model
  reproduce.sh           # optional: docker/podman one-shot
  com2/                   # anonymized serial excerpts (optional)
```

Command of record (Verus installed at that pin):

```
cargo verus verify -p ept_model
```

Expected: `80 verified, 0 errors` (includes the N-guest CI alias).

Pin: `release/0.2026.07.12.0b42f4c`  
Linux zip SHA-256: `f6f4f5d08e07d3e1ad721d775bda5ba96b9dd0c73b48fc17f2e071866fbd01c0`

## README.txt contents (anonymous)

```
Isolon ept_model artifact (EuroSys 2027 fall, anonymous).

This crate is host-only. It is not linked into the UEFI binary.
Install Verus release/0.2026.07.12.0b42f4c using sha256_linux
from verus-version.toml. Then:

  cargo verus verify -p ept_model

Expected last line: 80 verified, 0 errors.
The theorem is guest-exclusivity of two ghost maps.
Live executable EPT is not in this zip and is not L3.
Hypervisor access to virtio rings is not a second EPT owner.
```

## COM2 excerpts

If included: strip product markers, hostnames, iDRAC addresses, and
operator names. Keep enough to support each R640 row of the split table
(VMXON, Linux shell, virtio-blk, virtio-net, SMP, four-VM probes, HTTP,
El Torito). Prefer excerpts over full SOL pastes.

## Do not include

- `.git/`
- `site/`, `docs/hda.md`, living paper with author block
- iDRAC passwords, unique service tags
- `CLAUDE.md` / ADR files that name the product

## Building the zip (operator)

```bash
mkdir -p /tmp/isolo-ept-artifact
cp -a ept_model /tmp/isolo-ept-artifact/
cp verus-version.toml /tmp/isolo-ept-artifact/
# write anonymous README + stub Cargo.toml by hand
(cd /tmp && zip -r isolo-ept-artifact.zip isolo-ept-artifact)
```

Upload as HotCRP supplementary material, not as the 12-page PDF.
