//! ADR-021 B6. One extra VMCS while a second product guest is seated.
//!
//! The page tables map a supervisor page whose first byte is `HLT`.
//! When a 32 MiB slab is free, the same VMCS instead enters the RayNu-F
//! test app in that slab. That is firmware, not the seated operating system.
//! Entering that VMCS and coming back does not mean the seated guest ran.
//! [`crate::mgmt::product_pair`] stays at zero for the companion until that
//! guest itself is entered. A failed entry is permanent. An exit that stole
//! the instruction (external interrupt, interrupt window, preemption) can
//! be tried again, up to [`MAX_TRIES`].
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002 / ADR-021).
//! VERIFICATION: L1 (host tests). The VM-entry instruction is uefi-only.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};

use crate::mgmt::megaraid::PERC_IMAGE_BYTES;
use crate::mgmt::product_seats::Seat;

/// Guest-physical page that holds `HLT`.
pub const CODE_GPA: u64 = 0x1000;
pub const PML4_GPA: u64 = 0x2000;
pub const PDPT_GPA: u64 = 0x3000;
pub const PD_GPA: u64 = 0x4000;
pub const PT_GPA: u64 = 0x5000;
pub const STACK_GPA: u64 = 0x6000;
/// First byte of the seated disk's sector, in the guest's own RAM.
pub const SECTOR_GPA: u64 = 0x7000;
/// Private RAM for the seated guest. Not the running guest's slab.
pub const SEAT_RAM_BYTES: u64 = 0x8000;
/// RayNu-F slab for a seated firmware entry. Same size as the product guest.
pub const SEAT_FW_BYTES: u64 = crate::memory::ept_hw::GUEST_UEFI_LOW_RAM_BYTES;
/// Firmware exits in one turn before the running guest is resumed.
pub const SEAT_FW_EXIT_CAP: u32 = 2048;
/// Turns are not capped. Each one still yields after [`SEAT_FW_EXIT_CAP`] exits.
/// Guest code that calls `ReadBlocks` for LBA 0, then halts.
pub const SEAT_DISK_STUB_GPA: u64 = 0x00B0_0000;
/// Buffer that call fills. One sector.
pub const SEAT_DISK_BUF_GPA: u64 = 0x00B0_1000;
/// File bytes of the seated bootloader, then the loaded PE image.
pub const SEAT_LOADER_FILE_GPA: u64 = 0x0180_0000;
pub const SEAT_LOADER_IMAGE_GPA: u64 = 0x0100_0000;
pub const SEAT_LOADER_MAX_FILE: u64 = 8 * 1024 * 1024;
pub const HLT_OPCODE: u8 = 0xF4;
/// `mov al, imm8; cmp al, imm8; hlt`. The halt is this far into the page.
pub const DISK_HLT_OFF: u64 = 4;
/// `mov eax, sector; movzx eax, [rax]; cmp al, imm8; hlt`.
pub const SEAT_HLT_OFF: u64 = 10;

/// Present, writable, accessed. Supervisor. No NX.
const GUEST_LINK: u64 = 0x23;
/// Present, writable, accessed, dirty. Supervisor. No NX.
const GUEST_LEAF: u64 = 0x63;

const IDLE: u8 = 0;
const IN_FLIGHT: u8 = 1;
const RETURNED: u8 = 2;
const FAILED: u8 = 3;
const PARKED: u8 = 4;

/// Attempts before a stolen exit is left alone.
pub const MAX_TRIES: u8 = 4;

static PROBE: AtomicU8 = AtomicU8::new(IDLE);
static TRIES: AtomicU8 = AtomicU8::new(0);
static PLANTED: AtomicBool = AtomicBool::new(false);
static PLANT_OFF: AtomicU64 = AtomicU64::new(0);
static SEAT_HPA: AtomicU64 = AtomicU64::new(0);
static FW_ON: AtomicBool = AtomicBool::new(false);
static FW_HPA: AtomicU64 = AtomicU64::new(0);
static FW_EXITS: AtomicU32 = AtomicU32::new(0);
static SLICES: AtomicU32 = AtomicU32::new(0);
static READ_ON: AtomicBool = AtomicBool::new(false);
static READ_BASE: AtomicU64 = AtomicU64::new(0);
static READ_BYTES: AtomicU64 = AtomicU64::new(0);
static LOADER: AtomicBool = AtomicBool::new(false);
static KERNEL: AtomicBool = AtomicBool::new(false);

pub fn clear_for_test() {
    PROBE.store(IDLE, Ordering::Release);
    TRIES.store(0, Ordering::Release);
    PLANTED.store(false, Ordering::Release);
    PLANT_OFF.store(0, Ordering::Release);
    SEAT_HPA.store(0, Ordering::Release);
    FW_ON.store(false, Ordering::Release);
    FW_HPA.store(0, Ordering::Release);
    FW_EXITS.store(0, Ordering::Release);
    SLICES.store(0, Ordering::Release);
    READ_ON.store(false, Ordering::Release);
    READ_BASE.store(0, Ordering::Release);
    READ_BYTES.store(0, Ordering::Release);
    LOADER.store(false, Ordering::Release);
    KERNEL.store(false, Ordering::Release);
}

