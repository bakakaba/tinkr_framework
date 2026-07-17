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

## Documentation

Documentation style rules live in the `documentation` skill
(`.agents/skills/documentation/SKILL.md`) — purpose over implementation,
minimal Arguments sections, runnable doctests only.

## Conventions

- All deps are declared in the root `[workspace.dependencies]`; crates use `{ workspace = true }`.
  Add new deps at the root, not per-crate.
- Root re-exports of the crate's own items are deliberately minimal (`pub use server::Server`
  only); prefer module-qualified paths (`bootstrap::init`, `utilities::new_id`) in docs and
  examples. Dependencies that appear in the public API are re-exported (`axum` plus the
  flattened `Router`/`routing`, and `tonic` behind the `grpc` feature) so users build against
  the versions the framework supports — use these re-exports in docs and the demo instead of
  direct axum/tonic deps where possible.
- `bootstrap::init` must stay a single function with no config parameters (per maintainer);
  it intentionally panics on double init.
- Tests that set the global tracing subscriber go in their own integration-test file
  (own process), e.g. `tests/bootstrap_double_init.rs`.
- The demo's gRPC code is generated from `crates/demo/proto/hello.proto` with `buf generate`
  (remote BSR plugins, versions pinned in `crates/demo/buf.gen.yaml`) and checked in under
  `crates/demo/src/gen/`. After editing the proto, run `just gen` (requires the `buf` CLI and
  network access) and commit the result — CI fails if the generated code drifts. Never edit
  `src/gen/` by hand.
- Releases are automated: `release-please` opens a release PR from conventional commits on
  `main`; merging it tags the release and publishes `tinkr_framework` to crates.io.
- Commit messages: conventional commits (`feat:`, `refactor:`, `chore:`).
