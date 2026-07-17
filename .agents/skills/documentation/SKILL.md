---
name: documentation
description: Use when writing or reviewing doc comments (rustdoc `///`, `//!`), README content, or API documentation in this repository. Defines the house style: purpose over implementation, minimal Arguments sections, self-documenting errors, runnable examples only.
---

# Documentation Style

Doc comments describe purpose and observable behavior. They never describe
implementation.

## Rules

1. **Purpose first.** Open with saying what the item is for.
2. **No implementation detail in doc comments.** Internal types, dependency
   crate names, algorithms, and design rationale don't belong in `///`/`//!`.
   Rationale useful to maintainers goes in regular `//` comments instead.
3. **Observable contracts are not implementation detail.** Environment
   variables read (`RUST_LOG`, `KUBERNETES_SERVICE_HOST`, ...), output
   formats, and panic conditions affect users — document them.
   Exception for naming dependencies: keep a dependency name/link when it is
   the most precise description of behavior (e.g. `tracing-stackdriver` for
   the `gcp` log format).
4. **Inputs/outputs only when non-obvious.** The signature documents names
   and types. Add an `# Arguments` section with one bullet per argument only
   when an argument can easily be misunderstood (e.g. accepted forms of
   `serve(target)`, "pass the generated server type"). No `# Returns`
   heading — a short prose sentence when the output shape isn't obvious
   (e.g. `new_id` returns `{prefix}_{ULID}`).
5. **Errors are self-documenting.** Document each variant on the error type;
   do not repeat "Returns `Error::X` if ..." on functions.
6. **`# Panics` section** on any public function that can panic, stating when.
7. **Examples are welcome — as runnable doctests.** Every example must be
   executed by `cargo test` (never `no_run`, `ignore`, or untested blocks;
   README included). Convoluted uses should always have one. Flows that can't
   run in a doctest (e.g. `serve()` blocks on signals) reference
   `crates/demo/examples/` instead — demonstrate topics with full demos
   there, not inline pseudo-code.
8. **Crate root docs stay minimal:** a purpose statement plus a
   `# Features` section. Don't enumerate modules or items — rustdoc lists
   them with their own summaries. Don't announce re-exports in prose; they
   are discoverable through tooling.
9. **Single source of truth.** Never restate API specifics away from the code
   they describe — copies drift. Crate docs don't repeat item docs; item docs
   don't repeat each other.
10. **Sparse on purpose.** Don't add doc-lint enforcement
    (`missing_errors_doc`, etc.) — the goal is documentation only where it
    earns its place.

## Litmus test

For every sentence ask: does the user need this to *use* the API correctly?
If it explains *how* the item works rather than *what it does for you*,
delete it or demote it to a `//` comment.
