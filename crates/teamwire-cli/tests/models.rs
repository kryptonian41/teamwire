//! `teamwire models`: the model list, the evidence and past outcomes for one
//! task class, and the JSON. The rendering is compiled in from the command's
//! own file, so a `FakeHarness` can stand in for the harness; the binary is
//! run for what only it decides (the harness picked, the exit codes, the
//! `--home` flag reaching pi's settings).

#[path = "../src/cmd/models/view.rs"]
mod view;

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;
use teamwire_core::{
    CatalogSource, Command, Evidence, EvidenceEntry, EvidenceKind, HarnessError, HarnessErrorCode,
    ModelCatalog, ModelInfo, RunOutput, TaskClass,
};
use teamwire_fakes::{FakeHarness, TestHome};
use teamwire_otel::{OutcomeReport, OutcomeRow, VerifyCounts};

use teamwire_core::models::normalise_model;
use view::{View, gather};

fn model(provider: Option<&str>, id: &str, aliases: &[&str], context: Option<u64>) -> ModelInfo {
    ModelInfo {
        id: id.to_owned(),
        aliases: aliases.iter().map(|a| (*a).to_owned()).collect(),
        provider: provider.map(str::to_owned),
        context_tokens: context,
        thinking: true,
    }
}

/// A pi-like list: one model through two providers, one with a date and
/// version suffix, and a model with an alias.
fn catalog() -> ModelCatalog {
    ModelCatalog {
        source: CatalogSource::Listed,
        models: vec![
            model(
                Some("claude-bridge"),
                "claude-opus-5-5",
                &[],
                Some(1_000_000),
            ),
            model(
                Some("amazon-bedrock"),
                "us.anthropic.claude-opus-5-5-20261001-v1:0",
                &[],
                Some(200_000),
            ),
            model(
                Some("claude-bridge"),
                "claude-sonnet-5-5",
                &["sonnet"],
                None,
            ),
            ModelInfo {
                thinking: false,
                ..model(Some("openai"), "gpt-6-luna", &[], Some(65_500))
            },
        ],
        efforts: vec!["low".into(), "high".into()],
    }
}

fn entry(class: TaskClass, model: &str, source: &str, rank: u32) -> EvidenceEntry {
    EvidenceEntry {
        task_class: class,
        model: model.to_owned(),
        effort: Some("high".to_owned()),
        rank,
        claim: format!("{model} scored {rank} on {source}."),
        source: source.to_owned(),
        url: format!("https://example.com/{rank}"),
        published: "2026-10-01".to_owned(),
        kind: EvidenceKind::Benchmark,
    }
}

fn evidence() -> Evidence {
    let rc = TaskClass::RootCause;
    Evidence {
        as_of: "2026-11-02".to_owned(),
        entries: vec![
            entry(rc, "gpt-9", "Board A", 1),
            entry(rc, "anthropic.claude-opus-5-5-v1:0", "Board A", 2),
            entry(rc, "claude-sonnet-5-5-20260901", "Board B", 2),
            entry(rc, "Claude-Opus-5-5", "Board B", 1),
            entry(TaskClass::Judgement, "claude-opus-5-5", "Board C", 1),
        ],
    }
}

fn row(class: &str, model: &str, members: u64) -> OutcomeRow {
    OutcomeRow {
        harness: "pi".to_owned(),
        model: model.to_owned(),
        effort: "high".to_owned(),
        task_class: class.to_owned(),
        kind: "implementer".to_owned(),
        members,
        verify: VerifyCounts {
            pass: members,
            flagged: 0,
            fail: 0,
            none: 0,
        },
        reprompts: 1,
        tokens_median: Some(1_234_567.4),
        minutes_median: Some(12.25),
        harder_than_class: 0,
    }
}

fn report() -> OutcomeReport {
    OutcomeReport {
        rows: vec![
            row("root_cause", "claude-bridge/claude-opus-5-5", 3),
            row("root_cause", "claude-bridge/claude-sonnet-5-5", 7),
            row("judgement", "claude-bridge/claude-opus-5-5", 9),
            row("unclassified", "claude-bridge/claude-opus-5-5", 4),
            row("unclassified", "unset", 2),
        ],
        teams: 6,
        errors: vec!["/h/teams/x/team.db: locked".to_owned()],
    }
}

fn fake(models: Result<ModelCatalog, HarnessError>) -> FakeHarness {
    let harness = FakeHarness::new();
    harness.set_models(models);
    harness
}

