use super::*;

fn out(code: i32, stdout: &str, stderr: &str) -> RunOutput {
    RunOutput {
        exit_code: code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
    }
}

#[test]
fn result_is_read_from_stdout() {
    let v = interpret(&out(
        0,
        r#"{"id":"cli:agent:list","result":{"type":"agent_list","agents":[]}}"#,
        "",
    ))
    .unwrap();
    assert_eq!(v.get("type").and_then(Value::as_str), Some("agent_list"));
}

#[test]
fn error_envelope_on_stderr_is_mapped_by_code() {
    let e = interpret(&out(
        1,
        "",
        r#"{"error":{"code":"agent_not_found","message":"whatever"},"id":"x"}"#,
    ))
    .unwrap_err();
    assert_eq!(e.code, HostErrorCode::NotFound);
}

#[test]
fn usage_text_with_exit_2_is_bad_argument() {
    let e = interpret(&out(2, "", "usage: herdr agent get <target>")).unwrap_err();
    assert_eq!(e.code, HostErrorCode::BadArgument);
    assert!(e.detail.contains("usage: herdr agent get"));
}

#[test]
fn garbage_on_exit_0_is_other() {
    let e = interpret(&out(0, "<<not json>>", "")).unwrap_err();
    assert_eq!(e.code, HostErrorCode::Other);
}

#[test]
fn a_timeout_is_timeout() {
    let mut o = out(124, "", "");
    o.timed_out = true;
    assert_eq!(interpret(&o).unwrap_err().code, HostErrorCode::Timeout);
}

#[test]
fn a_read_returns_herdrs_plain_text() {
    // Real herdr 0.9.0 prints the screen with no envelope and no final newline.
    let screen = " ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel";
    assert_eq!(interpret_text(&out(0, screen, "")).unwrap(), screen);
}

#[test]
fn a_read_of_a_screen_showing_json_is_still_text() {
    let screen = r#"{"error":{"code":"agent_not_found","message":"x"},"id":"y"}"#;
    assert_eq!(interpret_text(&out(0, screen, "")).unwrap(), screen);
}

#[test]
fn a_failed_read_is_mapped_like_any_other_call() {
    let e = interpret_text(&out(
        1,
        "",
        r#"{"error":{"code":"agent_not_found","message":"gone"},"id":"cli:agent:read"}"#,
    ))
    .unwrap_err();
    assert_eq!(e.code, HostErrorCode::NotFound);
    let e = interpret_text(&out(2, "", "usage: herdr agent read <TARGET>")).unwrap_err();
    assert_eq!(e.code, HostErrorCode::BadArgument);
    let mut o = out(124, "", "");
    o.timed_out = true;
    assert_eq!(interpret_text(&o).unwrap_err().code, HostErrorCode::Timeout);
}
