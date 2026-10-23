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
    pnpm -r typecheck
    pnpm -r lint
    pnpm -r test
    cargo test -p flick --test e2e --test e2e_numbers --test test_command --test dts_command --test build_command --test ffi_command --test cli_run -- --ignored
    cargo test -p flick-core --test consteval_node -- --ignored

# Run SQL queries against PostgreSQL in Docker (after `just e2e` installs the runtime)
e2e-postgres:
    cargo test -p flick --test sql_postgres -- --ignored

# Mutation-fuzz the parser (the nightly CI job runs 1 000 000 inputs)
fuzz iterations="1000000":
    FLICK_FUZZ_ITERATIONS={{iterations}} cargo test --release -p flick-core --test fuzz

# Build the wasm module the in-browser demo loads and hold it to its size budget
wasm:
    cargo build -p flick-wasm --target wasm32-unknown-unknown --release
    test $(wc -c < target/wasm32-unknown-unknown/release/flick_wasm.wasm) -le 4194304

# Everything CI runs
check: lint test

# Regenerate the tree-sitter grammar Zed uses and run its corpus tests
tree-sitter:
    cd editors/tree-sitter-flick && tree-sitter generate && tree-sitter test
