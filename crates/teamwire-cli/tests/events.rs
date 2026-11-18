//! `teamwire events`, run as the real binary against a live store and
//! against archives (`docs/api/cli.md`).
#![expect(
    clippy::disallowed_methods,
    reason = "the test copies database files and starts the teamwire binary"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "a test that indexes past the end panics and fails, which is the intended outcome"
)]
#![expect(
    clippy::unwrap_used,
    reason = "helper functions in an integration-test file unwrap; a failure there fails the test"
)]

#[path = "../../teamwire-watch/tests/support/fixture.rs"]
pub mod fixture;

use std::path::Path;
use std::process::Output;

use fixture::{Bed, role};
use serde_json::Value;
use teamwire_core::{
    Clock, DeliveryState, DialogVerdict, PermissionAnswer, PermissionRequest, SessionId, Timestamp,
};
use teamwire_store::NewVerifyRecord;

/// A team with one member, a message the lead got, and a verify verdict.
fn populated() -> Bed {
    let mut bed = Bed::new();
    bed.add("api");
    bed.question("api", DeliveryState::HandedOff);
    let now = bed.clock.now();
    bed.store
        .write(|tx| {
            tx.insert_verify_record(
                now,
                &NewVerifyRecord {
                    role: role("api"),
                    head_sha: "abc123".to_owned(),
                    verdict: "FLAGGED".to_owned(),
                    exit_code: Some(0),
                    seconds: Some(4),
                    flags: vec!["outside-owns:src/a.ts".to_owned()],
                    log_sha256: None,
                },
            )
        })
        .unwrap();
    bed
}

/// `teamwire events` with a cleared environment. The tests often run inside
/// a member's session, and any of its identity variables, such as a pi or
/// Codex member's `TEAMWIRE_TOKEN_FILE`, would make the lead's command refuse.
fn events(home: &Path, args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(home)
        .arg("events")
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap()
}

