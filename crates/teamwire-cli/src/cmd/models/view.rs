//! What `teamwire models` shows, as text and as JSON: one harness's model
//! list, and for one task class the public evidence and the outcomes of the
//! user's past teams, in two sections that stay apart.
//!
//! Never: read a file or print. `models.rs` reads the evidence and the
//! state directory and prints the result. This file takes them as values,
//! so a test can give it a fake harness and a made-up outcome report; the
//! CLI tests compile this file in for that reason, so it names nothing of
//! this crate but its sibling `family`.
//!
//! Invariants: nothing here ranks across sources, marks one model as best,
//! blends the evidence with the outcomes, or turns a count into a
//! percentage. The lead chooses.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use teamwire_core::{
    CatalogSource, Evidence, EvidenceEntry, Harness, HarnessError, ModelCatalog, ModelFamily,
    ModelInfo, TaskClass,
};
use teamwire_otel::{OutcomeReport, OutcomeRow};

use super::family::{catalog_json, families};

/// How long a harness may take to list its models. pi's `--list-models`
/// takes a few seconds on a machine with many providers.
const MODELS_TIMEOUT: Duration = Duration::from_secs(30);

/// A group of past members smaller than this is marked as a small sample.
pub(crate) const SMALL_SAMPLE: u64 = 5;

/// The class a past member counts under when its team set none.
const UNCLASSIFIED: &str = "unclassified";

/// Everything one run of the command shows.
#[derive(Debug)]
pub(crate) struct View {
    pub(crate) harness: String,
    pub(crate) catalog: ModelCatalog,
    /// The family of each model of `catalog`, in its order.
    pub(crate) families: Vec<ModelFamily>,
    /// Present with `--for`.
    pub(crate) class: Option<ClassView>,
}

/// The two sections `--for` adds. They stay apart in text and in JSON.
#[derive(Debug)]
pub(crate) struct ClassView {
    pub(crate) class: TaskClass,
    pub(crate) evidence: ClassEvidence,
    pub(crate) outcomes: ClassOutcomes,
}

/// One class's public evidence, split by whether this harness reaches the
/// model. Each half is grouped by source, in rank order.
#[derive(Debug, Serialize)]
pub(crate) struct ClassEvidence {
    pub(crate) as_of: String,
    pub(crate) reachable: Vec<Reached>,
    pub(crate) unreachable: Vec<EvidenceEntry>,
}

/// An evidence entry whose model this harness reaches.
#[derive(Debug, Serialize)]
pub(crate) struct Reached {
    #[serde(flatten)]
    pub(crate) entry: EvidenceEntry,
    /// The values a spec's `model` takes for the matching models, in the
    /// harness's order. pi can reach one model through several providers.
    pub(crate) spec_values: Vec<String>,
}

/// One class's rows of the outcome report.
#[derive(Debug, Serialize)]
pub(crate) struct ClassOutcomes {
    /// How many teams the report read, of every class.
    pub(crate) teams: u64,
    pub(crate) rows: Vec<OutcomeLine>,
    pub(crate) unclassified: Unclassified,
    /// Why part of the report could not be read.
    pub(crate) errors: Vec<String>,
}

/// One group of past members, and whether it is too small to read much into.
#[derive(Debug, Serialize)]
pub(crate) struct OutcomeLine {
    #[serde(flatten)]
    pub(crate) row: OutcomeRow,
    pub(crate) small_sample: bool,
}

/// The past members whose team set no task class, of every harness. They
/// cannot be counted under any class.
#[derive(Debug, Default, Serialize)]
pub(crate) struct Unclassified {
    pub(crate) rows: u64,
    pub(crate) members: u64,
}

/// Asks `harness` for its models and, with `class`, picks that class's
/// evidence and outcomes. `outcomes` is called only for a class, because it
/// reads every team on the machine.
pub(crate) fn gather(
    harness: &dyn Harness,
    class: Option<TaskClass>,
    evidence: &Evidence,
    outcomes: &dyn Fn() -> OutcomeReport,
) -> Result<View, HarnessError> {
    let catalog = harness.models(MODELS_TIMEOUT)?;
    let class = class.map(|class| ClassView {
        class,
        evidence: class_evidence(evidence, class, &catalog),
        outcomes: class_outcomes(outcomes(), class),
    });
    Ok(View {
        harness: harness.id().as_str().to_owned(),
        families: families(harness, &catalog),
        catalog,
        class,
    })
}

/// The value a spec's `model` takes for `model`: `provider/id` when the
/// harness reaches it through a named provider, else the id.
pub(crate) fn spec_value(model: &ModelInfo) -> String {
    match &model.provider {
        Some(provider) => format!("{provider}/{}", model.id),
        None => model.id.clone(),
    }
}