fn view_for(class: Option<TaskClass>) -> Result<View, HarnessError> {
    gather(&fake(Ok(catalog())), class, &evidence(), &report)
}

// ---- the matching rule ----

#[test]
fn a_name_loses_its_case_provider_region_date_and_version() {
    for (name, want) in [
        ("claude-opus-4-5", "claude-opus-4-5"),
        ("Claude-Opus-4-5", "claude-opus-4-5"),
        (
            "us.anthropic.claude-opus-4-5-20251101-v1:0",
            "claude-opus-4-5",
        ),
        (
            "anthropic.claude-haiku-4-5-20251001-v1:0",
            "claude-haiku-4-5",
        ),
        ("global.anthropic.claude-sonnet-4-5", "claude-sonnet-4-5"),
        ("anthropic.claude-opus-4-6-v1", "claude-opus-4-6"),
        ("amazon.nova-2-lite-v1:0", "nova-2-lite"),
        ("deepseek.v3.2", "deepseek-v3.2"),
        ("amazon-bedrock/amazon.nova-premier-v1:0", "nova-premier"),
        ("claude-bridge/claude-opus-5-5", "claude-opus-5-5"),
        ("claude-sonnet-4-5@20250929", "claude-sonnet-4-5"),
        ("gpt-4o-2024-08-06", "gpt-4o"),
        ("claude-opus-4-20250514", "claude-opus-4"),
    ] {
        assert_eq!(normalise_model(name), want, "{name}");
    }
}

#[test]
fn a_models_own_dots_and_version_stay() {
    for name in [
        "gpt-5.2",
        "gpt-6.1-sol",
        "deepseek-v3.2",
        "deepseek-v4",
        "deepseek-v4-pro",
        "deepseek-coder-v2",
        "kimi-k2.5",
        "o3",
    ] {
        assert_eq!(normalise_model(name), name);
    }
    assert_ne!(normalise_model("gpt-5.2"), normalise_model("gpt-5"));
}

#[test]
fn bedrocks_dotted_deepseek_reaches_the_hyphenated_evidence_name() {
    let pi = ModelCatalog {
        models: vec![
            model(Some("amazon-bedrock"), "deepseek.v3.2", &[], Some(163_800)),
            model(Some("amazon-bedrock"), "deepseek.r1-v1:0", &[], None),
        ],
        ..catalog()
    };
    let evidence = Evidence {
        as_of: "2026-11-03".to_owned(),
        entries: vec![
            entry(TaskClass::ContainedFeature, "deepseek-v3.2", "Board A", 1),
            entry(
                TaskClass::ContainedFeature,
                "deepseek-v3.2-exp",
                "Board A",
                2,
            ),
        ],
    };
    let view = gather(
        &fake(Ok(pi)),
        Some(TaskClass::ContainedFeature),
        &evidence,
        &report,
    )
    .unwrap();
    let ev = &view.class.as_ref().unwrap().evidence;
    let reached: Vec<(&str, Vec<String>)> = ev
        .reachable
        .iter()
        .map(|r| (r.entry.model.as_str(), r.spec_values.clone()))
        .collect();
    assert_eq!(
        reached,
        [(
            "deepseek-v3.2",
            vec!["amazon-bedrock/deepseek.v3.2".to_owned()]
        )]
    );
    let unreached: Vec<&str> = ev.unreachable.iter().map(|e| e.model.as_str()).collect();
    assert_eq!(unreached, ["deepseek-v3.2-exp"]);
}

#[test]
fn openrouters_dotted_claude_reaches_the_hyphenated_evidence_name() {
    let pi = ModelCatalog {
        models: vec![
            model(Some("openrouter"), "anthropic/claude-opus-5.5", &[], None),
            model(
                Some("openrouter"),
                "anthropic/claude-opus-5.5:batch",
                &[],
                None,
            ),
            model(Some("openrouter"), "anthropic/claude-opus-5", &[], None),
        ],
        ..catalog()
    };
    let evidence = Evidence {
        as_of: "2026-11-14".to_owned(),
        entries: vec![
            entry(TaskClass::Judgement, "claude-opus-5-5", "Board A", 1),
            entry(TaskClass::Judgement, "claude-opus-4-6", "Board A", 2),
        ],
    };
    let view = gather(
        &fake(Ok(pi)),
        Some(TaskClass::Judgement),
        &evidence,
        &report,
    )
    .unwrap();
    let ev = &view.class.as_ref().unwrap().evidence;
    let reached: Vec<(&str, Vec<String>)> = ev
        .reachable
        .iter()
        .map(|r| (r.entry.model.as_str(), r.spec_values.clone()))
        .collect();
    assert_eq!(
        reached,
        [(
            "claude-opus-5-5",
            vec!["openrouter/anthropic/claude-opus-5.5".to_owned()]
        )]
    );
    let unreached: Vec<&str> = ev.unreachable.iter().map(|e| e.model.as_str()).collect();
    assert_eq!(unreached, ["claude-opus-4-6"]);
}

