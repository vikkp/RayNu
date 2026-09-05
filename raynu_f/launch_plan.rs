//! RayNu-F F2 launch plan: where everything lands in the guest slab and the
//! initial long-mode register state. Pure data, host-tested; the UEFI launcher
//! consumes it verbatim so the values are reviewable here rather than buried
//! in VMWRITEs.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! The guest is the existing 32 MiB identity-mapped guest-firmware slab. We
//! reuse the identity page tables the OVMF path already builds
//! (`vmx::guest_pt::build_identity_4g` at `GUEST_UEFI_HV_PML4`) and place our
//! tables, the loaded app, and a stack in otherwise-unused windows above it.

use super::tables::IMAGE_BYTES;
use super::testapp::{TESTAPP_IMAGE_BASE, TESTAPP_SIZE_OF_IMAGE};

/// Identity PML4 the guest-firmware path builds (`GUEST_UEFI_HV_PML4`).
pub const F2_IDENTITY_PML4: u64 = 0x0040_0000;
/// RayNu-F table image base (tables.rs layout lives here).
pub const F2_TABLES_BASE: u64 = 0x0080_0000;
/// Where the test app is loaded — deliberately not its `ImageBase`
/// (`0x0040_0000`) so DIR64 relocation is exercised.
pub const F2_APP_LOAD_BASE: u64 = 0x0090_0000;
/// Guest stack: 64 KiB ending at `F2_STACK_TOP`.
pub const F2_STACK_BYTES: u64 = 0x1_0000;
pub const F2_STACK_TOP: u64 = 0x00A1_0000;
/// Opaque, non-null `EFI_HANDLE` for the loaded image (tagged like the
/// console handles in `tables.rs`).
pub const F2_IMAGE_HANDLE: u64 = 0x5246_0000_0000_0010;

/// CR0 = PG | WP | NE | ET | MP | PE.
pub const F2_CR0: u64 = 0x8001_0033;
/// CR4 = PAE | OSFXSR | OSXMMEXCPT. VMXE is host-owned by the launcher.
pub const F2_CR4: u64 = 0x0000_0620;
/// EFER = LME | LMA (no NXE: our tables are RWX identity pages).
pub const F2_EFER: u64 = 0x0000_0500;
/// RFLAGS reserved bit 1 only; IF clear (no interrupts until F3 timer tick).
pub const F2_RFLAGS: u64 = 0x2;
/// Long-mode 64-bit code segment access rights (P, DPL0, S, code, R, A, L).
pub const F2_CS_AR: u64 = 0xA09B;
/// Flat data segment access rights (P, DPL0, S, data, W, A).
pub const F2_DS_AR: u64 = 0xC093;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanError {
    SlabTooSmall,
    Overlap,
}

/// Everything the UEFI launcher needs, in one reviewable place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaunchPlan {
    pub slab_bytes: u64,
    pub identity_pml4: u64,
    pub tables_base: u64,
    pub system_table: u64,
    pub app_load_base: u64,
    pub app_image_base: u64,
    pub app_size: u64,
    pub stack_base: u64,
    pub stack_top: u64,
    /// RSP at entry: `StartImage` would have pushed a return address, so
    /// `RSP % 16 == 8` at the first instruction (MS x64 ABI).
    pub rsp: u64,
    /// `RCX` = `EFI_HANDLE ImageHandle`.
    pub rcx: u64,
    /// `RDX` = `EFI_SYSTEM_TABLE *`.
    pub rdx: u64,
    pub cr0: u64,
    pub cr3: u64,
    pub cr4: u64,
    pub efer: u64,
    pub rflags: u64,
    pub cs_ar: u64,
    pub ds_ar: u64,
}

fn overlaps(a: (u64, u64), b: (u64, u64)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Build the F2 plan for a guest slab of `slab_bytes` (32 MiB today).
pub fn plan_f2(slab_bytes: u64) -> Result<LaunchPlan, PlanError> {
    let tables = (F2_TABLES_BASE, F2_TABLES_BASE + IMAGE_BYTES as u64);
    let app = (
        F2_APP_LOAD_BASE,
        F2_APP_LOAD_BASE + TESTAPP_SIZE_OF_IMAGE as u64,
    );
    let stack = (F2_STACK_TOP - F2_STACK_BYTES, F2_STACK_TOP);
    // Identity tables occupy [PML4, PML4 + 4 GiB/2 MiB worth of PDs); the
    // OVMF path reserves through `0x40B000`. Treat the first 64 KiB as taken.
    let pt = (F2_IDENTITY_PML4, F2_IDENTITY_PML4 + 0x1_0000);

    for r in [tables, app, stack, pt] {
        if r.1 > slab_bytes {
            return Err(PlanError::SlabTooSmall);
        }
    }
    let regions = [tables, app, stack, pt];
    for i in 0..regions.len() {
        for j in (i + 1)..regions.len() {
            if overlaps(regions[i], regions[j]) {
                return Err(PlanError::Overlap);
            }
        }
    }

    Ok(LaunchPlan {
        slab_bytes,
        identity_pml4: F2_IDENTITY_PML4,
        tables_base: F2_TABLES_BASE,
        system_table: F2_TABLES_BASE + super::tables::IMAGE_SYSTEM_TABLE_OFF as u64,
        app_load_base: F2_APP_LOAD_BASE,
        app_image_base: TESTAPP_IMAGE_BASE,
        app_size: TESTAPP_SIZE_OF_IMAGE as u64,
        stack_base: stack.0,
        stack_top: stack.1,
        rsp: F2_STACK_TOP - 8,
        rcx: F2_IMAGE_HANDLE,
        rdx: F2_TABLES_BASE + super::tables::IMAGE_SYSTEM_TABLE_OFF as u64,
        cr0: F2_CR0,
        cr3: F2_IDENTITY_PML4,
        cr4: F2_CR4,
        efer: F2_EFER,
        rflags: F2_RFLAGS,
        cs_ar: F2_CS_AR,
        ds_ar: F2_DS_AR,
    })
}
