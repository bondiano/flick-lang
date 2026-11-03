# Every flick run here uses the std of this repository, not the release a
# compiler fetches by its tag.
export FLICK_STD_PATH := justfile_directory() + "/packages/std"

# Show available recipes
[private]
default:
    @just --list

# Static checks CI runs
lint:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    just feature-check
    cargo clippy -p flick-wasm --target wasm32-unknown-unknown -- -D warnings
    cargo machete

# Format everything
fmt:
    cargo fmt --all

# Run the test suites; the reference model has its own recipe, `model`
test:
    cargo test --workspace --exclude flick-model
    just feature-test

# The reference model of λFlick: its unit tests, the cases of
# tests/run/model.txt checked and run against their `.stdout`, and the
# model properties on generated programs (`PROPTEST_CASES` sets how many)
model:
    cargo test -p flick-model -- --include-ignored

# Regenerate formal/Formal/Elab/PreludeSchemes.lean from the checker's Prelude
formal-schemes:
    FLICK_FORMAL_SCHEMES=1 cargo test -p flick-model --lib translate::lean

# Check each capability independently, without workspace feature unification
feature-check:
    cargo clippy -p flick-core --all-targets --no-default-features -- -D warnings
    for feature in build-artifacts source-maps typescript diagnostic-fixes native-project terminal-diagnostics builtin-docs server-stdlib ide terminal-diagnostics,diagnostic-fixes; do cargo clippy -p flick-core --all-targets --no-default-features --features "$feature" -- -D warnings || exit $?; done

# Exercise minimal and independently enabled compiler capabilities
feature-test:
    cargo test -p flick-core --no-default-features --features embedded-std --lib --test features
    for feature in build-artifacts source-maps typescript diagnostic-fixes native-project terminal-diagnostics builtin-docs server-stdlib ide terminal-diagnostics,diagnostic-fixes; do cargo test -p flick-core --no-default-features --test features --features "embedded-std,$feature" || exit $?; done

# The Go runtime of the go target: vet and its unit tests.
go-test:
    cd runtime-go && go vet ./... && go test ./...

e2e: docs-wasm
    cargo build -p flick
    pnpm install --frozen-lockfile
    pnpm -r typecheck
    pnpm -r lint
    pnpm -r test
    cargo test -p flick --test e2e --test e2e_numbers --test go_main --test test_command --test dts_command --test build_command --test ffi_command --test cli_run --test cli_devtools --test cli_template --test cli_fullstack --test first_slice --test examples --test unicode_path --test source_packages --test workspace -- --ignored
    cargo test -p flick-core --test consteval_node -- --ignored
    just go-test
    just language

# The language tests of tests/language on Node and as a Go executable, with
# one seed: both must pass and write the same report
language seed="42":
    cargo build -p flick
    cd tests/language && ../../target/debug/flick test --emit js --seed {{seed}} --reporter json > ../../target/language-js.txt || { cat ../../target/language-js.txt; exit 1; }
    cd tests/language && ../../target/debug/flick test --emit go --seed {{seed}} --reporter json > ../../target/language-go.txt || { cat ../../target/language-go.txt; exit 1; }
    diff -u target/language-js.txt target/language-go.txt
    cat target/language-go.txt

# Bundle the tracked programs (release) with rolldown and hold them to their gzip budgets
bundle-size:
    node tools/bundle-size/index.mjs "$(cargo build -p flick --message-format=json-render-diagnostics | jq -r 'select(.executable != null) | .executable')"

# Time the language server on a file: opens, hovers and a hover after an edit
lsp-bench file="examples/mtl/src/Mtl/StateT.fl" +positions="22:5":
    cargo build --release -p flick
    node tools/lsp-bench/index.mjs target/release/flick {{file}} {{positions}}

# Hold the language server to a long open/edit/close session and print its resident memory (Linux)
lsp-soak file="examples/mtl/src/Mtl/StateT.fl" cycles="400":
    cargo build --release -p flick
    node tools/lsp-bench/soak.mjs target/release/flick {{file}} {{cycles}}

# Build the SSR load server of tools/load/ssr (release) and bundle its client
load-ssr-build:
    cargo build -p flick
    cd tools/load/ssr && ../../../target/debug/flick build
    node tools/load/bundle.mjs tools/load/ssr/dist/flick-client.js tools/load/ssr/dist/client.js

# Closed-loop HTTP load on the SSR server, React in production mode: path is render, stream, collect, suspended or html
load-ssr path="render" soak="60" port="3918": load-ssr-build
    NODE_ENV=production PORT={{port}} CLIENT_JS=tools/load/ssr/dist/client.js node tools/load/ssr/dist/flick-server.js & pid=$!; sleep 2; \
      node tools/load/index.mjs --url "http://127.0.0.1:{{port}}/{{path}}/{id}" \
        --expect 'request {id}{{ if path == "suspended" { "|\"item of {id}\"|>item of {id}<" } else { "" } }}' \
        --soak {{soak}} --pid $pid; \
      status=$?; kill $pid; exit $status

