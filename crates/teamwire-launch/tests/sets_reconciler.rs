//! The set warnings of LCH-34 about a reconciler: alone when no reader
//! carries its label, early while a reader of its set has not reported.
//! A reader has reported when the store holds a completion of its current
//! generation.

#[expect(
    dead_code,
    reason = "each test binary uses a different part of the shared helpers"
)]
#[cfg(test)]
#[path = "support/common.rs"]
mod common;

#[cfg(test)]
#[path = "support/sets.rs"]
mod sets;

use common::{Rig, pane_member, team_spec};
use serde_json::json;
use sets::{
    GPT, complete, fail, launch, reader, reconciler, remove, reprompt, set_warnings, three_readers,
};

#[test]
fn a_correct_set_and_a_reconciler_added_after_every_report_print_no_set_warning() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let readers = three_readers(cwd);
    assert!(set_warnings(&rig, &readers, false).is_empty());
    let mut store = launch(&rig, &readers);
    for role in ["reva", "revb", "revc"] {
        complete(&rig, &mut store, role, 1);
    }
    drop(store);
    let warnings = set_warnings(&rig, &[reconciler("rec", Some("rev1"), cwd)], true);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_reconciler_whose_label_no_reader_carries_is_alone() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let [a, b, _] = three_readers(cwd);
    let warnings = set_warnings(&rig, &[a, b, reconciler("rec", Some("rev2"), cwd)], false);
    assert_eq!(
        warnings,
        ["set_reconciler_alone: rec is a reconciler with no set to read"]
    );
}

#[test]
fn an_unlabeled_reconciler_in_a_team_with_no_set_label_is_alone() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let plain = common::with(pane_member("api", "api", cwd), "model", json!(GPT));
    let warnings = set_warnings(&rig, &[plain, reconciler("rec", None, cwd)], false);
    assert_eq!(
        warnings,
        ["set_reconciler_alone: rec is a reconciler with no set to read"]
    );
}

/// A preset holds the readers and the reconciler in one file, with a
/// `_launch_after` note on the reconciler. The note earns no exemption.
#[test]
fn a_reconciler_in_the_spec_file_of_its_readers_is_early() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let [a, b, c] = three_readers(cwd);
    let rec = common::with(
        reconciler("rec", Some("rev1"), cwd),
        "_launch_after",
        json!("every reader has reported"),
    );
    let warnings = set_warnings(&rig, &[a, b, c, rec], false);
    let early = |r: &str| {
        format!(
            "set_reconciler_early: rec reconciles set rev1, but {r} has not reported; add the \
             reconciler after every reader has reported"
        )
    };
    assert_eq!(warnings, [early("reva"), early("revb"), early("revc")]);
}

#[test]
fn a_reconciler_added_while_a_stored_reader_has_no_result_is_early() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let mut store = launch(&rig, &three_readers(cwd));
    complete(&rig, &mut store, "reva", 1);
    complete(&rig, &mut store, "revc", 1);
    drop(store);
    let warnings = set_warnings(&rig, &[reconciler("rec", Some("rev1"), cwd)], true);
    assert_eq!(
        warnings,
        [
            "set_reconciler_early: rec reconciles set rev1, but revb has not reported; add the \
          reconciler after every reader has reported"
        ]
    );
}

/// A reader that reported, then was reprompted, holds only a completion of
/// an older generation: it has not reported.
#[test]
fn a_reader_with_a_completion_of_an_older_generation_only_has_not_reported() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let mut store = launch(&rig, &three_readers(cwd));
    for role in ["reva", "revb", "revc"] {
        complete(&rig, &mut store, role, 1);
    }
    reprompt(&rig, &mut store, "revb");
    drop(store);
    let warnings = set_warnings(&rig, &[reconciler("rec", Some("rev1"), cwd)], true);
    assert_eq!(
        warnings,
        [
            "set_reconciler_early: rec reconciles set rev1, but revb has not reported; add the \
          reconciler after every reader has reported"
        ]
    );
}

/// `reva` fails and never reports, and its replacement `revd` reports: the
/// reconciler gets no `set_reconciler_early` because of the lost `reva`.
#[test]
fn a_lost_reader_never_holds_back_the_reconciler() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let mut store = launch(&rig, &three_readers(cwd));
    fail(&rig, &mut store, "reva");
    drop(store);
    let plan = rig
        .validate(
            &team_spec("demo", &[reader("revd", Some(GPT), "rev1", cwd)]),
            true,
        )
        .unwrap();
    let (_, mut store) = rig.launch("demo", plan);
    for role in ["revb", "revc", "revd"] {
        complete(&rig, &mut store, role, 1);
    }
    drop(store);
    let warnings = set_warnings(&rig, &[reconciler("rec", Some("rev1"), cwd)], true);
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A reader that reported and was then removed still counts, so the set is not too small and the reconciler not alone.
#[test]
fn a_reader_removed_after_it_reported_still_counts() {
    let rig = Rig::new();
    let cwd = rig.work.path();
    let [a, b, _] = three_readers(cwd);
    let mut store = launch(&rig, &[a, b]);
    complete(&rig, &mut store, "reva", 1);
    complete(&rig, &mut store, "revb", 1);
    remove(&rig, &mut store, "revb");
    let rec = [reconciler("rec", Some("rev1"), cwd)];
    let warnings = set_warnings(&rig, &rec, true);
    assert!(warnings.is_empty(), "revb removed: {warnings:?}");
    remove(&rig, &mut store, "reva");
    drop(store);
    let warnings = set_warnings(&rig, &rec, true);
    assert!(warnings.is_empty(), "both removed: {warnings:?}");
}