pub fn probe_in_flight() -> bool {
    PROBE.load(Ordering::Acquire) == IN_FLIGHT
}

/// An entry that left the CPU without parking. The next slice may start again.
/// A parked seat is left parked.
pub fn unstick_in_flight_probe() {
    if PROBE
        .compare_exchange(IN_FLIGHT, IDLE, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        FW_ON.store(false, Ordering::Release);
    }
}

pub fn probe_returned() -> bool {
    PROBE.load(Ordering::Acquire) == RETURNED
}

/// Start the one entry. False when one is in flight, already returned, failed,
/// or the retry budget is spent.
pub fn begin_probe() -> bool {
    if PROBE.load(Ordering::Acquire) != IDLE {
        return false;
    }
    if TRIES.load(Ordering::Acquire) >= MAX_TRIES {
        return false;
    }
    if PROBE
        .compare_exchange(IDLE, IN_FLIGHT, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    TRIES.fetch_add(1, Ordering::AcqRel);
    true
}

pub fn note_probe_hlt() {
    let _ = PROBE.compare_exchange(IN_FLIGHT, RETURNED, Ordering::AcqRel, Ordering::Acquire);
}

/// External interrupt, interrupt-window, or preemption stole the `HLT`.
pub fn note_probe_retry() {
    let _ = PROBE.compare_exchange(IN_FLIGHT, IDLE, Ordering::AcqRel, Ordering::Acquire);
}

pub fn note_probe_failed() {
    PROBE.store(FAILED, Ordering::Release);
}

pub fn probe_exit_is_retry(basic: u32) -> bool {
    matches!(basic, 1 | 7 | 52)
}

/// HLT must exit. An interrupt-window or NMI-window exit would steal the instruction.
/// One seated disk. `spare_base == 0` is the 8 GiB window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeatDisk {
    pub spare_base: u64,
    pub disk_bytes: u64,
}

pub fn seat_disk(seat: Seat) -> SeatDisk {
    match seat {
        Seat::Window => SeatDisk {
            spare_base: 0,
            disk_bytes: PERC_IMAGE_BYTES,
        },
        Seat::Tail { off, bytes } => SeatDisk {
            spare_base: off,
            disk_bytes: bytes,
        },
    }
}

pub fn disks_differ(running: SeatDisk, parked: SeatDisk) -> bool {
    running.spare_base != parked.spare_base
}

/// `mov al, imm8; cmp al, imm8; hlt` so the halt exit only happens after
/// the seated disk's byte was the immediate.
pub fn plant_disk_byte(code: &mut [u8; 4096], byte: u8) {
    code[0] = 0xB0;
    code[1] = byte;
    code[2] = 0x3C;
    code[3] = byte;
    code[4] = HLT_OPCODE;
}

pub fn note_disk_plant(spare_base: u64) {
    PLANT_OFF.store(spare_base, Ordering::Release);
    PLANTED.store(true, Ordering::Release);
}

pub fn take_disk_plant() -> Option<u64> {
    if PLANTED.swap(false, Ordering::AcqRel) {
        Some(PLANT_OFF.load(Ordering::Acquire))
    } else {
        None
    }
}

/// Load the seated sector from the guest's own RAM, then halt.
pub fn plant_seat_program(code: &mut [u8; 4096], sector_gpa: u32, byte: u8) {
    code[..16].fill(0);
    code[0] = 0xB8;
    code[1..5].copy_from_slice(&sector_gpa.to_le_bytes());
    code[5] = 0x0F;
    code[6] = 0xB6;
    code[7] = 0x00;
    code[8] = 0x3C;
    code[9] = byte;
    code[10] = HLT_OPCODE;
}

pub fn note_seat_ram(hpa: u64) {
    SEAT_HPA.store(hpa, Ordering::Release);
}

pub fn take_seat_ram() -> Option<u64> {
    let hpa = SEAT_HPA.swap(0, Ordering::AcqRel);
    if hpa == 0 { None } else { Some(hpa) }
}

/// True when the two ranges share a byte. A zero length or a zero base on
/// `b` (unknown running-guest RAM) is not an overlap.
pub fn note_seat_firmware(hpa: u64) {
    FW_HPA.store(hpa, Ordering::Release);
    FW_EXITS.store(0, Ordering::Release);
    FW_ON.store(true, Ordering::Release);
}

pub fn seat_firmware_active() -> bool {
    FW_ON.load(Ordering::Acquire)
}

pub fn seat_firmware_hpa() -> u64 {
    FW_HPA.load(Ordering::Acquire)
}

/// Count one firmware exit. `true` when the cap is reached and the running
/// guest should resume.
pub fn seat_firmware_exit_capped() -> bool {
    let n = FW_EXITS.fetch_add(1, Ordering::AcqRel);
    n >= SEAT_FW_EXIT_CAP
}

/// The bootloader stays in its VMCS. The running guest is resumed.
pub fn pause_seat() {
    let _ = PROBE.compare_exchange(IN_FLIGHT, PARKED, Ordering::AcqRel, Ordering::Acquire);
}

