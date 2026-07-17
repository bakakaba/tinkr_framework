# Run format check and clippy
lint:
    cargo fmt --check
    cargo clippy

# Auto-fix formatting and clippy warnings
fix:
    cargo fmt
    cargo clippy --fix

# Run all tests
test:
    cargo test

# Regenerate the demo's gRPC code from proto/hello.proto (requires buf)
gen:
    cd crates/demo && buf generate

# Regenerate the demo's config.schema.json from its config structs
schema:
    cd crates/demo && cargo run --example gen_schema