// ---- the model list ----

#[test]
fn the_plain_list_shows_the_harness_source_efforts_and_spec_values() {
    let text = view::text(&view_for(None).unwrap());
    let lines: Vec<&str> = text.lines().collect();
    let line = |i: usize| lines.get(i).copied().unwrap_or_default();
    assert_eq!(line(0), "harness: fake");
    assert!(line(1).starts_with("source: listed. "), "{text}");
    assert_eq!(line(2), "efforts: low, high");
    assert!(line(3).starts_with("models: 4."), "{text}");
    let cells = |i: usize| line(i + 4).split_whitespace().collect::<Vec<_>>();
    assert_eq!(cells(0), ["model", "aliases", "context", "thinking"]);
    assert_eq!(
        cells(1),
        ["claude-bridge/claude-opus-5-5", "-", "1M", "yes"]
    );
    assert_eq!(
        cells(2),
        [
            "amazon-bedrock/us.anthropic.claude-opus-5-5-20261001-v1:0",
            "-",
            "200K",
            "yes"
        ]
    );
    assert_eq!(
        cells(3),
        ["claude-bridge/claude-sonnet-5-5", "sonnet", "-", "yes"]
    );
    assert_eq!(cells(4), ["openai/gpt-6-luna", "-", "65.5K", "no"]);
    assert_eq!(lines.len(), 9, "no class sections without --for: {text}");
}

#[test]
fn a_builtin_list_says_a_model_may_be_out_of_reach() {
    let builtin = ModelCatalog {
        source: CatalogSource::Builtin,
        models: Vec::new(),
        ..catalog()
    };
    let view = gather(&fake(Ok(builtin)), None, &evidence(), &report).unwrap();
    let text = view::text(&view);
    assert!(text.contains("source: builtin. "), "{text}");
    assert!(text.contains("may still be out of reach"), "{text}");
    assert!(text.contains("models: none"), "{text}");
}

#[test]
fn a_failing_harness_list_is_an_error_with_its_reason() {
    let err = HarnessError::new(HarnessErrorCode::Timeout, "pi took too long");
    let got = gather(&fake(Err(err)), None, &evidence(), &report).unwrap_err();
    assert_eq!(got.code, HarnessErrorCode::Timeout);
    assert!(got.to_string().contains("pi took too long"), "{got}");
}

#[test]
fn the_outcome_report_is_read_only_for_a_class() {
    let read = std::cell::Cell::new(0);
    let counted = || {
        read.set(read.get() + 1);
        report()
    };
    gather(&fake(Ok(catalog())), None, &evidence(), &counted).unwrap();
    assert_eq!(read.get(), 0);
    gather(
        &fake(Ok(catalog())),
        Some(TaskClass::RootCause),
        &evidence(),
        &counted,
    )
    .unwrap();
    assert_eq!(read.get(), 1);
}

// ---- --for <class> ----

