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
use super::tables::{crc32_feed, crc32_finish, crc32_start};

/// `'R' 'F'` — the RayNu-F service I/O port. Unused by OVMF / QEMU / PC legacy.
pub const RAYNU_F_SERVICE_PORT: u16 = 0x5246;
/// Each trampoline slot is 16 bytes (14 used + 2 NOP pad).
pub const TRAMPOLINE_SLOT_BYTES: usize = 16;
/// Number of service slots we lay out.
pub const TRAMPOLINE_SLOT_COUNT: usize = 69;
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
        } else if slot < TRAMPOLINE_SLOT_COUNT {
            Some(ServiceId(Self::RUNTIME_BASE + (slot - 55) as u32))
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

/// All RayNu-F firmware state the dispatcher mutates. Lives in the hypervisor
/// as a single BSP-owned instance; host tests build their own.
pub struct FirmwareState {
    pub pool: PagePool,
    pub events: Events,
    pub watchdog_sets: u32,
}

impl FirmwareState {
    pub const fn new() -> Self {
        FirmwareState {
            pool: PagePool::new(),
            events: Events::new(),
            watchdog_sets: 0,
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

/// Dispatch one RayNu-F service call. Console, memory, event/timer, TPL and
/// misc boot services are implemented; protocol/handle/image services are
/// `EFI_UNSUPPORTED` in this slice (F4/F5), and runtime services too.
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

        // ---- not yet (F4/F5): handles, protocols, images, runtime ----------
        _ => EFI_UNSUPPORTED,
    };
    // Keep the pub consts referenced so the event-type API stays a single
    // source of truth for tests.
    let _ = (EVT_TIMER, EVT_NOTIFY_WAIT, EVT_NOTIFY_SIGNAL, TPL_HIGH_LEVEL, EFI_DEVICE_ERROR, EFI_NOT_FOUND);
    out
}

fn finish(mut out: Dispatched, status: u64) -> Dispatched {
    out.status = status;
    out
}
