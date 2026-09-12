Isolon supplementary artifact (EuroSys 2027 fall, anonymous)
Submission id 394

WHAT THIS IS
------------
Two things a reviewer can check without taking our word for them:

  1. ept_model/  -- the host-only Verus crate that carries the L3
     exclusivity theorems of the paper. Replay the 80/0 line yourself.
  2. com2/       -- iDRAC SOL serial captures behind the PowerEdge R640
     column of the split evaluation table.

The crate is host-only. It is NOT linked into the UEFI binary, and this
zip contains no hypervisor source. That separation is the paper's point,
not an omission: L3 names the ghost model, the shipped binary is L1/L2.

REPLAYING THE PROOF
-------------------
  ./reproduce.sh

The script downloads only the exact Verus release tag named in
verus-version.toml, checks its sha256 against the pin, and runs:

  cargo verus verify -p ept_model

Expected last line:  80 verified, 0 errors

That count includes the N-guest CI alias. There is no admit() on the
exclusivity path. Requires Linux x86-64, curl, unzip, and the Rust
toolchain named in verus-version.toml.

If you would rather install Verus yourself, the pin is:

  tag     release/0.2026.07.12.0b42f4c
  commit  0b42f4cee92a178937608cf55e512371d2fd8cd4
  asset   verus-0.2026.07.12.0b42f4c-x86-linux.zip
  sha256  f6f4f5d08e07d3e1ad721d775bda5ba96b9dd0c73b48fc17f2e071866fbd01c0

WHAT THE THEOREM SAYS, AND DOES NOT
-----------------------------------
The proved property is exclusivity among guests over two ghost maps:
a frame in `owned` has exactly one guest as EPT owner, and no second
guest holds an EPT mapping to it. Guest id 0 is reserved; hypervisor
frames are modelled as absent from `owned`, which is a definition, not
a lemma.

Not proved here, and not claimed anywhere in the paper:
  - The live executable EPT engine. It is not in this zip and is not L3.
  - The range registry that covers guest-physical space at Linux boot.
    It has no correspondence in this model. That disjointness is the
    subject of the paper, not a defect discovered after the fact.
  - Any statement about the EFI's own page tables.
  - DMA or IOMMU confinement.
Hypervisor access to virtio rings from VMX root is not a second EPT
owner; see the threat-model section.

SERIAL CAPTURES
---------------
com2/00-INDEX.txt maps each file to the table rows it supports.
com2/10-iso-loop-install-disk-boot-login.txt is the dedicated ISO-loop
transcript (install-complete, disk-boot, root=UUID=, login) from the
R640 close; host and CI builds never print that install-complete
marker. com2/11-eltorito-cd-efi.txt is the El Torito CD EFI marker
excerpt (not a distro installer).

REDACTION
---------
The captures and the crate were passed through one mechanical rename
pass before packaging. It rewrites identifiers only: product and
project names, in-tree document identifiers, MAC addresses, lab IPv4
addresses, build-host prompts, and commit or CI job identifiers. Serial
line order, timing, register values, addresses, marker sequence, and
outcomes are unmodified. Milestone markers keep their structure with
the product prefix replaced (X-M4-SMP-OK and so on), so a reader can
still follow the boot sequence. Nothing was removed to make a run look
better than it was; the failures are in here on purpose.

MANIFEST.txt lists a sha256 for every file in this zip.
