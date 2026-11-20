//! `teamwire models`: the models a harness reaches on this machine, and
//! with `--for <class>` the public evidence and the user's past teams for
//! that task class, side by side and never blended.
//!
//! The evidence is `examples/models/evidence.json`, compiled into the
//! binary the way `preset` compiles the presets, so the command works
//! wherever it is installed. The past teams come from `outcome_report`,
//! which reads every team and archive under the state directory.
//!
//! Never: write anything, or name one model as the best. The lead chooses.

mod family;
mod view;

use std::process::ExitCode;

use teamwire_core::{Evidence, HarnessId, TaskClass};

use crate::cmd::shared::{REFUSED, fail, home_or_refuse, say};
use crate::wiring::Wiring;
use crate::{Global, ModelsArgs, USAGE};

/// The public evidence, as of the date inside it.
const EVIDENCE: &str = include_str!("../../../../examples/models/evidence.json");

pub(crate) fn run(args: &ModelsArgs, global: &Global, wiring: &Wiring) -> ExitCode {
    let ModelsArgs {
        harness,
        class,
        json,
    } = args;
    let class = match class.as_deref().map(TaskClass::parse) {
        None => None,
        Some(Ok(class)) => Some(class),
        Some(Err(_)) => {
            let names: Vec<&str> = TaskClass::ALL.iter().map(|c| c.as_str()).collect();
            return fail(
                "models",
                &format!(
                    "no task class named {}; the classes are {}",
                    class.as_deref().unwrap_or_default(),
                    names.join(", ")
                ),
                USAGE,
            );
        }
    };
    let id = match harness {
        Some(id) => match HarnessId::parse(id) {
            Ok(id) => id,
            Err(e) => return fail("models", &e.to_string(), USAGE),
        },
        None => wiring.lead_harness.clone(),
    };
    let Ok(adapter) = wiring.registry.harness(&id) else {
        return fail(
            "models",
            &format!(
                "no harness named {id}; the harnesses are {}",
                wiring.registry.harness_ids().join(", ")
            ),
            USAGE,
        );
    };
    let evidence: Evidence = match serde_json::from_str(EVIDENCE) {
        Ok(evidence) => evidence,
        Err(e) => {
            return fail(
                "models",
                &format!("the evidence file is damaged: {e}"),
                REFUSED,
            );
        }
    };
    let home = match class {
        Some(_) => match home_or_refuse("models", global) {
            Ok(home) => Some(home),
            Err(code) => return code,
        },
        None => None,
    };
    let outcomes = || {
        home.as_deref()
            .map(teamwire_otel::outcome_report)
            .unwrap_or_default()
    };
    match view::gather(adapter, class, &evidence, &outcomes) {
        Ok(view) if *json => say(&format!("{}\n", view::json(&view))),
        Ok(view) => say(&view::text(&view)),
        Err(e) => {
            return fail(
                "models",
                &format!("harness {id} could not list its models: {e}"),
                REFUSED,
            );
        }
    }
    ExitCode::SUCCESS
}
