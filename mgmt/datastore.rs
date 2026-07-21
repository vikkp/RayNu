//! M7.2 datastore / image library (outside Proven Core).
//!
//! Pillar: [Z] [A]
//! Proven Core: **outside** (ADR-002 / ADR-009)
//! VERIFICATION: N/A
//!
//! Register / list / delete images (ISO, disk, template). Host/`cfg(test)`
//! persists a catalog under an ESP-shaped path (`EFI/RAYNU/images/`). UEFI
//! SimpleFileSystem write remains stubbed until wired (honest residual).

use super::api::{
    auth_allows, ApiReply, RestMethod, RestRequest, RestResponse, BRINGUP_AUTH_TOKEN,
};

/// Host / CI marker when the M7.2 datastore gate passes.
pub const M7_STORE_OK_MARKER: &str = "RAYNU-V-M7-STORE-OK";

/// Datastore GAP closed in M7.2.
pub const STORE_GAP_NOTE: &str = "GAP(CLOSED M7.2): Datastore";

/// ESP-relative image library directory (R640 / USB FAT32 layout).
pub const ESP_IMAGES_REL: &str = "EFI/RAYNU/images";

/// Catalog filename under [`ESP_IMAGES_REL`].
pub const CATALOG_FILE: &str = "catalog.txt";

/// Max images in the management-plane library.
pub const IMAGE_CAP: usize = 32;

/// Max UTF-8 bytes for an image display name.
pub const IMAGE_NAME_CAP: usize = 64;

/// Image kind for the library (ISO install media, disk, template).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Iso = 1,
    Disk = 2,
    Template = 3,
}

impl ImageKind {
    pub fn tag(self) -> u8 {
        self as u8
    }

    pub fn from_tag(t: u8) -> Option<Self> {
        match t {
            1 => Some(ImageKind::Iso),
            2 => Some(ImageKind::Disk),
            3 => Some(ImageKind::Template),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ImageKind::Iso => "iso",
            ImageKind::Disk => "disk",
            ImageKind::Template => "template",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "iso" => Some(ImageKind::Iso),
            "disk" => Some(ImageKind::Disk),
            "template" => Some(ImageKind::Template),
            _ => None,
        }
    }
}

/// Error from datastore mutations / persist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InvalidId,
    Full,
    NotFound,
    BadState,
    BadName,
    UnsupportedOnFirmware,
    Io,
}

/// One image slot in the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageRecord {
    pub id: u64,
    pub kind: ImageKind,
    pub size_bytes: u64,
    name: [u8; IMAGE_NAME_CAP],
    name_len: usize,
}

impl ImageRecord {
    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    fn with_name(id: u64, kind: ImageKind, size_bytes: u64, name: &str) -> Result<Self, StoreError> {
        let nb = name.as_bytes();
        if nb.is_empty() || nb.len() > IMAGE_NAME_CAP {
            return Err(StoreError::BadName);
        }
        let mut buf = [0u8; IMAGE_NAME_CAP];
        buf[..nb.len()].copy_from_slice(nb);
        Ok(Self {
            id,
            kind,
            size_bytes,
            name: buf,
            name_len: nb.len(),
        })
    }
}

/// Fixed-capacity image library table.
pub struct ImageTable {
    slots: [Option<ImageRecord>; IMAGE_CAP],
    len: usize,
}

impl ImageTable {
    pub const fn new() -> Self {
        Self {
            slots: [None; IMAGE_CAP],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, id: u64) -> Option<&ImageRecord> {
        self.slots.iter().flatten().find(|r| r.id == id)
    }

    /// Register an image (ISO / disk / template metadata).
    pub fn register(
        &mut self,
        id: u64,
        kind: ImageKind,
        size_bytes: u64,
        name: &str,
    ) -> Result<(), StoreError> {
        let _ = (STORE_GAP_NOTE, M7_STORE_OK_MARKER, ESP_IMAGES_REL);
        if id == 0 {
            return Err(StoreError::InvalidId);
        }
        if self.get(id).is_some() {
            return Err(StoreError::BadState);
        }
        let rec = ImageRecord::with_name(id, kind, size_bytes, name)?;
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                *slot = Some(rec);
                self.len += 1;
                return Ok(());
            }
        }
        Err(StoreError::Full)
    }

    /// Delete an image by id.
    pub fn delete(&mut self, id: u64) -> Result<(), StoreError> {
        for slot in self.slots.iter_mut() {
            if let Some(r) = slot {
                if r.id == id {
                    *slot = None;
                    self.len = self.len.saturating_sub(1);
                    return Ok(());
                }
            }
        }
        Err(StoreError::NotFound)
    }

    /// Iterate registered images (stable slot order).
    pub fn for_each<F: FnMut(&ImageRecord)>(&self, mut f: F) {
        for slot in self.slots.iter().flatten() {
            f(slot);
        }
    }
}

