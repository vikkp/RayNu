//! Guest image / boot-spec types (ADR-014).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009 / ADR-014)
//! VERIFICATION: N/A
//!
//! Product ISO install is typed (`linux_iso` | `windows_iso` | `generic_uefi`)
//! and boots **UEFI guest firmware + virtio** first. Packed Linux bzImage is
//! lab/G0 only. This module does not VMLAUNCH and does not change E4 SPA start.

/// Guest image / install-media type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestImageType {
    /// Distro installer ISO (product path: UEFI + virtio).
    LinuxIso = 1,
    /// Windows installer ISO (later; same firmware path).
    WindowsIso = 2,
    /// Any El Torito / EFI boot image.
    GenericUefi = 3,
    /// Packed Linux boot protocol. Lab/M3–M4 G0 only — not the product installer.
    LinuxBzImage = 4,
}

impl GuestImageType {
    pub fn tag(self) -> u8 {
        self as u8
    }

    pub fn from_tag(t: u8) -> Option<Self> {
        match t {
            1 => Some(Self::LinuxIso),
            2 => Some(Self::WindowsIso),
            3 => Some(Self::GenericUefi),
            4 => Some(Self::LinuxBzImage),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxIso => "linux_iso",
            Self::WindowsIso => "windows_iso",
            Self::GenericUefi => "generic_uefi",
            Self::LinuxBzImage => "linux_bzimage",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "linux_iso" => Some(Self::LinuxIso),
            "windows_iso" => Some(Self::WindowsIso),
            "generic_uefi" => Some(Self::GenericUefi),
            "linux_bzimage" => Some(Self::LinuxBzImage),
            _ => None,
        }
    }

    pub const fn is_lab_only(self) -> bool {
        matches!(self, Self::LinuxBzImage)
    }

    /// Product ISO types boot UEFI firmware. Lab bzImage uses Linux boot protocol.
    pub const fn firmware(self) -> GuestFirmware {
        match self {
            Self::LinuxBzImage => GuestFirmware::LinuxBootProtocol,
            Self::LinuxIso | Self::WindowsIso | Self::GenericUefi => GuestFirmware::Uefi,
        }
    }
}

/// Guest firmware for first instruction fetch after VMLAUNCH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestFirmware {
    /// Packed Linux boot protocol (`guest/linux_boot.rs`). G0 lab only.
    LinuxBootProtocol,
    /// Guest UEFI (El Torito / OVMF-style) + virtio disk. Product default.
    Uefi,
}

impl GuestFirmware {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LinuxBootProtocol => "linux_boot_protocol",
            Self::Uefi => "uefi",
        }
    }
}

/// Boot-order entry for product install (CD then disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootDevice {
    Cdrom,
    Disk,
}

/// SPA create/start boot contract (ADR-014). Not wired to REST yet — E4 spec
/// `{cpu,ram,disk,iso}` stays. Extra fields land with the real ISO installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestBootSpec {
    pub image_type: GuestImageType,
    pub firmware: GuestFirmware,
    pub iso_id: u64,
    pub disk_mib: u32,
    pub boot_order: [BootDevice; 2],
}

impl GuestBootSpec {
    /// Product ISO install: UEFI firmware, CD then virtio disk.
    /// Rejects lab-only `linux_bzimage`.
    pub fn product_iso(image_type: GuestImageType, iso_id: u64, disk_mib: u32) -> Option<Self> {
        if image_type.is_lab_only() || iso_id == 0 || disk_mib == 0 {
            return None;
        }
        Some(Self {
            image_type,
            firmware: GuestFirmware::Uefi,
            iso_id,
            disk_mib,
            boot_order: [BootDevice::Cdrom, BootDevice::Disk],
        })
    }

    pub fn is_product_path(&self) -> bool {
        !self.image_type.is_lab_only() && self.firmware == GuestFirmware::Uefi
    }
}

/// True when ADR-014 is on disk with the multi-OS constraint.
pub fn adr014_present() -> bool {
    let adr = include_str!("../docs/adr/ADR-014.md");
    adr.contains("linux_iso")
        && adr.contains("windows_iso")
        && adr.contains("generic_uefi")
        && adr.contains("UEFI guest firmware")
        && adr.contains("Do not promote")
        && adr.contains("Windows is in scope later")
}

#[cfg(test)]
#[path = "guest_image_test.rs"]
mod guest_image_test;
