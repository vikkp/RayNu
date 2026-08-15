# v0.1.0-invpcidfix — allow guest INVPCID (+ nogbpages)

`v0.1.0-gprsfix` reached real Linux earlyprintk (`RAYNU-V-M3-LINUX-EARLY-OK`)
then panic'd:

```text
PANIC: early exception 0x06 IP … Code: … 66 0f 38 82 04 24
```

`0x06` = `#UD`. Bytes `66 0f 38 82` = **INVPCID**. Secondary VM-execution
controls only had EPT+RDTSCP; without **Enable INVPCID** (bit 12) the guest
#UD's on Linux PCID TLB invalidation.

Also add `nogbpages` — log showed "Using GB pages for direct mapping" while
precise EPT is 512 MiB / 2 MiB leaves.

```bash
( cd releases/v0.1.0-invpcidfix && shasum -a 256 -c r640-hypervisor.efi.sha256 )
./tools/make-boot-media.sh --kit releases/v0.1.0-invpcidfix
```

Expect: past early exception 0x06 toward GTIMER2 / SHELL (not INVPCID panic).
