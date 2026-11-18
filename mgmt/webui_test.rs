use super::*;

#[test]
fn webui_embedded_and_branded() {
    assert!(webui_present());
    assert!(webui_len() > 500);
    assert!(webui_html_wires_api());
    assert!(spa_operator_surface_present());
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

/// The page is one script. A bracket that does not close leaves Sign in
/// with no handler and the tiles on their HTML defaults.
#[test]
fn spa_script_brackets_balance() {
    let html = include_str!("../assets/webui.html");
    let start = html.find("<script>").expect("script open");
    let body = &html[start + "<script>".len()..];
    let end = body.find("</script>").expect("script close");
    let js = &body[..end];
    assert!(
        js.contains("async function saveCpu"),
        "vCPU save is a function the Guests row calls"
    );
    assert!(
        bracket_fault(js).is_none(),
        "SPA script brackets: {:?}",
        bracket_fault(js)
    );
}

fn bracket_fault(js: &str) -> Option<String> {
    let b = js.as_bytes();
    let mut i = 0;
    let mut stack: Vec<u8> = Vec::new();
    let mut prev = b' ';
    while i < b.len() {
        let c = b[i];
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            prev = b'\n';
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
            prev = b' ';
            continue;
        }
        if c == b'\'' || c == b'"' {
            let q = c;
            i += 1;
            while i < b.len() && b[i] != q {
                if b[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            i = (i + 1).min(b.len());
            prev = q;
            continue;
        }
        if c == b'/' && regex_here(prev) {
            i += 1;
            while i < b.len() && b[i] != b'/' {
                if b[i] == b'\\' {
                    i += 2;
                } else if b[i] == b'[' {
                    i += 1;
                    while i < b.len() && b[i] != b']' {
                        if b[i] == b'\\' {
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    i = (i + 1).min(b.len());
                } else {
                    i += 1;
                }
            }
            i = (i + 1).min(b.len());
            while i < b.len() && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            prev = b'/';
            continue;
        }
        match c {
            b'(' | b'[' | b'{' => stack.push(c),
            b')' => {
                if stack.pop() != Some(b'(') {
                    return Some(format!("')' at {i}"));
                }
            }
            b']' => {
                if stack.pop() != Some(b'[') {
                    return Some(format!("']' at {i}"));
                }
            }
            b'}' => {
                if stack.pop() != Some(b'{') {
                    return Some(format!("'}}' at {i}"));
                }
            }
            _ => {}
        }
        if !c.is_ascii_whitespace() {
            prev = c;
        }
        i += 1;
    }
    if stack.is_empty() {
        None
    } else {
        Some(format!("unclosed {stack:?}"))
    }
}

fn regex_here(prev: u8) -> bool {
    !matches!(
        prev,
        b')' | b']'
            | b'}'
            | b'0'..=b'9'
            | b'a'..=b'z'
            | b'A'..=b'Z'
            | b'_'
            | b'$'
    )
}
