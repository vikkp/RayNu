//! Audit report generator (SOX / ISO-style templates) — outside Proven Core.
//!
//! Pillar: [A]
//! Proven Core: **outside** (ADR-002); consumes sealed `AuditRing` snapshots
//! VERIFICATION: N/A
//!
//! M5.4: deterministic JSON/CSV reports from a frozen ring snapshot, driven by
//! embedded schemas (ADR-003 `.assets.schemas` → PE `.aschema`).
//! PDF remains a documented GAP → M6 (external auditor sign-off also M6).

use super::integrity::{AuditEvent, AuditRing};

/// Host / CI marker when the M5.4 report gate passes.
pub const M5_REPORT_OK_MARKER: &str = "RAYNU-V-M5-REPORT-OK";

/// PE section name for embedded report schemas (ADR-003 `.assets.schemas`).
pub const SECTION_SCHEMAS: &str = ".aschema";

/// Documented PDF gap (JSON/CSV close M5.4).
pub const PDF_GAP_NOTE: &str = "GAP: PDF report → M6";

/// SOX-style schema (embedded text + PE).
pub const SCHEMA_SOX: &str = include_str!("../assets/schemas/sox_access_control.json");
/// ISO-style schema (embedded text + PE).
pub const SCHEMA_ISO: &str = include_str!("../assets/schemas/iso_event_inventory.json");

#[link_section = ".aschema"]
#[used]
static PE_SCHEMA_SOX: [u8; include_bytes!("../assets/schemas/sox_access_control.json").len()] =
    *include_bytes!("../assets/schemas/sox_access_control.json");

#[link_section = ".aschema"]
#[used]
static PE_SCHEMA_ISO: [u8; include_bytes!("../assets/schemas/iso_event_inventory.json").len()] =
    *include_bytes!("../assets/schemas/iso_event_inventory.json");

/// Report output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Json,
    Csv,
    /// Optional — not required to close M5.4 (`GAP: PDF report → M6`).
    Pdf,
}

/// Auditor-facing report kind (one SOX-style, one ISO-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    /// SOX-style access / control-change summary.
    SoxAccessControl,
    /// ISO-style security event inventory.
    IsoEventInventory,
}

/// Error from report rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportError {
    BufferTooSmall,
    PdfNotImplemented,
    BadRing,
}

/// Frozen, deterministic view of a ring for report generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RingSnapshot {
    pub tip_hash: u64,
    pub event_count: u32,
    pub chain_ok: bool,
    pub vmcs: u32,
    pub ept_map: u32,
    pub ept_unmap: u32,
    pub msr_block: u32,
    pub lifecycle: u32,
    pub other: u32,
}

impl RingSnapshot {
    /// Build a snapshot from a sealed ring (must verify before use for reports).
    pub fn from_ring(ring: &AuditRing) -> Self {
        let mut snap = Self {
            tip_hash: ring.tip_hash(),
            event_count: ring.len() as u32,
            chain_ok: ring.verify_chain(),
            vmcs: 0,
            ept_map: 0,
            ept_unmap: 0,
            msr_block: 0,
            lifecycle: 0,
            other: 0,
        };
        for i in 0..ring.len() {
            let Some(rec) = ring.get(i) else {
                snap.chain_ok = false;
                continue;
            };
            match rec.event {
                AuditEvent::VmcsCreated { .. } => snap.vmcs += 1,
                AuditEvent::EptMapped { .. } => snap.ept_map += 1,
                AuditEvent::EptUnmapped { .. } => snap.ept_unmap += 1,
                AuditEvent::MsrBlocked { .. } => snap.msr_block += 1,
                AuditEvent::VmCreated { .. }
                | AuditEvent::VmStarted { .. }
                | AuditEvent::VmStopped { .. }
                | AuditEvent::VmDestroyed { .. } => snap.lifecycle += 1,
                _ => snap.other += 1,
            }
        }
        snap
    }

    pub fn lifecycle_mutations(&self) -> u32 {
        self.lifecycle
    }
}

/// True when both SOX and ISO schemas are present and name their standards.
pub fn schemas_present() -> bool {
    !PE_SCHEMA_SOX.is_empty()
        && !PE_SCHEMA_ISO.is_empty()
        && SCHEMA_SOX.contains("sox_access_control")
        && SCHEMA_SOX.contains("SOX-style")
        && SCHEMA_ISO.contains("iso_event_inventory")
        && SCHEMA_ISO.contains("ISO-style")
        && SECTION_SCHEMAS.len() <= 8
        && SECTION_SCHEMAS == ".aschema"
}