impl Default for ImageTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Firmware persist entry — SimpleFileSystem / NVMe write not wired yet.
pub fn persist_catalog_uefi(_store: &ImageTable) -> Result<(), StoreError> {
    let _ = (STORE_GAP_NOTE, ESP_IMAGES_REL, CATALOG_FILE);
    Err(StoreError::UnsupportedOnFirmware)
}

fn parse_u64(s: &str) -> Option<u64> {
    let mut n: u64 = 0;
    if s.is_empty() {
        return None;
    }
    for b in s.bytes() {
        if !(b'0'..=b'9').contains(&b) {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(n)
}

enum StoreOp {
    List,
    Get { id: u64 },
    Register { id: u64, kind: ImageKind },
    Delete { id: u64 },
}

fn route_store(method: RestMethod, path: &str) -> Result<StoreOp, ()> {
    let path = path.trim().trim_end_matches('/');
    if path == "/images" {
        return match method {
            RestMethod::Get => Ok(StoreOp::List),
            _ => Err(()),
        };
    }
    let rest = path.strip_prefix("/images/").ok_or(())?;
    let mut segs = rest.split('/');
    let id_s = segs.next().ok_or(())?;
    let id = parse_u64(id_s).ok_or(())?;
    let kind_s = segs.next();
    if segs.next().is_some() {
        return Err(());
    }
    match (method, kind_s) {
        (RestMethod::Get, None) => Ok(StoreOp::Get { id }),
        (RestMethod::Post, None) => Ok(StoreOp::Register {
            id,
            kind: ImageKind::Iso,
        }),
        (RestMethod::Post, Some(k)) => {
            let kind = ImageKind::parse(k).ok_or(())?;
            Ok(StoreOp::Register { id, kind })
        }
        (RestMethod::Delete, None) => Ok(StoreOp::Delete { id }),
        _ => Err(()),
    }
}

fn default_name(id: u64, kind: ImageKind) -> heapless_name::NameBuf {
    heapless_name::format_default(id, kind)
}

/// Tiny no-alloc name helper (avoid pulling alloc into UEFI).
mod heapless_name {
    use super::{ImageKind, IMAGE_NAME_CAP};

    pub struct NameBuf {
        buf: [u8; IMAGE_NAME_CAP],
        len: usize,
    }

    impl NameBuf {
        pub fn as_str(&self) -> &str {
            core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
        }
    }

    pub fn format_default(id: u64, kind: ImageKind) -> NameBuf {
        let mut buf = [0u8; IMAGE_NAME_CAP];
        let prefix = kind.as_str().as_bytes();
        let mut len = 0;
        for &b in prefix {
            if len < IMAGE_NAME_CAP {
                buf[len] = b;
                len += 1;
            }
        }
        if len < IMAGE_NAME_CAP {
            buf[len] = b'-';
            len += 1;
        }
        // decimal id
        let mut tmp = [0u8; 20];
        let mut i = 20;
        let mut v = id;
        if v == 0 {
            if len < IMAGE_NAME_CAP {
                buf[len] = b'0';
                len += 1;
            }
        } else {
            while v > 0 && i > 0 {
                i -= 1;
                tmp[i] = b'0' + (v % 10) as u8;
                v /= 10;
            }
            for &b in &tmp[i..] {
                if len < IMAGE_NAME_CAP {
                    buf[len] = b;
                    len += 1;
                }
            }
        }
        NameBuf { buf, len }
    }
}

/// REST dispatch for `/images` (Bearer auth, same mock token as VM API).
pub fn dispatch_store_rest(store: &mut ImageTable, req: RestRequest<'_>) -> RestResponse {
    if !auth_allows(req.auth_token) {
        return RestResponse {
            status: 401,
            reply: None,
        };
    }
    match route_store(req.method, req.path) {
        Ok(StoreOp::List) => RestResponse {
            status: 200,
            reply: Some(ApiReply::Listed { count: store.len() }),
        },
        Ok(StoreOp::Get { id }) => match store.get(id) {
            Some(r) => RestResponse {
                status: 200,
                reply: Some(ApiReply::Image {
                    id: r.id,
                    kind_tag: r.kind.tag(),
                    size_bytes: r.size_bytes,
                }),
            },
            None => RestResponse {
                status: 404,
                reply: None,
            },
        },
        Ok(StoreOp::Register { id, kind }) => {
            let name = default_name(id, kind);
            match store.register(id, kind, 0, name.as_str()) {
                Ok(()) => RestResponse {
                    status: 201,
                    reply: Some(ApiReply::Ok),
                },
                Err(StoreError::BadState) | Err(StoreError::InvalidId) | Err(StoreError::BadName) => {
                    RestResponse {
                        status: 409,
                        reply: None,
                    }
                }
                Err(StoreError::Full) => RestResponse {
                    status: 507,
                    reply: None,
                },
                Err(_) => RestResponse {
                    status: 500,
                    reply: None,
                },
            }
        }
        Ok(StoreOp::Delete { id }) => match store.delete(id) {
            Ok(()) => RestResponse {
                status: 200,
                reply: Some(ApiReply::Ok),
            },
            Err(StoreError::NotFound) => RestResponse {
                status: 404,
                reply: None,
            },
            Err(_) => RestResponse {
                status: 500,
                reply: None,
            },
        },
        Err(()) => RestResponse {
            status: 400,
            reply: None,
        },
    }
}

/// Encode catalog lines: `id|kind|size|name` (one per image).
pub fn catalog_encode(store: &ImageTable, out: &mut [u8]) -> Result<usize, StoreError> {
    let mut n = 0;
    let mut err = false;
    store.for_each(|r| {
        if err {
            return;
        }
        let line = format_catalog_line(r);
        let lb = line.as_bytes();
        if n + lb.len() > out.len() {
            err = true;
            return;
        }
        out[n..n + lb.len()].copy_from_slice(lb);
        n += lb.len();
    });
    if err {
        Err(StoreError::Io)
    } else {
        Ok(n)
    }
}

fn format_catalog_line(r: &ImageRecord) -> heapless_line::LineBuf {
    heapless_line::encode(r)
}

mod heapless_line {
    use super::{ImageRecord, IMAGE_NAME_CAP};

    pub struct LineBuf {
        buf: [u8; 32 + IMAGE_NAME_CAP],
        len: usize,
    }

    impl LineBuf {
        pub fn as_bytes(&self) -> &[u8] {
            &self.buf[..self.len]
        }
    }

    fn push(buf: &mut [u8], len: &mut usize, s: &[u8]) {
        let n = s.len().min(buf.len().saturating_sub(*len));
        buf[*len..*len + n].copy_from_slice(&s[..n]);
        *len += n;
    }

    fn push_u64(buf: &mut [u8], len: &mut usize, mut v: u64) {
        let mut tmp = [0u8; 20];
        let mut i = 20;
        if v == 0 {
            push(buf, len, b"0");
            return;
        }
        while v > 0 && i > 0 {
            i -= 1;
            tmp[i] = b'0' + (v % 10) as u8;
            v /= 10;
        }
        push(buf, len, &tmp[i..]);
    }

    pub fn encode(r: &ImageRecord) -> LineBuf {
        let mut buf = [0u8; 32 + IMAGE_NAME_CAP];
        let mut len = 0;
        push_u64(&mut buf, &mut len, r.id);
        push(&mut buf, &mut len, b"|");
        push_u64(&mut buf, &mut len, u64::from(r.kind.tag()));
        push(&mut buf, &mut len, b"|");
        push_u64(&mut buf, &mut len, r.size_bytes);
        push(&mut buf, &mut len, b"|");
        push(&mut buf, &mut len, r.name().as_bytes());
        push(&mut buf, &mut len, b"\n");
        LineBuf { buf, len }
    }
}

/// Decode catalog text into `store` (clears first).
pub fn catalog_decode(store: &mut ImageTable, text: &str) -> Result<(), StoreError> {
    *store = ImageTable::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, '|');
        let id = parse_u64(parts.next().ok_or(StoreError::Io)?).ok_or(StoreError::Io)?;
        let kind_n = parse_u64(parts.next().ok_or(StoreError::Io)?).ok_or(StoreError::Io)?;
        let size = parse_u64(parts.next().ok_or(StoreError::Io)?).ok_or(StoreError::Io)?;
        let name = parts.next().ok_or(StoreError::Io)?;
        let kind = ImageKind::from_tag(kind_n as u8).ok_or(StoreError::Io)?;
        store.register(id, kind, size, name)?;
    }
    Ok(())
}

