use super::{overview_json, prop_host_overview, HOST_OVERVIEW_PATH, OVERVIEW_BODY_CAP};

#[test]
fn overview_is_the_five_objects_under_one_root() {
    let mut buf = [0u8; OVERVIEW_BODY_CAP];
    let n = overview_json(&mut buf);
    assert!(n > 0);
    let body = core::str::from_utf8(&buf[..n]).unwrap();
    assert!(
        body.starts_with("{\"ok\":true,\"guests\":{\"spare_bytes\":"),
        "{body}"
    );
    assert!(body.contains("\"guests\":[]"), "{body}");
    assert!(body.contains(",\"addr\":{\"ok\":true,\"saved\":"), "{body}");
    assert!(
        body.contains(",\"clock\":{\"ok\":true,\"synced\":"),
        "{body}"
    );
    assert!(
        body.contains(",\"operator\":{\"ok\":true,\"set\":"),
        "{body}"
    );
    assert!(
        body.contains(",\"cert\":{\"ok\":true,\"source\":"),
        "{body}"
    );
    assert!(body.ends_with("}}"), "{body}");
    assert!(!body.contains("\"token\""), "{body}");
    assert_eq!(body.matches('{').count(), body.matches('}').count());
}

#[test]
fn a_short_buffer_sends_nothing() {
    let mut full = [0u8; OVERVIEW_BODY_CAP];
    let n = overview_json(&mut full);
    assert!(n > 0);
    let mut short = [0u8; 64];
    assert_eq!(overview_json(&mut short), 0);
    let mut almost = vec![0u8; n - 1];
    assert_eq!(overview_json(&mut almost), 0);
    let mut exact = vec![0u8; n];
    assert_eq!(overview_json(&mut exact), n);
}

#[test]
fn the_page_polls_this_route_and_not_the_five() {
    assert_eq!(HOST_OVERVIEW_PATH, "/host/overview");
    assert!(prop_host_overview());
}
