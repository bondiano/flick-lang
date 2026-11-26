# Every flick run here uses the std of this repository, not the release a
# compiler fetches by its tag.
export FLICK_STD_PATH := justfile_directory() + "/packages/std"

# Share compiled Wasmtime components across tests that isolate XDG_CACHE_HOME,
# so each fresh cache directory does not recompile component.wasm from scratch.
export FLICK_WASMTIME_CACHE_DIR := justfile_directory() + "/target/tmp/wasmtime-cache"

# The compiler the e2e suites run, built in the `e2e` profile
flick := justfile_directory() + "/target/e2e/flick"

# The official backend components
import 'just/backends.just'

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
    node tools/style/no-iife.mjs

# Format everything
fmt:
    cargo fmt --all

# Rewrite the ```flick blocks of the site's pages with the safe fixes of
# `flick check --fix`, as a syntax change rewrites the `.fl` files with it
docs-fix:
    cargo build -p flick
    find docs/src/content/docs \( -name '*.md' -o -name '*.mdx' \) -print0 | xargs -0 target/debug/flick check --fix

# Run the test suites; the reference model has its own recipe, `model`
test: unit feature-test

# The tests of the workspace crates but the model, or of `packages`
unit *packages="--workspace --exclude flick-model":
    cargo test {{packages}}

# The non-`flick`, non-model workspace crates as CI runs them
unit-crates:
    rm -f target/tmp/effect-coverage.md
    just unit --workspace --exclude flick-model --exclude flick

