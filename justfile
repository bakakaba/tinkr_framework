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
