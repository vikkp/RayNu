//! RayNu-F service trampolines and the host-side dispatcher.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! A guest calls a UEFI service with the MS x64 ABI (`RCX, RDX, R8, R9`, then
//! stack). Every function pointer in our tables targets a 14-byte stub in
//! guest memory:
//!
//! ```text
//! 49 89 D2        mov r10, rdx        ; save arg2 (DX is needed for OUT)
//! B8 <id:u32>     mov eax, <service>  ; which service
//! 66 BA 46 52     mov dx, 0x5246      ; 'RF' — RayNu-F service port
//! EF              out dx, eax         ; I/O exit → hypervisor
//! C3              ret                 ; RAX already holds EFI_STATUS
//! ```
//!
//! The `OUT` is an ordinary I/O VM-exit handled by the guest-firmware I/O path
//! (outside the Proven Core). No VMCALL / hypercall surface is touched. The
//! hypervisor reads `RCX / R10 / R8 / R9` (+ stack args at `[RSP+0x28..]`,
//! since the stub's `call` return address sits at `[RSP]`), performs the
//! service against guest memory it owns, writes `RAX = EFI_STATUS`, and
//! resumes at `ret`. `R10`/`R11` are volatile in the MS x64 ABI.

use super::events::{
    Events, TimeSource, WaitOutcome, EVT_NOTIFY_SIGNAL, EVT_NOTIFY_WAIT, EVT_TIMER,
    TPL_HIGH_LEVEL,
};
use super::memory::{
    encode_descriptor, MemRun, PagePool, EFI_BUFFER_TOO_SMALL, MAX_DESCRIPTORS,
    MEMORY_DESCRIPTOR_SIZE, MEMORY_DESCRIPTOR_VERSION, POOL_HEADER_BYTES, POOL_MAGIC,
};
use super::blockio::{
    lba_offset, validate_transfer, BlockMedia, EFI_DEVICE_ERROR as BLK_DEVICE_ERROR,
    MAX_TRANSFER_BYTES,
};
use super::filesystem::{FileSystem, FILE_SLOTS, MAX_PATH_BYTES};
use super::protocol::{
    Guid, Protocols, ALL_HANDLES, BY_PROTOCOL, EFI_BUFFER_TOO_SMALL as PROTO_BUFFER_TOO_SMALL,
    EFI_NOT_FOUND as PROTO_NOT_FOUND, EFI_OUT_OF_RESOURCES, MAX_HANDLES,
};
use super::tables::{crc32_feed, crc32_finish, crc32_start};

/// `'R' 'F'` — the RayNu-F service I/O port. Unused by OVMF / QEMU / PC legacy.
pub const RAYNU_F_SERVICE_PORT: u16 = 0x5246;
/// Each trampoline slot is 16 bytes (14 used + 2 NOP pad).
pub const TRAMPOLINE_SLOT_BYTES: usize = 16;
/// Number of service slots we lay out (11 console + 44 boot + 14 runtime + 4 BlockIo).
pub const TRAMPOLINE_SLOT_COUNT: usize = 84;
/// Exact stub length before padding.
pub const TRAMPOLINE_STUB_BYTES: usize = 14;

/// UEFI `EFI_STATUS` values (x64, high bit = error).
pub const EFI_SUCCESS: u64 = 0;
pub const EFI_INVALID_PARAMETER: u64 = 0x8000_0000_0000_0002;
pub const EFI_UNSUPPORTED: u64 = 0x8000_0000_0000_0003;
pub const EFI_DEVICE_ERROR: u64 = 0x8000_0000_0000_0007;
pub const EFI_NOT_READY: u64 = 0x8000_0000_0000_0006;
pub const EFI_NOT_FOUND: u64 = 0x8000_0000_0000_000E;

/// Longest `OutputString` we will read from the guest (CHAR16 units).
pub const OUTPUT_STRING_CAP_CHARS: usize = 4096;
/// Cap for `CopyMem` / `SetMem` / `CalculateCrc32` (16 MiB — the slab is 32).
pub const MEM_OP_CAP_BYTES: u64 = 16 * 1024 * 1024;
/// Stack args: return address at `[RSP]`, 32-byte shadow, then arg 5.
pub const STACK_ARG5_OFF: u64 = 8 + 0x20;

/// Service identifier carried in EAX at the `OUT`. Ranges:
/// `0x100..` ConOut, `0x180..` ConIn, `0x200..` boot services (spec order),
/// `0x300..` runtime services (spec order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceId(pub u32);

#[allow(non_upper_case_globals)]
impl ServiceId {
    pub const CONOUT_BASE: u32 = 0x100;
    pub const CONIN_BASE: u32 = 0x180;
    pub const BOOT_BASE: u32 = 0x200;
    pub const RUNTIME_BASE: u32 = 0x300;
    pub const BLOCKIO_BASE: u32 = 0x400;
    pub const SFS_BASE: u32 = 0x500;
    pub const FILE_BASE: u32 = 0x510;

    pub const ConOutReset: ServiceId = ServiceId(0x100);
    pub const ConOutOutputString: ServiceId = ServiceId(0x101);
    pub const ConOutTestString: ServiceId = ServiceId(0x102);
    pub const ConOutQueryMode: ServiceId = ServiceId(0x103);
    pub const ConOutSetMode: ServiceId = ServiceId(0x104);
    pub const ConOutSetAttribute: ServiceId = ServiceId(0x105);
    pub const ConOutClearScreen: ServiceId = ServiceId(0x106);
    pub const ConOutSetCursorPosition: ServiceId = ServiceId(0x107);
    pub const ConOutEnableCursor: ServiceId = ServiceId(0x108);
    pub const ConInReset: ServiceId = ServiceId(0x180);
    pub const ConInReadKeyStroke: ServiceId = ServiceId(0x181);

    // Boot services, spec order (EFI_BOOT_SERVICES).
    pub const RaiseTPL: ServiceId = ServiceId(0x200);
    pub const RestoreTPL: ServiceId = ServiceId(0x201);
    pub const AllocatePages: ServiceId = ServiceId(0x202);
    pub const FreePages: ServiceId = ServiceId(0x203);
    pub const GetMemoryMap: ServiceId = ServiceId(0x204);
    pub const AllocatePool: ServiceId = ServiceId(0x205);
    pub const FreePool: ServiceId = ServiceId(0x206);
    pub const CreateEvent: ServiceId = ServiceId(0x207);
    pub const SetTimer: ServiceId = ServiceId(0x208);
    pub const WaitForEvent: ServiceId = ServiceId(0x209);
    pub const SignalEvent: ServiceId = ServiceId(0x20A);
    pub const CloseEvent: ServiceId = ServiceId(0x20B);
    pub const CheckEvent: ServiceId = ServiceId(0x20C);
    pub const InstallProtocolInterface: ServiceId = ServiceId(0x20D);
    pub const ReinstallProtocolInterface: ServiceId = ServiceId(0x20E);
    pub const UninstallProtocolInterface: ServiceId = ServiceId(0x20F);
    pub const HandleProtocol: ServiceId = ServiceId(0x210);
    pub const Reserved: ServiceId = ServiceId(0x211);
    pub const RegisterProtocolNotify: ServiceId = ServiceId(0x212);
    pub const LocateHandle: ServiceId = ServiceId(0x213);
    pub const LocateDevicePath: ServiceId = ServiceId(0x214);
    pub const InstallConfigurationTable: ServiceId = ServiceId(0x215);
    pub const LoadImage: ServiceId = ServiceId(0x216);
    pub const StartImage: ServiceId = ServiceId(0x217);
    pub const Exit: ServiceId = ServiceId(0x218);
    pub const UnloadImage: ServiceId = ServiceId(0x219);
    pub const ExitBootServices: ServiceId = ServiceId(0x21A);
    pub const GetNextMonotonicCount: ServiceId = ServiceId(0x21B);
    pub const Stall: ServiceId = ServiceId(0x21C);
    pub const SetWatchdogTimer: ServiceId = ServiceId(0x21D);
    pub const ConnectController: ServiceId = ServiceId(0x21E);
    pub const DisconnectController: ServiceId = ServiceId(0x21F);
    pub const OpenProtocol: ServiceId = ServiceId(0x220);
    pub const CloseProtocol: ServiceId = ServiceId(0x221);
    pub const OpenProtocolInformation: ServiceId = ServiceId(0x222);
    pub const ProtocolsPerHandle: ServiceId = ServiceId(0x223);
    pub const LocateHandleBuffer: ServiceId = ServiceId(0x224);
    pub const LocateProtocol: ServiceId = ServiceId(0x225);
    pub const InstallMultipleProtocolInterfaces: ServiceId = ServiceId(0x226);
    pub const UninstallMultipleProtocolInterfaces: ServiceId = ServiceId(0x227);
    pub const CalculateCrc32: ServiceId = ServiceId(0x228);
    pub const CopyMem: ServiceId = ServiceId(0x229);
    pub const SetMem: ServiceId = ServiceId(0x22A);
    pub const CreateEventEx: ServiceId = ServiceId(0x22B);

    // EFI_BLOCK_IO_PROTOCOL methods (shared by the CD and disk instances).
    pub const BlockIoReset: ServiceId = ServiceId(0x400);
    pub const BlockIoReadBlocks: ServiceId = ServiceId(0x401);
    pub const BlockIoWriteBlocks: ServiceId = ServiceId(0x402);
    pub const BlockIoFlushBlocks: ServiceId = ServiceId(0x403);

