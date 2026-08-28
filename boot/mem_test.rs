use super::*;

#[test]
fn bump_allocates_then_exhausts() {
    let mut bump = FrameBump::new(0x1000, 2);
    assert_eq!(bump.capacity_pages(), 2);
    assert_eq!(bump.alloc_frame().unwrap().0, 0x1000);
    assert_eq!(bump.alloc_frame().unwrap().0, 0x2000);
    assert!(bump.alloc_frame().is_none());
}

#[test]
fn take_remaining_drains_pool() {
    let mut bump = FrameBump::new(0x1000, 4);
    assert_eq!(bump.alloc_frame().unwrap().0, 0x1000);
    let (start, pages) = bump.take_remaining().unwrap();
    assert_eq!(start, 0x2000);
    assert_eq!(pages, 3);
    assert!(bump.take_remaining().is_none());
    assert!(bump.alloc_frame().is_none());
}

#[test]
fn pick_skips_low_memory() {
    let regions = [(0x0, 100u64), (0x200000, 16u64)];
    let (start, pages) = pick_conventional_region(&regions, 8).unwrap();
    assert!(start >= 1024 * 1024);
    assert_eq!(start, 0x200000);
    assert_eq!(pages, 16);
}

#[test]
fn pick_requires_min_pages() {
    let regions = [(0x200000, 2u64)];
    assert!(pick_conventional_region(&regions, 8).is_none());
}

#[test]
fn pick_prefer_clips_to_precise_window() {
    // Simulate R640: tiny low hole + huge high DRAM (would win legacy pick).
    let regions = [
        (0x100000u64, 0x1000u64),        // 16 MiB at 1 MiB
        (0x140110000u64, 16_000_000u64), // ~61 GiB high
    ];
    let prefer = 512 * 1024 * 1024;
    let (start, pages) =
        pick_conventional_region_prefer(&regions, 16, prefer).expect("pref pool");
    assert_eq!(start, 0x100000);
    let end = start + pages * PAGE_SIZE;
    assert!(end <= prefer, "pool must stay inside precise EPT window");
    // Legacy (prefer_end=0) still picks the huge high span.
    let (hi_start, _) = pick_conventional_region(&regions, 16).unwrap();
    assert_eq!(hi_start, 0x140110000);
}

#[test]
fn pick_prefer_guest_ram_leaves_bar_window() {
    // Conventional covers [16MiB, 512MiB) like the R640 clipped pool case.
    let start = 16 * 1024 * 1024u64;
    let end = 512 * 1024 * 1024u64;
    let pages = (end - start) / PAGE_SIZE;
    let regions = [(start, pages)];
    let guest_ram = 256 * 1024 * 1024u64;
    let (p0, p_pages) =
        pick_conventional_region_prefer(&regions, 256, guest_ram).expect("guest-ram pool");
    assert_eq!(p0, start);
    assert!(p0 + p_pages * PAGE_SIZE <= guest_ram);
    // BAR/shell window [256MiB, 512MiB) must remain outside the pool.
    assert!(p0 + p_pages * PAGE_SIZE <= 256 * 1024 * 1024);
}

#[test]
fn pick_prefer_precise_allows_512mib_when_asked() {
    let start = 16 * 1024 * 1024u64;
    let end = 512 * 1024 * 1024u64;
    let pages = (end - start) / PAGE_SIZE;
    let regions = [(start, pages)];
    let prefer = 512 * 1024 * 1024u64;
    let (p0, p_pages) =
        pick_conventional_region_prefer(&regions, 256, prefer).expect("precise pool");
    assert_eq!(p0, start);
    assert_eq!(p0 + p_pages * PAGE_SIZE, prefer);
}

#[test]
fn pick_above_takes_dram_past_precise() {
    let regions = [
        (0x100000u64, 0x1F000u64),        // [1MiB, 512MiB)
        (0x140110000u64, 16_000_000u64), // ~61 GiB high
    ];
    let (start, pages) =
        pick_conventional_region_above(&regions, 512, 512 * 1024 * 1024).expect("high dram");
    assert_eq!(start, 0x140110000);
    assert_eq!(pages, 16_000_000);
    assert!(pick_conventional_region_above(&regions, 512, 0x140110000u64 + 16_000_000 * PAGE_SIZE)
        .is_none());
    assert_eq!(
        conventional_pages_above(&regions, 512 * 1024 * 1024),
        16_000_000
    );
}

#[test]
fn pick_above_clips_spanning_region_at_precise() {
    let start = 0x100000u64;
    let end = 4 * 1024 * 1024 * 1024u64;
    let pages = (end - start) / PAGE_SIZE;
    let regions = [(start, pages)];
    let floor = 512 * 1024 * 1024u64;
    let (s, p) = pick_conventional_region_above(&regions, 512, floor).expect("span");
    assert_eq!(s, floor);
    assert_eq!(s + p * PAGE_SIZE, end);
    assert_eq!(conventional_pages_above(&regions, floor), p);
}
