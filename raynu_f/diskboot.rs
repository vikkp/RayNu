//! One disk publisher for the running guest and a seated guest.
//!
//! Pillar: [Z] · Proven Core: **outside** (ADR-016 / ADR-021 B6)
//!
//! GRUB `efidisk` names a Media/HardDrive path `,gptN` when no whole-disk
//! path is that partition's parent. Nested `3492ebc` and iron `623a84da`
//! both printed `disk ',gpt2' not found`. The running guest's F7 path
//! publishes the whole disk as `hd0` and, only for a loader that carries
//! a file path, the ESP as `hd0,gpt1`. The seated guest calls this same
//! function.

use super::blockio::{
    BlockMedia, BLOCKIO_FLUSH_OFF, BLOCKIO_READ_OFF, BLOCKIO_RESET_OFF, BLOCKIO_REVISION2,
    BLOCKIO_SIZE, BLOCKIO_WRITE_OFF, MEDIA_SIZE,
};
use super::fat::DISK_BOOT_CANDIDATES;
use super::gpt::EspPartition;
use super::memory::{ALLOCATE_ANY_PAGES, EFI_LOADER_DATA};
use super::protocol::{
    device_path_is_grub_partition_child, encode_esp_device_path, encode_filepath_device_path,
    encode_whole_disk_device_path, Guid, DEVICE_PATH_BYTES, DP_END_LEN, DP_HD_LEN,
    DP_SIG_TYPE_GUID, DP_SUBTYPE_END_ENTIRE, DP_SUBTYPE_HD, DP_TYPE_END, DP_TYPE_MEDIA,
    GUID_BLOCK_IO, GUID_DEVICE_PATH, GUID_SIMPLE_FILE_SYSTEM, HANDLE_DISK, HANDLE_ESP,
};
use super::services::{FirmwareState, GuestMem};

/// GPAs the firmware image already reserved for the whole disk.
pub struct F7DiskPublish {
    pub whole_path_gpa: u64,
    pub media_gpa: u64,
    pub blockio_disk: u64,
    pub sfs: u64,
    pub disk_bytes: u64,
}

/// `true` when this staged path publishes a Media/FilePath node and the ESP
/// handle. `\EFI\BOOT\BOOTX64.EFI` is false: its device is the whole disk.
pub fn publish_file_path_for(path: &[u8]) -> bool {
    DISK_BOOT_CANDIDATES
        .iter()
        .find(|c| c.path == path)
        .map(|c| c.publish_file_path)
        .unwrap_or(false)
}

/// Publish `hd0`, and the ESP when `publish_partition` is set.
///
/// `LoadedImage.DeviceHandle` is the ESP only for a file-path loader.
/// A removable loader keeps the whole disk, which is the F7 path that
/// stopped nested GRUB from entering rescue on `,gpt2`.
pub fn publish_f7_disk(
    st: &mut FirmwareState,
    mem: &dyn GuestMem,
    esp: &EspPartition,
    publish_partition: bool,
    spec: &F7DiskPublish,
) -> bool {
    if spec.disk_bytes < 512
        || spec.whole_path_gpa == 0
        || spec.media_gpa == 0
        || spec.blockio_disk == 0
        || spec.sfs == 0
    {
        return false;
    }
    let media = BlockMedia::disk(spec.disk_bytes);
    let mut media_bytes = [0u8; MEDIA_SIZE];
    media.encode(&mut media_bytes);
    let mut whole = [0u8; DEVICE_PATH_BYTES];
    encode_whole_disk_device_path(&mut whole);
    if mem.write(spec.media_gpa, &media_bytes) != media_bytes.len()
        || mem.write(spec.whole_path_gpa, &whole) != whole.len()
    {
        return false;
    }
    st.media_disk = media;
    st.blockio_disk = spec.blockio_disk;
    st.device_path = spec.whole_path_gpa;
    st.device_handle = HANDLE_DISK;
    if st
        .protocols
        .install(HANDLE_DISK, GUID_BLOCK_IO, spec.blockio_disk)
        != 0
        || st
            .protocols
            .install(HANDLE_DISK, GUID_DEVICE_PATH, spec.whole_path_gpa)
            != 0
        || st
            .protocols
            .install(HANDLE_DISK, GUID_SIMPLE_FILE_SYSTEM, spec.sfs)
            != 0
    {
        return false;
    }
    if !publish_partition {
        return true;
    }
    publish_esp_partition(st, mem, esp, spec.blockio_disk, spec.sfs)
}

