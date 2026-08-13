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

- `grpc` (default): gates `tonic`/`tower`/`tonic-reflection` deps and all gRPC server
  code. New code touching gRPC must be `#[cfg(feature = "grpc")]`-gated and compile with
  `--no-default-features`. (`http` is enabled by both `grpc` and `otel`.)
- `otel` (default): gates the `opentelemetry*`/`tracing-opentelemetry` deps, the OTLP
  export pipeline (`src/otel.rs`), the request-span middleware, the request-metrics
  middleware (`src/otel/metrics.rs`: semconv `http.server.request.duration` +
  `http.server.active_requests`, gRPC-aware — `grpc-status` is read from response
  trailers via a body wrapper), and the `/health` `otel` field (per-signal booleans). Runtime-gated: inert unless an OTLP endpoint resolves (per-signal
  `OTEL_EXPORTER_OTLP_*_ENDPOINT` > `otel_endpoint` base config field;
  `OTEL_{TRACES,METRICS,LOGS}_EXPORTER=none` disables a signal). Auth headers come from
  `otel_headers`/`OTEL_EXPORTER_OTLP_{,SIGNAL_}HEADERS`; `https://` endpoints use rustls
  (system roots; the spec's `CERTIFICATE`/`CLIENT_CERTIFICATE`/`CLIENT_KEY` PEM-path env
  vars override — implemented in `src/otel.rs`, not upstream). `src/otel.rs` must not
  depend on `tonic` directly (use the `opentelemetry_otlp::tonic_types` re-exports): the
  crate must also compile with `--no-default-features --features otel` (otel without grpc).
- Deployed log output (env vars `KUBERNETES_SERVICE_HOST`, `K_SERVICE`, `CLOUD_RUN_JOB`)
  is the in-house JSON formatter in `src/logging.rs` (Cloud-Logging-compatible keys +
  generic `trace_id`/`span_id` correlation); local runs get the pretty fmt layer.
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
  crate's Cargo.toml — release-please bumps those version requirements via the
  extra-files updaters in `.github/release-please-config.json`, never in
  `[workspace.dependencies]`.
- All crates share one version: `[workspace.package] version` in the root Cargo.toml,
  inherited with `version.workspace = true`. Never version a crate individually. Adding a
  published crate means extending the release-please extra-files (the `Cargo.lock`
  jsonpath filter, plus a `dependencies.<name>.version` entry for any new internal
  `{ path, version }` requirement).
- Root re-exports of the crate's own items are deliberately minimal (`Server`, the `init!`
  macro, and the `config` re-export; the `bootstrap` module stays private). Prefer
  module-qualified paths for everything else (`utilities::new_id`) in docs and examples.
  Dependencies that appear in the public API are re-exported (`tinkr_config` as `config`,
  `axum` plus the flattened `Router`/`routing`, and `tonic` behind the `grpc` feature) so
  users build against the versions the framework supports — use these re-exports in docs
  and the demo instead of direct deps where possible. Generated tonic/prost code names
  `tonic`, `tonic_prost`, and `prost` directly, so consumer crates with generated services
  (the demo included) declare those three as direct deps; Cargo unifies them with the
  framework's versions as long as the majors match.
- `init!` is the single entry point: it loads the configuration (returning
  `&'static Config<T>`; `init!()` loads `Config<()>`) and sets up logging, and must be
  called before building a `Server` (`Server::new` panics otherwise). It intentionally
  panics on double init. The `Server` reads `name`/`version`/`port`/`shutdown_timeout`
  from the loaded config via `config::base()` — there is deliberately no builder
  method or argument to override them; `bind()` (repeatable) is the only serve-target
  knob, and calling it replaces the implicit dual-stack `{port}` bind.
- Lint opt-outs: prefer restructuring the code so no opt-out is needed. When one is
  genuinely required, hand-written code uses `#[expect(lint, reason = "...")]` — never
  bare `#[allow]` — so a redundant opt-out fails CI via `unfulfilled_lint_expectations`
  and the justification lives in the attribute. Enforced by `[workspace.lints]` in the
  root Cargo.toml (`clippy::allow_attributes`, `clippy::allow_attributes_without_reason`)
  plus the existing `-D warnings`. Exceptions: generated code (`src/gen/`, expected at the
  `pub mod pb` include site in the demo) and macro-emitted code (`quote!` blocks, where
  whether the lint fires depends on consumer input) keep `#[allow]`, with a `reason`
  where hand-written.
- Tests that set the global tracing subscriber, mutate environment variables, load the
  global configuration, or change the working directory go in their own integration-test
  file (own process), e.g. `tests/bootstrap_double_init.rs`, `tests/config_load.rs`.
- Configuration (`tinkr_config`): consumers derive `Configurable`; precedence is env var >
  config file > `#[config(default)]`. The file is resolved from the `config_file = ...`
  parameter of `load!`/`init!` (full control; `$CONFIG_FILE` ignored), else `$CONFIG_FILE`,
  else `config.toml` (CWD); explicitly named files must exist, only the default may be
  absent. `load!`/`get` are the global path (frozen,
  panic on double-load/unloaded-get, matching `init`); `parse` is the test/tooling seam.
  Top-level keys `port`, `env`, `shutdown_timeout`, `name`, `version` are reserved
  for the provided fields. The `env` field is typed: `tinkr_config::Env` by default,
  or a consumer enum deriving `Environment` selected via struct-level
  `#[config(env = MyEnv)]`. The derive resolves its runtime paths via `proc-macro-crate`
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
- Bumping the tonic/prost major version is a breaking change to the framework's public API
  (re-exports + `Server` bounds): bump the plugin pins in `crates/demo/buf.gen.yaml` to the
  matching releases and rerun `just gen` in the same PR —
  CI (generated-code drift check + compile) fails otherwise. Minor/patch bumps need none of
  this; Cargo unifies them.
- Releases are automated and treat the workspace as one unit: `release-please` (single
  root package, `release-type: simple`) opens a release PR from conventional commits on
  `main`, bumping the root `CHANGELOG.md`, the workspace version, the internal dep
  requirements, and `Cargo.lock` — every crate always releases at the same
  version, changed or not. Merging it tags the release (single `v*` tag + GitHub release)
  and publishes the three library crates in dependency order via crates.io Trusted
  Publishing (OIDC; no long-lived token). The root `CHANGELOG.md` is the only changelog;
  each published crate has a `CHANGELOG.md` symlink to it so the content ships in the
  `.crate` package (cargo dereferences symlinks when packaging).