    // EFI_SIMPLE_FILE_SYSTEM_PROTOCOL.
    pub const SfsOpenVolume: ServiceId = ServiceId(0x500);
    // EFI_FILE_PROTOCOL (spec order).
    pub const FileOpen: ServiceId = ServiceId(0x510);
    pub const FileClose: ServiceId = ServiceId(0x511);
    pub const FileDelete: ServiceId = ServiceId(0x512);
    pub const FileRead: ServiceId = ServiceId(0x513);
    pub const FileWrite: ServiceId = ServiceId(0x514);
    pub const FileGetPosition: ServiceId = ServiceId(0x515);
    pub const FileSetPosition: ServiceId = ServiceId(0x516);
    pub const FileGetInfo: ServiceId = ServiceId(0x517);
    pub const FileSetInfo: ServiceId = ServiceId(0x518);
    pub const FileFlush: ServiceId = ServiceId(0x519);

    /// Boot service `i` in `EFI_BOOT_SERVICES` spec order (0 = RaiseTPL).
    pub const fn boot_service(i: usize) -> ServiceId {
        ServiceId(Self::BOOT_BASE + i as u32)
    }

    /// Runtime service `i` in `EFI_RUNTIME_SERVICES` spec order (0 = GetTime).
    pub const fn runtime_service(i: usize) -> ServiceId {
        ServiceId(Self::RUNTIME_BASE + i as u32)
    }

    /// Compact trampoline slot for this id, or `None` if not a laid-out slot.
    pub const fn slot_index(self) -> Option<usize> {
        let v = self.0;
        if v >= Self::CONOUT_BASE && v <= Self::CONOUT_BASE + 8 {
            Some((v - Self::CONOUT_BASE) as usize)
        } else if v >= Self::CONIN_BASE && v <= Self::CONIN_BASE + 1 {
            Some(9 + (v - Self::CONIN_BASE) as usize)
        } else if v >= Self::BOOT_BASE && v < Self::BOOT_BASE + 44 {
            Some(11 + (v - Self::BOOT_BASE) as usize)
        } else if v >= Self::RUNTIME_BASE && v < Self::RUNTIME_BASE + 14 {
            Some(55 + (v - Self::RUNTIME_BASE) as usize)
        } else if v >= Self::BLOCKIO_BASE && v < Self::BLOCKIO_BASE + 4 {
            Some(69 + (v - Self::BLOCKIO_BASE) as usize)
        } else if v == Self::SFS_BASE {
            Some(73)
        } else if v >= Self::FILE_BASE && v < Self::FILE_BASE + 10 {
            Some(74 + (v - Self::FILE_BASE) as usize)
        } else {
            None
        }
    }

    /// Inverse of [`slot_index`].
    pub const fn from_slot(slot: usize) -> Option<ServiceId> {
        if slot < 9 {
            Some(ServiceId(Self::CONOUT_BASE + slot as u32))
        } else if slot < 11 {
            Some(ServiceId(Self::CONIN_BASE + (slot - 9) as u32))
        } else if slot < 55 {
            Some(ServiceId(Self::BOOT_BASE + (slot - 11) as u32))
        } else if slot < 69 {
            Some(ServiceId(Self::RUNTIME_BASE + (slot - 55) as u32))
        } else if slot < 73 {
            Some(ServiceId(Self::BLOCKIO_BASE + (slot - 69) as u32))
        } else if slot == 73 {
            Some(ServiceId(Self::SFS_BASE))
        } else if slot < TRAMPOLINE_SLOT_COUNT {
            Some(ServiceId(Self::FILE_BASE + (slot - 74) as u32))
        } else {
            None
        }
    }

    /// Whether this id is one RayNu-F laid out (guards a garbage EAX).
    pub const fn is_known(self) -> bool {
        self.slot_index().is_some()
    }

    /// Short name for serial logs.
    pub fn name(self) -> &'static str {
        match self {
            ServiceId::ConOutReset => "ConOut.Reset",
            ServiceId::ConOutOutputString => "ConOut.OutputString",
            ServiceId::ConOutTestString => "ConOut.TestString",
            ServiceId::ConOutQueryMode => "ConOut.QueryMode",
            ServiceId::ConOutSetMode => "ConOut.SetMode",
            ServiceId::ConOutSetAttribute => "ConOut.SetAttribute",
            ServiceId::ConOutClearScreen => "ConOut.ClearScreen",
            ServiceId::ConOutSetCursorPosition => "ConOut.SetCursorPosition",
            ServiceId::ConOutEnableCursor => "ConOut.EnableCursor",
            ServiceId::ConInReset => "ConIn.Reset",
            ServiceId::ConInReadKeyStroke => "ConIn.ReadKeyStroke",
            ServiceId::RaiseTPL => "RaiseTPL",
            ServiceId::RestoreTPL => "RestoreTPL",
            ServiceId::AllocatePages => "AllocatePages",
            ServiceId::FreePages => "FreePages",
            ServiceId::GetMemoryMap => "GetMemoryMap",
            ServiceId::AllocatePool => "AllocatePool",
            ServiceId::FreePool => "FreePool",
            ServiceId::CreateEvent => "CreateEvent",
            ServiceId::SetTimer => "SetTimer",
            ServiceId::WaitForEvent => "WaitForEvent",
            ServiceId::SignalEvent => "SignalEvent",
            ServiceId::CloseEvent => "CloseEvent",
            ServiceId::CheckEvent => "CheckEvent",
            ServiceId::HandleProtocol => "HandleProtocol",
            ServiceId::LocateHandle => "LocateHandle",
            ServiceId::LoadImage => "LoadImage",
            ServiceId::StartImage => "StartImage",
            ServiceId::Exit => "Exit",
            ServiceId::ExitBootServices => "ExitBootServices",
            ServiceId::GetNextMonotonicCount => "GetNextMonotonicCount",
            ServiceId::Stall => "Stall",
            ServiceId::SetWatchdogTimer => "SetWatchdogTimer",
            ServiceId::OpenProtocol => "OpenProtocol",
            ServiceId::CloseProtocol => "CloseProtocol",
            ServiceId::LocateHandleBuffer => "LocateHandleBuffer",
            ServiceId::LocateProtocol => "LocateProtocol",
            ServiceId::CalculateCrc32 => "CalculateCrc32",
            ServiceId::CopyMem => "CopyMem",
            ServiceId::SetMem => "SetMem",
            ServiceId::CreateEventEx => "CreateEventEx",
            ServiceId::InstallProtocolInterface => "InstallProtocolInterface",
            ServiceId::BlockIoReset => "BlockIo.Reset",
            ServiceId::BlockIoReadBlocks => "BlockIo.ReadBlocks",
            ServiceId::BlockIoWriteBlocks => "BlockIo.WriteBlocks",
            ServiceId::BlockIoFlushBlocks => "BlockIo.FlushBlocks",
            ServiceId::SfsOpenVolume => "Sfs.OpenVolume",
            ServiceId::FileOpen => "File.Open",
            ServiceId::FileClose => "File.Close",
            ServiceId::FileDelete => "File.Delete",
            ServiceId::FileRead => "File.Read",
            ServiceId::FileWrite => "File.Write",
            ServiceId::FileGetPosition => "File.GetPosition",
            ServiceId::FileSetPosition => "File.SetPosition",
            ServiceId::FileGetInfo => "File.GetInfo",
            ServiceId::FileSetInfo => "File.SetInfo",
            ServiceId::FileFlush => "File.Flush",
            ServiceId(v) if (Self::BOOT_BASE..Self::BOOT_BASE + 44).contains(&v) => "BootServices",
            ServiceId(v) if (Self::RUNTIME_BASE..Self::RUNTIME_BASE + 14).contains(&v) => {
                "RuntimeServices"
            }
            _ => "unknown",
        }
    }
}

/// Encode one 16-byte trampoline slot for `id`.
pub fn encode_trampoline(id: ServiceId) -> [u8; TRAMPOLINE_SLOT_BYTES] {
    let mut s = [0x90u8; TRAMPOLINE_SLOT_BYTES];
    s[0] = 0x49; // mov r10, rdx
    s[1] = 0x89;
    s[2] = 0xD2;
    s[3] = 0xB8; // mov eax, imm32
    s[4..8].copy_from_slice(&id.0.to_le_bytes());
    s[8] = 0x66; // mov dx, imm16
    s[9] = 0xBA;
    s[10..12].copy_from_slice(&RAYNU_F_SERVICE_PORT.to_le_bytes());
    s[12] = 0xEF; // out dx, eax
    s[13] = 0xC3; // ret
    s
}

/// Fill a trampoline page with all laid-out slots. `buf` must be at least
/// `TRAMPOLINE_SLOT_COUNT * TRAMPOLINE_SLOT_BYTES` bytes.
pub fn write_trampolines(buf: &mut [u8]) {
    for slot in 0..TRAMPOLINE_SLOT_COUNT {
        let Some(id) = ServiceId::from_slot(slot) else {
            continue;
        };
        let off = slot * TRAMPOLINE_SLOT_BYTES;
        if off + TRAMPOLINE_SLOT_BYTES > buf.len() {
            break;
        }
        buf[off..off + TRAMPOLINE_SLOT_BYTES].copy_from_slice(&encode_trampoline(id));
    }
}

