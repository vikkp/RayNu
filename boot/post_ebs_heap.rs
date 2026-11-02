//! Post-EBS heap that returns memory on `dealloc`.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! `rsa` and `p256` allocate during a TLS handshake and drop those
//! temporaries when the flight finishes. The old bump ignored `dealloc`,
//! so every handshake permanently took ~73 KiB (measured on the host).
//! A watermark rewind would also free the `EphemeralSecret` the session
//! still holds. This heap frees the block `Drop` names and coalesces
//! neighbors, so the next handshake reuses the same bytes.
//!
//! BSP-only. Not the Proven Core allocator.

use core::alloc::Layout;
use core::ptr;

const MAGIC: u32 = 0x4E55_4845;
const HDR: usize = 16;
const FTR: usize = 8;
const MIN_BLOCK: usize = 32;

#[repr(C)]
struct Hdr {
    magic: u32,
    free: u32,
    size: u64,
}

/// First-fit heap over one caller-owned buffer. `base` stays 16-aligned.
pub struct Heap {
    base: *mut u8,
    len: usize,
    bump: usize,
    free_head: *mut u8,
    live: usize,
    high: usize,
}

impl Heap {
    /// # Safety
    /// `base` is 16-aligned, exclusive to this heap, and valid for `len`
    /// bytes for the heap's lifetime. `len` may be shortened to a 16-byte
    /// boundary.
    pub unsafe fn new(base: *mut u8, len: usize) -> Self {
        Self {
            base,
            len: len & !15,
            bump: 0,
            free_head: ptr::null_mut(),
            live: 0,
            high: 0,
        }
    }

    pub fn live(&self) -> usize {
        self.live
    }

    pub fn high(&self) -> usize {
        self.high
    }

    /// Bytes the bump has walked. Shrinks when a free reaches the tail.
    pub fn reserved(&self) -> usize {
        self.bump
    }

