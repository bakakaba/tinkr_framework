compose := `command -v podman >/dev/null 2>&1 && echo "podman compose" || echo "docker compose"`

# Start the local observability stack (Grafana at http://localhost:3000)
up:
    {{compose}} up -d

# Stop the local observability stack (storage is ephemeral: this resets it)
down:
    {{compose}} down

# Run format check and clippy, exactly as CI does (warnings fail)
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Auto-fix formatting and clippy warnings
fix:
    cargo fmt --all
    cargo clippy --fix --workspace --all-targets

# Run all tests
test:
    cargo test

# Regenerate the demo's gRPC code from proto/hello.proto (requires buf)
[working-directory: 'crates/demo']
gen:
    buf generate

# Regenerate the demo's config.schema.json from its config structs
[working-directory: 'crates/demo']
schema:
    cargo run --example gen_schema

# Run a demo example (quickstart, kitchen_sink, config); default: quickstart
[working-directory: 'crates/demo']
dev demo="quickstart":
    cargo run --example {{demo}}