#[test]
fn evidence_splits_into_reachable_then_unreachable_grouped_by_source_in_rank_order() {
    let view = view_for(Some(TaskClass::RootCause)).unwrap();
    let ev = &view.class.as_ref().unwrap().evidence;
    assert_eq!(ev.as_of, "2026-11-02");
    let reached: Vec<(&str, &str, u32)> = ev
        .reachable
        .iter()
        .map(|r| {
            (
                r.entry.source.as_str(),
                r.entry.model.as_str(),
                r.entry.rank,
            )
        })
        .collect();
    assert_eq!(
        reached,
        [
            ("Board A", "anthropic.claude-opus-5-5-v1:0", 2),
            ("Board B", "Claude-Opus-5-5", 1),
            ("Board B", "claude-sonnet-5-5-20260901", 2),
        ]
    );
    let spec_values = |i: usize| ev.reachable.get(i).map(|r| r.spec_values.clone()).unwrap();
    assert_eq!(
        spec_values(0),
        [
            "claude-bridge/claude-opus-5-5",
            "amazon-bedrock/us.anthropic.claude-opus-5-5-20261001-v1:0"
        ]
    );
    assert_eq!(spec_values(2), ["claude-bridge/claude-sonnet-5-5"]);
    let unreached: Vec<&str> = ev.unreachable.iter().map(|e| e.model.as_str()).collect();
    assert_eq!(unreached, ["gpt-9"], "another class's entries stay out");

    let text = view::text(&view);
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| unreachable!("{needle}: {text}"))
    };
    assert!(
        at("Public evidence for root_cause, as of 2026-11-02.")
            < at("Models this harness reaches: 3 entries.")
    );
    assert!(
        at("Models this harness reaches") < at("Models this harness does not reach: 1 entries.")
    );
    assert!(
        at(
            "rank 1: Claude-Opus-5-5, effort high (benchmark), spec model claude-bridge/claude-opus-5-5"
        ) > 0
    );
    assert!(at("does not reach") < at("rank 1: gpt-9, effort high (benchmark)\n"));
    assert!(at("Claude-Opus-5-5 scored 1 on Board B.") > 0);
    assert!(at("2026-10-01 https://example.com/1") > 0);
    assert!(
        at("Public evidence") < at("Your past teams, root_cause"),
        "evidence comes first"
    );
}

#[test]
fn past_teams_show_the_class_rows_counts_and_the_unclassified_rows() {
    let view = view_for(Some(TaskClass::RootCause)).unwrap();
    let outcomes = &view.class.as_ref().unwrap().outcomes;
    assert_eq!(outcomes.teams, 6);
    assert_eq!(outcomes.rows.len(), 2, "only root_cause rows");
    assert_eq!(
        (outcomes.unclassified.rows, outcomes.unclassified.members),
        (2, 6)
    );

    let text = view::text(&view);
    let (_, past) = text
        .split_once("Your past teams, root_cause: 2 groups, from 6 teams read.")
        .unwrap();
    assert!(past.contains("total tokens incl. cache reads"), "{past}");
    assert!(
        past.contains("including cache reads, which make up most of each total"),
        "{past}"
    );
    assert!(
        past.contains("Unclassified: 2 groups, 6 members."),
        "{past}"
    );
    assert!(
        past.contains("Not read: /h/teams/x/team.db: locked"),
        "{past}"
    );
    assert!(
        !text.contains('%') || !past.contains('%'),
        "no percentages: {past}"
    );
    assert!(!past.contains("judgement"), "{past}");
    assert!(
        past.contains("1234567"),
        "the median is shown as a whole count: {past}"
    );
}

#[test]
fn a_group_of_fewer_than_five_is_marked_a_small_sample() {
    let view = view_for(Some(TaskClass::RootCause)).unwrap();
    let outcomes = &view.class.as_ref().unwrap().outcomes;
    let marks: Vec<(u64, bool)> = outcomes
        .rows
        .iter()
        .map(|l| (l.row.members, l.small_sample))
        .collect();
    assert_eq!(marks, [(3, true), (7, false)]);
    let text = view::text(&view);
    let line = |model: &str| {
        text.lines()
            .find(|l| l.contains(model) && l.contains("implementer"))
            .unwrap()
            .to_owned()
    };
    assert!(
        line("claude-opus-5-5").ends_with("small (fewer than 5)"),
        "{text}"
    );
    assert!(!line("claude-sonnet-5-5").contains("small"), "{text}");
}

#[test]
fn nothing_names_one_best_model() {
    let text = view::text(&view_for(Some(TaskClass::RootCause)).unwrap()).to_lowercase();
    for word in ["best", "recommend", "score:", "winner"] {
        assert!(!text.contains(word), "{word}: {text}");
    }
}

// ---- --json ----

