# B21 WP8 — D17 guest memory review

ADR-024 D17: a value read from guest memory that decides a host-side bound, index, or address is read once into a local and validated there.

This file is the start of that review. It is not finished. WP8 is not ticked.

## Reviewed

`devices/guest_virtio_blk.rs` queue walk, `drain_queue` from the avail index.

| Read | Line | Local | Validated there? |
|---|---|---|---|
| avail idx | `read_u16` into `avail_idx` | 1996 | The walk stops by comparing `last_avail` to that local. The slot is `last_avail % qsize` (2003). `qsize` is the device queue size, not a fresh guest read. |
| avail ring entry | `head` | 2004 | Used as the first descriptor index. Not checked against `qsize` before the address is formed. |
| descriptor | one 16-byte read into `raw` | 2020–2028 | `addr`, `len`, `flags`, `next` are each taken once from that buffer. `len` is a guest `u32`. The data copy loops `while off < len` and caps each step at 64 bytes (`2062–2064`) but does not cap `len` itself. `next` is stored into `desc` (`2044`) with no `next < qsize` check. The chain stops when `chain > qsize` (`2016`). |
| sector | `hdr[8..16]` | 2048 | Passed into `xfer_data_seg` (`2083`). The sector is not range-checked in the walk. |

`xfer_data_seg` (`1935`) turns `len` into `n` and computes `sec` from `sector` plus the byte offset (`1952`). A later disk-status return can reject an out-of-range sector. That check is not at the read.

## Not reviewed

The rest of the exit path: virtio-net, the IDE/ATAPI path, EPT handlers, and RayNu-F service argument pointers. Not reviewed. WP8 stays open.
