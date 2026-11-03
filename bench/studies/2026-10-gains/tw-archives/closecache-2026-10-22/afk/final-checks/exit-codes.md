# Final checks on cf45e6a (afk/closecache), run by the lead in .worktrees/closecache-impl, 2026-10-22T14:48:53Z
- `cargo test -p teamwire-close --locked`: exit 0
- `cargo test -p teamwire-cli --locked --test close`: exit 0
- `scripts/gate.sh fast`: exit 0