/// GPA of the trampoline for `id` given the trampoline page base.
/// Unknown ids map to slot 0 (ConOut.Reset), which is harmless.
pub const fn trampoline_slot_gpa(trampoline_base: u64, id: ServiceId) -> u64 {
    let slot = match id.slot_index() {
        Some(s) => s,
        None => 0,
    };
    trampoline_base + (slot * TRAMPOLINE_SLOT_BYTES) as u64
}

/// Decode a trampoline slot's service id from its bytes (for tests / audit).
pub fn decode_trampoline(slot: &[u8]) -> Option<ServiceId> {
    if slot.len() < TRAMPOLINE_STUB_BYTES {
        return None;
    }
    if slot[0..4] != [0x49, 0x89, 0xD2, 0xB8]
        || slot[8..10] != [0x66, 0xBA]
        || slot[10..12] != RAYNU_F_SERVICE_PORT.to_le_bytes()
        || slot[12] != 0xEF
        || slot[13] != 0xC3
    {
        return None;
    }
    Some(ServiceId(u32::from_le_bytes([slot[4], slot[5], slot[6], slot[7]])))
}

/// True when an I/O exit is a RayNu-F service call (`out dx, eax` to `0x5246`).
pub const fn is_service_call(port: u16, is_in: bool, size: u64) -> bool {
    port == RAYNU_F_SERVICE_PORT && !is_in && size == 4
}

/// Guest memory access (host tests use a `RefCell<Vec>`; the hypervisor walks
/// the guest's page tables). Both return bytes transferred.
pub trait GuestMem {
    fn read(&self, addr: u64, buf: &mut [u8]) -> usize;
    fn write(&self, addr: u64, buf: &[u8]) -> usize;
}

/// Console: output bytes, and (for `ConIn`) whether/what input is pending.
pub trait ConsoleSink {
    fn write_byte(&mut self, b: u8);
    fn has_input(&self) -> bool {
        false
    }
    fn read_input(&mut self) -> Option<u8> {
        None
    }
}

/// Arguments captured at the `OUT` exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceArgs {
    /// `RCX` (arg 1, usually `This`).
    pub a1: u64,
    /// `R10` (arg 2 — the stub moved RDX here before loading DX).
    pub a2: u64,
    /// `R8` (arg 3).
    pub a3: u64,
    /// `R9` (arg 4).
    pub a4: u64,
    /// Guest `RSP` at the exit (inside the stub; `[RSP]` = return address).
    pub rsp: u64,
}

impl ServiceArgs {
    pub const fn regs(a1: u64, a2: u64, a3: u64, a4: u64) -> Self {
        ServiceArgs { a1, a2, a3, a4, rsp: 0 }
    }
}

/// Backing-store access for block devices. Plain `fn` pointers keep
/// `FirmwareState` a `const`-constructible static with no lifetimes.
pub type BlockReadFn = fn(u32, u64, &mut [u8]) -> bool;
pub type BlockWriteFn = fn(u32, u64, &[u8]) -> bool;

/// All RayNu-F firmware state the dispatcher mutates. Lives in the hypervisor
/// as a single BSP-owned instance; host tests build their own.
pub struct FirmwareState {
    pub pool: PagePool,
    pub events: Events,
    pub protocols: Protocols,
    pub watchdog_sets: u32,
    /// `This` pointer → media, published by the launcher.
    pub blockio_cd: u64,
    pub blockio_disk: u64,
    pub media_cd: BlockMedia,
    pub media_disk: BlockMedia,
    pub read_blocks: Option<BlockReadFn>,
    pub write_blocks: Option<BlockWriteFn>,
    /// F5: mounted FAT volume + open-file table.
    pub fs: FileSystem,
    /// Base GPA of the per-slot `EFI_FILE_PROTOCOL` array, and the SFS struct.
    pub file_proto_base: u64,
    pub sfs: u64,
    /// Byte offset of the FAT volume inside the CD backing store (the El
    /// Torito boot image extent).
    pub fat_volume_off: u64,
    /// F5: the one image `LoadImage` has staged, if any.
    pub loaded_image_proto: u64,
    /// Device path published for the boot volume, and the handle owning it.
    pub device_path: u64,
    pub device_handle: u64,
    pub image_handle: u64,
    pub image_base: u64,
    pub image_size: u64,
    pub image_entry: u64,
    /// System table GPA (handed to a started image in RDX).
    pub system_table: u64,
    /// GPA of the `EFI_CONFIGURATION_TABLE` array the system table points at.
    pub config_table: u64,
    /// `StartImage` has handed control to `image_handle` and it has not
    /// `Exit`ed yet (so `Exit` knows there is a caller to unwind to).
    pub image_started: bool,
    /// Successful block reads / writes (host bookkeeping for markers).
    pub block_reads: u32,
    pub block_writes: u32,
}

impl FirmwareState {
    pub const fn new() -> Self {
        FirmwareState {
            pool: PagePool::new(),
            events: Events::new(),
            protocols: Protocols::new(),
            watchdog_sets: 0,
            blockio_cd: 0,
            blockio_disk: 0,
            media_cd: BlockMedia::cd(0),
            media_disk: BlockMedia::disk(0),
            read_blocks: None,
            write_blocks: None,
            fs: FileSystem::new(),
            file_proto_base: 0,
            sfs: 0,
            fat_volume_off: 0,
            loaded_image_proto: 0,
            device_path: 0,
            device_handle: 0,
            image_handle: 0,
            image_base: 0,
            image_size: 0,
            image_entry: 0,
            system_table: 0,
            config_table: 0,
            image_started: false,
            block_reads: 0,
            block_writes: 0,
        }
    }

    /// Resolve a `This` pointer to its media description.
    pub fn media_for(&self, this: u64) -> Option<&BlockMedia> {
        if this != 0 && this == self.blockio_cd {
            Some(&self.media_cd)
        } else if this != 0 && this == self.blockio_disk {
            Some(&self.media_disk)
        } else {
            None
        }
    }
}

/// Result of one dispatch, for logging/audit alongside the `EFI_STATUS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dispatched {
    pub id: ServiceId,
    pub status: u64,
    /// CHAR16 units consumed by `OutputString` (0 otherwise).
    pub chars_out: usize,
    /// `WaitForEvent` outcome, when applicable.
    pub wait: Option<WaitOutcome>,
    /// `AllocatePages`/`AllocatePool` succeeded on this call.
    pub alloc_ok: bool,
    /// `ExitBootServices` succeeded on this call.
    pub exited_boot_services: bool,
    /// A `BlockIo` read or write completed on this call.
    pub block_io_ok: bool,
    /// A file read through `EFI_FILE_PROTOCOL` completed on this call.
    pub file_read_ok: bool,
    /// `LoadImage` staged an image on this call.
    pub image_loaded: bool,
    /// `StartImage`: redirect the guest to `(entry, image_handle)` instead of
    /// returning to the caller. The caller's return address stays on the
    /// stack, so the image's `ret` lands back after the `StartImage` call.
    pub start_image: Option<(u64, u64)>,
    /// `Exit(ImageHandle, ExitStatus, …)` from the started image: the host
    /// must unwind the guest to the `StartImage` caller (its saved RSP /
    /// callee-saved GPRs) and return `ExitStatus` in RAX there.
    pub exit_image: Option<u64>,
}

fn read_u64(mem: &dyn GuestMem, addr: u64) -> Option<u64> {
    if addr == 0 {
        return None;
    }
    let mut b = [0u8; 8];
    if mem.read(addr, &mut b) < 8 {
        return None;
    }
    Some(u64::from_le_bytes(b))
}

fn write_u64(mem: &dyn GuestMem, addr: u64, v: u64) -> bool {
    addr != 0 && mem.write(addr, &v.to_le_bytes()) == 8
}

fn write_u32(mem: &dyn GuestMem, addr: u64, v: u32) -> bool {
    addr != 0 && mem.write(addr, &v.to_le_bytes()) == 4
}

/// Stack argument `n` (5-based) at the `OUT` exit.
pub fn stack_arg(mem: &dyn GuestMem, rsp: u64, n: u32) -> Option<u64> {
    if n < 5 || rsp == 0 {
        return None;
    }
    read_u64(mem, rsp + STACK_ARG5_OFF + u64::from(n - 5) * 8)
}

/// Read a NUL-terminated CHAR16 string from guest memory and write it to the
/// console as bytes. ASCII, CR, LF pass through; other code units become `?`.
/// Stops at `OUTPUT_STRING_CAP_CHARS`. Returns CHAR16 units consumed.
pub fn output_string(mem: &dyn GuestMem, sink: &mut dyn ConsoleSink, addr: u64) -> usize {
    let mut n = 0usize;
    let mut cur = addr;
    while n < OUTPUT_STRING_CAP_CHARS {
        let mut b = [0u8; 2];
        if mem.read(cur, &mut b) < 2 {
            break;
        }
        let ch = u16::from_le_bytes(b);
        if ch == 0 {
            break;
        }
        n += 1;
        cur = cur.wrapping_add(2);
        let out = if ch == u16::from(b'\r') || ch == u16::from(b'\n') {
            ch as u8
        } else if (0x20..0x7f).contains(&ch) {
            ch as u8
        } else {
            b'?'
        };
        sink.write_byte(out);
    }
    n
}