pub fn seat_paused() -> bool {
    PROBE.load(Ordering::Acquire) == PARKED
}

/// Another turn for the parked bootloader. False only when it is no longer parked.
pub fn resume_seat() -> bool {
    if PROBE
        .compare_exchange(PARKED, IN_FLIGHT, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return false;
    }
    FW_EXITS.store(0, Ordering::Release);
    let _ = SLICES.fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
        Some(n.saturating_add(1))
    });
    true
}

pub fn seat_slices() -> u32 {
    SLICES.load(Ordering::Acquire)
}

/// Parked for good. Later slices do not enter it again.
pub fn finish_paused_seat() {
    PROBE.store(RETURNED, Ordering::Release);
    FW_ON.store(false, Ordering::Release);
}

pub fn clear_seat_firmware() {
    FW_ON.store(false, Ordering::Release);
}

/// Remember which image the seated firmware may read. Does not change the
/// running guest's disk.
pub fn bind_seat_disk(spare_base: u64, disk_bytes: u64) {
    READ_BASE.store(spare_base, Ordering::Release);
    READ_BYTES.store(disk_bytes, Ordering::Release);
    READ_ON.store(true, Ordering::Release);
}

pub fn clear_seat_disk() {
    READ_ON.store(false, Ordering::Release);
    LOADER.store(false, Ordering::Release);
}

pub fn note_seat_loader() {
    LOADER.store(true, Ordering::Release);
}

pub fn seat_loader_loaded() -> bool {
    LOADER.load(Ordering::Acquire)
}

