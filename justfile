# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy -p flick-wasm --target wasm32-unknown-unknown -- -D warnings
    cargo machete

# Format everything
fmt:
    cargo fmt --all

# Run the test suites
test:
    cargo test --workspace

# Install the JS runtime, check its pinned Effect, run compiled programs on Node
e2e: docs-wasm
    cargo build -p flick
    pnpm install --frozen-lockfile
    pnpm -r typecheck
    pnpm -r lint
    pnpm -r test
    cargo test -p flick --test e2e --test e2e_numbers --test test_command --test dts_command --test build_command --test ffi_command --test cli_run --test cli_devtools --test cli_template --test cli_fullstack --test first_slice --test examples -- --ignored
    cargo test -p flick-core --test consteval_node -- --ignored

# Bundle the tracked programs (release) with rolldown and hold them to their gzip budgets
bundle-size:
    node tools/bundle-size/index.mjs "$(cargo build -p flick --message-format=json-render-diagnostics | jq -r 'select(.executable != null) | .executable')"

# Time the language server on a file: opens, hovers and a hover after an edit
lsp-bench file="examples/mtl/src/Mtl/StateT.fl" +positions="24:5":
    cargo build --release -p flick
    node tools/lsp-bench/index.mjs target/release/flick {{file}} {{positions}}

# Hold the language server to a long open/edit/close session and print its resident memory (Linux)
lsp-soak file="examples/mtl/src/Mtl/StateT.fl" cycles="400":
    cargo build --release -p flick
    node tools/lsp-bench/soak.mjs target/release/flick {{file}} {{cycles}}

# Closed-loop HTTP load on the SSR server of tools/load/ssr (release): path is render, stream or html
load-ssr path="render" port="3918":
    cargo build -p flick
    cd tools/load/ssr && ../../../target/debug/flick build
    PORT={{port}} node tools/load/ssr/dist/flick-main.js & pid=$!; sleep 2; \
      node tools/load/index.mjs --url "http://127.0.0.1:{{port}}/{{path}}/{id}" --expect 'request {id}' --pid $pid; \
      status=$?; kill $pid; exit $status

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

# Build the playground's compiler: the wasm module and its JS bindings
docs-wasm: wasm
    cargo install wasm-bindgen-cli --version 0.2.129 --locked --root target/tools
    target/tools/bin/wasm-bindgen --target web --out-dir docs/src/playground/wasm target/wasm32-unknown-unknown/release/flick_wasm.wasm

# Serve the documentation site; its playground is a Flick program
docs-dev: docs-wasm
    cargo build -p flick
    pnpm -C docs dev

# Build the documentation site into docs/dist
docs: docs-wasm
    cargo build -p flick
    pnpm -C docs build

# Everything CI runs
check: lint test

# Check that the Zed extension (and its language server launcher) compiles
zed:
    cd editors/zed && CARGO_TARGET_DIR=../../target/zed cargo build --release --target wasm32-wasip2

# Regenerate the tree-sitter grammar Zed uses and run its corpus tests
tree-sitter:
    cd editors/tree-sitter-flick && tree-sitter generate && tree-sitter test

# Move to another Effect release: new surface snapshot, report, pins; the coverage linter stays red until review
effect-bump version:
    pnpm -C tools/dts-importer build
    node tools/dts-importer/index.mjs bump {{version}}

# Print the Effect coverage counters by status and module
effect-coverage:
    @cargo test -q -p flick-core --test effect_surface the_manifest_classifies > /dev/null
    @cat target/tmp/effect-coverage.md