fn get_memory_map(
    st: &mut FirmwareState,
    mem: &dyn GuestMem,
    a: ServiceArgs,
    slab_bytes: u64,
) -> u64 {
    // (MapSize*, MemoryMap*, MapKey*, DescriptorSize*, DescriptorVersion*)
    let p_size = a.a1;
    let p_map = a.a2;
    let p_key = a.a3;
    let p_dsize = a.a4;
    let Some(p_dver) = stack_arg(mem, a.rsp, 5) else {
        return EFI_INVALID_PARAMETER;
    };
    let Some(size) = read_u64(mem, p_size) else {
        return EFI_INVALID_PARAMETER;
    };
    let mut runs = [MemRun { typ: 0, start: 0, pages: 0 }; MAX_DESCRIPTORS];
    let n = st.pool.memory_map(slab_bytes, &mut runs);
    let need = n as u64 * MEMORY_DESCRIPTOR_SIZE;
    if p_map == 0 || size < need {
        let _ = write_u64(mem, p_size, need);
        return EFI_BUFFER_TOO_SMALL;
    }
    let mut d = [0u8; 40];
    for (i, r) in runs[..n].iter().enumerate() {
        encode_descriptor(r, &mut d);
        if mem.write(p_map + (i as u64) * MEMORY_DESCRIPTOR_SIZE, &d) != 40 {
            return EFI_INVALID_PARAMETER;
        }
    }
    let key = st.pool.next_map_key();
    let _ = write_u64(mem, p_size, need);
    let _ = write_u64(mem, p_key, key);
    let _ = write_u64(mem, p_dsize, MEMORY_DESCRIPTOR_SIZE);
    let _ = write_u32(mem, p_dver, MEMORY_DESCRIPTOR_VERSION);
    EFI_SUCCESS
}

fn copy_mem(mem: &dyn GuestMem, dst: u64, src: u64, len: u64) -> u64 {
    if len == 0 {
        return EFI_SUCCESS;
    }
    if dst == 0 || src == 0 || len > MEM_OP_CAP_BYTES {
        return EFI_INVALID_PARAMETER;
    }
    let mut buf = [0u8; 4096];
    let forward = dst <= src || dst >= src.saturating_add(len);
    let mut done = 0u64;
    while done < len {
        let chunk = (len - done).min(4096) as usize;
        let (s, d) = if forward {
            (src + done, dst + done)
        } else {
            (src + len - done - chunk as u64, dst + len - done - chunk as u64)
        };
        if mem.read(s, &mut buf[..chunk]) != chunk || mem.write(d, &buf[..chunk]) != chunk {
            return EFI_INVALID_PARAMETER;
        }
        done += chunk as u64;
    }
    EFI_SUCCESS
}

fn set_mem(mem: &dyn GuestMem, dst: u64, len: u64, val: u8) -> u64 {
    if len == 0 {
        return EFI_SUCCESS;
    }
    if dst == 0 || len > MEM_OP_CAP_BYTES {
        return EFI_INVALID_PARAMETER;
    }
    let buf = [val; 4096];
    let mut done = 0u64;
    while done < len {
        let chunk = (len - done).min(4096) as usize;
        if mem.write(dst + done, &buf[..chunk]) != chunk {
            return EFI_INVALID_PARAMETER;
        }
        done += chunk as u64;
    }
    EFI_SUCCESS
}

fn calculate_crc32(mem: &dyn GuestMem, data: u64, len: u64, p_out: u64) -> u64 {
    if data == 0 || len == 0 || p_out == 0 || len > MEM_OP_CAP_BYTES {
        return EFI_INVALID_PARAMETER;
    }
    let mut buf = [0u8; 512];
    let mut crc = crc32_start();
    let mut done = 0u64;
    while done < len {
        let chunk = (len - done).min(512) as usize;
        if mem.read(data + done, &mut buf[..chunk]) != chunk {
            return EFI_INVALID_PARAMETER;
        }
        crc = crc32_feed(crc, &buf[..chunk]);
        done += chunk as u64;
    }
    if write_u32(mem, p_out, crc32_finish(crc)) {
        EFI_SUCCESS
    } else {
        EFI_INVALID_PARAMETER
    }
}

fn read_guid(mem: &dyn GuestMem, addr: u64) -> Option<Guid> {
    if addr == 0 {
        return None;
    }
    let mut g = [0u8; 16];
    if mem.read(addr, &mut g) < 16 {
        return None;
    }
    Some(g)
}

/// `LocateHandle(SearchType, *Guid, SearchKey, *BufferSize, *Buffer)`.
fn locate_handle(st: &FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    let search = a.a1 as u32;
    if search == BY_PROTOCOL && a.a2 == 0 {
        return EFI_INVALID_PARAMETER;
    }
    if search != ALL_HANDLES && search != BY_PROTOCOL {
        return EFI_UNSUPPORTED; // ByRegisterNotify needs notify registration
    }
    let guid = read_guid(mem, a.a2);
    let Some(p_buf) = stack_arg(mem, a.rsp, 5) else {
        return EFI_INVALID_PARAMETER;
    };
    let p_size = a.a4;
    let Some(size) = read_u64(mem, p_size) else {
        return EFI_INVALID_PARAMETER;
    };
    let mut out = [0u64; MAX_HANDLES];
    let n = st.protocols.locate(search, guid.as_ref(), &mut out);
    if n == 0 {
        return PROTO_NOT_FOUND;
    }
    let need = n as u64 * 8;
    if p_buf == 0 || size < need {
        let _ = write_u64(mem, p_size, need);
        return PROTO_BUFFER_TOO_SMALL;
    }
    for i in 0..n {
        if !write_u64(mem, p_buf + i as u64 * 8, out[i]) {
            return EFI_INVALID_PARAMETER;
        }
    }
    let _ = write_u64(mem, p_size, need);
    EFI_SUCCESS
}

/// `LocateHandleBuffer(SearchType, *Guid, SearchKey, *NoHandles, **Buffer)` —
/// allocates the result array from our pool.
fn locate_handle_buffer(st: &mut FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    let search = a.a1 as u32;
    if search != ALL_HANDLES && search != BY_PROTOCOL {
        return EFI_UNSUPPORTED;
    }
    let guid = read_guid(mem, a.a2);
    let Some(p_buf) = stack_arg(mem, a.rsp, 5) else {
        return EFI_INVALID_PARAMETER;
    };
    if a.a4 == 0 || p_buf == 0 {
        return EFI_INVALID_PARAMETER;
    }
    let mut out = [0u64; MAX_HANDLES];
    let n = st.protocols.locate(search, guid.as_ref(), &mut out);
    if n == 0 {
        return PROTO_NOT_FOUND;
    }
    let bytes = n as u64 * 8;
    let pages = PagePool::pool_pages_for(bytes);
    let (status, base) = st.pool.allocate_pages(
        super::memory::ALLOCATE_ANY_PAGES,
        super::memory::EFI_BOOT_SERVICES_DATA,
        pages,
        0,
    );
    if status != EFI_SUCCESS {
        return status;
    }
    let arr = base + POOL_HEADER_BYTES;
    let ok = write_u64(mem, base, POOL_MAGIC)
        && write_u64(mem, base + 8, pages)
        && (0..n).all(|i| write_u64(mem, arr + i as u64 * 8, out[i]))
        && write_u64(mem, a.a4, n as u64)
        && write_u64(mem, p_buf, arr);
    if ok {
        EFI_SUCCESS
    } else {
        let _ = st.pool.free_pages_at(base, pages);
        EFI_INVALID_PARAMETER
    }
}

/// Most (GUID, interface) pairs one `InstallMultipleProtocolInterfaces` /
/// `UninstallMultipleProtocolInterfaces` call may carry (GRUB uses two).
pub const MAX_MULTI_PAIRS: usize = 8;

/// The variadic tail of `(Un)InstallMultipleProtocolInterfaces`, MS x64:
/// pair `k` (0-based) is `(arg 2k+2, arg 2k+3)` where args 1–4 are registers
/// and 5+ are on the stack. Ends at the first NULL GUID pointer. Returns the
/// pairs collected, or `None` on a malformed list (unreadable stack, too
/// many pairs, GUID unreadable).
fn collect_multi_pairs(
    mem: &dyn GuestMem,
    a: ServiceArgs,
    out: &mut [(Guid, u64); MAX_MULTI_PAIRS],
) -> Option<usize> {
    let arg = |n: u32| -> Option<u64> {
        match n {
            1 => Some(a.a1),
            2 => Some(a.a2),
            3 => Some(a.a3),
            4 => Some(a.a4),
            _ => stack_arg(mem, a.rsp, n),
        }
    };
    let mut n = 0usize;
    loop {
        let gp = arg(2 + 2 * n as u32)?;
        if gp == 0 {
            return Some(n);
        }
        if n == MAX_MULTI_PAIRS {
            return None;
        }
        let g = read_guid(mem, gp)?;
        let iface = arg(3 + 2 * n as u32)?;
        out[n] = (g, iface);
        n += 1;
    }
}

