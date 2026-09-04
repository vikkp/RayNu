//! RayNu-F — the guest UEFI boot firmware RayNu-V presents to a typed ISO.
//!
//! Pillar: [Z] single binary · [A] audit
//! Proven Core: **outside** (ADR-002 / ADR-016 guest firmware path)
//! VERIFICATION: L0 (scaffold; host tests only)
//!
//! ADR-016 decision: for the Everest E5 product path we **are** the guest
//! firmware instead of relaunching and puppeting OVMF. RayNu-F owns an
//! `EFI_SYSTEM_TABLE` + boot services and boots the ISO's own loader
//! (`\EFI\BOOT\BOOTX64.EFI`) over virtio-blk + CD. Because we author every
//! structure there is **no third-party firmware internal state to corrupt**
//! (the `3k–3o` OVMF WaitForEvent forcing is exactly what this avoids).
//!
//! This module is an **honest scaffold**: it declares the boot-service surface
//! we will implement and the honesty invariants. It does not yet publish an
//! EFI system table, does not run boot services, and does not boot an ISO.
//! It is **not** `RAYNU-V-M7-ISO-INSTALL-OK`.

#[cfg(test)]
#[path = "raynu_f_test.rs"]
mod raynu_f_test;

/// Host / CI marker when the RayNu-F scaffold gate passes. This is a
/// scaffold marker only — **not** the iron `ISO-INSTALL-OK`.
pub const RAYNU_F_SCAFFOLD_OK_MARKER: &str = "RAYNU-V-RAYNU-F-SCAFFOLD-OK";

/// Honest residual. RayNu-F is a scaffold; being-the-firmware is the E5 path
/// (ADR-016), not a shipped installer.
pub const RAYNU_F_RESIDUAL_NOTE: &str = "RayNu-F scaffold (ADR-016): be the guest UEFI firmware ourselves; no EFI_SYSTEM_TABLE published yet; no boot services; no ISO boot; virtio-blk/CD BlockIo + SimpleFileSystem + LoadImage/StartImage + serial ConIn/ConOut + GetMemoryMap/ExitBootServices + owned timer tick are planned, not implemented; drive guests only through architected inputs (No third-party firmware state mutation); the retained-OVMF VMLAUNCH path stays as diagnostic/fallback with its 3k-3o forcing disabled (RAYNU_F_NO_FW_STATE_MUTATION); not ISO-INSTALL-OK; last_commit stays 2b795a0";

/// Boot-service surface RayNu-F must provide to an ISO's own EFI loader.
/// Ordered roughly by bring-up dependency. This is the contract we own — the
/// reason there is nothing foreign to puppet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestFwProtocol {
    /// `EFI_SYSTEM_TABLE` + `EFI_BOOT_SERVICES`/`EFI_RUNTIME_SERVICES` tables.
    SystemTable,
    /// Serial `SimpleTextIn` / `SimpleTextOut` on the guest COM port.
    ConsoleSerial,
    /// Memory services: `AllocatePages`/`Pool`, `GetMemoryMap`, `ExitBootServices`.
    MemoryServices,
    /// Periodic timer tick we deliver on our own schedule (the OVMF path never
    /// got this on iron — here it is our own arch-timer, not a puppeting hack).
    TimerTick,
    /// `EFI_BLOCK_IO_PROTOCOL` over the CD (El Torito) and virtio-blk target.
    BlockIo,
    /// `EFI_SIMPLE_FILE_SYSTEM_PROTOCOL` (FAT ESP on the ISO / install disk).
    SimpleFileSystem,
    /// `LoadImage` + `StartImage` of `\EFI\BOOT\BOOTX64.EFI` (GRUB / stub).
    LoadStartImage,
}

/// The full planned surface, in bring-up order.
pub const RAYNU_F_PLANNED_PROTOCOLS: [GuestFwProtocol; 7] = [
    GuestFwProtocol::SystemTable,
    GuestFwProtocol::ConsoleSerial,
    GuestFwProtocol::MemoryServices,
    GuestFwProtocol::TimerTick,
    GuestFwProtocol::BlockIo,
    GuestFwProtocol::SimpleFileSystem,
    GuestFwProtocol::LoadStartImage,
];

/// Scaffold honesty: RayNu-F does not yet run. Flip to real evidence only when
/// the corresponding capability is implemented and proven — never from host/CI.
pub const fn raynu_f_is_functional() -> bool {
    false
}

/// Scaffold honesty: RayNu-F does not boot an ISO yet.
pub const fn raynu_f_boots_iso() -> bool {
    false
}

/// Governance invariant (ADR-016 #4): RayNu-F never mutates third-party
/// firmware internal state. It owns its own tables, so this is true by
/// construction — there is nothing foreign to force.
pub const fn raynu_f_mutates_foreign_firmware_state() -> bool {
    false
}

/// Number of boot-service protocols RayNu-F must own before an ISO can boot.
pub const fn raynu_f_planned_protocol_count() -> usize {
    RAYNU_F_PLANNED_PROTOCOLS.len()
}
