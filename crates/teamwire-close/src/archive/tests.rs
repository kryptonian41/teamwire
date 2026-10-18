//! The archive must check out at every instant of the second backup.
#![expect(
    clippy::disallowed_methods,
    reason = "a test writes files as a stand-in for a crash or a bad disk"
)]

use std::cell::RefCell;

use teamwire_core::{Clock, Config, HarnessId, HostId, LeadRoute, Registry, TeamName};
use teamwire_fakes::{FakeClock, FakePlatform, TestHome};
use teamwire_store::{CloseEnd, Ctx, NewTeam, Store};

use super::*;

/// A stage, whether the archive verified there, and whether `team.db` was new.
type Seen = (String, Result<(), String>, bool);

#[test]
fn every_state_of_the_second_backup_verifies() {
    let home = TestHome::new().unwrap();
    let team = TeamName::parse("alpha").unwrap();
    let mut store = Store::open(home.path(), &team).unwrap();
    let clock = FakeClock::new();
    let platform = FakePlatform::new();
    let registry = Registry::new();
    let config = Config::default();
    let now = clock.now();
    store
        .write(|tx| {
            tx.create_team(
                now,
                &NewTeam {
                    name: team.clone(),
                    title: "Alpha".to_owned(),
                    lead_harness: HarnessId::from_static("claude-code"),
                    lead_host: HostId::from_static("herdr"),
                    lead_route: LeadRoute::Sink,
                    lead_session: None,
                },
            )?;
            tx.begin_close(now, teamwire_core::Pid::new(1))
        })
        .unwrap();
    let team_dir = home.path().join("teams").join("alpha");
    std::fs::write(team_dir.join("note.md"), "x\n").unwrap();
    let dest = home.path().join("archive").join("alpha-1");
    let ctx = Ctx {
        home: home.path(),
        store: &mut store,
        registry: &registry,
        platform: &platform,
        clock: &clock,
        config: &config,
    };
    write(&ctx, &TeamName::parse("alpha").unwrap(), &team_dir, &dest).unwrap();
    verify_files(&team_dir, &dest).unwrap();
    let old_db = std::fs::read(dest.join("team.db")).unwrap();

    ctx.store
        .write(|tx| tx.finish_close(now, &CloseEnd::Completed))
        .unwrap();
    let seen: RefCell<Vec<Seen>> = RefCell::new(Vec::new());
    refresh_db_observed(&ctx, &dest, &|stage| {
        let changed = std::fs::read(dest.join("team.db")).unwrap() != old_db;
        seen.borrow_mut()
            .push((stage.to_owned(), verify_files(&team_dir, &dest), changed));
    })
    .unwrap();
    let seen = seen.into_inner();
    let stages: Vec<&str> = seen.iter().map(|(s, _, _)| s.as_str()).collect();
    assert_eq!(stages, ["sums_next", "db_replaced", "sums_replaced"]);
    for (stage, result, _) in &seen {
        assert_eq!(result, &Ok(()), "{stage}");
    }
    // The middle states really differ: the database is old, then new.
    assert_eq!(
        seen.iter().map(|s| s.2).collect::<Vec<_>>(),
        [false, true, true]
    );
    // The end state has only the archive's own files, and the plain check passes.
    verify(&ctx, &team_dir, &dest, true).unwrap();
    assert!(!dest.join(SUMS_NEXT).exists() && !dest.join(DB_NEW).exists());

    // A crash left a partial backup and a next-sums file: still checkable,
    // and the next attempt cleans up.
    std::fs::write(dest.join(DB_NEW), b"partial").unwrap();
    verify_files(&team_dir, &dest).unwrap();
    refresh_db(&ctx, &dest).unwrap();
    verify(&ctx, &team_dir, &dest, true).unwrap();
    assert!(!dest.join(DB_NEW).exists());

    // A database that matches neither list is still refused.
    std::fs::write(
        dest.join(SUMS_NEXT),
        std::fs::read(dest.join(SUMS)).unwrap(),
    )
    .unwrap();
    std::fs::write(dest.join("team.db"), b"damaged").unwrap();
    assert!(verify_files(&team_dir, &dest).is_err());
}