/// `InstallMultipleProtocolInterfaces(*Handle, Guid*, Interface, …, NULL)`
/// (UEFI 2.10 §7.3). `*Handle == NULL` mints a new handle. All-or-nothing:
/// a failure rolls back the pairs already installed. GRUB's Linux loader
/// uses this to publish `LoadFile2` + a vendor `DevicePath` for the initrd
/// (nested `2d34fff`: `id=0x226 → EFI_UNSUPPORTED` was the last thing before
/// "Press any key to continue...").
fn install_multiple(st: &mut FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    if a.a1 == 0 {
        return EFI_INVALID_PARAMETER;
    }
    let Some(mut handle) = read_u64(mem, a.a1) else {
        return EFI_INVALID_PARAMETER;
    };
    let mut pairs = [([0u8; 16], 0u64); MAX_MULTI_PAIRS];
    let Some(n) = collect_multi_pairs(mem, a, &mut pairs) else {
        return EFI_INVALID_PARAMETER;
    };
    if n == 0 {
        return EFI_INVALID_PARAMETER;
    }
    // A NULL interface is legal only for protocols that carry no interface
    // (spec); a real pointer is what loaders pass. Refuse NULL up front so a
    // half-installed handle never exists.
    if pairs[..n].iter().any(|(_, iface)| *iface == 0) {
        return EFI_INVALID_PARAMETER;
    }
    let minted = handle == 0;
    if minted {
        match st.protocols.new_handle() {
            Some(h) => handle = h,
            None => return EFI_OUT_OF_RESOURCES,
        }
    }
    for i in 0..n {
        let (g, iface) = pairs[i];
        let s = st.protocols.install(handle, g, iface);
        if s != EFI_SUCCESS {
            for (g2, iface2) in pairs[..i].iter() {
                let _ = st.protocols.uninstall(handle, g2, *iface2);
            }
            return s;
        }
    }
    if minted && !write_u64(mem, a.a1, handle) {
        for (g, iface) in pairs[..n].iter() {
            let _ = st.protocols.uninstall(handle, g, *iface);
        }
        return EFI_INVALID_PARAMETER;
    }
    EFI_SUCCESS
}

/// `UninstallMultipleProtocolInterfaces(Handle, Guid*, Interface, …, NULL)`.
/// Every pair must be present with that interface; otherwise nothing is
/// removed and `EFI_INVALID_PARAMETER` is returned (spec).
fn uninstall_multiple(st: &mut FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    if a.a1 == 0 {
        return EFI_INVALID_PARAMETER;
    }
    let mut pairs = [([0u8; 16], 0u64); MAX_MULTI_PAIRS];
    let Some(n) = collect_multi_pairs(mem, a, &mut pairs) else {
        return EFI_INVALID_PARAMETER;
    };
    if n == 0 {
        return EFI_INVALID_PARAMETER;
    }
    if pairs[..n]
        .iter()
        .any(|(g, iface)| st.protocols.interface_for(a.a1, g) != Some(*iface))
    {
        return EFI_INVALID_PARAMETER;
    }
    for (g, iface) in pairs[..n].iter() {
        let _ = st.protocols.uninstall(a.a1, g, *iface);
    }
    EFI_SUCCESS
}

/// `InstallConfigurationTable(Guid*, Table)` (UEFI 2.10 §7.3): add,
/// replace (same GUID) or remove (`Table == NULL`) an entry in the array
/// behind `SystemTable->ConfigurationTable`, update `NumberOfTableEntries`
/// and re-CRC the system table header. The Linux EFI stub registers the
/// initrd it loaded this way (nested `166377a`: `id=0x215 → UNSUPPORTED` →
/// "Failed to load initrd").
fn install_configuration_table(st: &mut FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    use super::tables::{
        CONFIG_TABLE_ENTRY_BYTES, CONFIG_TABLE_MAX_ENTRIES, SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF,
        SYSTEM_TABLE_SIZE,
    };
    let Some(guid) = read_guid(mem, a.a1) else {
        return EFI_INVALID_PARAMETER;
    };
    let table = a.a2;
    if st.config_table == 0 || st.system_table == 0 {
        return EFI_UNSUPPORTED;
    }
    let Some(count) = read_u64(mem, st.system_table + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64)
    else {
        return EFI_INVALID_PARAMETER;
    };
    let count = (count as usize).min(CONFIG_TABLE_MAX_ENTRIES);
    let entry = |i: usize| st.config_table + (i * CONFIG_TABLE_ENTRY_BYTES) as u64;
    let mut found = None;
    for i in 0..count {
        if read_guid(mem, entry(i)) == Some(guid) {
            found = Some(i);
            break;
        }
    }
    let new_count = match (found, table) {
        (Some(i), 0) => {
            // Remove: shift the tail down one slot.
            let mut buf = [0u8; CONFIG_TABLE_ENTRY_BYTES];
            for j in i + 1..count {
                if mem.read(entry(j), &mut buf) != CONFIG_TABLE_ENTRY_BYTES
                    || mem.write(entry(j - 1), &buf) != CONFIG_TABLE_ENTRY_BYTES
                {
                    return EFI_INVALID_PARAMETER;
                }
            }
            let zero = [0u8; CONFIG_TABLE_ENTRY_BYTES];
            let _ = mem.write(entry(count - 1), &zero);
            count - 1
        }
        (Some(i), t) => {
            if !write_u64(mem, entry(i) + 16, t) {
                return EFI_INVALID_PARAMETER;
            }
            count
        }
        (None, 0) => return PROTO_NOT_FOUND,
        (None, t) => {
            if count >= CONFIG_TABLE_MAX_ENTRIES {
                return EFI_OUT_OF_RESOURCES;
            }
            if mem.write(entry(count), &guid) != 16 || !write_u64(mem, entry(count) + 16, t) {
                return EFI_INVALID_PARAMETER;
            }
            count + 1
        }
    };
    if !write_u64(
        mem,
        st.system_table + SYSTEM_TABLE_NUM_TABLE_ENTRIES_OFF as u64,
        new_count as u64,
    ) {
        return EFI_INVALID_PARAMETER;
    }
    // Header CRC32 covers the whole EFI_SYSTEM_TABLE with the CRC field zero.
    let mut tbl = [0u8; SYSTEM_TABLE_SIZE];
    if mem.read(st.system_table, &mut tbl) != SYSTEM_TABLE_SIZE {
        return EFI_INVALID_PARAMETER;
    }
    tbl[16..20].copy_from_slice(&0u32.to_le_bytes());
    let crc = crc32_finish(crc32_feed(crc32_start(), &tbl));
    if !write_u32(mem, st.system_table + 16, crc) {
        return EFI_INVALID_PARAMETER;
    }
    EFI_SUCCESS
}

/// Longest device path we will walk (bytes) and its node cap.
pub const MAX_DEVICE_PATH_BYTES: usize = 512;
pub const MAX_DEVICE_PATH_NODES: usize = 16;

/// Byte length of a guest device path up to (not including) its End-Entire
/// node, or `None` if malformed / unterminated within the caps.
fn device_path_len(mem: &dyn GuestMem, dp: u64) -> Option<usize> {
    let mut off = 0usize;
    for _ in 0..MAX_DEVICE_PATH_NODES {
        let mut hdr = [0u8; 4];
        if mem.read(dp + off as u64, &mut hdr) < 4 {
            return None;
        }
        let len = usize::from(u16::from_le_bytes([hdr[2], hdr[3]]));
        if hdr[0] == super::protocol::DP_TYPE_END {
            // End-Entire terminates; an End-Instance (0x01) would need
            // multi-instance paths we do not model.
            return if hdr[1] == super::protocol::DP_SUBTYPE_END_ENTIRE { Some(off) } else { None };
        }
        if len < 4 || off + len > MAX_DEVICE_PATH_BYTES {
            return None;
        }
        off += len;
    }
    None
}

