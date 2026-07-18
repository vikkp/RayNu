//! Audit report generator (SOX/ISO templates) — outside Proven Core.
//!
//! Pillar: [A]
//! Proven Core: **outside** (ADR-002)
//! VERIFICATION: N/A

/// Report output format stub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Json,
    Csv,
    Pdf,
}

/// Placeholder report builder — templates land in M5.
pub fn render_stub(format: ReportFormat) -> &'static str {
    match format {
        ReportFormat::Json => "{\"status\":\"scaffold\"}",
        ReportFormat::Csv => "status,scaffold",
        ReportFormat::Pdf => "%PDF-stub",
    }
}

#[cfg(test)]
#[path = "report_test.rs"]
mod report_test;