# `flick`'s tests in the `e2e` profile (optimized, with debug assertions and
# overflow checks): all, or the part `k/n` under nextest (`--bin flick` runs
# once in part 1 so its unit tests share the built-ins `OnceLock`)
unit-flick part="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{part}}" ]; then
        cargo test --profile e2e -p flick
    else
        if [[ "{{part}}" == 1/* ]]; then
            cargo test --profile e2e -p flick --bin flick
        fi
        cargo nextest run --cargo-profile e2e --profile ci -p flick -E 'not binary_id(flick::bin/flick)' --partition "count:{{part}}"
    fi

# The reference model of λFlick: its unit tests, the cases of
# tests/run/model.txt checked and run against their `.stdout`, the other
# cases against the reasons of tests/run/model_gaps*.txt, and the
# model properties on generated programs (`PROPTEST_CASES` sets how many)
model:
    cargo test -p flick-vm -- --include-ignored
    cargo test -p flick-model -- --include-ignored

# Regenerate formal/Formal/Elab/PreludeSchemes.lean and formal/Formal/Live/ElideSchemes.lean from the checker's Prelude
formal-schemes:
    FLICK_FORMAL_SCHEMES=1 cargo test -p flick-model --lib translate::lean

# Regenerate formal/FormalCert/Gen/Proofs.lean, the Lean replay of core's Option proofs, from the compiler's kernel
formal-proofs:
    FLICK_STD_PATH="$PWD/packages/std" FLICK_FORMAL_PROOFS=1 cargo test -p flick-core --lib ir::lower::proofs::replay

# Regenerate formal/FormalCert/GenSig.lean and the golden certificates from the model
formal-certs:
    FLICK_FORMAL_CERTS=1 cargo test -p flick-model --test certs the_certificates_are_fresh

# Certify a run of generated programs and check the certificates with Lean
model-certs: lean
    cargo test -p flick-model --test certs -- --ignored
    cd formal && ls ../target/certs/Batch/*.lean | xargs -n 1 -P 4 lake env lean

# Build the Lean 4 mechanization of λFlick in formal/ as CI does; --wfail fails on a `sorry`
lean:
    cd formal && lake build --wfail

# Check each capability independently, without workspace feature unification
feature-check:
    cargo clippy -p flick-core --all-targets --no-default-features -- -D warnings
    for feature in native-js native-project ide contracts; do cargo clippy -p flick-core --all-targets --no-default-features --features "$feature" -- -D warnings || exit $?; done
    cargo clippy -p flick-ffi --all-targets --no-default-features -- -D warnings
    cargo clippy -p flick --no-default-features -- -D warnings

# Exercise minimal and independently enabled compiler capabilities
feature-test:
    cargo test -p flick-core --no-default-features --features embedded-std --lib --test features
    for feature in native-js native-project ide contracts; do cargo test -p flick-core --no-default-features --test features --features "embedded-std,$feature" || exit $?; done
    cargo test -p flick-ffi --no-default-features

# The Go runtime of the go target: vet and its unit tests.
go-test:
    cd runtime-go && go vet ./... && go test ./...

# Regenerate the Go runtime's tables and test data from Node and Effect
# (tools/go-*); `just go-oracles check` also fails when one differs from git
go-oracles mode="":
    #!/usr/bin/env bash
    set -euo pipefail
    # the tables name their Node: only the pinned one keeps them
    grep -qxF "nodejs $(node --version | cut -c2-)" .tool-versions \
        || { echo "tools/go-* need the Node of .tool-versions" >&2; exit 1; }
    for s in go-arbitrary/gen go-cli/complete go-cli/parity go-collate/gen go-dates/gen go-encoding/gen \
        go-glob/gen go-intl/gen go-mime/gen go-path/gen go-rpc/envelopes go-unicode/gen go-unicode/graphemes; do
        node "tools/$s.mjs"
    done
    if [ "{{mode}}" = check ] && [ -n "$(git status --porcelain -- runtime-go/flickrt)" ]; then
        git status --short -- runtime-go/flickrt
        echo "tools/go-* generate other files than runtime-go/flickrt has: commit what they wrote" >&2
        exit 1
    fi

# The e2e set: the JS packages, the e2e suites, the cases of tests/run, the
# Go runtime and the language tests; CI runs the parts as jobs side by side
e2e: e2e-js e2e-tests e2e-cases go-test language

# Go target without Node (CI go jobs)
e2e-go:
    FLICK_E2E_GO_ONLY=1 cargo test --profile e2e -p flick --test e2e --test go_main --test unicode_path -- --ignored run_cases exit_codes_follow_the_outcome_and_signals run_and_test_use_the_go_executable a_project_builds_runs_and_tests_under_a_non_ascii_path
    cargo test --profile e2e -p flick --test go_direct_ffi -- --ignored

# The compiler in the `e2e` profile, built as a dependency of its own tests:
# their dev-dependencies turn on features of the compiler's crates, so a plain
# `cargo build -p flick` is another build, which the suites would then redo
e2e-compiler:
    cargo build -p flick --profile e2e --test e2e

# What the e2e suites run against: the compiler in the `e2e` profile, the JS
# packages, the runtime and the `.d.ts` importer
e2e-prepare: e2e-compiler
    pnpm install --frozen-lockfile
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    FLICK_BIN={{flick}} pnpm -C tools/dts-importer build

# The JS packages, with the playground's compiler: typecheck, lint, tests, the
# site's translations, and the built site, its links and in a browser
# (`CHROMIUM=$(which chromium)`, or `pnpm -C docs exec playwright install chromium` once)
e2e-js: docs-wasm e2e-prepare
    node docs/scripts/parity.mjs
    FLICK_BIN={{flick}} pnpm -r typecheck
    FLICK_BIN={{flick}} pnpm -r lint
    FLICK_BIN={{flick}} pnpm -r --workspace-concurrency=1 test
    FLICK_BIN={{flick}} pnpm -C docs build
    node docs/scripts/links.mjs
    node docs/scripts/chunks.mjs
    pnpm -C docs e2e

# The e2e suites of the compiler but the cases of tests/run, on the Effect
# runtime and on Flick's own, under nextest.
# Every ignored test of `flick` and `flick-core` runs here: the selection is
# "all their test targets, minus the exceptions", so a new suite is covered
# without touching this recipe. Each exception belongs to the recipe named
# beside it, and `flick/tests/ignored_tests.rs` holds this recipe to that.
e2e-tests: e2e-prepare backend-fixtures
    cargo nextest run --cargo-profile e2e --profile ci --run-ignored only -p flick -p flick-core \
      -E 'not (test(=run_cases) | test(~rt_cases) | binary_id(flick::sql_postgres) | binary_id(flick::conduit) | test(~dump) | test(~write_fixes_of_built_in_signatures))'

# The cases of tests/run, and the same cases on Flick's own runtime: all of
# them, or the shard `k/n` of them
e2e-cases shard="": e2e-prepare
    FLICK_E2E_SHARD={{shard}} cargo nextest run --cargo-profile e2e --profile ci --run-ignored only -p flick --test e2e -E 'test(=run_cases) | test(=rt::rt_cases)'

# The language tests of tests/language on Node and as a Go executable, with
# one seed: both must pass and write the same report; on Flick's own runtime
# too, but for the tests tests/language/rt-gaps.txt lists
language seed="42": e2e-compiler
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    cd tests/language && {{flick}} test --target js --seed {{seed}} --reporter json > ../../target/language-js.txt || { cat ../../target/language-js.txt; exit 1; }
    cd tests/language && {{flick}} test --target go --seed {{seed}} --reporter json > ../../target/language-go.txt || { cat ../../target/language-go.txt; exit 1; }
    diff -u target/language-js.txt target/language-go.txt
    cat target/language-go.txt
    node tools/language-rt/index.mjs {{flick}} {{seed}} target/language-js.txt

# A relative path: on Windows the shell would drop the backslashes of {{flick}}.
# The language tests as a Go executable alone, without Node, after `e2e-go` built the compiler
language-go seed="42":
    cd tests/language && ../../target/e2e/flick test --target go --seed {{seed}}

# The language tests of tests/language on the in-process VM compared against JS:
# every language test that passes on js must pass on vm with zero gaps or skips
language-vm seed="42": e2e-compiler
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    cd tests/language && {{flick}} test --target js --seed {{seed}} --reporter json > ../../target/language-js.txt || { cat ../../target/language-js.txt; exit 1; }
    cd tests/language && {{flick}} test --target vm --seed {{seed}} --reporter json > ../../target/language-vm.txt || { cat ../../target/language-vm.txt; exit 1; }
    diff -u target/language-js.txt target/language-vm.txt
    cat target/language-vm.txt

# The tests/run/model.txt corpus executed in process via `flick run --target vm`
# in debug and release profiles with empty PATH (no node or go spawned)
vm-cases: e2e-compiler
    node tools/vm-cases/index.mjs {{flick}}

# Bundle the tracked programs (release) with rolldown and hold them to their gzip budgets
bundle-size: e2e-compiler
    node tools/bundle-size/index.mjs "{{flick}}"

# Count the tokens of the Conduit example's source against the RealWorld Go backend and React client:
# o200k_base, and Claude's tokenizer when ANTHROPIC_API_KEY or OPENROUTER_API_KEY is set
tokens:
    node tools/tokens/index.mjs

# Time the language server on a file: opens, hovers and a hover after an edit
lsp-bench file="examples/mtl/src/Mtl/StateT.fl" +positions="22:5":
    cargo build --release -p flick
    node tools/lsp-bench/index.mjs target/release/flick {{file}} {{positions}}

# Measure proof editing over stdio and soak repeated proof-project edits
lsp-proof-bench samples="30" cycles="100":
    cargo build --release -p flick
    node tools/lsp-bench/proofs.mjs --samples {{samples}} --cycles {{cycles}} target/release/flick

# The performance baseline: corpus check time, hover latency, load-server throughput and Go binary size,
# written to target/perf.json and held to tools/perf/budgets.json
perf:
    cargo build --release -p flick
    node tools/perf/index.mjs target/release/flick

# Budgets go down: those of tools/perf and tools/bundle-size against a base ref; a looser one needs a `reason` beside it
ratchet base="origin/main":
    node tools/perf/ratchet.mjs {{base}}

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
    cd tools/load/json && ../../../target/debug/flick build --out-dir dist/js && ../../../target/debug/flick build --target go --out-dir dist/go

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

# The runtime contract, once on each target and on the model: the cases of
# docs/formal/runtime.md §6.2
conformance: e2e-prepare
    FLICK_E2E_ONLY=tests/run/runtime/,effect/shared_layer_merged cargo test --profile e2e -p flick --test e2e -- --ignored run_cases
    FLICK_MODEL_ONLY=runtime/ cargo test --profile e2e -p flick-model --test run -- --include-ignored model_prints

# The cases of tests/run/rt.txt on Flick's own runtime, in both profiles
conformance-rt: e2e-prepare
    cargo test --profile e2e -p flick --test e2e -- --ignored rt_cases

# Run SQL queries against PostgreSQL in Docker
e2e-postgres: e2e-prepare
    cargo test --profile e2e -p flick --test sql_postgres -- --ignored

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

# The test plugin built as a WebAssembly component, and the plugin tests of
# the compiler with wasmtime, which release builds include, with the unit
# tests that only a build with wasmtime has; the tests run in the `e2e`
# profile, as an unoptimized wasmtime compiles a component slowly
wasm-plugins: wasm-backends
    cargo build -p flick-test-pairs -p flick-example-typed-dsl --features guest --target wasm32-wasip2 --release
    cargo clippy -p flick-test-pairs -p flick-example-typed-dsl --features guest --target wasm32-wasip2 -- -D warnings
    cargo clippy -p flick --features wasm-plugins --all-targets -- -D warnings
    cargo test -p flick --profile e2e --features wasm-plugins --bin flick --test plugins
    pnpm install --frozen-lockfile
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    FLICK_BIN="$PWD/target/e2e/flick" node examples/typed-dsl/check.mjs

# Build the wasm module the in-browser demo loads and hold it to its size budget.
# The raw-byte budget retains the playground editor queries (hover, completion,
# fixes and `.d.ts`). Size optimization uses LLVM Oz, whole-program LTO and
# removal of debug information;
# native release builds retain thin LTO. Binaryen deduplicates across Rust
# crates and optimizes instruction size, preserving imports/exports and
# wasm-bindgen metadata.
# The optimized compiler keeps the editor query surface within the raw-byte
# budget below; runtime bundles have separate gzip budgets.
wasm:
    node --test tools/wasm-dedup/index.test.mjs
    CARGO_PROFILE_RELEASE_OPT_LEVEL=z CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_STRIP=debuginfo cargo build -p flick-wasm --target wasm32-unknown-unknown --release
    node tools/wasm-dedup/index.mjs target/wasm32-unknown-unknown/release/flick_wasm.wasm
    @wasm=target/wasm32-unknown-unknown/release/flick_wasm.wasm; \
    echo "flick_wasm.wasm: $(wc -c < $wasm) bytes, $(gzip -6c $wasm | wc -c) gzipped"; \
    test $(wc -c < $wasm) -le 4849000 # reason: field accessors (#788): accessor parsing, typing, diagnostics and IDE queries in the playground (4.766 MB with them); ordered guarded matches over known shapes in the release optimizer the playground runs (4.806 MB with them, 4.798 MB before); optional field steps (#789): path typing and lowering, the nested-Option codec rule, completion, hover and rewrites (+16 KB: 4.824 MB with them, 4.808 MB before); reused modules sharing their IR with the editor's module cache (#966, +4 KB); the std module PriorityQueue (#736), whose source the playground embeds (about 4.2 KB: 4,831,578 bytes with it); the std modules OrderedMap and OrderedSet (#735), whose sources the playground embeds (about 9.7 KB: 4,841,667 bytes with them); the `@native` clauses of the built-in modules selected for each build (#1095) and the Flick bodies of the list and string hooks the Lua target runs (#831), whose sources the playground embeds (about 3.6 KB: 4,848,036 bytes with them)

# Build the playground's compiler: the wasm module, its JS bindings, and the
# pinned official JS backend component assets loaded lazily by backend.worker.ts
docs-wasm: wasm
    cargo install wasm-bindgen-cli --version 0.2.130 --locked --root target/tools
    target/tools/bin/wasm-bindgen --target web --out-dir docs/src/playground/wasm target/wasm32-unknown-unknown/release/flick_wasm.wasm
    cp backends/js/package/component.wasm backends/js/package/backend.json docs/src/playground/wasm/

# Serve the documentation site; its playground is a Flick program
docs-dev: docs-wasm
    cargo build -p flick
    pnpm -C docs dev

# Build the documentation site into docs/dist
docs: docs-wasm
    cargo build -p flick
    pnpm -C docs build

# CI's lint, test and features jobs; the e2e, model and Lean jobs have recipes of their own
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

# Print the Effect coverage counters by status and module (`from-run` prints
# the report written by `unit-crates` without recompiling)
effect-coverage mode="":
    @if [ "{{mode}}" != "from-run" ]; then cargo test -q -p flick-effect-surface --test coverage the_manifest_classifies > /dev/null; elif [ ! -s target/tmp/effect-coverage.md ]; then echo "target/tmp/effect-coverage.md missing or empty" >&2; exit 1; fi
    @cat target/tmp/effect-coverage.md
