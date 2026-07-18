use super::*;

#[test]
fn json_stub_nonempty() {
    assert!(!render_stub(ReportFormat::Json).is_empty());
}
