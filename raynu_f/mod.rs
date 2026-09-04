//! RayNu-F — the guest UEFI boot firmware RayNu-V presents to a typed ISO.
//!
//! Pillar: [Z] single binary · [A] audit
//! Proven Core: **outside** (ADR-002 / ADR-016 guest firmware path)
//! VERIFICATION: L1 (host tests on byte-exact tables + dispatcher)
//!
//! ADR-016 decision: for the Everest E5 product path we **are** the guest
//! firmware instead of relaunching and puppeting OVMF. RayNu-F owns an
//! `EFI_SYSTEM_TABLE` + boot services and boots the ISO's own loader
//! (`\EFI\BOOT\BOOTX64.EFI`) over virtio-blk + CD. Because we author every
//! structure there is **no third-party firmware internal state to corrupt**
//! (the `3k–3o` OVMF WaitForEvent forcing is exactly what this avoids).
//!
//! Stage 1 (this slice): byte-exact UEFI 2.10 x64 tables (`tables`), a guest
//! service trampoline that turns each call into an I/O exit on port `0x5246`,
//! and a host-side dispatcher implementing the **serial console**
//! (`ConOut.OutputString`) — everything else honestly `EFI_UNSUPPORTED`. The
//! I/O exit is wired into the guest-firmware I/O path so it is live the moment
//! a guest calls it. Not yet: a PE loader or a launch, so no guest has called
//! it. It is **not** `RAYNU-V-M7-ISO-INSTALL-OK`.

pub mod blockio;
pub mod events;
pub mod launch_plan;
pub mod memory;
pub mod pe;
pub mod protocol;
pub mod services;
pub mod tables;
pub mod testapp;

#[cfg(test)]
#[path = "raynu_f_test.rs"]
mod raynu_f_test;

pub use services::{
    decode_trampoline, dispatch, encode_trampoline, is_service_call, output_string,
    trampoline_slot_gpa, write_trampolines, ConsoleSink, Dispatched, FirmwareState, GuestMem,
    ServiceArgs, ServiceId, EFI_INVALID_PARAMETER, EFI_NOT_READY, EFI_SUCCESS, EFI_UNSUPPORTED,
    OUTPUT_STRING_CAP_CHARS, RAYNU_F_SERVICE_PORT, TRAMPOLINE_SLOT_BYTES,
    TRAMPOLINE_SLOT_COUNT,
};
pub use blockio::{BlockMedia, MEDIA_ID_CD, MEDIA_ID_DISK};
pub use events::{TimeSource, WaitOutcome};
pub use protocol::{GUID_BLOCK_IO, HANDLE_CD, HANDLE_DISK};
pub use tables::write_block_media;

/// Serial marker on the first live guest `BlockIo` read or write.
/// Guest-exit-only.
pub const RAYNU_F_BLOCKIO_OK_MARKER: &str = "RAYNU-V-RAYNU-F-BLOCKIO-OK";

/// Host / CI marker when the F4 gate passes: handle/protocol database +
/// `BlockIo` media/validation/transfer against a mock guest. Host only.
pub const RAYNU_F_BLOCKIO_GATE_MARKER: &str = "RAYNU-V-RAYNU-F-BLOCKIO-GATE-OK";
pub use testapp::{TESTAPP_HLT_FAIL_OFF, TESTAPP_HLT_OK_OFF};

/// Serial marker the hypervisor prints the first time a live guest's
/// `WaitForEvent` returns because a **timer event fired on our clock**.
/// Guest-exit-only — never printed by host/CI unit tests.
pub const RAYNU_F_TIMER_OK_MARKER: &str = "RAYNU-V-RAYNU-F-TIMER-OK";

/// Serial marker on the first live guest `AllocatePages`/`AllocatePool`
/// success. Guest-exit-only.
pub const RAYNU_F_MEM_OK_MARKER: &str = "RAYNU-V-RAYNU-F-MEM-OK";

/// Serial marker when a live guest's `ExitBootServices` succeeds with a valid
/// map key. Guest-exit-only. Reserved for a real loader/kernel (F5/F6).
pub const RAYNU_F_EBS_OK_MARKER: &str = "RAYNU-V-RAYNU-F-EBS-OK";

/// Host / CI marker when the F3 gate passes: memory services, events/timers,
/// TPL, Stall, CopyMem/SetMem/CalculateCrc32 against a mock guest + manual
/// clock, and the v2 test app round-trips. Host only.
pub const RAYNU_F_SERVICES_OK_MARKER: &str = "RAYNU-V-RAYNU-F-SERVICES-OK";
pub use tables::{
    build_firmware_image, crc32, header_crc_valid, BuildError, FirmwareImageLayout,
    IMAGE_BYTES,
};
pub use launch_plan::{plan_f2, LaunchPlan, PlanError};
pub use pe::{load_pe32plus, parse_pe32plus, Loaded, PeError, PeImage};
pub use testapp::{build_test_app, TESTAPP_FILE_BYTES, TESTAPP_MESSAGE};

