# AGENTS.md

Rust workspace with two crates: `crates/tinkr_framework` (the published library) and
`crates/demo` (`publish = false`, exercises the framework end-to-end).

## Commands

- `just lint` — `cargo fmt --check` + `cargo clippy`
- `just fix` / `just test`
- Single test: `cargo test -p tinkr_framework <name>`; demo integration tests: `cargo test -p demo`
- Feature matrix matters — verify changes with all three:
  `cargo build`, `cargo build --all-features`, `cargo build --no-default-features`
- Doc links are feature-sensitive; check `cargo doc -p tinkr_framework --all-features --no-deps`
  for broken intra-doc link warnings after editing rustdoc.

## Features (crates/tinkr_framework)

- `grpc` (default): gates `tonic`/`tower`/`http` deps and all gRPC server code.
  New code touching gRPC must be `#[cfg(feature = "grpc")]`-gated and compile with
  `--no-default-features`.
- `gcp` (non-default): gates `tracing-stackdriver`; `bootstrap::init` picks the log layer
  per feature + deployment env vars (`KUBERNETES_SERVICE_HOST`, `K_SERVICE`, `CLOUD_RUN_JOB`).
- docs.rs builds with `all-features` and `--cfg docsrs`; use `#[cfg_attr(docsrs, doc(cfg(...)))]`
  on feature-gated public items.

## Conventions

- All deps are declared in the root `[workspace.dependencies]`; crates use `{ workspace = true }`.
  Add new deps at the root, not per-crate.
- Root re-exports are deliberately minimal (`pub use server::Server` only); prefer
  module-qualified paths (`bootstrap::init`, `utilities::new_id`) in docs and examples.
- `bootstrap::init` must stay a single function with no config parameters (per maintainer);
  it intentionally panics on double init.
- Tests that set the global tracing subscriber go in their own integration-test file
  (own process), e.g. `tests/bootstrap_double_init.rs`.
- `crates/demo/build.rs` compiles `proto/hello.proto` via `tonic-prost-build`; regenerating
  happens automatically on build, nothing to run manually.
- Commit messages: conventional commits (`feat:`, `refactor:`, `chore:`).