/// Render a report into `out`; returns bytes written.
pub fn render_report(
    kind: ReportKind,
    format: ReportFormat,
    snap: &RingSnapshot,
    out: &mut [u8],
) -> Result<usize, ReportError> {
    if !snap.chain_ok {
        return Err(ReportError::BadRing);
    }
    match format {
        ReportFormat::Pdf => {
            let _ = PDF_GAP_NOTE;
            Err(ReportError::PdfNotImplemented)
        }
        ReportFormat::Json => match kind {
            ReportKind::SoxAccessControl => render_sox_json(snap, out),
            ReportKind::IsoEventInventory => render_iso_json(snap, out),
        },
        ReportFormat::Csv => match kind {
            ReportKind::SoxAccessControl => render_sox_csv(snap, out),
            ReportKind::IsoEventInventory => render_iso_csv(snap, out),
        },
    }
}

fn render_sox_json(snap: &RingSnapshot, out: &mut [u8]) -> Result<usize, ReportError> {
    let mut w = Writer::new(out);
    w.push_str("{\"report\":\"sox_access_control\",\"schema\":\"sox_access_control/v1\",")?;
    w.push_str("\"tip_hash\":\"0x")?;
    w.push_hex_u64(snap.tip_hash)?;
    w.push_str("\",\"event_count\":")?;
    w.push_u32(snap.event_count)?;
    w.push_str(",\"vmcs_created\":")?;
    w.push_u32(snap.vmcs)?;
    w.push_str(",\"lifecycle_mutations\":")?;
    w.push_u32(snap.lifecycle_mutations())?;
    w.push_str(",\"msr_blocks\":")?;
    w.push_u32(snap.msr_block)?;
    w.push_str(",\"chain_ok\":")?;
    w.push_str(if snap.chain_ok { "true" } else { "false" })?;
    w.push_str("}")?;
    Ok(w.pos)
}

fn render_iso_json(snap: &RingSnapshot, out: &mut [u8]) -> Result<usize, ReportError> {
    let mut w = Writer::new(out);
    w.push_str("{\"report\":\"iso_event_inventory\",\"schema\":\"iso_event_inventory/v1\",")?;
    w.push_str("\"tip_hash\":\"0x")?;
    w.push_hex_u64(snap.tip_hash)?;
    w.push_str("\",\"total\":")?;
    w.push_u32(snap.event_count)?;
    w.push_str(",\"vmcs\":")?;
    w.push_u32(snap.vmcs)?;
    w.push_str(",\"ept_map\":")?;
    w.push_u32(snap.ept_map)?;
    w.push_str(",\"ept_unmap\":")?;
    w.push_u32(snap.ept_unmap)?;
    w.push_str(",\"msr_block\":")?;
    w.push_u32(snap.msr_block)?;
    w.push_str(",\"lifecycle\":")?;
    w.push_u32(snap.lifecycle)?;
    w.push_str(",\"other\":")?;
    w.push_u32(snap.other)?;
    w.push_str(",\"chain_ok\":")?;
    w.push_str(if snap.chain_ok { "true" } else { "false" })?;
    w.push_str("}")?;
    Ok(w.pos)
}

fn render_sox_csv(snap: &RingSnapshot, out: &mut [u8]) -> Result<usize, ReportError> {
    let mut w = Writer::new(out);
    w.push_str("report,schema,tip_hash,event_count,vmcs_created,lifecycle_mutations,msr_blocks,chain_ok\n")?;
    w.push_str("sox_access_control,sox_access_control/v1,0x")?;
    w.push_hex_u64(snap.tip_hash)?;
    w.push_str(",")?;
    w.push_u32(snap.event_count)?;
    w.push_str(",")?;
    w.push_u32(snap.vmcs)?;
    w.push_str(",")?;
    w.push_u32(snap.lifecycle_mutations())?;
    w.push_str(",")?;
    w.push_u32(snap.msr_block)?;
    w.push_str(",")?;
    w.push_str(if snap.chain_ok { "true" } else { "false" })?;
    w.push_str("\n")?;
    Ok(w.pos)
}

fn render_iso_csv(snap: &RingSnapshot, out: &mut [u8]) -> Result<usize, ReportError> {
    let mut w = Writer::new(out);
    w.push_str(
        "report,schema,tip_hash,total,vmcs,ept_map,ept_unmap,msr_block,lifecycle,other,chain_ok\n",
    )?;
    w.push_str("iso_event_inventory,iso_event_inventory/v1,0x")?;
    w.push_hex_u64(snap.tip_hash)?;
    w.push_str(",")?;
    w.push_u32(snap.event_count)?;
    w.push_str(",")?;
    w.push_u32(snap.vmcs)?;
    w.push_str(",")?;
    w.push_u32(snap.ept_map)?;
    w.push_str(",")?;
    w.push_u32(snap.ept_unmap)?;
    w.push_str(",")?;
    w.push_u32(snap.msr_block)?;
    w.push_str(",")?;
    w.push_u32(snap.lifecycle)?;
    w.push_str(",")?;
    w.push_u32(snap.other)?;
    w.push_str(",")?;
    w.push_str(if snap.chain_ok { "true" } else { "false" })?;
    w.push_str("\n")?;
    Ok(w.pos)
}

struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn push_str(&mut self, s: &str) -> Result<(), ReportError> {
        let b = s.as_bytes();
        if self.pos + b.len() > self.buf.len() {
            return Err(ReportError::BufferTooSmall);
        }
        self.buf[self.pos..self.pos + b.len()].copy_from_slice(b);
        self.pos += b.len();
        Ok(())
    }

    fn push_u32(&mut self, mut n: u32) -> Result<(), ReportError> {
        let mut tmp = [0u8; 10];
        let mut i = tmp.len();
        if n == 0 {
            return self.push_str("0");
        }
        while n > 0 {
            i -= 1;
            tmp[i] = b'0' + (n % 10) as u8;
            n /= 10;
        }
        let s = core::str::from_utf8(&tmp[i..]).unwrap_or("0");
        self.push_str(s)
    }

    fn push_hex_u64(&mut self, n: u64) -> Result<(), ReportError> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut tmp = [0u8; 16];
        for (i, slot) in tmp.iter_mut().enumerate() {
            let shift = 60 - (i * 4);
            *slot = HEX[((n >> shift) & 0xf) as usize];
        }
        let s = core::str::from_utf8(&tmp).unwrap_or("0");
        self.push_str(s)
    }
}

/// Fill `ring` with a deterministic mandatory sample for report props.
fn sample_ring() -> AuditRing {
    let mut ring = AuditRing::new();
    let _ = ring.append(AuditEvent::VmcsCreated {
        vcpu_id: 0,
        vmcs_id: 1,
    });
    let _ = ring.append(AuditEvent::EptMapped {
        guest_id: 1,
        gpa: 0x1000,
        hpa: 0x2000,
    });
    let _ = ring.append(AuditEvent::EptUnmapped {
        guest_id: 1,
        gpa: 0x1000,
        hpa: 0x2000,
    });
    let _ = ring.append(AuditEvent::MsrBlocked {
        vcpu_id: 0,
        msr_index: 0x3A,
    });
    let _ = ring.append(AuditEvent::VmCreated { guest_id: 1 });
    let _ = ring.append(AuditEvent::VmStarted { guest_id: 1 });
    let _ = ring.append(AuditEvent::VmStopped { guest_id: 1 });
    let _ = ring.append(AuditEvent::VmDestroyed { guest_id: 1 });
    ring
}

/// Host-testable: SOX + ISO JSON/CSV render; same snapshot → identical bytes.
pub fn prop_reports_deterministic() -> bool {
    if !schemas_present() {
        return false;
    }
    let ring = sample_ring();
    if !ring.verify_chain() {
        return false;
    }
    let snap = RingSnapshot::from_ring(&ring);
    if !snap.chain_ok || snap.event_count != 8 {
        return false;
    }

    let mut a = [0u8; 512];
    let mut b = [0u8; 512];
    for &(kind, format) in &[
        (ReportKind::SoxAccessControl, ReportFormat::Json),
        (ReportKind::SoxAccessControl, ReportFormat::Csv),
        (ReportKind::IsoEventInventory, ReportFormat::Json),
        (ReportKind::IsoEventInventory, ReportFormat::Csv),
    ] {
        let na = match render_report(kind, format, &snap, &mut a) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let nb = match render_report(kind, format, &snap, &mut b) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if na != nb || a[..na] != b[..nb] {
            return false;
        }
        // Sanity: output mentions the report id.
        let Ok(text) = core::str::from_utf8(&a[..na]) else {
            return false;
        };
        match kind {
            ReportKind::SoxAccessControl => {
                if !text.contains("sox_access_control") {
                    return false;
                }
            }
            ReportKind::IsoEventInventory => {
                if !text.contains("iso_event_inventory") {
                    return false;
                }
            }
        }
    }

    // PDF is explicitly not implemented for M5.4 close.
    matches!(
        render_report(
            ReportKind::SoxAccessControl,
            ReportFormat::Pdf,
            &snap,
            &mut a
        ),
        Err(ReportError::PdfNotImplemented)
    ) && PDF_GAP_NOTE.contains("M6")
        && M5_REPORT_OK_MARKER == "RAYNU-V-M5-REPORT-OK"
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