#[test]
fn the_json_holds_harness_catalog_and_with_a_class_evidence_and_outcomes() {
    let plain = view::json(&view_for(None).unwrap());
    let keys: Vec<&String> = plain.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["catalog", "harness"]);
    assert_eq!(at(&plain, "/harness"), "fake");
    assert_eq!(at(&plain, "/catalog/source"), "listed");
    assert_eq!(at(&plain, "/catalog/models/2/aliases/0"), "sonnet");
    assert_eq!(at(&plain, "/catalog/efforts/1"), "high");

    let doc = view::json(&view_for(Some(TaskClass::RootCause)).unwrap());
    let keys: Vec<&String> = doc.as_object().unwrap().keys().collect();
    assert_eq!(
        keys,
        ["catalog", "evidence", "harness", "outcomes", "task_class"]
    );
    assert_eq!(at(&doc, "/task_class"), "root_cause");
    let ev = at(&doc, "/evidence");
    assert_eq!(at(ev, "/as_of"), "2026-11-02");
    assert_eq!(at(ev, "/reachable").as_array().unwrap().len(), 3);
    assert_eq!(at(ev, "/reachable/1/model"), "Claude-Opus-5-5");
    assert_eq!(at(ev, "/reachable/1/rank"), 1);
    assert_eq!(at(ev, "/reachable/1/kind"), "benchmark");
    assert_eq!(
        at(ev, "/reachable/1/spec_values/0"),
        "claude-bridge/claude-opus-5-5"
    );
    assert_eq!(at(ev, "/unreachable/0/model"), "gpt-9");
    assert!(at(ev, "/unreachable/0").get("spec_values").is_none());
    let out = at(&doc, "/outcomes");
    assert_eq!(at(out, "/teams"), 6);
    assert_eq!(at(out, "/rows/0/members"), 3);
    assert_eq!(at(out, "/rows/0/small_sample"), true);
    assert_eq!(at(out, "/rows/0/verify/PASS"), 3);
    assert_eq!(at(out, "/rows/1/small_sample"), false);
    assert_eq!(at(out, "/unclassified/rows"), 2);
    assert_eq!(at(out, "/unclassified/members"), 6);
    assert_eq!(at(out, "/errors/0"), "/h/teams/x/team.db: locked");
}

// ---- the binary ----

/// The binary with a clean environment: no member variables, no session
/// markers, a `PATH` that holds only `bin` and the system directories, so
/// `pi` is the fake one or none.
fn teamwire(home: &Path, bin: &Path, env: &[(&str, &str)], args: &[&str]) -> RunOutput {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_teamwire")).args(args.iter().copied());
    cmd.env_clear = true;
    cmd.env = [
        ("PATH", format!("{}:/usr/bin:/bin", bin.display())),
        ("HOME", home.display().to_string()),
        ("TEAMWIRE_HOME", home.display().to_string()),
    ]
    .into_iter()
    .chain(env.iter().map(|(k, v)| (*k, (*v).to_owned())))
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    teamwire_platform::native()
        .run(&cmd, Duration::from_secs(60))
        .unwrap_or_else(|e| unreachable!("the binary did not start: {e}"))
}

/// The value at a JSON pointer, or null.
fn at<'a>(doc: &'a Value, pointer: &str) -> &'a Value {
    doc.pointer(pointer).unwrap_or(&Value::Null)
}

fn out(o: &RunOutput) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn err(o: &RunOutput) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// A `pi` that prints a model table with one Bedrock row and one bridge row.
fn fake_pi(bin: &Path, exit: u8) -> std::io::Result<()> {
    let script = format!(
        "#!/bin/sh\n\
         [ \"$1\" = --list-models ] || exit 9\n\
         echo 'provider        model               context  max-out  thinking  images'\n\
         echo 'amazon-bedrock  anthropic.claude-opus-5-5  1M  128K  yes  yes'\n\
         echo 'claude-bridge   claude-opus-5-5     1M       128K     yes       yes'\n\
         exit {exit}\n"
    );
    let path = bin.join("pi");
    teamwire_platform::native()
        .write_atomic(&path, script.as_bytes())
        .map_err(std::io::Error::other)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
}

#[test]
fn without_harness_the_command_lists_the_leads_harness() {
    let home = TestHome::new().unwrap();
    let o = teamwire(
        home.path(),
        home.path(),
        &[("TEAMWIRE_LEAD_HARNESS", "codex")],
        &["models"],
    );
    assert!(o.success(), "{}", err(&o));
    assert!(
        out(&o).starts_with("harness: codex\nsource: builtin. "),
        "{}",
        out(&o)
    );
    let o = teamwire(
        home.path(),
        home.path(),
        &[("CLAUDE_CODE_SESSION_ID", "s1")],
        &["models"],
    );
    assert!(out(&o).starts_with("harness: claude-code\n"), "{}", out(&o));
}