/// First time the seated firmware's `ExitBootServices` succeeds.
pub fn note_seat_kernel() -> bool {
    KERNEL
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub fn seat_disk_bound() -> Option<(u64, u64)> {
    if READ_ON.load(Ordering::Acquire) {
        Some((
            READ_BASE.load(Ordering::Acquire),
            READ_BYTES.load(Ordering::Acquire),
        ))
    } else {
        None
    }
}

/// Write a `ReadBlocks` call at [`SEAT_DISK_STUB_GPA`]. Returns that GPA.
/// GPAs for the file and the loaded image when both fit in the 32 MiB slab
/// and do not overlap.
pub fn seat_loader_spans(file_len: u64, image_len: u64) -> Option<(u64, u64)> {
    if file_len == 0 || file_len > SEAT_LOADER_MAX_FILE || image_len == 0 {
        return None;
    }
    let file_end = SEAT_LOADER_FILE_GPA.checked_add(file_len)?;
    let image_end = SEAT_LOADER_IMAGE_GPA.checked_add(image_len)?;
    if file_end > SEAT_FW_BYTES || image_end > SEAT_FW_BYTES {
        return None;
    }
    if SEAT_LOADER_IMAGE_GPA < file_end && SEAT_LOADER_FILE_GPA < image_end {
        return None;
    }
    Some((SEAT_LOADER_FILE_GPA, SEAT_LOADER_IMAGE_GPA))
}

pub fn plant_disk_read(slab: &mut [u8]) -> Option<u64> {
    let end = (SEAT_DISK_BUF_GPA as usize).checked_add(512)?;
    if slab.len() < end {
        return None;
    }
    let tables = crate::raynu_f::launch_plan::F2_TABLES_BASE;
    let blockio = tables + crate::raynu_f::tables::IMAGE_BLOCKIO_DISK_OFF as u64;
    let read_fn = crate::raynu_f::trampoline_slot_gpa(
        tables + crate::raynu_f::tables::IMAGE_TRAMPOLINE_OFF as u64,
        crate::raynu_f::ServiceId::BlockIoReadBlocks,
    );
    let at = SEAT_DISK_STUB_GPA as usize;
    let b = &mut slab[at..at + 64];
    b.fill(0);
    let mut i = 0usize;
    b[i..i + 7].copy_from_slice(&[0x48, 0x81, 0xEC, 0x28, 0x00, 0x00, 0x00]);
    i += 7;
    b[i] = 0x48;
    b[i + 1] = 0xB9;
    b[i + 2..i + 10].copy_from_slice(&blockio.to_le_bytes());
    i += 10;
    b[i..i + 5].copy_from_slice(&[0xBA, 0x02, 0x00, 0x00, 0x00]);
    i += 5;
    b[i..i + 3].copy_from_slice(&[0x45, 0x31, 0xC0]);
    i += 3;
    b[i..i + 6].copy_from_slice(&[0x41, 0xB9, 0x00, 0x02, 0x00, 0x00]);
    i += 6;
    b[i] = 0x48;
    b[i + 1] = 0xB8;
    b[i + 2..i + 10].copy_from_slice(&SEAT_DISK_BUF_GPA.to_le_bytes());
    i += 10;
    b[i..i + 5].copy_from_slice(&[0x48, 0x89, 0x44, 0x24, 0x20]);
    i += 5;
    b[i] = 0x48;
    b[i + 1] = 0xB8;
    b[i + 2..i + 10].copy_from_slice(&read_fn.to_le_bytes());
    i += 10;
    b[i] = 0xFF;
    b[i + 1] = 0xD0;
    i += 2;
    b[i] = HLT_OPCODE;
    slab[SEAT_DISK_BUF_GPA as usize..SEAT_DISK_BUF_GPA as usize + 512].fill(0);
    let _ = i;
    Some(SEAT_DISK_STUB_GPA)
}

/// Set the I/O-bitmap bit so `port` exits. Ports at or above `0x8000` live
/// in the second bitmap and are left alone.
pub fn set_io_port_exit(bitmap: &mut [u8], port: u16) {
    if port >= 0x8000 {
        return;
    }
    let i = (port / 8) as usize;
    if i < bitmap.len() {
        bitmap[i] |= 1 << (port % 8);
    }
}

/// Where the seated firmware app will enter. GPAs are offsets in the slab.
pub struct SeatFirmware {
    pub entry: u64,
    pub hlt_ok: u64,
    pub hlt_fail: u64,
    pub rsp: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub cr3: u64,
}

/// Place RayNu-F's identity map, tables, and test app in `slab`.
/// Does not touch the running guest's firmware state.
pub fn place_seat_firmware(slab: &mut [u8]) -> Option<SeatFirmware> {
    use crate::raynu_f::{
        build_firmware_image, build_test_app, load_pe32plus, plan_f2, IMAGE_BYTES,
        TESTAPP_FILE_BYTES, TESTAPP_HLT_FAIL_OFF, TESTAPP_HLT_OK_OFF,
    };
    if (slab.len() as u64) < SEAT_FW_BYTES {
        return None;
    }
    let plan = plan_f2(SEAT_FW_BYTES).ok()?;
    let hpa = slab.as_mut_ptr() as u64;
    if unsafe {
        crate::vmx::guest_pt::build_identity_4g(hpa, SEAT_FW_BYTES, plan.cr3).is_err()
    } {
        return None;
    }
    let tb = plan.tables_base as usize;
    let image_end = tb.checked_add(IMAGE_BYTES)?;
    if image_end > slab.len() {
        return None;
    }
    build_firmware_image(plan.tables_base, &mut slab[tb..image_end]).ok()?;
    let mut file = [0u8; TESTAPP_FILE_BYTES];
    build_test_app(&mut file).ok()?;
    let ab = plan.app_load_base as usize;
    let app_len = plan.app_size as usize;
    let app_end = ab.checked_add(app_len)?;
    if app_end > slab.len() {
        return None;
    }
    let loaded = load_pe32plus(&file, plan.app_load_base, &mut slab[ab..app_end]).ok()?;
    Some(SeatFirmware {
        entry: loaded.entry,
        hlt_ok: loaded.entry.wrapping_add(TESTAPP_HLT_OK_OFF),
        hlt_fail: loaded.entry.wrapping_add(TESTAPP_HLT_FAIL_OFF),
        rsp: plan.rsp,
        rcx: plan.rcx,
        rdx: plan.rdx,
        cr3: plan.cr3,
    })
}

pub fn ranges_overlap(a: u64, a_len: u64, b: u64, b_len: u64) -> bool {
    if a_len == 0 || b_len == 0 || b == 0 {
        return false;
    }
    let a_end = a.saturating_add(a_len);
    let b_end = b.saturating_add(b_len);
    a < b_end && b < a_end
}

pub fn primary_controls(cloned: u32) -> u32 {
    const HLT_EXITING: u32 = 1 << 7;
    const INTERRUPT_WINDOW: u32 = 1 << 2;
    const NMI_WINDOW: u32 = 1 << 22;
    (cloned | HLT_EXITING) & !INTERRUPT_WINDOW & !NMI_WINDOW
}

/// Pages for one companion VMCS, its EPT, and a long-mode guest walk.
#[repr(C, align(4096))]
pub struct CompanionPages {
    pub vmcs: [u8; 4096],
    pub ept_pml4: [u8; 4096],
    pub ept_pdpt: [u8; 4096],
    pub ept_pd: [u8; 4096],
    pub ept_pt: [u8; 4096],
    pub guest_pml4: [u8; 4096],
    pub guest_pdpt: [u8; 4096],
    pub guest_pd: [u8; 4096],
    pub guest_pt: [u8; 4096],
    pub code: [u8; 4096],
    pub stack: [u8; 4096],
    pub sector: [u8; 4096],
}

impl CompanionPages {
    pub const ZERO: Self = Self {
        vmcs: [0; 4096],
        ept_pml4: [0; 4096],
        ept_pdpt: [0; 4096],
        ept_pd: [0; 4096],
        ept_pt: [0; 4096],
        guest_pml4: [0; 4096],
        guest_pdpt: [0; 4096],
        guest_pd: [0; 4096],
        guest_pt: [0; 4096],
        code: [0; 4096],
        stack: [0; 4096],
        sector: [0; 4096],
    };
}

pub struct CompanionPlan {
    pub vmcs: u64,
    pub eptp: u64,
    pub guest_cr3: u64,
    pub rip: u64,
    pub rsp: u64,
}

struct Hold {
    gpr: [u64; 15],
    xmm: [u8; 256],
    ubuntu: u64,
    cr2: u64,
}

static mut HOLD: Hold = Hold {
    gpr: [0; 15],
    xmm: [0; 256],
    ubuntu: 0,
    cr2: 0,
};

/// Remember the running guest's registers before a second VMCS overwrites them.
/// RSP stays in the VMCS. The fifteen slots are RAX, RBX, RCX, RDX, RSI, RDI,
/// RBP, then R8 through R15.
///
/// SAFETY: VMX-root, single threaded.
pub unsafe fn hold_guest(gpr: [u64; 15], xmm: [u8; 256], ubuntu: u64, cr2: u64) {
    let hold = &mut *core::ptr::addr_of_mut!(HOLD);
    hold.gpr = gpr;
    hold.xmm = xmm;
    hold.ubuntu = ubuntu;
    hold.cr2 = cr2;
}

/// SAFETY: VMX-root, single threaded. Valid after [`hold_guest`].
pub unsafe fn held_guest() -> ([u64; 15], [u8; 256], u64, u64) {
    let hold = &*core::ptr::addr_of!(HOLD);
    (hold.gpr, hold.xmm, hold.ubuntu, hold.cr2)
}

fn page_addr(page: &[u8; 4096]) -> u64 {
    page.as_ptr() as u64
}

fn write_pte(page: &mut [u8; 4096], index: usize, val: u64) {
    let off = index * 8;
    page[off..off + 8].copy_from_slice(&val.to_le_bytes());
}

fn read_pte(page: &[u8; 4096], index: usize) -> u64 {
    let off = index * 8;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&page[off..off + 8]);
    u64::from_le_bytes(buf)
}