/// `LocateDevicePath(Guid*, DevicePath**, Handle*)` (UEFI 2.10 §7.3): the
/// handle supporting `Guid` whose device path is the longest prefix of
/// `*DevicePath`; `*DevicePath` is advanced to the first unmatched node.
/// This is how the Linux EFI stub finds GRUB's initrd `LoadFile2` handle
/// (vendor media path `5568E427-68FC-4F3D-AC74-CA555231CC68`).
fn locate_device_path(st: &FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> u64 {
    let Some(guid) = read_guid(mem, a.a1) else {
        return EFI_INVALID_PARAMETER;
    };
    if a.a2 == 0 || a.a3 == 0 {
        return EFI_INVALID_PARAMETER;
    }
    let Some(want) = read_u64(mem, a.a2) else {
        return EFI_INVALID_PARAMETER;
    };
    let Some(want_len) = device_path_len(mem, want) else {
        return EFI_INVALID_PARAMETER;
    };
    let mut handles = [0u64; MAX_HANDLES];
    let n = st.protocols.locate(BY_PROTOCOL, Some(&guid), &mut handles);
    let mut best: Option<(u64, usize)> = None;
    for &h in &handles[..n] {
        let Some(hdp) = st.protocols.interface_for(h, &super::protocol::GUID_DEVICE_PATH) else {
            continue;
        };
        let Some(hlen) = device_path_len(mem, hdp) else {
            continue;
        };
        if hlen > want_len {
            continue;
        }
        // Byte-compare the handle's whole path against the request's prefix.
        let mut same = true;
        let mut off = 0usize;
        let mut x = [0u8; 64];
        let mut y = [0u8; 64];
        while off < hlen {
            let k = (hlen - off).min(64);
            if mem.read(hdp + off as u64, &mut x[..k]) < k
                || mem.read(want + off as u64, &mut y[..k]) < k
                || x[..k] != y[..k]
            {
                same = false;
                break;
            }
            off += k;
        }
        if same && best.map_or(true, |(_, l)| hlen > l) {
            best = Some((h, hlen));
        }
    }
    let Some((h, matched)) = best else {
        return PROTO_NOT_FOUND;
    };
    if write_u64(mem, a.a3, h) && write_u64(mem, a.a2, want + matched as u64) {
        EFI_SUCCESS
    } else {
        EFI_INVALID_PARAMETER
    }
}

/// `ReadBlocks` / `WriteBlocks`: `(This, MediaId, Lba, BufferSize, *Buffer)`.
fn block_transfer(
    st: &mut FirmwareState,
    mem: &dyn GuestMem,
    a: ServiceArgs,
    write: bool,
) -> u64 {
    let Some(buffer) = stack_arg(mem, a.rsp, 5) else {
        return EFI_INVALID_PARAMETER;
    };
    let Some(media) = st.media_for(a.a1).copied() else {
        return EFI_INVALID_PARAMETER;
    };
    let media_id = a.a2 as u32;
    let lba = a.a3;
    let size = a.a4;
    let v = validate_transfer(&media, media_id, lba, size, buffer, write);
    if v != EFI_SUCCESS || size == 0 {
        return v;
    }
    let off = lba_offset(&media, lba);
    let mut buf = [0u8; 4096];
    let mut done = 0u64;
    while done < size {
        let chunk = (size - done).min(4096) as usize;
        if write {
            let Some(w) = st.write_blocks else {
                return BLK_DEVICE_ERROR;
            };
            if mem.read(buffer + done, &mut buf[..chunk]) != chunk
                || !w(media_id, off + done, &buf[..chunk])
            {
                return BLK_DEVICE_ERROR;
            }
        } else {
            let Some(r) = st.read_blocks else {
                return BLK_DEVICE_ERROR;
            };
            if !r(media_id, off + done, &mut buf[..chunk])
                || mem.write(buffer + done, &buf[..chunk]) != chunk
            {
                return BLK_DEVICE_ERROR;
            }
        }
        done += chunk as u64;
    }
    if write {
        st.block_writes = st.block_writes.saturating_add(1);
    } else {
        st.block_reads = st.block_reads.saturating_add(1);
    }
    let _ = MAX_TRANSFER_BYTES;
    EFI_SUCCESS
}

/// A `VolumeRead` over the CD backing store, offset by the FAT volume's
/// extent inside the ISO (the El Torito boot image).
struct FatVolumeReader<'a> {
    read: BlockReadFn,
    base: u64,
    media_id: u32,
    _mem: &'a (),
}

impl super::fat::VolumeRead for FatVolumeReader<'_> {
    fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
        (self.read)(self.media_id, self.base + off, buf)
    }
}

/// `This` pointer → open-file slot index.
fn file_slot_of(st: &FirmwareState, this: u64) -> Option<usize> {
    let base = st.file_proto_base;
    if base == 0 || this < base {
        return None;
    }
    let stride = super::tables::IMAGE_FILE_PROTO_STRIDE as u64;
    let d = this - base;
    if d % stride != 0 {
        return None;
    }
    let slot = (d / stride) as usize;
    if slot < FILE_SLOTS {
        Some(slot)
    } else {
        None
    }
}

/// Read a NUL-terminated CHAR16 path from the guest into ASCII.
fn read_guest_path(mem: &dyn GuestMem, addr: u64, out: &mut [u8; MAX_PATH_BYTES]) -> Option<usize> {
    if addr == 0 {
        return None;
    }
    let mut units = [0u16; MAX_PATH_BYTES];
    for (i, u) in units.iter_mut().enumerate() {
        let mut b = [0u8; 2];
        if mem.read(addr + i as u64 * 2, &mut b) < 2 {
            return None;
        }
        *u = u16::from_le_bytes(b);
        if *u == 0 {
            break;
        }
    }
    super::filesystem::utf16_path_to_ascii(&units, out)
}

/// `LoadImage(BootPolicy, Parent, DevicePath, SourceBuffer, SourceSize, *ImageHandle)`.
/// Publish `EFI_LOADED_IMAGE_PROTOCOL` on `st.image_handle` for the image
/// described by `st.image_*` (UEFI 2.10 §9.1). A loader's first call is
/// `OpenProtocol(ImageHandle, LoadedImage)` to find the volume it booted
/// from, so the launcher must call this for a directly-staged image too, not
/// only `LoadImage` (GRUB on nested `7ee3a3b`: two `EFI_NOT_FOUND` before
/// anything else). Returns `false` if the struct could not be written or no
/// struct slot was published in the firmware tables.
pub fn publish_loaded_image(st: &mut FirmwareState, mem: &dyn GuestMem, parent: u64) -> bool {
    let li = st.loaded_image_proto;
    if li == 0 || st.image_handle == 0 {
        return false;
    }
    let ok = write_u32(mem, li + super::protocol::LOADED_IMAGE_REVISION_OFF as u64, super::protocol::LOADED_IMAGE_REVISION)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_PARENT_OFF as u64, parent)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_SYSTEM_TABLE_OFF as u64, st.system_table)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_IMAGE_BASE_OFF as u64, st.image_base)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_DEVICE_HANDLE_OFF as u64, st.device_handle)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_FILE_PATH_OFF as u64, st.device_path)
        && write_u64(mem, li + super::protocol::LOADED_IMAGE_IMAGE_SIZE_OFF as u64, st.image_size);
    if !ok {
        return false;
    }
    let _ = st.protocols.install(st.image_handle, super::protocol::GUID_LOADED_IMAGE, li);
    true
}

/// Only the `SourceBuffer` form is supported; a `DevicePath`-only load needs
/// device-path parsing we do not publish (honest `EFI_UNSUPPORTED`).
fn load_image(st: &mut FirmwareState, mem: &dyn GuestMem, a: ServiceArgs) -> (u64, bool) {
    let Some(src_size) = stack_arg(mem, a.rsp, 5) else {
        return (EFI_INVALID_PARAMETER, false);
    };
    let Some(p_handle) = stack_arg(mem, a.rsp, 6) else {
        return (EFI_INVALID_PARAMETER, false);
    };
    let src = a.a4;
    if src == 0 || src_size == 0 {
        return (EFI_UNSUPPORTED, false);
    }
    if p_handle == 0 {
        return (EFI_INVALID_PARAMETER, false);
    }
    // Parse enough to size the image, then allocate from our pool.
    let mut hdr = [0u8; super::pe::MAX_HEADER_BYTES];
    let take = (src_size as usize).min(super::pe::MAX_HEADER_BYTES);
    if take < 0x40 || mem.read(src, &mut hdr[..take]) < take {
        return (EFI_INVALID_PARAMETER, false);
    }
    let Ok(pe) = super::pe::parse_pe32plus(&hdr[..take]) else {
        // Not a PE32+ x64 EFI application.
        return (0x8000_0000_0000_0012, false); // EFI_LOAD_ERROR
    };
    let pages = (u64::from(pe.size_of_image) + 4095) / 4096;
    let (status, base) = st.pool.allocate_pages(
        super::memory::ALLOCATE_ANY_PAGES,
        super::memory::EFI_LOADER_CODE,
        pages,
        0,
    );
    if status != EFI_SUCCESS {
        return (status, false);
    }
    let loaded = match super::pe::load_pe32plus_guest(mem, src, src_size, base, pages * 4096) {
        Ok(l) => l,
        Err(_) => {
            let _ = st.pool.free_pages_at(base, pages);
            return (0x8000_0000_0000_0012, false); // EFI_LOAD_ERROR
        }
    };
    st.image_base = loaded.load_base;
    st.image_size = u64::from(loaded.size_of_image);
    st.image_entry = loaded.entry;
    st.image_handle = super::protocol::HANDLE_IMAGE;
    if !publish_loaded_image(st, mem, a.a2) {
        let _ = st.pool.free_pages_at(base, pages);
        return (EFI_INVALID_PARAMETER, false);
    }
    if !write_u64(mem, p_handle, st.image_handle) {
        let _ = st.pool.free_pages_at(base, pages);
        return (EFI_INVALID_PARAMETER, false);
    }
    (EFI_SUCCESS, true)
}