fn publish_esp_partition(
    st: &mut FirmwareState,
    mem: &dyn GuestMem,
    esp: &EspPartition,
    disk_blockio: u64,
    sfs: u64,
) -> bool {
    let (status, gpa) = st
        .pool
        .allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 1, 0);
    if status != 0 {
        return false;
    }
    let media = BlockMedia::esp(esp.size_lba());
    let mut media_bytes = [0u8; MEDIA_SIZE];
    media.encode(&mut media_bytes);
    let media_gpa = gpa + 0x40;
    let path_gpa = gpa + 0x80;
    let offs = [
        BLOCKIO_RESET_OFF,
        BLOCKIO_READ_OFF,
        BLOCKIO_WRITE_OFF,
        BLOCKIO_FLUSH_OFF,
    ];
    let mut ptrs = [0u64; 4];
    for (i, off) in offs.iter().enumerate() {
        let mut b = [0u8; 8];
        if mem.read(disk_blockio + *off as u64, &mut b) != 8 {
            return false;
        }
        ptrs[i] = u64::from_le_bytes(b);
    }
    let mut proto = [0u8; BLOCKIO_SIZE];
    proto[..8].copy_from_slice(&BLOCKIO_REVISION2.to_le_bytes());
    proto[8..16].copy_from_slice(&media_gpa.to_le_bytes());
    proto[0x10..0x18].copy_from_slice(&ptrs[0].to_le_bytes());
    proto[0x18..0x20].copy_from_slice(&ptrs[1].to_le_bytes());
    proto[0x20..0x28].copy_from_slice(&ptrs[2].to_le_bytes());
    proto[0x28..0x30].copy_from_slice(&ptrs[3].to_le_bytes());
    let mut path = [0u8; 96];
    let Some(n) = encode_esp_device_path(
        esp.partition_number,
        esp.start_lba,
        esp.size_lba(),
        esp.unique_guid,
        &mut path,
    ) else {
        return false;
    };
    if mem.write(gpa, &proto) != proto.len()
        || mem.write(media_gpa, &media_bytes) != media_bytes.len()
        || mem.write(path_gpa, &path[..n]) != n
    {
        return false;
    }
    if st.protocols.install(HANDLE_ESP, GUID_BLOCK_IO, gpa) != 0
        || st.protocols.install(HANDLE_ESP, GUID_DEVICE_PATH, path_gpa) != 0
        || st
            .protocols
            .install(HANDLE_ESP, GUID_SIMPLE_FILE_SYSTEM, sfs)
            != 0
    {
        return false;
    }
    st.blockio_esp = gpa;
    st.media_esp = media;
    st.esp_byte_off = esp.start_lba.saturating_mul(512);
    st.device_handle = HANDLE_ESP;
    true
}

/// Media/FilePath node for a staged loader. The device handle keeps its
/// own path. Shim reads this node when it opens `grubx64.efi` beside itself.
pub fn publish_loader_filepath(
    st: &mut FirmwareState,
    mem: &dyn GuestMem,
    path: &[u8],
) -> Option<u64> {
    let mut buf = [0u8; 96];
    let n = encode_filepath_device_path(path, &mut buf)?;
    let (status, gpa) = st
        .pool
        .allocate_pages(ALLOCATE_ANY_PAGES, EFI_LOADER_DATA, 1, 0);
    if status != 0 {
        return None;
    }
    if mem.write(gpa, &buf[..n]) == n {
        Some(gpa)
    } else {
        None
    }
}