/// Host / CI marker when the F2a gate passes: PE32+ loader (headers, sections,
/// DIR64 relocs) + the RayNu-F test app round-trips through it + the F2 launch
/// plan is consistent. Host only — nothing has been launched.
pub const RAYNU_F_LOADER_OK_MARKER: &str = "RAYNU-V-RAYNU-F-LOADER-OK";

/// Host / CI marker when the RayNu-F scaffold gate passes. Scaffold only —
/// **not** the iron `ISO-INSTALL-OK`.
pub const RAYNU_F_SCAFFOLD_OK_MARKER: &str = "RAYNU-V-RAYNU-F-SCAFFOLD-OK";

/// Host / CI marker when the Stage 1 tables + console dispatcher gate passes
/// (byte-exact tables, valid CRCs, trampolines decode, `OutputString` lands).
/// Host only — no guest has executed these tables yet.
pub const RAYNU_F_TABLES_OK_MARKER: &str = "RAYNU-V-RAYNU-F-TABLES-OK";

/// Serial marker the hypervisor prints the first time a **live guest** calls
/// `ConOut.OutputString` through the RayNu-F trampoline. Never printed by
/// host/CI unit tests — only a guest VM-exit produces it.
pub const RAYNU_F_CONOUT_OK_MARKER: &str = "RAYNU-V-RAYNU-F-CONOUT-OK";

/// Honest residual. RayNu-F Stage 1 is tables + console dispatcher; the E5
/// path is being-the-firmware (ADR-016), not a shipped installer.
pub const RAYNU_F_RESIDUAL_NOTE: &str = "RayNu-F F3 (ADR-016): be the guest UEFI firmware ourselves; byte-exact UEFI 2.10 x64 EFI_SYSTEM_TABLE + EFI_BOOT_SERVICES (44) + EFI_RUNTIME_SERVICES (14) + SIMPLE_TEXT_OUTPUT/INPUT with valid header CRC32 and L\"RayNu-F\" vendor; every fn ptr is a 14-byte guest trampoline (mov r10,rdx; mov eax,id; mov dx,0x5246; out dx,eax; ret) so a call is an I/O exit handled outside the Proven Core (no VMCALL); F2b closed on nested VT-x raynuvsrv1 (RAYNU-V-RAYNU-F-CONOUT-OK, exits=2); F3 host-proven: AllocatePages/FreePages/AllocatePool/FreePool over a 20 MiB slab pool, coalesced GetMemoryMap + ExitBootServices map-key, CreateEvent/SetTimer/CheckEvent/WaitForEvent/SignalEvent/CloseEvent on an owned host-side firmware clock (TSC calibrated against a pre-EBS UEFI Stall; no guest IDT/PIT/LAPIC needed in the firmware phase), TPL, Stall, GetNextMonotonicCount, SetWatchdogTimer, CalculateCrc32, CopyMem, SetMem, ConIn.WaitForKey real event + ReadKeyStroke from host serial RX; notify-function dispatch, event groups, handles/protocols/LoadImage/StartImage and runtime services are EFI_UNSUPPORTED (honest, F4/F5); pool is page-granular; v2 test app Stall→CreateEvent→SetTimer→WaitForEvent→AllocatePages→OutputString with OK/FAIL hlt addresses; RAYNU-V-RAYNU-F-TIMER-OK / MEM-OK print only on a live guest call; drive guests only through architected inputs (No third-party firmware state mutation); retained-OVMF VMLAUNCH stays diagnostic with 3k-3o forcing disabled (RAYNU_F_NO_FW_STATE_MUTATION); not ISO-INSTALL-OK; last_commit stays 2b795a0";

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

/// Protocols whose **tables/dispatch** exist after F3. "Implemented" here
/// means host-testable code that a guest call reaches. F2b proved
/// SystemTable + ConsoleSerial on a live guest; MemoryServices + TimerTick
/// are host-proven and await the F3 nested run.
pub const RAYNU_F_STAGE1_PROTOCOLS: [GuestFwProtocol; 4] = [
    GuestFwProtocol::SystemTable,
    GuestFwProtocol::ConsoleSerial,
    GuestFwProtocol::MemoryServices,
    GuestFwProtocol::TimerTick,
];

/// Whether a protocol has Stage 1 tables/dispatch behind it.
pub fn raynu_f_protocol_has_dispatch(p: GuestFwProtocol) -> bool {
    RAYNU_F_STAGE1_PROTOCOLS.contains(&p)
}

/// Honesty: RayNu-F has not launched a guest. Flip to real evidence only when
/// a loader + launch exist and a guest executed our tables — never from host/CI.
pub const fn raynu_f_is_functional() -> bool {
    false
}

/// Honesty: RayNu-F does not boot an ISO yet.
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