fn ept_link(hpa: u64) -> u64 {
    0b111 | (hpa & !0xfff)
}

fn ept_leaf(hpa: u64) -> u64 {
    // R/W/X, WB memory type in bits 5:3.
    0b111 | (6 << 3) | (hpa & !0xfff)
}

fn pack_eptp(pml4: u64) -> u64 {
    // WB, 4-level walk.
    6 | (3 << 3) | (pml4 & !0xfff)
}

/// Fill EPT and guest page tables. Addresses are the page pointers.
pub fn program_tables(pages: &mut CompanionPages) -> CompanionPlan {
    pages.ept_pml4.fill(0);
    pages.ept_pdpt.fill(0);
    pages.ept_pd.fill(0);
    pages.ept_pt.fill(0);
    pages.guest_pml4.fill(0);
    pages.guest_pdpt.fill(0);
    pages.guest_pd.fill(0);
    pages.guest_pt.fill(0);
    pages.code.fill(0);
    pages.stack.fill(0);
    pages.sector.fill(0);
    pages.code[0] = HLT_OPCODE;

    let ept_pml4 = page_addr(&pages.ept_pml4);
    let ept_pdpt = page_addr(&pages.ept_pdpt);
    let ept_pd = page_addr(&pages.ept_pd);
    let ept_pt = page_addr(&pages.ept_pt);
    let guest_pml4 = page_addr(&pages.guest_pml4);
    let guest_pdpt = page_addr(&pages.guest_pdpt);
    let guest_pd = page_addr(&pages.guest_pd);
    let guest_pt = page_addr(&pages.guest_pt);
    let code = page_addr(&pages.code);
    let stack = page_addr(&pages.stack);
    let sector = page_addr(&pages.sector);
    let vmcs = page_addr(&pages.vmcs);

    write_pte(&mut pages.ept_pml4, 0, ept_link(ept_pdpt));
    write_pte(&mut pages.ept_pdpt, 0, ept_link(ept_pd));
    write_pte(&mut pages.ept_pd, 0, ept_link(ept_pt));
    let ept_map = [
        (CODE_GPA, code),
        (PML4_GPA, guest_pml4),
        (PDPT_GPA, guest_pdpt),
        (PD_GPA, guest_pd),
        (PT_GPA, guest_pt),
        (STACK_GPA, stack),
        (SECTOR_GPA, sector),
    ];
    for (gpa, hpa) in ept_map {
        let idx = ((gpa >> 12) & 511) as usize;
        write_pte(&mut pages.ept_pt, idx, ept_leaf(hpa));
    }

    write_pte(&mut pages.guest_pml4, 0, guest_link(PDPT_GPA));
    write_pte(&mut pages.guest_pdpt, 0, guest_link(PD_GPA));
    write_pte(&mut pages.guest_pd, 0, guest_link(PT_GPA));
    write_pte(
        &mut pages.guest_pt,
        (CODE_GPA >> 12) as usize,
        CODE_GPA | GUEST_LEAF,
    );
    write_pte(
        &mut pages.guest_pt,
        (STACK_GPA >> 12) as usize,
        STACK_GPA | GUEST_LEAF,
    );
    write_pte(
        &mut pages.guest_pt,
        (SECTOR_GPA >> 12) as usize,
        SECTOR_GPA | GUEST_LEAF,
    );

    CompanionPlan {
        vmcs,
        eptp: pack_eptp(ept_pml4),
        guest_cr3: PML4_GPA,
        rip: CODE_GPA,
        rsp: STACK_GPA + 0xFF0,
    }
}

