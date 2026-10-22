# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# Format everything
fmt:
    cargo fmt --all

# Run the test suites
test:
    cargo test --workspace

# Everything CI runs
check: lint test
