//! `Pi::models` against a recorded `pi --list-models` table: the rows it
//! parses, the providers it leaves out, and the failures it reports.

use std::sync::Arc;
use std::time::Duration;

use teamwire_core::{
    CatalogSource, Harness, HarnessErrorCode, PlatformError, PlatformErrorCode, RunOutput,
};
use teamwire_fakes::FakePlatform;
use teamwire_harness_pi::{Pi, PiConfig};

/// `pi --list-models` from pi 1.4.0 on 2026-11-03, cut to a few rows.
const TABLE: &str = include_str!("fixtures/list-models.txt");
const WAIT: Duration = Duration::from_secs(7);

fn pi(platform: &Arc<FakePlatform>, exclude: &[&str]) -> Pi {
    Pi::new(
        platform.clone(),
        PiConfig {
            agent_dir: "/unused".into(),
            pi_bin: "pi".into(),
            exclude_providers: exclude.iter().map(|p| (*p).to_owned()).collect(),
        },
    )
}

fn output(code: i32, stdout: &str, timed_out: bool) -> Result<RunOutput, PlatformError> {
    Ok(RunOutput {
        exit_code: code,
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        timed_out,
    })
}

fn listed(exclude: &[&str]) -> teamwire_core::ModelCatalog {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(output(0, TABLE, false));
    pi(&platform, exclude).models(WAIT).unwrap()
}

#[test]
fn the_table_is_parsed_one_model_per_row() {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(output(0, TABLE, false));
    let catalog = pi(&platform, &[]).models(WAIT).unwrap();

    let runs = platform.runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0.program, "pi");
    assert_eq!(runs[0].0.args, ["--list-models"]);
    assert_eq!(runs[0].1, WAIT);

    assert_eq!(catalog.source, CatalogSource::Listed);
    assert_eq!(
        catalog.efforts,
        ["off", "minimal", "low", "medium", "high", "xhigh", "max"]
    );
    assert_eq!(catalog.models.len(), TABLE.lines().count() - 1);

    let row = |provider: &str, id: &str| {
        catalog
            .models
            .iter()
            .find(|m| m.provider.as_deref() == Some(provider) && m.id == id)
            .unwrap_or_else(|| panic!("no row {provider}/{id}"))
    };
    // An `M` context.
    let opus = row("claude-bridge", "claude-opus-5-5");
    assert_eq!(opus.context_tokens, Some(1_000_000));
    assert!(opus.thinking);
    assert!(opus.aliases.is_empty());
    // A `K` context.
    assert_eq!(
        row("claude-bridge", "claude-haiku-4-5").context_tokens,
        Some(200_000)
    );
    // A `K` context with a decimal, and a decimal `M`.
    assert_eq!(
        row("amazon-bedrock", "deepseek.v3.2").context_tokens,
        Some(163_800)
    );
    assert_eq!(
        row("amazon-bedrock", "global.moonshotai.kimi-k3").context_tokens,
        Some(1_000_000)
    );
    // A model that takes no thinking level.
    let nova = row("amazon-bedrock", "amazon.nova-lite-v1:0");
    assert!(!nova.thinking);
    assert_eq!(nova.context_tokens, Some(300_000));
}

#[test]
fn an_excluded_provider_loses_its_rows_and_the_others_stay() {
    let all = listed(&[]);
    let kept = listed(&["amazon-bedrock"]);
    assert!(!kept.models.is_empty());
    assert!(
        kept.models
            .iter()
            .all(|m| m.provider.as_deref() == Some("claude-bridge"))
    );
    let bridge = |c: &teamwire_core::ModelCatalog| {
        c.models
            .iter()
            .filter(|m| m.provider.as_deref() == Some("claude-bridge"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(bridge(&kept), bridge(&all));
    assert!(kept.find("amazon.nova-lite-v1:0").is_none());
    assert!(kept.find("claude-opus-5-5").is_some());
}

#[test]
fn a_provider_pi_does_not_list_changes_nothing() {
    assert_eq!(listed(&["no-such-provider"]), listed(&[]));
}

#[test]
fn a_failed_run_is_an_error_and_never_an_empty_list() {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(Ok(RunOutput {
        exit_code: 1,
        stdout: Vec::new(),
        stderr: b"pi: no config".to_vec(),
        timed_out: false,
    }));
    let err = pi(&platform, &[]).models(WAIT).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Other);
    assert!(err.detail.contains("pi: no config"), "{err}");

    // A table on stdout does not rescue a non-zero exit.
    platform.queue_run(output(2, TABLE, false));
    assert!(pi(&platform, &[]).models(WAIT).is_err());
}

#[test]
fn a_timeout_is_an_error_of_its_own() {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(output(RunOutput::TIMEOUT_EXIT, "", true));
    let err = pi(&platform, &[]).models(WAIT).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Timeout);
}

#[test]
fn a_program_that_cannot_start_is_an_error() {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(Err(PlatformError::new(
        PlatformErrorCode::NotFound,
        "pi: not found",
    )));
    let err = pi(&platform, &[]).models(WAIT).unwrap_err();
    assert_eq!(err.code, HarnessErrorCode::Io);
}

#[test]
fn output_that_is_not_the_table_is_an_error() {
    for text in [
        "",
        "No models available",
        "provider model\nclaude-bridge x",
        "provider model context thinking\nclaude-bridge claude-opus-5-5 1M",
        "provider model context thinking\nclaude-bridge claude-opus-5-5 big yes",
        "provider model context thinking\nclaude-bridge claude-opus-5-5 1M maybe",
    ] {
        let platform = Arc::new(FakePlatform::new());
        platform.queue_run(output(0, text, false));
        let err = pi(&platform, &[]).models(WAIT).unwrap_err();
        assert_eq!(err.code, HarnessErrorCode::BadInput, "{text:?}");
    }
}

#[test]
fn a_header_with_no_rows_is_an_empty_list() {
    let platform = Arc::new(FakePlatform::new());
    platform.queue_run(output(
        0,
        "provider  model  context  max-out  thinking  images\n",
        false,
    ));
    assert!(pi(&platform, &[]).models(WAIT).unwrap().models.is_empty());
}
