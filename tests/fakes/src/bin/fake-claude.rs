//! A fake `claude` for tests. All of its behaviour is in
//! `teamwire_fakes::claude_bin`.

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    teamwire_fakes::claude_bin::run(&argv)
}
