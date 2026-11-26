use std::process::{Command, Output, Stdio};

fn wiki(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_wiki"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run wiki")
}

#[test]
fn no_arguments_prints_plain_help() {
    let out = wiki(&[]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Usage: wiki [OPTIONS] <QUERY>"));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn boolean_flags_reject_values() {
    let out = wiki(&["--json=unexpected", "rust"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--json does not take a value"));
}

#[test]
fn option_terminator_allows_hyphenated_queries() {
    let out = wiki(&["--lang", "../evil", "--", "--help"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid Wikipedia language code"));
}

#[test]
fn rejects_unknown_sources_before_fetching() {
    let out = wiki(&["--source", "example", "rust"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown source"));
}

#[cfg(unix)]
#[test]
fn rejects_non_utf8_arguments_without_panicking() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let out = Command::new(env!("CARGO_BIN_EXE_wiki"))
        .arg(OsString::from_vec(vec![b'r', 0x80]))
        .stdin(Stdio::null())
        .output()
        .expect("failed to run wiki");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("valid UTF-8"));
}