/// `class`'s entries, reachable ones first, each half grouped by source in
/// the order the file first names each source, then by rank. An entry is
/// reachable when the harness lists the same model under any spelling
/// (`ModelCatalog::same_model`).
pub(crate) fn class_evidence(
    evidence: &Evidence,
    class: TaskClass,
    catalog: &ModelCatalog,
) -> ClassEvidence {
    let mut entries: Vec<&EvidenceEntry> = evidence
        .entries
        .iter()
        .filter(|e| e.task_class == class)
        .collect();
    let mut sources: Vec<&str> = Vec::new();
    for e in &entries {
        if !sources.contains(&e.source.as_str()) {
            sources.push(&e.source);
        }
    }
    let source_at = |e: &EvidenceEntry| sources.iter().position(|s| *s == e.source);
    entries.sort_by_key(|e| (source_at(e), e.rank));
    let mut reachable = Vec::new();
    let mut unreachable = Vec::new();
    for entry in entries {
        let mut spec_values: Vec<String> = Vec::new();
        for m in catalog.same_model(&entry.model) {
            let value = spec_value(m);
            if !spec_values.contains(&value) {
                spec_values.push(value);
            }
        }
        if spec_values.is_empty() {
            unreachable.push(entry.clone());
        } else {
            reachable.push(Reached {
                entry: entry.clone(),
                spec_values,
            });
        }
    }
    ClassEvidence {
        as_of: evidence.as_of.clone(),
        reachable,
        unreachable,
    }
}

/// `class`'s rows of `report`, and the count of rows no class can claim.
pub(crate) fn class_outcomes(report: OutcomeReport, class: TaskClass) -> ClassOutcomes {
    let mut rows = Vec::new();
    let mut unclassified = Unclassified::default();
    for row in report.rows {
        if row.task_class == UNCLASSIFIED {
            unclassified.rows += 1;
            unclassified.members += row.members;
        } else if row.task_class == class.as_str() {
            rows.push(OutcomeLine {
                small_sample: row.members < SMALL_SAMPLE,
                row,
            });
        }
    }
    ClassOutcomes {
        teams: report.teams,
        rows,
        unclassified,
        errors: report.errors,
    }
}

/// The view as one JSON object.
pub(crate) fn json(view: &View) -> Value {
    let catalog = catalog_json(&view.catalog, &view.families);
    let mut doc = json!({ "harness": view.harness, "catalog": catalog });
    if let (Some(class), Value::Object(map)) = (&view.class, &mut doc) {
        map.insert("task_class".into(), json!(class.class.as_str()));
        map.insert("evidence".into(), json!(class.evidence));
        map.insert("outcomes".into(), json!(class.outcomes));
    }
    doc
}

/// The view as text for a reader.
pub(crate) fn text(view: &View) -> String {
    let catalog = &view.catalog;
    let source = match catalog.source {
        CatalogSource::Listed => {
            "listed. The harness program printed this list, so it holds the models it \
             reaches on this machine. A provider can still refuse a listed model."
        }
        CatalogSource::Builtin => {
            "builtin. The harness has no list command, so this list comes from the \
             harness's docs. A model on it may still be out of reach for this account."
        }
    };
    let mut out = vec![
        format!("harness: {}", view.harness),
        format!("source: {source}"),
        format!("efforts: {}", catalog.efforts.join(", ")),
    ];
    if catalog.models.is_empty() {
        out.push("models: none".to_owned());
    } else {
        out.push(format!(
            "models: {}. Write a value of the first column in a spec's `model`.",
            catalog.models.len()
        ));
        let mut rows = vec![cells(&[
            "model", "family", "aliases", "context", "thinking",
        ])];
        rows.extend(
            catalog
                .models
                .iter()
                .zip(&view.families)
                .map(|(m, family)| {
                    vec![
                        spec_value(m),
                        family.as_str().to_owned(),
                        or_dash(&m.aliases.join(", ")),
                        m.context_tokens.map_or_else(|| "-".to_owned(), tokens),
                        if m.thinking { "yes" } else { "no" }.to_owned(),
                    ]
                }),
        );
        out.extend(table(&rows));
    }
    if let Some(class) = &view.class {
        out.push(String::new());
        out.extend(evidence_text(class));
        out.push(String::new());
        out.extend(outcomes_text(class));
    }
    out.join("\n") + "\n"
}

fn evidence_text(view: &ClassView) -> Vec<String> {
    let ev = &view.evidence;
    let mut out = vec![
        format!(
            "Public evidence for {}, as of {}.",
            view.class.as_str(),
            ev.as_of
        ),
        "Ranks compare the entries of one source only. The order of the sources means nothing."
            .to_owned(),
        format!(
            "Models this harness reaches: {} entries.",
            ev.reachable.len()
        ),
    ];
    let reached: Vec<(&EvidenceEntry, Option<&[String]>)> = ev
        .reachable
        .iter()
        .map(|r| (&r.entry, Some(r.spec_values.as_slice())))
        .collect();
    out.extend(entries_text(&reached));
    out.push(format!(
        "Models this harness does not reach: {} entries.",
        ev.unreachable.len()
    ));
    let unreached: Vec<(&EvidenceEntry, Option<&[String]>)> =
        ev.unreachable.iter().map(|e| (e, None)).collect();
    out.extend(entries_text(&unreached));
    out
}