fn guest_link(gpa: u64) -> u64 {
    gpa | GUEST_LINK
}

const SEAT_GPAS: [u64; 7] = [
    CODE_GPA, PML4_GPA, PDPT_GPA, PD_GPA, PT_GPA, STACK_GPA, SECTOR_GPA,
];

fn guest_page<'a>(pages: &'a CompanionPages, gpa: u64) -> Option<&'a [u8; 4096]> {
    match gpa {
        CODE_GPA => Some(&pages.code),
        PML4_GPA => Some(&pages.guest_pml4),
        PDPT_GPA => Some(&pages.guest_pdpt),
        PD_GPA => Some(&pages.guest_pd),
        PT_GPA => Some(&pages.guest_pt),
        STACK_GPA => Some(&pages.stack),
        SECTOR_GPA => Some(&pages.sector),
        _ => None,
    }
}

/// Copy the guest pages into `ram` and point EPT at that RAM.
/// `ram_hpa` is the host address of `ram[0]`.
pub fn install_seat_ram(pages: &mut CompanionPages, ram_hpa: u64, ram: &mut [u8]) -> bool {
    if ram_hpa & 0xfff != 0 || (ram.len() as u64) < SEAT_RAM_BYTES {
        return false;
    }
    ram[..SEAT_RAM_BYTES as usize].fill(0);
    for gpa in SEAT_GPAS {
        let Some(src) = guest_page(pages, gpa) else {
            return false;
        };
        let off = gpa as usize;
        ram[off..off + 4096].copy_from_slice(src);
    }
    for gpa in SEAT_GPAS {
        let idx = ((gpa >> 12) & 511) as usize;
        write_pte(&mut pages.ept_pt, idx, ept_leaf(ram_hpa + gpa));
    }
    true
}

/// Identity EPT of `bytes` using 2 MiB leaves. `ram_hpa` and `bytes` are 2 MiB aligned.
pub fn program_ept_2m(pages: &mut CompanionPages, ram_hpa: u64, bytes: u64) -> Option<u64> {
    const TWO_MIB: u64 = 2 * 1024 * 1024;
    if ram_hpa & (TWO_MIB - 1) != 0 || bytes == 0 || bytes & (TWO_MIB - 1) != 0 {
        return None;
    }
    let n = (bytes / TWO_MIB) as usize;
    if n == 0 || n > 512 {
        return None;
    }
    pages.ept_pml4.fill(0);
    pages.ept_pdpt.fill(0);
    pages.ept_pd.fill(0);
    let pdpt = page_addr(&pages.ept_pdpt);
    let pd = page_addr(&pages.ept_pd);
    let pml4 = page_addr(&pages.ept_pml4);
    write_pte(&mut pages.ept_pml4, 0, ept_link(pdpt));
    write_pte(&mut pages.ept_pdpt, 0, ept_link(pd));
    for i in 0..n {
        let hpa = ram_hpa + (i as u64) * TWO_MIB;
        write_pte(
            &mut pages.ept_pd,
            i,
            crate::memory::ept_hw::ept_leaf_large(hpa, 6),
        );
    }
    Some(pack_eptp(pml4))
}

pub fn ept_pd(pages: &CompanionPages, index: usize) -> u64 {
    read_pte(&pages.ept_pd, index)
}

/// Host address of `gpa` when the EPT page directory has a 2 MiB leaf.
pub fn ept_2m_hpa(pages: &CompanionPages, gpa: u64) -> Option<u64> {
    const TWO_MIB: u64 = 2 * 1024 * 1024;
    let idx = (gpa >> 21) as usize;
    if idx >= 512 {
        return None;
    }
    let leaf = read_pte(&pages.ept_pd, idx);
    if leaf & (1 << 7) == 0 || leaf & 0b111 != 0b111 {
        return None;
    }
    let base = leaf & 0x000f_ffff_ffe0_0000;
    Some(base + (gpa & (TWO_MIB - 1)))
}

/// 2 MiB base of a GPA just past the seated firmware slab, up to 64 MiB.
/// Device MMIO above that is not RAM.
pub fn seat_ram_extend_base(gpa: u64) -> Option<u64> {
    const TWO_MIB: u64 = 2 * 1024 * 1024;
    const LIMIT: u64 = 64 * 1024 * 1024;
    if gpa < SEAT_FW_BYTES || gpa >= LIMIT {
        return None;
    }
    Some(gpa & !(TWO_MIB - 1))
}

pub fn set_ept_2m_leaf(pages: &mut CompanionPages, index: usize, hpa: u64) -> bool {
    const TWO_MIB: u64 = 2 * 1024 * 1024;
    if index >= 512 || hpa & (TWO_MIB - 1) != 0 {
        return false;
    }
    write_pte(
        &mut pages.ept_pd,
        index,
        crate::memory::ept_hw::ept_leaf_large(hpa, 6),
    );
    true
}

