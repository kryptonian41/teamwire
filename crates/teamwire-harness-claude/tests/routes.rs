//! INV-4: a repository setting can hold back Claude's cross-session
//! messages, so the adapter never offers that route. Members are woken by
//! typing through the host.

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{Harness, HostErrorCode, SessionId, one_line};
use teamwire_fakes::FakePlatform;
use teamwire_harness_claude::{ClaudeCode, ClaudeConfig};

fn claude() -> ClaudeCode {
    ClaudeCode::new(
        Arc::new(FakePlatform::new()),
        ClaudeConfig {
            claude_dir: "/nowhere".into(),
            claude_json: "/nowhere/j".into(),
            claude_bin: "claude".into(),
        },
    )
}

#[test]
fn default_route_is_host_typing() {
    let claude = claude();
    assert!(
        !claude.capabilities().can_push,
        "no route but the host's typing"
    );
    let line = one_line("hello", 20);
    let err = claude
        .push(
            &SessionId::parse("s-1").unwrap(),
            &line,
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(err.code, HostErrorCode::Unsupported);
}