# Hydrate streamed pages of the SSR server in Chromium, optionally while `load-ssr suspended` runs on the same port
load-ssr-browser pages="500" tabs="8" port="3919": load-ssr-build
    NODE_ENV=production PORT={{port}} CLIENT_JS=tools/load/ssr/dist/client.js node tools/load/ssr/dist/flick-server.js & pid=$!; sleep 2; \
      node tools/load/browser.mjs --url "http://127.0.0.1:{{port}}/suspended/{id}" --pages {{pages}} --tabs {{tabs}}; \
      status=$?; kill $pid; exit $status

# Build the HTTP + JSON load server of tools/load/json for the JS and the go target
load-json-build:
    cargo build -p flick
    cd tools/load/json && ../../../target/debug/flick build --out-dir dist/js && ../../../target/debug/flick build --emit go --out-dir dist/go

# Closed-loop HTTP load on the JSON server built for `target` (js or go), then its startup time and size;
# the server and the client run on separate CPUs (`taskset` lists), so the client does not take the server's
load-json target="go" soak="30" port="3920" workers="8" server_cpus="0-7" client_cpus="8-31": load-json-build
    if [ "{{target}}" = go ]; then server="taskset -c {{server_cpus}} tools/load/json/dist/go/flick-server"; else server="taskset -c {{server_cpus}} node tools/load/json/dist/js/flick-server.js"; fi; \
      PORT={{port}} $server & pid=$!; sleep 2; \
      taskset -c {{client_cpus}} node tools/load/index.mjs --url "http://127.0.0.1:{{port}}/item/{id}" --expect '"item {id}"' --soak {{soak}} --workers {{workers}} --pid $pid; \
      status=$?; kill $pid; \
      node tools/load/startup.mjs --url "http://127.0.0.1:{{port}}/item/x" -- $server; \
      if [ "{{target}}" = go ]; then du -b tools/load/json/dist/go/flick-server; else du -sb tools/load/json/dist/js; fi; \
      exit $status

# Time the programs of tools/bench, built in release, on Node and as Go executables: the median of `runs`
bench runs="5":
    cargo build -p flick
    node tools/bench/index.mjs target/debug/flick {{runs}}

# Run the runtime cases `runs` times each on both targets with random interleavings
# (FLICK_STRESS) and check every trace against λRT; prefixes pick other portable cases. Not in CI
stress runs="20" +prefixes="runtime/":
    cargo build -p flick
    cargo build -p flick-model --example rt_trace
    pnpm -C runtime build
    node tools/stress/index.mjs target/debug/flick target/debug/examples/rt_trace {{runs}} {{prefixes}}

# Run SQL queries against PostgreSQL in Docker (after `just e2e` installs the runtime)
e2e-postgres:
    cargo test -p flick --test sql_postgres -- --ignored

# The Conduit example's Go server against the official RealWorld API suite:
# needs Docker, Go and hurl (`nix shell nixpkgs#hurl -c just conduit-api`)
conduit-api:
    cargo test -p flick --test conduit -- --ignored

# Build and run Conduit, the client and the server, with PostgreSQL through
# Docker Compose; it answers at http://127.0.0.1:8000
conduit-run:
    CARGO_TARGET_DIR=target/own cargo build -p flick
    cd examples/conduit && ../../target/own/debug/flick build --package server
    cd examples/conduit/client && FLICK_BIN=../../../target/own/debug/flick pnpm build
    cd examples/conduit/server && docker compose up --build

# The Conduit client against the official RealWorld Playwright suite, served
# by the Go server over a fresh PostgreSQL: needs Docker, Go and Chromium
# (`CHROMIUM=$(which chromium) just conduit-e2e`, or `pnpm --dir
# examples/conduit/client exec playwright install chromium`)
conduit-e2e:
    CARGO_TARGET_DIR=target/own cargo build -p flick
    cd examples/conduit && PATH="$PWD/../../target/own/debug:$PATH" ../../target/own/debug/flick run e2e

# Mutation-fuzz the parser (the nightly CI job runs 1 000 000 inputs)
fuzz iterations="1000000":
    FLICK_FUZZ_ITERATIONS={{iterations}} cargo test --release -p flick-core --test fuzz

# Build the wasm module the in-browser demo loads and hold it to its size budget
wasm:
    cargo build -p flick-wasm --target wasm32-unknown-unknown --release
    test $(wc -c < target/wasm32-unknown-unknown/release/flick_wasm.wasm) -le 4194304

# Build the playground's compiler: the wasm module and its JS bindings
docs-wasm: wasm
    cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/tools
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