/// Host-physical address of `gpa` through the EPT just programmed.
pub fn ept_hpa(pages: &CompanionPages, gpa: u64) -> Option<u64> {
    let link_ok = |pte: u64| pte & 0b111 == 0b111;
    if !link_ok(read_pte(&pages.ept_pml4, 0))
        || !link_ok(read_pte(&pages.ept_pdpt, 0))
        || !link_ok(read_pte(&pages.ept_pd, 0))
    {
        return None;
    }
    let idx = ((gpa >> 12) & 511) as usize;
    let leaf = read_pte(&pages.ept_pt, idx);
    if !link_ok(leaf) {
        return None;
    }
    Some(leaf & !0xfff)
}

/// Guest-physical address a long-mode walk of `va` produces.
pub fn guest_leaf_gpa(pages: &CompanionPages, va: u64) -> Option<u64> {
    let present = |pte: u64| pte & 1 == 1;
    if !present(read_pte(&pages.guest_pml4, 0))
        || !present(read_pte(&pages.guest_pdpt, 0))
        || !present(read_pte(&pages.guest_pd, 0))
    {
        return None;
    }
    let idx = ((va >> 12) & 511) as usize;
    let leaf = read_pte(&pages.guest_pt, idx);
    if !present(leaf) || (leaf & 4) != 0 {
        return None;
    }
    Some(leaf & !0xfff)
}

const _: () = assert!(crate::vmx::fields::VMCS_CLONE_FIELDS.len() <= 128);

#[cfg(test)]
mod tests {
    use super::*;

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    #[test]
    fn hlt_page_is_supervisor_and_reachable() {
        let mut pages = Box::new(CompanionPages::ZERO);
        let plan = program_tables(&mut pages);
        assert_eq!(plan.rip, CODE_GPA);
        assert_eq!(plan.guest_cr3, PML4_GPA);
        assert_eq!(plan.rsp, STACK_GPA + 0xFF0);
        assert_eq!(plan.vmcs & 0xfff, 0);
        assert_eq!(plan.eptp & 0xfff, 0x1E);
        assert_eq!(pages.code[0], HLT_OPCODE);
        assert_eq!(ept_hpa(&pages, CODE_GPA), Some(page_addr(&pages.code) & !0xfff));
        assert_eq!(ept_hpa(&pages, STACK_GPA), Some(page_addr(&pages.stack) & !0xfff));
        assert_eq!(guest_leaf_gpa(&pages, CODE_GPA), Some(CODE_GPA));
        assert_eq!(guest_leaf_gpa(&pages, STACK_GPA), Some(STACK_GPA));
        assert_eq!(guest_leaf_gpa(&pages, SECTOR_GPA), Some(SECTOR_GPA));
        assert!(guest_leaf_gpa(&pages, 0).is_none());
    }

    #[test]
    fn seat_ram_is_where_the_guest_executes() {
        let mut pages = Box::new(CompanionPages::ZERO);
        program_tables(&mut pages);
        pages.sector[0] = 0x55;
        plant_seat_program(&mut pages.code, SECTOR_GPA as u32, 0x55);
        #[repr(C, align(4096))]
        struct Ram([u8; SEAT_RAM_BYTES as usize]);
        let mut ram = Box::new(Ram([0; SEAT_RAM_BYTES as usize]));
        let hpa = ram.0.as_ptr() as u64;
        assert_eq!(hpa & 0xfff, 0);
        assert!(install_seat_ram(&mut pages, hpa, &mut ram.0));
        assert_eq!(ept_hpa(&pages, CODE_GPA), Some(hpa + CODE_GPA));
        assert_eq!(ept_hpa(&pages, SECTOR_GPA), Some(hpa + SECTOR_GPA));
        assert_eq!(ram.0[CODE_GPA as usize], 0xB8);
        assert_eq!(ram.0[CODE_GPA as usize + SEAT_HLT_OFF as usize], HLT_OPCODE);
        assert_eq!(ram.0[SECTOR_GPA as usize], 0x55);
        assert_eq!(guest_leaf_gpa(&pages, CODE_GPA), Some(CODE_GPA));
        assert!(ranges_overlap(hpa, SEAT_RAM_BYTES, hpa + 0x1000, 0x1000));
        assert!(!ranges_overlap(hpa, SEAT_RAM_BYTES, hpa + SEAT_RAM_BYTES, 0x1000));
        assert!(!ranges_overlap(hpa, SEAT_RAM_BYTES, 0, 0x8000));
    }