#[test]
fn harness_names_another_harness_and_an_unknown_one_is_a_usage_error() {
    let home = TestHome::new().unwrap();
    let lead = [("TEAMWIRE_LEAD_HARNESS", "codex")];
    let o = teamwire(
        home.path(),
        home.path(),
        &lead,
        &["models", "--harness", "claude-code"],
    );
    assert!(o.success(), "{}", err(&o));
    assert!(out(&o).starts_with("harness: claude-code\n"), "{}", out(&o));
    assert!(out(&o).contains("\n  claude-opus-5-5 "), "{}", out(&o));
    let o = teamwire(
        home.path(),
        home.path(),
        &lead,
        &["models", "--harness", "nope"],
    );
    assert_eq!(Some(o.exit_code), Some(3), "{}", err(&o));
    assert!(err(&o).contains("claude-code, codex, pi"), "{}", err(&o));
}

#[test]
fn an_unknown_class_is_refused_with_the_seven_classes() {
    let home = TestHome::new().unwrap();
    let o = teamwire(
        home.path(),
        home.path(),
        &[],
        &["models", "--harness", "codex", "--for", "hard"],
    );
    assert_eq!(Some(o.exit_code), Some(3), "{}", err(&o));
    assert!(out(&o).is_empty());
    for class in TaskClass::ALL {
        assert!(err(&o).contains(class.as_str()), "{}", err(&o));
    }
}

#[test]
fn a_harness_that_cannot_list_its_models_fails_with_the_reason() {
    let home = TestHome::new().unwrap();
    let bin = tempfile::tempdir().unwrap();
    // No pi on PATH at all.
    let o = teamwire(home.path(), bin.path(), &[], &["models", "--harness", "pi"]);
    assert_eq!(Some(o.exit_code), Some(1), "{}", out(&o));
    assert!(
        err(&o).contains("harness pi could not list its models"),
        "{}",
        err(&o)
    );
    // A pi that fails.
    fake_pi(bin.path(), 2).unwrap();
    let o = teamwire(home.path(), bin.path(), &[], &["models", "--harness", "pi"]);
    assert_eq!(Some(o.exit_code), Some(1), "{}", out(&o));
    assert!(err(&o).contains("`pi --list-models` failed"), "{}", err(&o));
}

#[test]
fn for_a_class_the_binary_shows_the_compiled_evidence_and_empty_past_teams() {
    let home = TestHome::new().unwrap();
    let bin = tempfile::tempdir().unwrap();
    fake_pi(bin.path(), 0).unwrap();
    let o = teamwire(
        home.path(),
        bin.path(),
        &[],
        &["models", "--harness", "pi", "--for", "root_cause", "--json"],
    );
    assert!(o.success(), "{}", err(&o));
    let doc: Value = serde_json::from_str(&out(&o)).unwrap();
    let file: Evidence =
        serde_json::from_str(include_str!("../../../examples/models/evidence.json")).unwrap();
    assert_eq!(at(&doc, "/evidence/as_of"), file.as_of.as_str());
    let shown = at(&doc, "/evidence/reachable").as_array().unwrap().len()
        + at(&doc, "/evidence/unreachable").as_array().unwrap().len();
    let in_file = file
        .entries
        .iter()
        .filter(|e| e.task_class == TaskClass::RootCause)
        .count();
    assert_eq!(shown, in_file);
    assert_eq!(at(&doc, "/outcomes/teams"), 0);
    assert_eq!(at(&doc, "/outcomes/rows").as_array().unwrap().len(), 0);
}

#[test]
fn the_home_flag_decides_which_config_toml_excludes_pi_providers() {
    let home = TestHome::new().unwrap();
    let flagged = TestHome::new().unwrap();
    teamwire_platform::native()
        .write_atomic(
            &flagged.path().join("config.toml"),
            b"TEAMWIRE_PI_EXCLUDE_PROVIDERS = \"amazon-bedrock\"\n",
        )
        .unwrap();
    let bin = tempfile::tempdir().unwrap();
    fake_pi(bin.path(), 0).unwrap();
    let flag = flagged.path().to_str().unwrap();

    // TEAMWIRE_HOME has no config.toml, so pi's whole list shows.
    let o = teamwire(home.path(), bin.path(), &[], &["models", "--harness", "pi"]);
    assert!(out(&o).contains("amazon-bedrock/"), "{}", out(&o));

    // --home names the directory whose config.toml leaves Bedrock out.
    let o = teamwire(
        home.path(),
        bin.path(),
        &[],
        &["--home", flag, "models", "--harness", "pi"],
    );
    assert!(o.success(), "{}", err(&o));
    assert!(!out(&o).contains("amazon-bedrock"), "{}", out(&o));
    assert!(
        out(&o).contains("claude-bridge/claude-opus-5-5"),
        "{}",
        out(&o)
    );
}
