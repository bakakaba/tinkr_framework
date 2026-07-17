# AGENTS.md

Rust workspace with four crates: `crates/tinkr_framework` (the published library),
`crates/tinkr_config` + `crates/tinkr_config_macros` (published; layered configuration and
its derive macro, re-exported as `tinkr_framework::config`), and `crates/demo`
(`publish = false`, exercises the framework end-to-end).

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

- External deps are declared in the root `[workspace.dependencies]`; crates use
  `{ workspace = true }`. Add new deps at the root, not per-crate. **Exception:** internal
  crate deps that appear in published manifests (`tinkr_config` in the framework,
  `tinkr_config_macros` in `tinkr_config`) declare `{ path, version }` in the consuming
  crate's Cargo.toml — release-please only bumps version requirements there, never in
  `[workspace.dependencies]`.
- Root re-exports of the crate's own items are deliberately minimal (`Server`, the `init!`
  macro, and the `config` re-export; the `bootstrap` module stays private). Prefer
  module-qualified paths for everything else (`utilities::new_id`) in docs and examples.
  Dependencies that appear in the public API are re-exported (`tinkr_config` as `config`,
  `axum` plus the flattened `Router`/`routing`, and `tonic` behind the `grpc` feature) so
  users build against the versions the framework supports — use these re-exports in docs
  and the demo instead of direct deps where possible.
- `init!` is the single entry point: it loads the configuration (returning
  `&'static Config<T>`; `init!()` loads `Config<()>`) and sets up logging, and must be
  called before building a `Server` (`Server::new` panics otherwise). It intentionally
  panics on double init. The `Server` reads `name`/`version`/`port`/`shutdown_timeout`
  from the loaded config via `config::get::<()>()` — there is deliberately no builder
  method or argument to override them; `bind()` (repeatable) is the only serve-target
  knob, and calling it replaces the implicit dual-stack `{port}` bind.
- Tests that set the global tracing subscriber, mutate environment variables, load the
  global configuration, or change the working directory go in their own integration-test
  file (own process), e.g. `tests/bootstrap_double_init.rs`, `tests/config_load.rs`.
- Configuration (`tinkr_config`): consumers derive `Configurable`; precedence is env var >
  `config.toml` (CWD) > `#[config(default)]`. `load!`/`get` are the global path (frozen,
  panic on double-load/unloaded-get, matching `init`); `parse` is the test/tooling seam.
  Top-level keys `port`, `environment`, `shutdown_timeout`, `name`, `version` are reserved
  for the provided fields. The derive resolves its runtime paths via `proc-macro-crate`
  (direct `tinkr_config` dep or the `tinkr_framework::config` re-export) — don't use the
  derive inside `tinkr_config` itself (the provided fields are hand-written for this
  reason).
- The demo's `config.schema.json` is generated (`just schema`) and checked in — CI fails
  if the committed schema drifts from the config structs. Never edit it by hand.
- The demo's gRPC code is generated from `crates/demo/proto/hello.proto` with `buf generate`
  (remote BSR plugins, versions pinned in `crates/demo/buf.gen.yaml`) and checked in under
  `crates/demo/src/gen/`. After editing the proto, run `just gen` (requires the `buf` CLI and
  network access) and commit the result — CI fails if the generated code drifts. Never edit
  `src/gen/` by hand.
- Releases are automated: `release-please` opens a release PR from conventional commits on
  `main` (all crates version-locked via `linked-versions`); merging it tags the release
  (single `v*` tag + GitHub release, owned by `tinkr_framework`) and publishes the three
  library crates with `cargo publish --workspace` in dependency order. `tinkr_config` and
  `tinkr_config_macros` keep their own `CHANGELOG.md` (entries assigned by touched paths);
  only `demo` skips changelogs.
- Commit messages: conventional commits (`feat:`, `refactor:`, `chore:`).
