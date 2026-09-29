//! Crash tests: the real `teamwire` binary is killed with SIGKILL at random
//! points, then the store is reopened and checked (`docs/test-strategy.md`
//! section 3). The tests live in `tests/`; this library is empty.
//!
//! Never: run against a real herdr or Claude session, or a real home.