fn json(home: &Path, args: &[&str]) -> Value {
    let out = events(home, args);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

/// A home holding only the archive folders `names`, each a copy of the
/// team's database. The store is closed first so the copy holds every row.
fn archived(bed: Bed, names: &[&str], sums: bool) -> tempfile::TempDir {
    let Bed { home, store, .. } = bed;
    drop(store);
    let db = home.path().join("teams/alpha/team.db");
    let out = tempfile::tempdir().unwrap();
    for name in names {
        let dir = out.path().join("archive").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::copy(&db, dir.join("team.db")).unwrap();
        if sums {
            std::fs::write(dir.join("SHA256SUMS"), b"").unwrap();
        }
    }
    out
}

fn refused(home: &Path, args: &[&str]) -> String {
    let out = events(home, args);
    assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn a_live_store_prints_its_rows_oldest_first() {
    let bed = populated();
    let view = json(bed.home.path(), &["alpha", "--json"]);
    assert_eq!(view["team"], "alpha");
    assert_eq!(view["source"], "live");
    assert!(view["archive"].is_null());
    assert_eq!(view["errors"], serde_json::json!([]));
    let rows = view["events"].as_array().unwrap();
    let seqs: Vec<u64> = rows.iter().map(|r| r["seq"].as_u64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    assert_eq!(view["last_seq"].as_u64(), seqs.last().copied());
    let of = |kind: &str| rows.iter().find(|r| r["type"] == kind).unwrap();
    assert_eq!(of("message.sent")["message"], "m001");
    assert_eq!(of("delivery.handed_off")["message"], "m001");
    let verify = of("verify.recorded");
    assert!(verify["message"].is_null());
    assert_eq!(verify["role"], "api");
    assert_eq!(verify["data"]["verdict"], "FLAGGED");
}

#[test]
fn after_leaves_out_earlier_rows_and_keeps_the_true_last_seq() {
    let bed = populated();
    let all = json(bed.home.path(), &["alpha", "--json"]);
    let last = all["last_seq"].as_u64().unwrap();
    let after = json(
        bed.home.path(),
        &["alpha", "--after", &(last - 1).to_string(), "--json"],
    );
    assert_eq!(after["events"].as_array().unwrap().len(), 1);
    assert_eq!(after["events"][0]["seq"], last);
    let none = json(
        bed.home.path(),
        &["alpha", "--after", &last.to_string(), "--json"],
    );
    assert_eq!(none["events"], serde_json::json!([]));
    assert_eq!(none["last_seq"], last);
}

#[test]
fn without_json_each_row_is_one_line() {
    let bed = populated();
    let out = events(bed.home.path(), &["alpha"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let line = text.lines().find(|l| l.contains(" message.sent ")).unwrap();
    let parts: Vec<&str> = line.split(' ').collect();
    assert_eq!(parts.len(), 5, "{line}");
    assert_eq!(parts[4], "m001");
    let verify = text
        .lines()
        .find(|l| l.contains(" verify.recorded "))
        .unwrap();
    assert!(verify.ends_with(" api -"), "{verify}");
}

#[test]
fn a_team_with_no_live_store_reads_its_newest_archive() {
    let home = archived(
        populated(),
        &["alpha-2026-10-12", "alpha-2026-10-13-091500"],
        true,
    );
    let view = json(home.path(), &["alpha", "--json"]);
    assert_eq!(view["source"], "archive");
    assert_eq!(view["archive"], "alpha-2026-10-13-091500");
    assert!(!view["events"].as_array().unwrap().is_empty());
}

#[test]
fn archive_reads_the_named_directory() {
    let home = archived(populated(), &["alpha-2026-10-12", "alpha-2026-10-13"], true);
    let view = json(
        home.path(),
        &["alpha", "--archive", "alpha-2026-10-12", "--json"],
    );
    assert_eq!(view["source"], "archive");
    assert_eq!(view["archive"], "alpha-2026-10-12");
}

#[test]
fn archive_refuses_a_name_that_is_not_this_team_s_archive() {
    // Each bad name holds a real database, so only the name check can refuse it.
    let bad = [
        "../x",
        "a/b",
        "alpha-2026-10-12/..",
        "beta-2026-10-12",
        "alpha-2026-1-12",
        "alpha-2026-10-12-0915",
        "alpha-2026-10-12-091500-",
        "alpha-2026-10-12-091500-x",
        "alpha-2026-10-12-091500-0",
        "alpha",
        "alpha-2026-10-12x",
    ];
    let mut names = vec!["alpha-2026-10-12", "beta-2026-10-12"];
    names.extend(bad);
    let home = archived(populated(), &names, true);
    for name in bad {
        let text = refused(home.path(), &["alpha", "--archive", name]);
        assert!(text.contains("invalid archive"), "{name}: {text}");
        assert!(
            text.contains("-YYYY-MM-DD-HHMMSS-<n>"),
            "the refusal names all three forms: {text}"
        );
    }
    // A well-formed name whose database belongs to another team.
    let text = refused(home.path(), &["beta", "--archive", "beta-2026-10-12"]);
    assert!(text.contains("not an archive of team beta"), "{text}");
}

#[test]
fn an_archive_without_sha256sums_is_read_with_one_error_line() {
    let home = archived(populated(), &["alpha-2026-10-12"], false);
    let view = json(
        home.path(),
        &["alpha", "--archive", "alpha-2026-10-12", "--json"],
    );
    assert!(!view["events"].as_array().unwrap().is_empty());
    let errors = view["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].as_str().unwrap().contains("SHA256SUMS"));
}

#[test]
fn a_member_session_is_refused() {
    let bed = populated();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"))
        .arg("--home")
        .arg(bed.home.path())
        .args(["events", "alpha", "--json"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TEAMWIRE_TEAM", "alpha")
        .env("TEAMWIRE_ROLE", "api")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("refused.identity")
    );
    assert!(out.stdout.is_empty());
}

#[test]
fn a_team_never_seen_is_refused() {
    let home = tempfile::tempdir().unwrap();
    refused(home.path(), &["ghost", "--json"]);
}

#[test]
fn a_dialog_row_shows_its_call_verdict_and_reason() {
    let mut bed = Bed::new();
    bed.add("api");
    let now = bed.clock.now();
    let later = Timestamp::from_unix_millis(now.unix_millis() + 100_000).unwrap();
    let member = bed
        .store
        .read(|tx| tx.member(&role("api")))
        .unwrap()
        .unwrap();
    let session = SessionId::parse("s-1").unwrap();
    let request = |tool: &str, summary: &str, why_not: Option<&str>| PermissionRequest {
        session: session.clone(),
        subagent: false,
        tool: tool.to_owned(),
        summary: summary.to_owned(),
        input: Some(serde_json::json!({"command": summary})),
        why_not: why_not.map(str::to_owned),
    };
    bed.store
        .write(|tx| {
            tx.set_member_session(&role("api"), &session)?;
            let id = tx.open_dialog(
                now,
                &role("api"),
                member.generation,
                &request("Bash", "rmdir /usr\u{202e}", None),
                later,
            )?;
            let answer = PermissionAnswer {
                verdict: DialogVerdict::Deny,
                reason: "removes a system folder; clear only target/tmp".to_owned(),
            };
            tx.answer_dialog(now, &role("api"), &id, &answer)?;
            tx.close_dialog(now, &id)?;
            tx.open_dialog(
                now,
                &role("api"),
                member.generation,
                &request("AskUserQuestion", "AskUserQuestion", Some("not Bash")),
                later,
            )?;
            Ok(())
        })
        .unwrap();
    let out = events(bed.home.path(), &["alpha"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = String::from_utf8(out.stdout).unwrap();
    let line = |kind: &str| -> Vec<&str> {
        text.lines()
            .filter(|l| l.split(' ').nth(2) == Some(kind))
            .collect()
    };
    assert!(
        line("dialog.opened")[0].ends_with(" api - Bash: rmdir /usr\\u{202e}"),
        "{text}"
    );
    assert!(
        line("dialog.answered")[0].ends_with(
            " api - deny Bash rmdir /usr\\u{202e}: removes a system folder; clear only target/tmp"
        ),
        "{text}"
    );
    assert!(line("dialog.closed")[0].ends_with(" api - taken"), "{text}");
    assert!(
        line("dialog.opened")[1]
            .ends_with(" api - AskUserQuestion: AskUserQuestion (cannot answer: not Bash)"),
        "{text}"
    );
    assert!(!text.contains('\u{202e}'), "{text}");
}
