# Anonymization (double-blind)

EuroSys 2027 is double-blind. Public non-archival work is allowed, but the
**submitted PDF** must not name you, and it must use a **different title
and system name** than the public living paper.

## PDF must not contain

| String | Why |
|--------|-----|
| `RayNu`, `RayNu-V`, `RayNu-F`, `raynuv.com` | product identity |
| `vikkp`, GitHub URLs, PR numbers | identity |
| `Pandey`, `ORCID`, `0009-0001-2160-6357` | identity |
| `RAYNU-V-*` gate markers | unique public strings |
| `Latitude 7490` as a unique laptop | prefer “lab host” |
| Acknowledgments naming you | use “Removed for double-blind review” |

## PDF may contain

- **Isolon** (submission system name)
- Dell PowerEdge R640 as a hardware class
- Verus pin `0.2026.07.12.0b42f4c` and Kani `0.67.0`
- Crate name `ept_model` (generic)
- Counts: 80 verified / 0 errors, LOC tables
- Honest gaps: live EPT L2, nested ≠ iron, installer open

## Title rule

| Venue | Title | System |
|-------|--------|--------|
| Public living paper | RayNu-V: A Formally Verified Bare-Metal Hypervisor | RayNu-V |
| This submission | Isolon: Keeping Isolation Claims Honest in a Bare-Metal Type-1 Hypervisor | Isolon |

Do not cite the living page as “our previous work.” If a reviewer googles
Isolon phrases and finds the site, that is allowed public TR overlap; the
PDF itself must still be a good-faith anonymous rewrite.

## Repos / artifacts

If you upload code: strip `.git`, author names, `raynuv` paths, and
`RAYNU-V` markers from comments where practical. Prefer a tarball of
`ept_model/` + `verus-version.toml` + a stub `Cargo.toml`. See
[`ARTIFACT.md`](ARTIFACT.md).

## Check before upload

```bash
cd docs/paper/eurosys2027/paper
make anonymity-check
pdftotext main.pdf - | grep -nE 'RayNu|vikkp|Pandey|ORCID|RAYNU-V|github.com'
```

That grep must be empty.
