# B21 WP8 — D17 guest memory review

Plan section 5, WP8. ADR-024 D17: a value read from guest memory that decides a host-side bound, index, or address is read once into a local and validated there. Ring indices, descriptor fields, request headers, and page-table entries are read once, checked on the local, and used from the local. A descriptor chain has a bound. MMIO instruction bytes are copied once, at most 15 bytes, and decoded from the copy.

`devices/guest_virtio_console.rs` is not in the tree. There is no virtio-console device. The guest console is the virtual 16550.

## Fixes

| Site | What changed |
|---|---|
| `devices/guest_virtio_blk.rs` `process_blk_queue` (1984) | `next >= qsize` at 2056 stops the chain. `head >= qsize` does not walk. The sum of data-descriptor lengths is capped at `MAX_REQ_BYTES` (165: `QUEUE_MAX` × `VIRTIO_XFER_CHUNK`, 8 MiB) at 2045. Above that the request is `VIRTIO_BLK_S_IOERR` and the data buffer is not copied. Host tests `descriptor_next_past_qsize_is_ioerr` and `request_longer_than_max_req_bytes_is_ioerr`. |
| `devices/guest_virtio_net.rs` TX and RX walks | `next >= QSIZE` is not folded with `%`. `head >= QSIZE` is not used as an index. The walk is still bounded by `CHAIN_MAX` (32). A copied length is `min` of the descriptor length and the 1514-byte frame. |

## Sites

| Site | Line | Verdict |
|---|---|---|
| virtio-blk avail idx | `read_u16` into `avail_idx` | Once. The walk compares `last_avail` to that local. The slot is `last_avail % qsize`. |
| virtio-blk avail entry | `head` | Once. `head >= qsize` does not walk. |
| virtio-blk descriptor | one 16-byte read into `raw`, then `addr`, `len`, `flags`, `next` | Once each. `next >= qsize` rejects. Data sum capped. Chain stops when `chain > qsize`. |
| virtio-blk header | 16 bytes into `hdr`, then `ty` and `sector` | Once. The disk function returns a status for an out-of-range sector. |
| virtio-net avail idx | `read_unaligned` into `idx` | Once. Compared to `last_avail`. Slot is `last % QSIZE`. |
| virtio-net avail entry | `head` | Once. `head >= QSIZE` is not an index. |
| virtio-net descriptor | `addr`, `len`, `flags`, `next` each one `read_unaligned` | Once. `next >= QSIZE` stops the chain. `CHAIN_MAX` bounds it. Payload `take` is inside the frame buffer. |
| ATAPI `copy_product_iso_range` | `from_raw_parts` of the product ISO window, not guest RAM | The caller `fill_read_drq` caps the DRQ at `XFER_CAP` / `XFER_SEC` and rejects `end > m.len` before the copy. The CDB allocation length is not used as that byte count. |
| `guest_msix` | table lives in the device struct, written by MMIO | `fire` uses the stored address and vector. It does not read guest RAM. A vector below 32 or an address outside the MSI page is dropped. |
| `read_guest_pte` | one `read_unaligned` of 8 bytes, returned as a local | The walk uses that local. The index is `linear` shifted and masked to 9 bits. Present and page-size bits are checked on the local before the next address is formed. |
| `dump_low_ram_insn` | `copy_guest_linear_bytes` into a 16-byte local | Printed from the local. Not a bound for a later decision. |
| `copy_mmio_insn` / `decode_mmio_or_retry` | copy into a 16-byte local; `mmio_effective_len` and `virtio_mmio_retry_decode_len` keep the decoded length in 1..=15 | Decode reads the copy. |
| `raynu_f_read_backing` | host slice, length is `buf.len()` | CD offset is checked against `iso.len()` before the copy. The comment says the dispatcher already checked the offset against media geometry. No guest pointer is read twice. |

Payload copies that do not decide a bound are not listed again.
