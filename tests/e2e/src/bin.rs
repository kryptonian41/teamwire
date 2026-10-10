//! The `teamwire` binary the end-to-end tests run. `teamwire_fakes::teamwire_cli`
//! builds it once per test process, or with `TEAMWIRE_FAKES_PREBUILT=1` uses
//! the one already built after checking that it is not older than its sources.
//! The crash tests get theirs the same way.

use std::path::PathBuf;

/// The path of the built `teamwire` binary.
pub(crate) fn teamwire() -> PathBuf {
    teamwire_fakes::teamwire_cli()
        .unwrap_or_else(|e| panic!("cannot get the teamwire binary: {e}"))
        .to_path_buf()
}