/// GRUB `efidisk` name for `path`, given every BlockIo device path.
///
/// A HardDrive node with no parent disk is `,gptN` (or `,msdosN`). That is
/// the rescue string. A matching whole-disk path makes it `hd0,gptN`.
/// A path that is itself a whole disk is `hd0`.
pub fn grub_efidisk_name(path: &[u8], block_paths: &[&[u8]], out: &mut [u8]) -> Option<usize> {
    let (off, len) = last_node(path)?;
    if path[off] == DP_TYPE_MEDIA && path[off + 1] == DP_SUBTYPE_HD && len >= DP_HD_LEN {
        let part = u32::from_le_bytes(path[off + 4..off + 8].try_into().ok()?);
        let gpt = path[off + 41] == DP_SIG_TYPE_GUID;
        let mut parent = [0u8; 128];
        if off + DP_END_LEN > parent.len() {
            return None;
        }
        parent[..off].copy_from_slice(&path[..off]);
        parent[off] = DP_TYPE_END;
        parent[off + 1] = DP_SUBTYPE_END_ENTIRE;
        parent[off + 2..off + 4].copy_from_slice(&(DP_END_LEN as u16).to_le_bytes());
        let mut disk = None;
        let mut whole = 0u32;
        for candidate in block_paths {
            if device_path_is_grub_partition_child(candidate) {
                continue;
            }
            if paths_equal(candidate, &parent[..off + DP_END_LEN]) {
                disk = Some(whole);
                break;
            }
            whole = whole.saturating_add(1);
        }
        let mut n = 0usize;
        if let Some(i) = disk {
            push(out, &mut n, b"hd")?;
            push_u32(out, &mut n, i)?;
        }
        push(out, &mut n, b",")?;
        push(out, &mut n, if gpt { b"gpt" } else { b"msdos" })?;
        push_u32(out, &mut n, part)?;
        return Some(n);
    }
    let mut whole = 0u32;
    let mut index = None;
    for candidate in block_paths {
        if device_path_is_grub_partition_child(candidate) {
            continue;
        }
        if paths_equal(candidate, path) {
            index = Some(whole);
            break;
        }
        whole = whole.saturating_add(1);
    }
    let mut n = 0usize;
    push(out, &mut n, b"hd")?;
    push_u32(out, &mut n, index.unwrap_or(0))?;
    Some(n)
}

fn last_node(path: &[u8]) -> Option<(usize, usize)> {
    let mut off = 0usize;
    let mut last = None;
    while off + 4 <= path.len() {
        let len = node_len(path, off)?;
        if path[off] == DP_TYPE_END {
            break;
        }
        last = Some((off, len));
        off += len;
    }
    last
}

fn node_len(path: &[u8], off: usize) -> Option<usize> {
    if off + 4 > path.len() {
        return None;
    }
    let len = u16::from_le_bytes([path[off + 2], path[off + 3]]) as usize;
    if len < 4 || off + len > path.len() {
        return None;
    }
    Some(len)
}

fn paths_equal(a: &[u8], b: &[u8]) -> bool {
    let mut ia = 0usize;
    let mut ib = 0usize;
    loop {
        let Some(la) = node_len(a, ia) else {
            return false;
        };
        let Some(lb) = node_len(b, ib) else {
            return false;
        };
        if la != lb || a[ia..ia + la] != b[ib..ib + lb] {
            return false;
        }
        let end = a[ia] == DP_TYPE_END;
        ia += la;
        ib += lb;
        if end {
            return true;
        }
    }
}

fn push(out: &mut [u8], n: &mut usize, bytes: &[u8]) -> Option<()> {
    let end = n.checked_add(bytes.len())?;
    if end > out.len() {
        return None;
    }
    out[*n..end].copy_from_slice(bytes);
    *n = end;
    Some(())
}

fn push_u32(out: &mut [u8], n: &mut usize, mut v: u32) -> Option<()> {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    push(out, n, &buf[i..])
}