/// Dispatch one RayNu-F service call. Console, memory, event/timer, TPL,
/// misc boot services, the handle/protocol database and `BlockIo` are
/// implemented; image services (`LoadImage`/`StartImage`),
/// `SimpleFileSystem` and runtime services are `EFI_UNSUPPORTED` (F5).
pub fn dispatch(
    id: ServiceId,
    args: ServiceArgs,
    mem: &dyn GuestMem,
    sink: &mut dyn ConsoleSink,
    st: &mut FirmwareState,
    clock: &dyn TimeSource,
    slab_bytes: u64,
) -> Dispatched {
    let mut out = Dispatched {
        id,
        status: EFI_UNSUPPORTED,
        chars_out: 0,
        wait: None,
        alloc_ok: false,
        exited_boot_services: false,
        block_io_ok: false,
        file_read_ok: false,
        image_loaded: false,
        start_image: None,
        exit_image: None,
    };
    let a = args;
    out.status = match id {
        // ---- console ---------------------------------------------------
        ServiceId::ConOutReset => EFI_SUCCESS,
        ServiceId::ConOutOutputString | ServiceId::ConOutTestString => {
            if a.a2 == 0 {
                EFI_INVALID_PARAMETER
            } else if id == ServiceId::ConOutTestString {
                EFI_SUCCESS
            } else {
                out.chars_out = output_string(mem, sink, a.a2);
                EFI_SUCCESS
            }
        }
        ServiceId::ConOutSetMode => {
            if a.a2 == 0 {
                EFI_SUCCESS
            } else {
                EFI_UNSUPPORTED
            }
        }
        ServiceId::ConOutSetAttribute
        | ServiceId::ConOutClearScreen
        | ServiceId::ConOutSetCursorPosition
        | ServiceId::ConOutEnableCursor => EFI_SUCCESS,
        ServiceId::ConOutQueryMode => {
            // (This, ModeNumber, *Columns, *Rows): one 80x25 mode.
            if a.a2 != 0 || a.a3 == 0 || a.a4 == 0 {
                EFI_UNSUPPORTED
            } else if write_u64(mem, a.a3, 80) && write_u64(mem, a.a4, 25) {
                EFI_SUCCESS
            } else {
                EFI_INVALID_PARAMETER
            }
        }
        ServiceId::ConInReset => EFI_SUCCESS,
        ServiceId::ConInReadKeyStroke => {
            // (This, *EFI_INPUT_KEY{ScanCode u16, UnicodeChar u16})
            if a.a2 == 0 {
                EFI_INVALID_PARAMETER
            } else if let Some(b) = sink.read_input() {
                let key = [0u8, 0, b, 0];
                if mem.write(a.a2, &key) == 4 {
                    EFI_SUCCESS
                } else {
                    EFI_INVALID_PARAMETER
                }
            } else {
                EFI_NOT_READY
            }
        }

        // ---- TPL -------------------------------------------------------
        ServiceId::RaiseTPL => st.events.raise_tpl(a.a1),
        ServiceId::RestoreTPL => {
            st.events.restore_tpl(a.a1);
            EFI_SUCCESS
        }

        // ---- memory ----------------------------------------------------
        ServiceId::AllocatePages => {
            // (Type, MemoryType, Pages, *Memory)
            let Some(cur) = read_u64(mem, a.a4) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            let (status, addr) = st.pool.allocate_pages(a.a1 as u32, a.a2 as u32, a.a3, cur);
            if status == EFI_SUCCESS {
                if write_u64(mem, a.a4, addr) {
                    out.alloc_ok = true;
                    EFI_SUCCESS
                } else {
                    let _ = st.pool.free_pages_at(addr, a.a3);
                    EFI_INVALID_PARAMETER
                }
            } else {
                status
            }
        }
        ServiceId::FreePages => st.pool.free_pages_at(a.a1, a.a2),
        ServiceId::AllocatePool => {
            // (PoolType, Size, **Buffer)
            if a.a3 == 0 {
                EFI_INVALID_PARAMETER
            } else {
                let pages = PagePool::pool_pages_for(a.a2);
                let (status, base) =
                    st.pool.allocate_pages(super::memory::ALLOCATE_ANY_PAGES, a.a1 as u32, pages, 0);
                if status != EFI_SUCCESS {
                    status
                } else {
                    let ok = write_u64(mem, base, POOL_MAGIC)
                        && write_u64(mem, base + 8, pages)
                        && write_u64(mem, a.a3, base + POOL_HEADER_BYTES);
                    if ok {
                        out.alloc_ok = true;
                        EFI_SUCCESS
                    } else {
                        let _ = st.pool.free_pages_at(base, pages);
                        EFI_INVALID_PARAMETER
                    }
                }
            }
        }
        ServiceId::FreePool => {
            if a.a1 < POOL_HEADER_BYTES {
                EFI_INVALID_PARAMETER
            } else {
                let base = a.a1 - POOL_HEADER_BYTES;
                match (read_u64(mem, base), read_u64(mem, base + 8)) {
                    (Some(m), Some(pages)) if m == POOL_MAGIC => {
                        let _ = write_u64(mem, base, 0);
                        st.pool.free_pages_at(base, pages)
                    }
                    _ => EFI_INVALID_PARAMETER,
                }
            }
        }
        ServiceId::GetMemoryMap => get_memory_map(st, mem, a, slab_bytes),
        ServiceId::ExitBootServices => {
            let s = st.pool.exit_boot_services(a.a2);
            out.exited_boot_services = s == EFI_SUCCESS;
            s
        }

        // ---- events / timers ------------------------------------------
        ServiceId::CreateEvent | ServiceId::CreateEventEx => {
            // CreateEvent(Type, NotifyTpl, NotifyFn, NotifyCtx, *Event)
            // CreateEventEx(Type, NotifyTpl, NotifyFn, NotifyCtx, *Group, *Event)
            let n = if id == ServiceId::CreateEvent { 5 } else { 6 };
            let Some(p_event) = stack_arg(mem, a.rsp, n) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            if p_event == 0 {
                EFI_INVALID_PARAMETER
            } else {
                let (status, h) = st.events.create(a.a1 as u32, a.a2, a.a3, a.a4);
                if status == EFI_SUCCESS {
                    if write_u64(mem, p_event, h) {
                        EFI_SUCCESS
                    } else {
                        let _ = st.events.close(h);
                        EFI_INVALID_PARAMETER
                    }
                } else {
                    status
                }
            }
        }
        ServiceId::SetTimer => st.events.set_timer(a.a1, a.a2 as u32, a.a3, clock.now_100ns()),
        ServiceId::SignalEvent => st.events.signal(a.a1),
        ServiceId::CloseEvent => st.events.close(a.a1),
        ServiceId::CheckEvent => st.events.check(a.a1, clock.now_100ns(), sink.has_input()),
        ServiceId::WaitForEvent => {
            // (NumberOfEvents, *Events, *Index)
            let n = a.a1 as usize;
            if n == 0 || n > 64 || a.a2 == 0 {
                EFI_INVALID_PARAMETER
            } else {
                let mut handles = [0u64; 64];
                let mut ok = true;
                for i in 0..n {
                    match read_u64(mem, a.a2 + (i as u64) * 8) {
                        Some(h) => handles[i] = h,
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if !ok {
                    EFI_INVALID_PARAMETER
                } else {
                    let input = || sink.has_input();
                    let (status, idx, outcome) = st.events.wait(&handles[..n], clock, &input);
                    out.wait = Some(outcome);
                    if status == EFI_SUCCESS && a.a3 != 0 {
                        let _ = write_u64(mem, a.a3, idx);
                    }
                    status
                }
            }
        }

        // ---- misc ------------------------------------------------------
        ServiceId::GetNextMonotonicCount => {
            let c = st.events.next_monotonic();
            if write_u64(mem, a.a1, c) {
                EFI_SUCCESS
            } else {
                EFI_INVALID_PARAMETER
            }
        }
        ServiceId::Stall => super::events::stall(clock, a.a1),
        ServiceId::SetWatchdogTimer => {
            st.watchdog_sets = st.watchdog_sets.saturating_add(1);
            EFI_SUCCESS
        }
        ServiceId::CalculateCrc32 => calculate_crc32(mem, a.a1, a.a2, a.a3),
        ServiceId::CopyMem => copy_mem(mem, a.a1, a.a2, a.a3),
        ServiceId::SetMem => set_mem(mem, a.a1, a.a2, a.a3 as u8),

        // ---- handles / protocols (F4) ----------------------------------
        ServiceId::HandleProtocol => {
            // (Handle, *Guid, **Interface)
            match read_guid(mem, a.a2) {
                Some(g) if a.a3 != 0 => match st.protocols.interface_for(a.a1, &g) {
                    Some(iface) if write_u64(mem, a.a3, iface) => EFI_SUCCESS,
                    Some(_) => EFI_INVALID_PARAMETER,
                    None => PROTO_NOT_FOUND,
                },
                _ => EFI_INVALID_PARAMETER,
            }
        }
        ServiceId::OpenProtocol => {
            // (Handle, *Guid, **Interface, AgentHandle, ControllerHandle, Attributes)
            // Agent/controller bookkeeping and BY_CHILD/BY_DRIVER are not
            // modelled; a GET_PROTOCOL-style open is what loaders need.
            match read_guid(mem, a.a2) {
                Some(g) => match st.protocols.interface_for(a.a1, &g) {
                    Some(iface) => {
                        if a.a3 == 0 || write_u64(mem, a.a3, iface) {
                            EFI_SUCCESS
                        } else {
                            EFI_INVALID_PARAMETER
                        }
                    }
                    None => PROTO_NOT_FOUND,
                },
                None => EFI_INVALID_PARAMETER,
            }
        }
        ServiceId::CloseProtocol => EFI_SUCCESS,
        ServiceId::LocateProtocol => {
            // (*Guid, Registration, **Interface)
            match read_guid(mem, a.a1) {
                Some(g) if a.a3 != 0 => match st.protocols.first_interface(&g) {
                    Some(iface) if write_u64(mem, a.a3, iface) => EFI_SUCCESS,
                    Some(_) => EFI_INVALID_PARAMETER,
                    None => PROTO_NOT_FOUND,
                },
                _ => EFI_INVALID_PARAMETER,
            }
        }
        ServiceId::LocateHandle => locate_handle(st, mem, a),
        ServiceId::LocateHandleBuffer => locate_handle_buffer(st, mem, a),
        ServiceId::LocateDevicePath => locate_device_path(st, mem, a),
        ServiceId::InstallConfigurationTable => install_configuration_table(st, mem, a),
        ServiceId::InstallMultipleProtocolInterfaces => install_multiple(st, mem, a),
        ServiceId::UninstallMultipleProtocolInterfaces => uninstall_multiple(st, mem, a),
        ServiceId::InstallProtocolInterface => {
            // (*Handle, *Guid, InterfaceType, Interface)
            let Some(handle) = read_u64(mem, a.a1) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            match read_guid(mem, a.a2) {
                // Only EFI_NATIVE_INTERFACE (0) exists.
                Some(g) if a.a3 == 0 && handle != 0 => st.protocols.install(handle, g, a.a4),
                _ => EFI_INVALID_PARAMETER,
            }
        }

        // ---- BlockIo (F4) ----------------------------------------------
        ServiceId::BlockIoReset => {
            if st.media_for(a.a1).is_some() {
                EFI_SUCCESS
            } else {
                EFI_INVALID_PARAMETER
            }
        }
        ServiceId::BlockIoReadBlocks => block_transfer(st, mem, a, false),
        ServiceId::BlockIoWriteBlocks => block_transfer(st, mem, a, true),
        ServiceId::BlockIoFlushBlocks => {
            // Writes reach the backing store synchronously (WriteCaching=0).
            if st.media_for(a.a1).is_some() {
                EFI_SUCCESS
            } else {
                EFI_INVALID_PARAMETER
            }
        }

        // ---- SimpleFileSystem / EFI_FILE_PROTOCOL (F5) -------------------
        ServiceId::SfsOpenVolume => {
            // (This, **Root)
            if a.a1 == 0 || a.a1 != st.sfs || a.a2 == 0 {
                EFI_INVALID_PARAMETER
            } else {
                let (status, handle) = st.fs.open_volume();
                if status == EFI_SUCCESS {
                    let slot = (handle & 0xFFF) as u64;
                    let this = st.file_proto_base
                        + slot * super::tables::IMAGE_FILE_PROTO_STRIDE as u64;
                    if write_u64(mem, a.a2, this) {
                        EFI_SUCCESS
                    } else {
                        let _ = st.fs.close(handle);
                        EFI_INVALID_PARAMETER
                    }
                } else {
                    status
                }
            }
        }
        ServiceId::FileOpen => {
            // (This, **New, FileName, OpenMode, Attributes) — 5th on stack.
            let Some(slot) = file_slot_of(st, a.a1) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            let mut path = [0u8; MAX_PATH_BYTES];
            let Some(n) = read_guest_path(mem, a.a3, &mut path) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            let Some(read) = st.read_blocks else {
                return finish(out, EFI_DEVICE_ERROR);
            };
            let r = FatVolumeReader {
                read,
                base: st.fat_volume_off,
                media_id: super::blockio::MEDIA_ID_CD,
                _mem: &(),
            };
            let this_handle = FileSystem::handle_for(slot);
            let (status, handle) = st.fs.open(this_handle, &path[..n], a.a4, &r);
            if status == EFI_SUCCESS {
                let ns = (handle & 0xFFF) as u64;
                let newthis =
                    st.file_proto_base + ns * super::tables::IMAGE_FILE_PROTO_STRIDE as u64;
                if a.a2 != 0 && write_u64(mem, a.a2, newthis) {
                    EFI_SUCCESS
                } else {
                    let _ = st.fs.close(handle);
                    EFI_INVALID_PARAMETER
                }
            } else {
                status
            }
        }
        ServiceId::FileClose => match file_slot_of(st, a.a1) {
            Some(slot) => st.fs.close(FileSystem::handle_for(slot)),
            None => EFI_INVALID_PARAMETER,
        },
        ServiceId::FileRead => {
            // (This, *BufferSize, Buffer)
            let Some(slot) = file_slot_of(st, a.a1) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            let Some(want) = read_u64(mem, a.a2) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            let Some(read) = st.read_blocks else {
                return finish(out, EFI_DEVICE_ERROR);
            };
            let r = FatVolumeReader {
                read,
                base: st.fat_volume_off,
                media_id: super::blockio::MEDIA_ID_CD,
                _mem: &(),
            };
            let h = FileSystem::handle_for(slot);
            let mut buf = [0u8; 4096];
            let mut done = 0u64;
            let mut status = EFI_SUCCESS;
            while done < want {
                let chunk = (want - done).min(4096) as usize;
                let (s, n) = st.fs.read(h, &mut buf[..chunk], &r);
                if s != EFI_SUCCESS {
                    status = s;
                    break;
                }
                if n == 0 {
                    break; // EOF
                }
                if a.a3 == 0 || mem.write(a.a3 + done, &buf[..n]) != n {
                    status = EFI_INVALID_PARAMETER;
                    break;
                }
                done += n as u64;
                if n < chunk {
                    break;
                }
            }
            if status == EFI_SUCCESS {
                if write_u64(mem, a.a2, done) {
                    out.file_read_ok = done > 0;
                    EFI_SUCCESS
                } else {
                    EFI_INVALID_PARAMETER
                }
            } else {
                status
            }
        }
        ServiceId::FileGetPosition => match file_slot_of(st, a.a1) {
            Some(slot) => match st.fs.position(FileSystem::handle_for(slot)) {
                Some(p) if a.a2 != 0 && write_u64(mem, a.a2, p) => EFI_SUCCESS,
                _ => EFI_INVALID_PARAMETER,
            },
            None => EFI_INVALID_PARAMETER,
        },
        ServiceId::FileSetPosition => match file_slot_of(st, a.a1) {
            Some(slot) => st.fs.set_position(FileSystem::handle_for(slot), a.a2),
            None => EFI_INVALID_PARAMETER,
        },
        ServiceId::FileGetInfo => {
            // (This, *InfoType, *BufferSize, Buffer) — only EFI_FILE_INFO.
            let Some(slot) = file_slot_of(st, a.a1) else {
                return finish(out, EFI_INVALID_PARAMETER);
            };
            match read_guid(mem, a.a2) {
                Some(g) if g == super::filesystem::GUID_FILE_INFO => {
                    let Some(size) = read_u64(mem, a.a3) else {
                        return finish(out, EFI_INVALID_PARAMETER);
                    };
                    let mut info = [0u8; 512];
                    let cap = (size as usize).min(info.len());
                    let (status, need) =
                        st.fs.file_info(FileSystem::handle_for(slot), &mut info[..cap]);
                    let _ = write_u64(mem, a.a3, need);
                    if status == EFI_SUCCESS {
                        if a.a4 != 0 && mem.write(a.a4, &info[..need as usize]) == need as usize {
                            EFI_SUCCESS
                        } else {
                            EFI_INVALID_PARAMETER
                        }
                    } else {
                        status
                    }
                }
                Some(_) => EFI_UNSUPPORTED, // FileSystemInfo / VolumeLabel not published
                None => EFI_INVALID_PARAMETER,
            }
        }
        // Read-only volume: these are refused honestly, not silently ignored.
        ServiceId::FileWrite | ServiceId::FileDelete | ServiceId::FileSetInfo => {
            if file_slot_of(st, a.a1).is_some() {
                0x8000_0000_0000_0008 // EFI_WRITE_PROTECTED
            } else {
                EFI_INVALID_PARAMETER
            }
        }
        ServiceId::FileFlush => {
            if file_slot_of(st, a.a1).is_some() {
                EFI_SUCCESS
            } else {
                EFI_INVALID_PARAMETER
            }
        }

        // ---- image services (F5) ---------------------------------------
        ServiceId::LoadImage => {
            let (status, ok) = load_image(st, mem, a);
            out.image_loaded = ok;
            status
        }
        ServiceId::StartImage => {
            // (ImageHandle, *ExitDataSize, **ExitData)
            if st.image_entry == 0 || a.a1 != st.image_handle {
                EFI_INVALID_PARAMETER
            } else {
                // Zero the optional ExitData out-params; we never produce any.
                if a.a2 != 0 {
                    let _ = write_u64(mem, a.a2, 0);
                }
                if a.a3 != 0 {
                    let _ = write_u64(mem, a.a3, 0);
                }
                out.start_image = Some((st.image_entry, st.image_handle));
                st.image_started = true;
                EFI_SUCCESS
            }
        }
        ServiceId::Exit => {
            // (ImageHandle, ExitStatus, ExitDataSize, *ExitData). The Linux
            // EFI stub calls this when efi_stub_entry() fails (nested
            // 166377a: UNSUPPORTED here left it to fall off the end → #GP).
            // The image's pages stay allocated (honest gap: no unload).
            if a.a1 != st.image_handle || !st.image_started {
                EFI_INVALID_PARAMETER
            } else {
                st.image_started = false;
                out.exit_image = Some(a.a2);
                EFI_SUCCESS
            }
        }

        // ---- not yet: Exit/UnloadImage, runtime services ----------------
        _ => EFI_UNSUPPORTED,
    };
    out.block_io_ok = matches!(
        id,
        ServiceId::BlockIoReadBlocks | ServiceId::BlockIoWriteBlocks
    ) && out.status == EFI_SUCCESS;
    // Keep the pub consts referenced so the event-type API stays a single
    // source of truth for tests.
    let _ = (EVT_TIMER, EVT_NOTIFY_WAIT, EVT_NOTIFY_SIGNAL, TPL_HIGH_LEVEL, EFI_DEVICE_ERROR, EFI_NOT_FOUND);
    out
}

fn finish(mut out: Dispatched, status: u64) -> Dispatched {
    out.status = status;
    out
}
