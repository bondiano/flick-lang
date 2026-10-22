# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p flick-wasm --target wasm32-unknown-unknown -- -D warnings

# Format everything
fmt:
    cargo fmt --all

# Run the test suites
test:
    cargo test --workspace

# Install the JS runtime, check its pinned Effect, run compiled programs on Node
e2e:
    pnpm install --frozen-lockfile
    pnpm -r test
    cargo test -p flick --test e2e -- --ignored

# Build the wasm module the in-browser demo loads
wasm:
    cargo build -p flick-wasm --target wasm32-unknown-unknown --release

# Everything CI runs
check: lint test