    pub fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(1);
        let align = layout.align().max(1);
        if align > 4096 || size > self.len {
            return ptr::null_mut();
        }
        let need = match block_need(size, align) {
            Some(n) => n,
            None => return ptr::null_mut(),
        };
        let (block, used) = match self.take_free(need) {
            Some(b) => b,
            None => match self.take_bump(need) {
                Some(b) => b,
                None => return ptr::null_mut(),
            },
        };
        // SAFETY: `block` is a block this heap just reserved, 16-aligned,
        // and `used` bytes sit inside the buffer.
        unsafe { self.claim(block, used, align) }
    }

    /// A pointer this heap did not hand out is ignored.
    pub fn dealloc(&mut self, ptr: *mut u8) {
        if ptr.is_null() {
            return;
        }
        let Some(block) = self.block_of(ptr) else {
            return;
        };
        // SAFETY: `block_of` only returns a block whose header magic matches.
        unsafe {
            if (*hdr(block)).free != 0 {
                return;
            }
            let orig = (*hdr(block)).size as usize;
            let mut size = orig;
            let mut block = block;
            if let Some((prev, prev_size)) = self.free_prev(block) {
                self.unlink(prev);
                block = prev;
                size += prev_size;
            }
            if let Some(next_size) = self.free_next(block, size) {
                let next = block.add(size);
                self.unlink(next);
                size += next_size;
            }
            self.live = self.live.saturating_sub(orig);
            self.release(block, size);
        }
    }

    fn take_free(&mut self, need: usize) -> Option<(*mut u8, usize)> {
        let mut prev: *mut u8 = ptr::null_mut();
        let mut cur = self.free_head;
        while !cur.is_null() {
            // SAFETY: free-list nodes are blocks this heap linked, inside
            // the buffer, and the next pointer sits at payload offset 0.
            let next = unsafe { read_next(cur) };
            let have = unsafe { (*hdr(cur)).size as usize };
            if have >= need {
                if prev.is_null() {
                    self.free_head = next;
                } else {
                    unsafe { write_next(prev, next) };
                }
                let used = unsafe { self.split(cur, have, need) };
                return Some((cur, used));
            }
            prev = cur;
            cur = next;
        }
        None
    }

    fn take_bump(&mut self, need: usize) -> Option<(*mut u8, usize)> {
        let next = self.bump.checked_add(need)?;
        if next > self.len {
            return None;
        }
        let block = unsafe { self.base.add(self.bump) };
        self.bump = next;
        Some((block, need))
    }

    /// # Safety
    /// `block` is 16-aligned, inside the buffer, and `have >= need`.
    /// `need` and `have - need` are multiples of 16. A remainder smaller
    /// than [`MIN_BLOCK`] stays with the returned block so the next block
    /// cannot overlap it.
    unsafe fn split(&mut self, block: *mut u8, have: usize, need: usize) -> usize {
        if have - need >= MIN_BLOCK {
            let rest = block.add(need);
            let rest_size = have - need;
            write_block(rest, rest_size, true);
            write_next(rest, self.free_head);
            self.free_head = rest;
            need
        } else {
            have
        }
    }

    /// # Safety
    /// `block` is a reserved block of `used` bytes inside the buffer.
    unsafe fn claim(&mut self, block: *mut u8, used: usize, align: usize) -> *mut u8 {
        write_block(block, used, false);
        self.live = self.live.saturating_add(used);
        if self.live > self.high {
            self.high = self.live;
        }
        if align <= 16 {
            return block.add(HDR);
        }
        let user = align_up(block as usize + HDR + 8, align) as *mut u8;
        (user.sub(8) as *mut usize).write(block as usize);
        user
    }

    fn block_of(&self, user: *mut u8) -> Option<*mut u8> {
        if user.is_null() {
            return None;
        }
        // SAFETY: either the header sits immediately before `user` (the
        // 16-aligned case) or the word before `user` is the block address
        // written by `claim`. A bad pointer fails the magic check.
        unsafe {
            let direct = user.sub(HDR);
            if self.header_ok(direct) {
                return Some(direct);
            }
            if (user as usize) < 8 {
                return None;
            }
            let block = (user.sub(8) as *const usize).read() as *mut u8;
            if self.header_ok(block) {
                Some(block)
            } else {
                None
            }
        }
    }

    fn header_ok(&self, block: *mut u8) -> bool {
        if block.is_null() || !self.in_range(block) {
            return false;
        }
        // SAFETY: `block` is inside the buffer. The header is 16 bytes.
        unsafe {
            let h = &*hdr(block);
            h.magic == MAGIC && h.size >= MIN_BLOCK as u64 && (h.size as usize) % 16 == 0
        }
    }

    fn in_range(&self, block: *mut u8) -> bool {
        let addr = block as usize;
        let base = self.base as usize;
        addr >= base && addr.saturating_sub(base) < self.bump && (addr - base) % 16 == 0
    }

    /// # Safety
    /// `block` is a live block inside the buffer.
    unsafe fn free_prev(&self, block: *mut u8) -> Option<(*mut u8, usize)> {
        if block == self.base {
            return None;
        }
        let prev_size = (block.sub(FTR) as *const u64).read() as usize;
        if prev_size < MIN_BLOCK || prev_size % 16 != 0 || (block as usize) < prev_size {
            return None;
        }
        let prev = block.sub(prev_size);
        if !self.header_ok(prev) || (*hdr(prev)).free == 0 {
            return None;
        }
        if (*hdr(prev)).size as usize != prev_size {
            return None;
        }
        Some((prev, prev_size))
    }

    /// # Safety
    /// `block` is a live block of `size` bytes.
    unsafe fn free_next(&self, block: *mut u8, size: usize) -> Option<usize> {
        let next = block.add(size);
        let end = self.base.add(self.bump);
        if next >= end || !self.header_ok(next) || (*hdr(next)).free == 0 {
            return None;
        }
        let next_size = (*hdr(next)).size as usize;
        if next.add(next_size) > end {
            return None;
        }
        Some(next_size)
    }

    fn unlink(&mut self, block: *mut u8) {
        let mut prev: *mut u8 = ptr::null_mut();
        let mut cur = self.free_head;
        while !cur.is_null() {
            if cur == block {
                // SAFETY: `cur` is on the free list, so its next word is set.
                let next = unsafe { read_next(cur) };
                if prev.is_null() {
                    self.free_head = next;
                } else {
                    unsafe { write_next(prev, next) };
                }
                return;
            }
            // SAFETY: same as above.
            let next = unsafe { read_next(cur) };
            prev = cur;
            cur = next;
        }
    }

    /// # Safety
    /// `block` is inside the buffer and `size` bytes do not overlap a live block.
    unsafe fn release(&mut self, block: *mut u8, size: usize) {
        if block.add(size) == self.base.add(self.bump) {
            self.bump = (block as usize) - (self.base as usize);
            return;
        }
        write_block(block, size, true);
        write_next(block, self.free_head);
        self.free_head = block;
    }
}

