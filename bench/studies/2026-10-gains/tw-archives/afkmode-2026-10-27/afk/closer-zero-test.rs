
// Closer's scratch test, appended to the clone's crates/teamwire-cli/tests/afk.rs only.
// Runs every reader against an AFK team with zero members and records each output.
fn closer_run(rig: &Rig, args: &[&str]) -> Output {
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_teamwire"));
    cmd.arg("--home")
        .arg(rig.home.path())
        .args(args)
        .current_dir(rig.repo.path())
        .env_clear()
        .env("HOME", rig.work.path())
        .env("PATH", format!("{}:/usr/bin:/bin", rig.fakes.display()))
        .env("HERDR_ENV", "1")
        .env("TEAMWIRE_CLAUDE_DIR", rig.claude.path())
        .env("CLAUDE_CODE_SESSION_ID", SESSION);
    cmd.output().unwrap()
}

fn closer_record(out_dir: &Path, name: &str, out: &Output) {
    let body = format!(
        "exit {:?}\n--- stdout\n{}--- stderr\n{}",
        out.status.code(),
        text(&out.stdout),
        text(&out.stderr)
    );
    std::fs::write(out_dir.join(format!("{name}.txt")), body).unwrap();
    if name.ends_with("json") {
        std::fs::write(out_dir.join(format!("{name}.out")), &out.stdout).unwrap();
    }
}

#[test]
fn closer_zero_member_readers() {
    let out_dir = PathBuf::from(std::env::var("CLOSER_OUT").unwrap());
    std::fs::create_dir_all(&out_dir).unwrap();
    let rig = Rig::new();
    let _lead = rig.lead();
    let task = rig.task(TASK);
    let args = ["--team", "zero", "--title", "Zero", "--task", task_arg(&task), "--budget", "2h"];
    closer_record(&out_dir, "00_afk_start", &rig.start(&args, true));
    for (name, a) in [
        ("01_status_team", vec!["status", "zero"]),
        ("02_status_team_json", vec!["status", "zero", "--json"]),
        ("03_status_all", vec!["status"]),
        ("04_status_all_json", vec!["status", "--json"]),
        ("05_teams", vec!["teams"]),
        ("06_teams_json", vec!["teams", "--json"]),
        ("07_verify", vec!["verify", "zero"]),
        ("08_metrics_team", vec!["metrics", "zero"]),
        ("09_metrics_all_json", vec!["metrics", "--json"]),
        ("10_reopen_dry", vec!["reopen", "zero", "--dry-run"]),
        ("11_watch_once", vec!["watch", "zero", "--once"]),
        ("12_hook_lead_stop", vec!["hook", "claude-lead-stop"]),
        ("13_close_dry", vec!["close", "zero", "--dry-run"]),
        ("14_close", vec!["close", "zero"]),
        ("15_teams_after", vec!["teams"]),
    ] {
        closer_record(&out_dir, name, &closer_run(&rig, &a));
    }
    // The exit-2 path, then close (D39 and docs/api/cli.md say close ends it).
    std::fs::create_dir_all(rig.team_dir("broken")).unwrap();
    std::fs::write(rig.team_dir("broken").join("afk"), "in the way").unwrap();
    let args = ["--team", "broken", "--title", "Broken", "--task", task_arg(&task)];
    closer_record(&out_dir, "20_afk_start_exit2", &rig.start(&args, true));
    closer_record(&out_dir, "21_afk_start_again", &rig.start(&["--team", "other", "--title", "Other", "--task", task_arg(&task)], true));
    closer_record(&out_dir, "22_close_broken", &closer_run(&rig, &["close", "broken"]));
    closer_record(&out_dir, "23_teams_end", &closer_run(&rig, &["teams"]));
}
