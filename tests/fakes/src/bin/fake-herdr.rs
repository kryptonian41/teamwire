//! A fake `herdr` for tests. All of its behaviour is in
//! `teamwire_fakes::herdr_bin`.

fn main() -> std::process::ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    teamwire_fakes::herdr_bin::run(&argv)
}