/// Read a protocol GUID the guest passed to `OpenProtocol`.
pub fn read_guid(mem: &dyn GuestMem, ptr: u64) -> Option<Guid> {
    if ptr == 0 {
        return None;
    }
    let mut g = [0u8; 16];
    if mem.read(ptr, &mut g) == 16 {
        Some(g)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raynu_f::events::TimeSource;
    use crate::raynu_f::fat::{parse_bpb, pick_disk_bootloader, resolve_path, VolumeRead};
    use crate::raynu_f::gpt::{
        find_esp, EspPartition, ESP_TYPE_GUID, GPT_HEADER_SIZE_MIN, GPT_REVISION_1_0,
        GPT_SIGNATURE, MBR_TYPE_GPT,
    };
    use crate::raynu_f::launch_plan::F2_TABLES_BASE;
    use crate::raynu_f::protocol::{
        encode_hd_device_path, GUID_DEVICE_PATH, GUID_LOADED_IMAGE, HANDLE_DISK, HANDLE_ESP,
        HANDLE_IMAGE, LOADED_IMAGE_DEVICE_HANDLE_OFF,
    };
    use crate::raynu_f::services::{
        dispatch, publish_loaded_image, ConsoleSink, FirmwareState, GuestMem, ServiceArgs,
        ServiceId, EFI_NOT_FOUND, EFI_SUCCESS,
    };
    use crate::raynu_f::tables::{
        crc32, IMAGE_BLOCKIO_DISK_OFF, IMAGE_DEVICE_PATH_OFF, IMAGE_LOADED_IMAGE_OFF,
        IMAGE_MEDIA_DISK_OFF, IMAGE_SFS_OFF, IMAGE_SYSTEM_TABLE_OFF,
    };
    use std::cell::RefCell;

    struct Mock {
        mem: RefCell<Vec<u8>>,
    }

    impl GuestMem for Mock {
        fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
            let mem = self.mem.borrow();
            let off = addr as usize;
            if off >= mem.len() {
                return 0;
            }
            let n = buf.len().min(mem.len() - off);
            buf[..n].copy_from_slice(&mem[off..off + n]);
            n
        }
        fn write(&self, addr: u64, buf: &[u8]) -> usize {
            let mut mem = self.mem.borrow_mut();
            let off = addr as usize;
            if off >= mem.len() {
                return 0;
            }
            let n = buf.len().min(mem.len() - off);
            mem[off..off + n].copy_from_slice(&buf[..n]);
            n
        }
    }

    struct Sink;
    impl ConsoleSink for Sink {
        fn write_byte(&mut self, _: u8) {}
    }
    struct Clock;
    impl TimeSource for Clock {
        fn now_100ns(&self) -> u64 {
            0
        }
    }
    struct Disk<'a>(&'a [u8]);
    impl VolumeRead for Disk<'_> {
        fn read_at(&self, off: u64, buf: &mut [u8]) -> bool {
            let start = off as usize;
            let Some(end) = start.checked_add(buf.len()) else {
                return false;
            };
            if end > self.0.len() {
                return false;
            }
            buf.copy_from_slice(&self.0[start..end]);
            true
        }
    }

    fn name_of(path: &[u8], others: &[&[u8]]) -> String {
        let mut out = [0u8; 32];
        let n = grub_efidisk_name(path, others, &mut out).expect("name");
        String::from_utf8(out[..n].to_vec()).unwrap()
    }

    #[test]
    fn lone_partition_is_the_rescue_name() {
        let mut path = [0u8; DEVICE_PATH_BYTES];
        encode_hd_device_path(2, 2048, 1000, [0xAB; 16], &mut path);
        assert_eq!(name_of(&path, &[]), ",gpt2");
    }

    #[test]
    fn parent_disk_names_the_partition() {
        let mut whole = [0u8; DEVICE_PATH_BYTES];
        encode_whole_disk_device_path(&mut whole);
        let mut esp = [0u8; 96];
        let n = encode_esp_device_path(1, 34, 128, [0x11; 16], &mut esp).unwrap();
        assert_eq!(name_of(&esp[..n], &[]), ",gpt1");
        assert_eq!(name_of(&esp[..n], &[&whole[..]]), "hd0,gpt1");
        assert_eq!(name_of(&whole, &[&whole[..]]), "hd0");
    }

    fn fat12(dir11: &[u8; 11], file11: &[u8; 11]) -> Vec<u8> {
        const SEC: usize = 512;
        const TOTAL: usize = 256;
        const RESERVED: usize = 1;
        const FAT_SECS: usize = 2;
        const ROOT_ENTS: usize = 64;
        let root_secs = ROOT_ENTS * 32 / SEC;
        let mut v = vec![0u8; TOTAL * SEC];
        v[0] = 0xEB;
        v[1] = 0x3C;
        v[2] = 0x90;
        v[3..11].copy_from_slice(b"RAYNUF  ");
        v[11..13].copy_from_slice(&(SEC as u16).to_le_bytes());
        v[13] = 1;
        v[14..16].copy_from_slice(&(RESERVED as u16).to_le_bytes());
        v[16] = 1;
        v[17..19].copy_from_slice(&(ROOT_ENTS as u16).to_le_bytes());
        v[19..21].copy_from_slice(&(TOTAL as u16).to_le_bytes());
        v[21] = 0xF8;
        v[22..24].copy_from_slice(&(FAT_SECS as u16).to_le_bytes());
        v[510] = 0x55;
        v[511] = 0xAA;
        let fat_start = RESERVED;
        let root_start = fat_start + FAT_SECS;
        let data_start = root_start + root_secs;
        let set_fat = |v: &mut Vec<u8>, cl: usize, val: u16| {
            let off = fat_start * SEC + cl * 3 / 2;
            let cur = u16::from_le_bytes([v[off], v[off + 1]]);
            let new = if cl & 1 == 0 {
                (cur & 0xF000) | (val & 0x0FFF)
            } else {
                (cur & 0x000F) | (val << 4)
            };
            v[off..off + 2].copy_from_slice(&new.to_le_bytes());
        };
        set_fat(&mut v, 0, 0xFF8);
        set_fat(&mut v, 1, 0xFFF);
        let mk = |name: &[u8; 11], attr: u8, cluster: u16, size: u32| {
            let mut e = [0u8; 32];
            e[..11].copy_from_slice(name);
            e[11] = attr;
            e[26..28].copy_from_slice(&cluster.to_le_bytes());
            e[28..32].copy_from_slice(&size.to_le_bytes());
            e
        };
        let root = root_start * SEC;
        v[root..root + 32].copy_from_slice(&mk(b"EFI        ", 0x10, 2, 0));
        set_fat(&mut v, 2, 0xFFF);
        let efi = (data_start + 2 - 2) * SEC;
        v[efi..efi + 32].copy_from_slice(&mk(b".          ", 0x10, 2, 0));
        v[efi + 32..efi + 64].copy_from_slice(&mk(b"..         ", 0x10, 0, 0));
        v[efi + 64..efi + 96].copy_from_slice(&mk(dir11, 0x10, 3, 0));
        set_fat(&mut v, 3, 0xFFF);
        let dir = (data_start + 3 - 2) * SEC;
        v[dir..dir + 32].copy_from_slice(&mk(b".          ", 0x10, 3, 0));
        v[dir + 32..dir + 64].copy_from_slice(&mk(b"..         ", 0x10, 2, 0));
        v[dir + 64..dir + 96].copy_from_slice(&mk(file11, 0x20, 4, 4));
        set_fat(&mut v, 4, 0xFFF);
        let file = (data_start + 4 - 2) * SEC;
        v[file..file + 4].copy_from_slice(b"test");
        v
    }

    fn gpt_with_fat(fat: &[u8]) -> (Vec<u8>, EspPartition) {
        const ESP_START: u64 = 34;
        let esp_sectors = ((fat.len() + 511) / 512) as u64;
        let esp_end = ESP_START + esp_sectors - 1;
        let disk_bytes = 2 * 1024 * 1024;
        let mut disk = vec![0u8; disk_bytes];
        disk[510] = 0x55;
        disk[511] = 0xAA;
        disk[0x1BE + 4] = MBR_TYPE_GPT;
        let entry_lba = 2u64;
        let n_entries = 128u32;
        let entry_size = 128u32;
        let off = (entry_lba * 512) as usize;
        disk[off..off + 16].copy_from_slice(&ESP_TYPE_GUID);
        disk[off + 16..off + 32].copy_from_slice(&[0x11; 16]);
        disk[off + 32..off + 40].copy_from_slice(&ESP_START.to_le_bytes());
        disk[off + 40..off + 48].copy_from_slice(&esp_end.to_le_bytes());
        let array_bytes = (n_entries * entry_size) as usize;
        let array_crc = crc32(&disk[off..off + array_bytes]);
        let hdr = &mut disk[512..1024];
        hdr[0..8].copy_from_slice(GPT_SIGNATURE);
        hdr[8..12].copy_from_slice(&GPT_REVISION_1_0.to_le_bytes());
        hdr[12..16].copy_from_slice(&GPT_HEADER_SIZE_MIN.to_le_bytes());
        hdr[24..32].copy_from_slice(&1u64.to_le_bytes());
        let alt = (disk_bytes as u64 / 512) - 1;
        hdr[32..40].copy_from_slice(&alt.to_le_bytes());
        hdr[40..48].copy_from_slice(&34u64.to_le_bytes());
        hdr[48..56].copy_from_slice(&(alt - 33).to_le_bytes());
        hdr[72..80].copy_from_slice(&entry_lba.to_le_bytes());
        hdr[80..84].copy_from_slice(&n_entries.to_le_bytes());
        hdr[84..88].copy_from_slice(&entry_size.to_le_bytes());
        hdr[88..92].copy_from_slice(&array_crc.to_le_bytes());
        let c = crc32(&hdr[..GPT_HEADER_SIZE_MIN as usize]);
        hdr[16..20].copy_from_slice(&c.to_le_bytes());
        let at = (ESP_START * 512) as usize;
        disk[at..at + fat.len()].copy_from_slice(fat);
        let esp = find_esp(&Disk(&disk)).expect("esp");
        (disk, esp)
    }

    fn spec() -> F7DiskPublish {
        F7DiskPublish {
            whole_path_gpa: F2_TABLES_BASE + IMAGE_DEVICE_PATH_OFF as u64,
            media_gpa: F2_TABLES_BASE + IMAGE_MEDIA_DISK_OFF as u64,
            blockio_disk: F2_TABLES_BASE + IMAGE_BLOCKIO_DISK_OFF as u64,
            sfs: F2_TABLES_BASE + IMAGE_SFS_OFF as u64,
            disk_bytes: 2 * 1024 * 1024,
        }
    }

    fn open_boot_device(publish_partition: bool, esp: &EspPartition) -> (Mock, FirmwareState, u64) {
        let mem = Mock {
            mem: RefCell::new(vec![0u8; 0x200_0000]),
        };
        let mut st = FirmwareState::new();
        st.loaded_image_proto = F2_TABLES_BASE + IMAGE_LOADED_IMAGE_OFF as u64;
        st.system_table = F2_TABLES_BASE + IMAGE_SYSTEM_TABLE_OFF as u64;
        st.sfs = spec().sfs;
        assert!(publish_f7_disk(
            &mut st,
            &mem,
            esp,
            publish_partition,
            &spec()
        ));
        if publish_partition {
            let gpa = publish_loader_filepath(&mut st, &mem, b"\\EFI\\ubuntu\\grubx64.efi")
                .expect("file path");
            st.image_file_path = gpa;
            st.note_staged_loader(b"\\EFI\\ubuntu\\grubx64.efi");
        }
        st.image_base = 0x0100_0000;
        st.image_size = 0x1000;
        st.image_entry = 0x0100_1000;
        st.image_handle = HANDLE_IMAGE;
        assert!(publish_loaded_image(&mut st, &mem, 0));
        let guid = 0x1000u64;
        let iface = 0x1100u64;
        mem.write(guid, &GUID_LOADED_IMAGE);
        assert_eq!(
            dispatch(
                ServiceId::OpenProtocol,
                ServiceArgs::regs(HANDLE_IMAGE, guid, iface, 0),
                &mem,
                &mut Sink,
                &mut st,
                &Clock,
                0x200_0000,
            )
            .status,
            EFI_SUCCESS
        );
        let device = {
            let mut b = [0u8; 8];
            let at = st.loaded_image_proto + LOADED_IMAGE_DEVICE_HANDLE_OFF as u64;
            assert_eq!(mem.read(at, &mut b), 8);
            u64::from_le_bytes(b)
        };
        mem.write(guid, &GUID_DEVICE_PATH);
        assert_eq!(
            dispatch(
                ServiceId::OpenProtocol,
                ServiceArgs::regs(device, guid, iface, 0),
                &mem,
                &mut Sink,
                &mut st,
                &Clock,
                0x200_0000,
            )
            .status,
            EFI_SUCCESS
        );
        let path_gpa = {
            let mut b = [0u8; 8];
            assert_eq!(mem.read(iface, &mut b), 8);
            u64::from_le_bytes(b)
        };
        (mem, st, path_gpa)
    }

    fn path_bytes(mem: &Mock, gpa: u64) -> Vec<u8> {
        let mut buf = vec![0u8; 96];
        let n = mem.read(gpa, &mut buf);
        buf.truncate(n);
        buf
    }

    #[test]
    fn removable_loader_device_is_hd0() {
        let fat = fat12(b"BOOT       ", b"BOOTX64 EFI");
        let (disk, esp) = gpt_with_fat(&fat);
        let vol = parse_bpb(&disk[(esp.start_lba as usize) * 512..][..512]).unwrap();
        let reader = Disk(&disk[(esp.start_lba as usize) * 512..]);
        let choice = pick_disk_bootloader(
            |p| matches!(resolve_path(&vol, &reader, p), Ok(e) if !e.is_dir()),
        )
        .unwrap();
        assert_eq!(choice.path, b"\\EFI\\BOOT\\BOOTX64.EFI");
        assert!(!choice.publish_file_path);
        let (mem, mut st, path_gpa) = open_boot_device(false, &esp);
        assert_eq!(st.device_handle, HANDLE_DISK);
        let path = path_bytes(&mem, path_gpa);
        let mut whole = [0u8; DEVICE_PATH_BYTES];
        encode_whole_disk_device_path(&mut whole);
        assert_eq!(name_of(&path, &[&whole[..]]), "hd0");
        assert_ne!(name_of(&path, &[&whole[..]]), ",gpt2");
    }

    #[test]
    fn ubuntu_loader_device_is_hd0_gpt1() {
        let fat = fat12(b"UBUNTU     ", b"GRUBX64 EFI");
        let (disk, esp) = gpt_with_fat(&fat);
        let vol = parse_bpb(&disk[(esp.start_lba as usize) * 512..][..512]).unwrap();
        let reader = Disk(&disk[(esp.start_lba as usize) * 512..]);
        let choice = pick_disk_bootloader(
            |p| matches!(resolve_path(&vol, &reader, p), Ok(e) if !e.is_dir()),
        )
        .unwrap();
        assert_eq!(choice.path, b"\\EFI\\ubuntu\\grubx64.efi");
        assert!(choice.publish_file_path);
        let (mem, mut st, path_gpa) = open_boot_device(true, &esp);
        assert_eq!(st.device_handle, HANDLE_ESP);
        let esp_path = path_bytes(&mem, path_gpa);
        let mut whole = [0u8; DEVICE_PATH_BYTES];
        encode_whole_disk_device_path(&mut whole);
        assert_eq!(name_of(&esp_path, &[]), ",gpt1");
        assert_eq!(name_of(&esp_path, &[&whole[..]]), "hd0,gpt1");
        mem.write(0x1000, &GUID_DEVICE_PATH);
        assert_eq!(
            dispatch(
                ServiceId::OpenProtocol,
                ServiceArgs::regs(HANDLE_IMAGE, 0x1000, 0x1200, 0),
                &mem,
                &mut Sink,
                &mut st,
                &Clock,
                0x200_0000,
            )
            .status,
            EFI_NOT_FOUND
        );
        assert_eq!(
            dispatch(
                ServiceId::OpenProtocol,
                ServiceArgs::regs(HANDLE_DISK, 0x1000, 0x1200, 0),
                &mem,
                &mut Sink,
                &mut st,
                &Clock,
                0x200_0000,
            )
            .status,
            EFI_SUCCESS
        );
    }
}
