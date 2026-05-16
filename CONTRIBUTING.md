# Contributing to LocalVibe

Thanks for your interest! LocalVibe is a small, opinionated project — issues and
PRs are welcome.

## Quick start

```bash
git clone https://github.com/Sok205/local_vibe
cd local_vibe
cargo build --workspace
cargo test  --workspace
```

Apple Silicon is required for the `lv-metal` backend; on Linux, exclude it:

```bash
cargo test --workspace --exclude lv-metal
```

## Before you open a PR

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace`

CI runs the same three commands and will block on any failure.

## Commit style

Follow Conventional Commits where it makes sense (`feat:`, `fix:`, `chore:`,
`docs:`, `refactor:`, `test:`). One logical change per commit.

## Filing issues

Please include:
- LocalVibe version (`lv --version`) and commit SHA
- OS / arch (`uname -a`)
- A minimal reproduction or the command + flags you ran
- Relevant log output (set `RUST_LOG=lv=debug`)

## Licensing

By contributing, you agree that your contributions will be dual-licensed under
the MIT and Apache-2.0 licenses, the same terms as the rest of the project.
