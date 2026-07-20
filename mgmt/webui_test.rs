use super::*;

#[test]
fn webui_embedded_and_branded() {
    assert!(webui_present());
    assert!(webui_len() > 500);
    assert!(webui_html_wires_api());
    assert_eq!(SECTION_WEBUI, ".aswebui");
    assert!(SECTION_WEBUI.len() <= 8);
}

#[test]
fn lazy_load_latches_on_first_use() {
    reset_webui_loaded_for_test();
    assert!(!webui_was_loaded());
    let a = load_webui().expect("webui");
    assert!(webui_was_loaded());
    let b = load_webui().expect("webui again");
    assert_eq!(a.as_ptr(), b.as_ptr());
}

#[test]
fn webui_list_start_stop() {
    assert!(prop_webui_list_start_stop());
}

#[test]
fn zstd_gap_documented() {
    assert!(WEBUI_ZSTD_GAP_NOTE.contains("zstd"));
}
