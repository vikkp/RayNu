//! Copy an armed A7 slot onto the boot stick before ExitBootServices.
//!
//! Pillar: [Z]
//! Proven Core: **outside** (ADR-002)
//!
//! The spare is read through the firmware Block I/O protocol. The only
//! write is on the volume this image was loaded from. An armed slot saves
//! `BOOTX64.OLD` and replaces `BOOTX64.EFI`. A revert copies `BOOTX64.OLD`
//! back. The Toshiba durable LUN is not this path. LBA 0 is not written.

/// Run before ExitBootServices. Host builds do nothing.
pub fn apply_before_ebs() {
    #[cfg(target_os = "uefi")]
    apply_uefi();
}

#[cfg(target_os = "uefi")]
fn apply_uefi() {
    use crate::boot::serial;
    use crate::mgmt::fw_update::{self, fw_map};
    use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams};
    use uefi::fs::FileSystem;
    use uefi::proto::media::block::BlockIO;
    use uefi::CString16;

    let Ok(handles) = boot::find_handles::<BlockIO>() else {
        return;
    };
    for handle in handles {
        // SAFETY: pre-EBS, BSP only. GetProtocol does not take the firmware's
        // own open away. The scoped protocol drops before the next handle.
        let Ok(bio) = (unsafe {
            boot::open_protocol::<BlockIO>(
                OpenProtocolParams {
                    handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        }) else {
            continue;
        };
        let media = bio.media();
        if !media.is_media_present() || media.block_size() != 512 {
            continue;
        }
        let align = media.io_align();
        if align > 4096 || (align > 1 && !align.is_power_of_two()) {
            continue;
        }
        let bytes = media
            .last_block()
            .saturating_add(1)
            .saturating_mul(u64::from(media.block_size()));
        let Some(map) = fw_map(bytes) else {
            continue;
        };
        let mut header = Aligned([0u8; 512]);
        if bio
            .read_blocks(media.media_id(), map.header_lba, &mut header.0)
            .is_err()
        {
            continue;
        }
        let Some(rec) = fw_update::decode(&header.0) else {
            continue;
        };
        if rec.revert {
            restore_old();
            return;
        }
        let Some((slot, len, sha)) = fw_update::stick_plan(&rec) else {
            continue;
        };
        let Some(base) = fw_update::chunk_lba(map, slot, 0) else {
            continue;
        };
        serial::write_str("boot: A7 stick read slot=");
        write_dec(u64::from(slot));
        serial::write_line("");
        let media_id = media.media_id();
        let ok = fw_update::read_stick_image(len, sha, |off, buf| {
            let mut sector = Aligned([0u8; 512]);
            let lba = base + off / 512;
            if bio.read_blocks(media_id, lba, &mut sector.0).is_err() {
                return false;
            }
            let n = buf.len().min(512);
            buf[..n].copy_from_slice(&sector.0[..n]);
            true
        });
        if !ok {
            serial::write_line("boot: A7 stick bad image");
            return;
        }
        let Some(image) = fw_update::stick_image() else {
            return;
        };
        let image_handle = boot::image_handle();
        let Ok(sfs) = boot::get_image_file_system(image_handle) else {
            serial::write_line("boot: A7 stick skip (no boot volume)");
            return;
        };
        let mut fs = FileSystem::new(sfs);
        let Ok(boot_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.EFI") else {
            return;
        };
        let Ok(old_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.OLD") else {
            return;
        };
        let Ok(next_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.A7") else {
            return;
        };
        let Ok(current) = fs.read(boot_path.as_ref()) else {
            serial::write_line("boot: A7 stick skip (no BOOTX64.EFI)");
            return;
        };
        if fs.write(old_path.as_ref(), &current).is_err() {
            serial::write_line("boot: A7 stick skip (OLD)");
            return;
        }
        if !replace_bootx64(&mut fs, &boot_path, &next_path, image) {
            return;
        }
        fw_update::note_stick_written();
        serial::write_str("boot: A7 stick wrote bytes=");
        write_dec(image.len() as u64);
        serial::write_line("");
        return;
    }
}

#[cfg(target_os = "uefi")]
fn restore_old() {
    use crate::boot::serial;
    use crate::mgmt::fw_update;
    use uefi::boot;
    use uefi::fs::FileSystem;
    use uefi::CString16;

    serial::write_line("boot: A7 stick restore");
    let image_handle = boot::image_handle();
    let Ok(sfs) = boot::get_image_file_system(image_handle) else {
        serial::write_line("boot: A7 stick skip (no boot volume)");
        return;
    };
    let mut fs = FileSystem::new(sfs);
    let Ok(boot_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.EFI") else {
        return;
    };
    let Ok(old_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.OLD") else {
        return;
    };
    let Ok(next_path) = CString16::try_from("\\EFI\\BOOT\\BOOTX64.A7") else {
        return;
    };
    let Ok(old) = fs.read(old_path.as_ref()) else {
        serial::write_line("boot: A7 stick skip (no OLD)");
        return;
    };
    if !fw_update::is_pe(&old) {
        serial::write_line("boot: A7 stick skip (OLD is not PE)");
        return;
    }
    if !replace_bootx64(&mut fs, &boot_path, &next_path, &old) {
        return;
    }
    fw_update::note_stick_restored();
    serial::write_str("boot: A7 stick restored bytes=");
    write_dec(old.len() as u64);
    serial::write_line("");
}

#[cfg(target_os = "uefi")]
fn replace_bootx64(
    fs: &mut uefi::fs::FileSystem,
    boot_path: &uefi::CString16,
    next_path: &uefi::CString16,
    image: &[u8],
) -> bool {
    use crate::boot::serial;
    if fs.write(next_path.as_ref(), image).is_err() {
        serial::write_line("boot: A7 stick skip (A7)");
        return false;
    }
    let Ok(back) = fs.read(next_path.as_ref()) else {
        serial::write_line("boot: A7 stick skip (readback)");
        return false;
    };
    if back.as_slice() != image {
        serial::write_line("boot: A7 stick mismatch");
        return false;
    }
    if fs.remove_file(boot_path.as_ref()).is_err() {
        serial::write_line("boot: A7 stick skip (remove)");
        return false;
    }
    if fs.rename(next_path.as_ref(), boot_path.as_ref()).is_err() {
        serial::write_line("boot: A7 stick BOOTX64.OLD remains; BOOTX64.EFI missing");
        return false;
    }
    true
}

#[cfg(target_os = "uefi")]
#[repr(C, align(4096))]
struct Aligned<T>(T);

#[cfg(target_os = "uefi")]
fn write_dec(mut n: u64) {
    use crate::boot::serial;
    if n == 0 {
        serial::write_byte(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20;
    while n > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    for &b in &buf[i..] {
        serial::write_byte(b);
    }
}
