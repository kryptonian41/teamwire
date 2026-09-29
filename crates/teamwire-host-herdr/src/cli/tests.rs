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
