//! Minimal EFI_TCP4 + service-binding bindings for M7.6 (ADR-012).
//!
//! Pillar: [Z] · Proven Core: **outside**
//! Boot Services only — must not be used after ExitBootServices.

#![cfg(feature = "uefi-bin")]

use core::ffi::c_void;
use core::ptr;
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::proto::unsafe_protocol;
use uefi::{Handle, Status, StatusExt};

/// EFI_TCP4_SERVICE_BINDING_PROTOCOL_GUID
pub const TCP4_SERVICE_BINDING_GUID: uefi::Guid =
    uefi::guid!("00720665-67eb-4a99-baf7-d3c33a1c7ce9");

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Ipv4Address(pub [u8; 4]);

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Tcp4AccessPoint {
    pub use_default_address: bool,
    pub station_address: Ipv4Address,
    pub subnet_mask: Ipv4Address,
    pub station_port: u16,
    pub remote_address: Ipv4Address,
    pub remote_port: u16,
    pub active_flag: bool,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Tcp4ConfigData {
    pub type_of_service: u8,
    pub time_to_live: u8,
    pub access_point: Tcp4AccessPoint,
    pub control_option: *const c_void,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Tcp4FragmentData {
    pub fragment_length: u32,
    pub fragment_buffer: *mut c_void,
}

#[derive(Debug)]
#[repr(C)]
pub struct Tcp4ReceiveData {
    pub urgent_flag: bool,
    pub data_length: u32,
    pub fragment_count: u32,
    pub fragment_table: [Tcp4FragmentData; 1],
}

#[derive(Debug)]
#[repr(C)]
pub struct Tcp4TransmitData {
    pub push: bool,
    pub urgent: bool,
    pub data_length: u32,
    pub fragment_count: u32,
    pub fragment_table: [Tcp4FragmentData; 1],
}

#[derive(Debug)]
#[repr(C)]
pub struct Tcp4CompletionToken {
    pub event: *mut c_void,
    pub status: Status,
}

#[repr(C)]
pub union Tcp4Packet {
    pub rx_data: *mut Tcp4ReceiveData,
    pub tx_data: *mut Tcp4TransmitData,
}

impl core::fmt::Debug for Tcp4Packet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Tcp4Packet(..)")
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct Tcp4IoToken {
    pub completion_token: Tcp4CompletionToken,
    pub packet: Tcp4Packet,
}

#[derive(Debug)]
#[repr(C)]
pub struct Tcp4ListenToken {
    pub completion_token: Tcp4CompletionToken,
    /// EFI_HANDLE (nullable until Accept completes).
    pub new_child_handle: *mut c_void,
}

#[derive(Debug)]
#[repr(C)]
#[unsafe_protocol("00720665-67eb-4a99-baf7-d3c33a1c7ce9")]
pub struct Tcp4ServiceBinding {
    create_child:
        unsafe extern "efiapi" fn(this: *mut Self, child_handle: *mut *mut c_void) -> Status,
    destroy_child: unsafe extern "efiapi" fn(this: *mut Self, child_handle: *mut c_void) -> Status,
}

impl Tcp4ServiceBinding {
    pub unsafe fn create_child_handle(&mut self) -> uefi::Result<Handle> {
        let mut child_raw: *mut c_void = ptr::null_mut();
        unsafe { (self.create_child)(self, &mut child_raw) }.to_result()?;
        Handle::from_ptr(child_raw).ok_or(Status::NOT_FOUND.into())
    }

    pub unsafe fn destroy_child_handle(&mut self, child: Handle) -> uefi::Result {
        unsafe { (self.destroy_child)(self, child.as_ptr()) }.to_result()
    }
}

#[derive(Debug)]
#[repr(C)]
#[unsafe_protocol("65530bc7-a359-410f-b022-f067671bbc4a")]
pub struct Tcp4Protocol {
    _get_mode_data: *mut c_void,
    configure: unsafe extern "efiapi" fn(this: *mut Self, config: *const Tcp4ConfigData) -> Status,
    _routes: *mut c_void,
    _connect: *mut c_void,
    accept: unsafe extern "efiapi" fn(this: *mut Self, token: *mut Tcp4ListenToken) -> Status,
    transmit: unsafe extern "efiapi" fn(this: *mut Self, token: *mut Tcp4IoToken) -> Status,
    receive: unsafe extern "efiapi" fn(this: *mut Self, token: *mut Tcp4IoToken) -> Status,
    _close: *mut c_void,
    cancel: unsafe extern "efiapi" fn(this: *mut Self, token: *mut c_void) -> Status,
    poll: unsafe extern "efiapi" fn(this: *mut Self) -> Status,
}

impl Tcp4Protocol {
    pub unsafe fn configure(&mut self, config: &Tcp4ConfigData) -> uefi::Result {
        unsafe { (self.configure)(self, config) }.to_result()
    }

    pub unsafe fn accept(&mut self, token: &mut Tcp4ListenToken) -> uefi::Result {
        unsafe { (self.accept)(self, token) }.to_result()
    }

    pub unsafe fn transmit(&mut self, token: &mut Tcp4IoToken) -> uefi::Result {
        unsafe { (self.transmit)(self, token) }.to_result()
    }

    pub unsafe fn receive(&mut self, token: &mut Tcp4IoToken) -> uefi::Result {
        unsafe { (self.receive)(self, token) }.to_result()
    }

    pub unsafe fn poll(&mut self) -> Status {
        unsafe { (self.poll)(self) }
    }

    pub unsafe fn cancel_all(&mut self) -> uefi::Result {
        unsafe { (self.cancel)(self, ptr::null_mut()) }.to_result()
    }
}

/// Create a Tcp4 child via service binding.
pub fn create_tcp4_child()
-> uefi::Result<(Handle, ScopedProtocol<Tcp4Protocol>, ScopedProtocol<Tcp4ServiceBinding>)> {
    let handles = boot::locate_handle_buffer(SearchType::ByProtocol(&TCP4_SERVICE_BINDING_GUID))?;
    let sb_handle = handles.first().copied().ok_or(Status::NOT_FOUND)?;

    let mut sb = unsafe {
        boot::open_protocol::<Tcp4ServiceBinding>(
            OpenProtocolParams {
                handle: sb_handle,
                agent: boot::image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )?
    };

    let child = unsafe { sb.create_child_handle()? };
    let tcp = boot::open_protocol_exclusive::<Tcp4Protocol>(child)?;
    Ok((child, tcp, sb))
}