/// Entries already in source order, with a heading at each new source.
fn entries_text(entries: &[(&EvidenceEntry, Option<&[String]>)]) -> Vec<String> {
    let mut out = Vec::new();
    let mut source: Option<&str> = None;
    for (e, spec_values) in entries {
        if source != Some(e.source.as_str()) {
            source = Some(&e.source);
            out.push(format!("  {}", e.source));
        }
        let effort = e
            .effort
            .as_deref()
            .map_or_else(String::new, |x| format!(", effort {x}"));
        let spec =
            spec_values.map_or_else(String::new, |v| format!(", spec model {}", v.join(" or ")));
        out.push(format!(
            "    rank {}: {}{effort} ({}){spec}",
            e.rank,
            e.model,
            e.kind.as_str()
        ));
        out.push(format!("      {}", e.claim));
        out.push(format!("      {} {}", e.published, e.url));
    }
    if out.is_empty() {
        out.push("  none".to_owned());
    }
    out
}

fn outcomes_text(view: &ClassView) -> Vec<String> {
    let o = &view.outcomes;
    let mut out = vec![
        format!(
            "Your past teams, {}: {} groups, from {} teams read.",
            view.class.as_str(),
            o.rows.len(),
            o.teams
        ),
        "Counts and medians only. A group shares a harness, model, effort and kind. The token \
         column is the median of each member's total tokens, including cache reads, which make \
         up most of each total."
            .to_owned(),
    ];
    if o.rows.is_empty() {
        out.push("  No member of a past team had this class.".to_owned());
    } else {
        let mut rows = vec![cells(&[
            "harness",
            "model",
            "effort",
            "kind",
            "members",
            "PASS",
            "FLAGGED",
            "FAIL",
            "unverified",
            "reprompts",
            "total tokens incl. cache reads",
            "minutes",
            "harder than class",
            "sample",
        ])];
        rows.extend(o.rows.iter().map(|line| {
            let r = &line.row;
            vec![
                r.harness.clone(),
                r.model.clone(),
                r.effort.clone(),
                r.kind.clone(),
                r.members.to_string(),
                r.verify.pass.to_string(),
                r.verify.flagged.to_string(),
                r.verify.fail.to_string(),
                r.verify.none.to_string(),
                r.reprompts.to_string(),
                r.tokens_median
                    .map_or_else(|| "-".to_owned(), |t| format!("{t:.0}")),
                r.minutes_median
                    .map_or_else(|| "-".to_owned(), |m| format!("{m:.1}")),
                r.harder_than_class.to_string(),
                if line.small_sample {
                    format!("small (fewer than {SMALL_SAMPLE})")
                } else {
                    String::new()
                },
            ]
        }));
        out.extend(table(&rows));
    }
    out.push(format!(
        "Unclassified: {} groups, {} members. Their teams set no task class, so they cannot be \
         split by class.",
        o.unclassified.rows, o.unclassified.members
    ));
    out.extend(o.errors.iter().map(|e| format!("Not read: {e}")));
    out
}

fn cells(texts: &[&str]) -> Vec<String> {
    texts.iter().map(|t| (*t).to_owned()).collect()
}

fn or_dash(text: &str) -> String {
    if text.is_empty() { "-" } else { text }.to_owned()
}

/// `rows` as lines indented by two spaces, each column as wide as its
/// widest cell.
fn table(rows: &[Vec<String>]) -> Vec<String> {
    let mut widths: Vec<usize> = Vec::new();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            let width = cell.chars().count();
            match widths.get_mut(i) {
                Some(w) => *w = (*w).max(width),
                None => widths.push(width),
            }
        }
    }
    rows.iter()
        .map(|row| {
            let padded: Vec<String> = row
                .iter()
                .zip(&widths)
                .map(|(cell, w)| format!("{cell:<w$}"))
                .collect();
            format!("  {}", padded.join("  ")).trim_end().to_owned()
        })
        .collect()
}

/// A context window as pi prints one: `1M`, `200K`, `65.5K`.
fn tokens(count: u64) -> String {
    match count {
        c if c >= 1_000_000 && c % 1_000_000 == 0 => format!("{}M", c / 1_000_000),
        c if c >= 1_000 && c % 1_000 == 0 => format!("{}K", c / 1_000),
        c if c >= 1_000 => format!("{}.{}K", c / 1_000, c % 1_000 / 100),
        c => c.to_string(),
    }
}