/// Host-only: write catalog under `{root}/EFI/RAYNU/images/catalog.txt`.
#[cfg(test)]
pub fn persist_catalog_host(root: &std::path::Path, store: &ImageTable) -> Result<(), StoreError> {
    use std::fs;
    use std::io::Write;

    let dir = root.join(ESP_IMAGES_REL);
    fs::create_dir_all(&dir).map_err(|_| StoreError::Io)?;
    let mut buf = [0u8; 4096];
    let n = catalog_encode(store, &mut buf)?;
    let path = dir.join(CATALOG_FILE);
    let mut f = fs::File::create(&path).map_err(|_| StoreError::Io)?;
    f.write_all(&buf[..n]).map_err(|_| StoreError::Io)?;
    Ok(())
}

/// Host-only: load catalog from `{root}/EFI/RAYNU/images/catalog.txt`.
#[cfg(test)]
pub fn load_catalog_host(root: &std::path::Path, store: &mut ImageTable) -> Result<(), StoreError> {
    use std::fs;

    let path = root.join(ESP_IMAGES_REL).join(CATALOG_FILE);
    let text = fs::read_to_string(&path).map_err(|_| StoreError::Io)?;
    catalog_decode(store, &text)
}

/// Host-testable: register / list / delete + REST shapes.
pub fn prop_datastore_package() -> bool {
    let _ = BRINGUP_AUTH_TOKEN;
    let mut store = ImageTable::new();
    if store
        .register(1, ImageKind::Iso, 700_000_000, "ubuntu.iso")
        .is_err()
    {
        return false;
    }
    if store
        .register(2, ImageKind::Disk, 20_000_000_000, "vm-disk")
        .is_err()
    {
        return false;
    }
    if store.len() != 2 || store.get(1).map(|r| r.kind) != Some(ImageKind::Iso) {
        return false;
    }
    if store.delete(2).is_err() || store.len() != 1 || store.get(2).is_some() {
        return false;
    }

    let tok = Some(BRINGUP_AUTH_TOKEN);
    let list = dispatch_store_rest(
        &mut store,
        RestRequest {
            method: RestMethod::Get,
            path: "/images",
            auth_token: tok,
        },
    );
    if list.status != 200 || list.reply != Some(ApiReply::Listed { count: 1 }) {
        return false;
    }
    let denied = dispatch_store_rest(
        &mut store,
        RestRequest {
            method: RestMethod::Post,
            path: "/images/9/iso",
            auth_token: None,
        },
    );
    if denied.status != 401 {
        return false;
    }
    let created = dispatch_store_rest(
        &mut store,
        RestRequest {
            method: RestMethod::Post,
            path: "/images/9/template",
            auth_token: tok,
        },
    );
    if created.status != 201 || store.get(9).map(|r| r.kind) != Some(ImageKind::Template) {
        return false;
    }
    if persist_catalog_uefi(&store) != Err(StoreError::UnsupportedOnFirmware) {
        return false;
    }

    let mut buf = [0u8; 512];
    let n = match catalog_encode(&store, &mut buf) {
        Ok(n) => n,
        Err(_) => return false,
    };
    let text = match core::str::from_utf8(&buf[..n]) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let mut loaded = ImageTable::new();
    if catalog_decode(&mut loaded, text).is_err() {
        return false;
    }
    loaded.len() == store.len()
        && loaded.get(1).is_some()
        && loaded.get(9).is_some()
        && STORE_GAP_NOTE.contains("CLOSED M7.2")
        && M7_STORE_OK_MARKER == "RAYNU-V-M7-STORE-OK"
        && ESP_IMAGES_REL.contains("EFI/RAYNU/images")
}

#[cfg(test)]
#[path = "datastore_test.rs"]
mod datastore_test;