    #[test]
    fn seat_firmware_entry_is_the_raynu_f_app() {
        let _guard = lock();
        clear_for_test();
        let mut slab = vec![0u8; SEAT_FW_BYTES as usize];
        let fw = place_seat_firmware(&mut slab).expect("firmware");
        let at = fw.entry as usize;
        let code = &crate::raynu_f::testapp::TESTAPP_CODE;
        assert_eq!(&slab[at..at + 4], &code[..4]);
        assert_eq!(slab[fw.hlt_ok as usize], HLT_OPCODE);
        assert_eq!(slab[fw.hlt_fail as usize], HLT_OPCODE);
        assert_eq!(fw.cr3, 0x0040_0000);
        assert_ne!(fw.rdx, 0);
        let entry = plant_disk_read(&mut slab).expect("stub");
        assert_eq!(entry, SEAT_DISK_STUB_GPA);
        assert_eq!(slab[entry as usize], 0x48);
        assert!(slab[entry as usize..entry as usize + 64]
            .windows(2)
            .any(|w| w == [0xFF, 0xD0]));
        assert_eq!(slab[entry as usize + 58], HLT_OPCODE);
        bind_seat_disk(PERC_IMAGE_BYTES, 512);
        assert_eq!(seat_disk_bound(), Some((PERC_IMAGE_BYTES, 512)));
        clear_seat_disk();
        assert!(seat_disk_bound().is_none());
        assert_eq!(
            seat_loader_spans(4096, 8192),
            Some((SEAT_LOADER_FILE_GPA, SEAT_LOADER_IMAGE_GPA))
        );
        assert!(seat_loader_spans(0, 4096).is_none());
        assert!(seat_loader_spans(SEAT_LOADER_MAX_FILE + 1, 4096).is_none());
        assert_eq!(seat_ram_extend_base(SEAT_FW_BYTES), Some(SEAT_FW_BYTES));
        assert_eq!(
            seat_ram_extend_base(SEAT_FW_BYTES + 4096),
            Some(SEAT_FW_BYTES)
        );
        assert!(seat_ram_extend_base(SEAT_FW_BYTES - 1).is_none());
        assert!(seat_ram_extend_base(64 * 1024 * 1024).is_none());
        let mut pages = Box::new(CompanionPages::ZERO);
        assert!(set_ept_2m_leaf(&mut pages, 16, 0x200_0000));
        assert_eq!(ept_pd(&pages, 16) & !0xfff, 0x200_0000);
        assert_eq!(ept_2m_hpa(&pages, 0x200_1000), Some(0x200_1000));
        assert!(ept_2m_hpa(&pages, 0x1000).is_none());
        assert!(!set_ept_2m_leaf(&mut pages, 16, 0x1000));
        assert!(begin_probe());
        pause_seat();
        assert!(seat_paused());
        assert!(resume_seat());
        assert!(seat_firmware_active() || probe_in_flight());
        assert_eq!(seat_slices(), 1);
        finish_paused_seat();
        assert!(!resume_seat());
        clear_for_test();
        let mut pages = Box::new(CompanionPages::ZERO);
        let eptp = program_ept_2m(&mut pages, 0x20_0000, 2 * 1024 * 1024).expect("ept");
        assert_eq!(eptp & 0xfff, 0x1E);
        let leaf = ept_pd(&pages, 0);
        assert_eq!(leaf & !0xfff, 0x20_0000);
        assert_ne!(leaf & (1 << 7), 0);
        assert!(program_ept_2m(&mut pages, 0x1000, 2 * 1024 * 1024).is_none());
        let mut bitmap = vec![0u8; 4096];
        set_io_port_exit(&mut bitmap, 0x5246);
        assert_ne!(bitmap[0x5246 / 8] & (1 << (0x5246 % 8)), 0);
        set_io_port_exit(&mut bitmap, 0x8000);
        assert_eq!(bitmap[0], 0);
    }

    #[test]
    fn a_stolen_exit_retries_and_a_failure_does_not() {
        let _guard = lock();
        clear_for_test();
        assert!(begin_probe());
        assert!(!begin_probe());
        assert!(probe_exit_is_retry(1));
        assert!(probe_exit_is_retry(7));
        assert!(probe_exit_is_retry(52));
        assert!(!probe_exit_is_retry(12));
        assert!(!probe_exit_is_retry(48));
        note_probe_retry();
        assert!(begin_probe());
        note_probe_hlt();
        assert!(probe_returned());
        assert!(!begin_probe());
        clear_for_test();
        assert!(begin_probe());
        note_probe_failed();
        assert!(!begin_probe());
        assert_eq!(primary_controls(0), 1 << 7);
        assert_eq!(primary_controls((1 << 2) | (1 << 22) | 0x100), (1 << 7) | 0x100);
        clear_for_test();
    }

    #[test]
    fn a_different_disk_plants_its_byte_before_the_halt() {
        let _guard = lock();
        clear_for_test();
        let window = seat_disk(Seat::Window);
        let tail = seat_disk(Seat::Tail {
            off: PERC_IMAGE_BYTES,
            bytes: 512,
        });
        assert!(disks_differ(window, tail));
        assert!(!disks_differ(window, window));
        let mut code = [0u8; 4096];
        plant_disk_byte(&mut code, 0x55);
        assert_eq!(&code[..5], &[0xB0, 0x55, 0x3C, 0x55, HLT_OPCODE]);
        note_disk_plant(tail.spare_base);
        assert_eq!(take_disk_plant(), Some(PERC_IMAGE_BYTES));
        assert!(take_disk_plant().is_none());
        clear_for_test();
    }

    #[test]
    fn retry_budget_stops() {
        let _guard = lock();
        clear_for_test();
        for _ in 0..MAX_TRIES {
            assert!(begin_probe());
            note_probe_retry();
        }
        assert!(!begin_probe());
        clear_for_test();
    }
}
