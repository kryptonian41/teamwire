//! End-to-end tests: the real `teamwire` binary, real git, `fake-herdr` and
//! `fake-claude` on a test-built `PATH`, and a temporary `TEAMWIRE_HOME`
//! (`docs/test-strategy.md` section 5). The tests live in `tests/`; this
//! library is empty.
//!
//! Never: run against a real herdr or Claude session, or a real home.
