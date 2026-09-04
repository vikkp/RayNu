//! RayNu-F service trampolines and the host-side dispatcher.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016)
//!
//! A guest calls a UEFI service with the MS x64 ABI (`RCX, RDX, R8, R9`). Every
//! function pointer in our tables targets a 14-byte stub in guest memory:
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
//! hypervisor reads `RCX / R10 / R8 / R9`, performs the service against guest
//! memory it owns, writes `RAX = EFI_STATUS`, and resumes at `ret`.
//! `R10`/`R11` are volatile in the MS x64 ABI, so clobbering R10 is legal.

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

/// Longest `OutputString` we will read from the guest (CHAR16 units).
pub const OUTPUT_STRING_CAP_CHARS: usize = 4096;

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

/// Guest memory reader abstraction (host tests use a `Vec`; the hypervisor
/// walks the guest's EPT/page tables).
pub trait GuestMem {
    /// Read up to `buf.len()` bytes at guest address `addr`; returns bytes read.
    fn read(&self, addr: u64, buf: &mut [u8]) -> usize;
}

/// Where console output goes (host tests capture; the hypervisor writes serial).
pub trait ConsoleSink {
    fn write_byte(&mut self, b: u8);
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
}

/// Result of one dispatch, for logging/audit alongside the `EFI_STATUS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dispatched {
    pub id: ServiceId,
    pub status: u64,
    /// CHAR16 units consumed by `OutputString` (0 otherwise).
    pub chars_out: usize,
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

/// Dispatch one RayNu-F service call. Only the serial console is implemented
/// in this slice; everything else returns `EFI_UNSUPPORTED` honestly rather
/// than pretending.
pub fn dispatch(
    id: ServiceId,
    args: ServiceArgs,
    mem: &dyn GuestMem,
    sink: &mut dyn ConsoleSink,
) -> Dispatched {
    let mut chars_out = 0usize;
    let status = match id {
        ServiceId::ConOutReset => EFI_SUCCESS,
        ServiceId::ConOutOutputString | ServiceId::ConOutTestString => {
            if args.a2 == 0 {
                EFI_INVALID_PARAMETER
            } else if id == ServiceId::ConOutTestString {
                EFI_SUCCESS
            } else {
                chars_out = output_string(mem, sink, args.a2);
                EFI_SUCCESS
            }
        }
        // Single 80x25 mode: QueryMode(This, 0, *Cols, *Rows) would need a
        // guest write; SetMode(0)/SetAttribute/ClearScreen/SetCursor/
        // EnableCursor are accepted as no-ops on a serial console.
        ServiceId::ConOutSetMode => {
            if args.a2 == 0 {
                EFI_SUCCESS
            } else {
                EFI_UNSUPPORTED
            }
        }
        ServiceId::ConOutSetAttribute
        | ServiceId::ConOutClearScreen
        | ServiceId::ConOutSetCursorPosition
        | ServiceId::ConOutEnableCursor => EFI_SUCCESS,
        ServiceId::ConOutQueryMode => EFI_UNSUPPORTED,
        ServiceId::ConInReset => EFI_SUCCESS,
        ServiceId::ConInReadKeyStroke => EFI_NOT_READY,
        _ => EFI_UNSUPPORTED,
    };
    Dispatched {
        id,
        status,
        chars_out,
    }
}