fn block_need(size: usize, align: usize) -> Option<usize> {
    let raw = if align <= 16 {
        HDR.checked_add(size)?.checked_add(FTR)?
    } else {
        HDR.checked_add(8)?
            .checked_add(align)?
            .checked_add(size)?
            .checked_add(FTR)?
    };
    let rounded = align_up(raw, 16);
    if rounded < MIN_BLOCK {
        Some(MIN_BLOCK)
    } else {
        Some(rounded)
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    let mask = align - 1;
    (addr + mask) & !mask
}

/// # Safety
/// `block` is 16-aligned and writable for [`HDR`] bytes.
unsafe fn hdr(block: *mut u8) -> *mut Hdr {
    block as *mut Hdr
}

/// # Safety
/// `block` is 16-aligned and writable for `size` bytes, `size >= MIN_BLOCK`.
unsafe fn write_block(block: *mut u8, size: usize, free: bool) {
    ptr::write(
        hdr(block),
        Hdr {
            magic: MAGIC,
            free: u32::from(free),
            size: size as u64,
        },
    );
    (block.add(size - FTR) as *mut u64).write(size as u64);
}

/// # Safety
/// `block` is a free block of at least [`MIN_BLOCK`] bytes.
unsafe fn read_next(block: *mut u8) -> *mut u8 {
    (block.add(HDR) as *const *mut u8).read()
}

/// # Safety
/// `block` is a free block of at least [`MIN_BLOCK`] bytes.
unsafe fn write_next(block: *mut u8, next: *mut u8) {
    (block.add(HDR) as *mut *mut u8).write(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(16))]
    struct Mem {
        bytes: [u8; 256 * 1024],
    }

    fn heap(mem: &mut Mem) -> Heap {
        // SAFETY: `mem.bytes` is 16-aligned and lives as long as `mem`.
        unsafe { Heap::new(mem.bytes.as_mut_ptr(), mem.bytes.len()) }
    }

    #[test]
    fn a_free_block_is_reused() {
        let mut mem = Mem { bytes: [0; 256 * 1024] };
        let mut h = heap(&mut mem);
        let layout = Layout::from_size_align(64 * 1024, 8).unwrap();
        let a = h.alloc(layout);
        assert!(!a.is_null());
        let reserved = h.reserved();
        assert!(h.live() > 0);
        h.dealloc(a);
        assert_eq!(h.live(), 0);
        assert_eq!(h.reserved(), 0, "tail free rewinds the bump");
        let b = h.alloc(layout);
        assert!(!b.is_null());
        assert!(h.reserved() <= reserved);
        h.dealloc(b);
        assert_eq!(h.live(), 0);
        assert!(h.high() > 0);
    }

    #[test]
    fn neighbors_coalesce_into_one_block() {
        let mut mem = Mem { bytes: [0; 256 * 1024] };
        let mut h = heap(&mut mem);
        let layout = Layout::from_size_align(1000, 8).unwrap();
        let a = h.alloc(layout);
        let b = h.alloc(layout);
        let c = h.alloc(layout);
        assert!(!a.is_null() && !b.is_null() && !c.is_null());
        h.dealloc(a);
        h.dealloc(c);
        h.dealloc(b);
        assert_eq!(h.live(), 0);
        assert_eq!(h.reserved(), 0);
        let big = h.alloc(Layout::from_size_align(2500, 8).unwrap());
        assert!(!big.is_null());
        h.dealloc(big);
        assert_eq!(h.live(), 0);
    }

    #[test]
    fn align_64_is_honored_and_freed() {
        let mut mem = Mem { bytes: [0; 256 * 1024] };
        let mut h = heap(&mut mem);
        let layout = Layout::from_size_align(24, 64).unwrap();
        let p = h.alloc(layout);
        assert!(!p.is_null());
        assert_eq!(p as usize % 64, 0);
        h.dealloc(p);
        assert_eq!(h.live(), 0);
        h.dealloc(p);
        assert_eq!(h.live(), 0, "a second free is ignored");
    }

    #[test]
    fn exhaustion_returns_null_until_a_free() {
        let mut mem = Mem { bytes: [0; 256 * 1024] };
        let mut h = heap(&mut mem);
        let layout = Layout::from_size_align(60 * 1024, 8).unwrap();
        let mut got = [ptr::null_mut(); 8];
        let mut n = 0;
        for slot in &mut got {
            let p = h.alloc(layout);
            if p.is_null() {
                break;
            }
            *slot = p;
            n += 1;
        }
        assert!(n >= 2);
        assert!(h.alloc(layout).is_null());
        h.dealloc(got[0]);
        let again = h.alloc(layout);
        assert!(!again.is_null());
        h.dealloc(again);
        for p in got.iter().skip(1).take(n - 1) {
            h.dealloc(*p);
        }
        assert_eq!(h.live(), 0);
    }
}
