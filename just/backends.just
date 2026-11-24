# The official backend components, built reproducibly and pinned.

# No WASI adapter: an official backend imports only the compiler input interface.
backend-component name:
    node --test tools/backend-package/*.test.mjs
    node tools/backend-package/build.mjs {{name}}
    mkdir -p target/backends
    cargo run -p flick-component-build -- target/backend-guest/wasm32-unknown-unknown/release/flick_backend_{{name}}.wasm target/backends/{{name}}.wasm

js-component: (backend-component "js")

go-component: (backend-component "go")

lua-component: (backend-component "lua")

# The Lua profile probes on the pinned hosts `nix develop` puts on PATH (or
# LUA51, LUAC, LUAJIT, LOVE): Lua 5.1.5, LuaJIT 2.1 with the JIT on and
# off, and LÖVE 11.5 headless; then the API snapshots of those hosts and
# the generated WoW addons, which no host here runs.
lua-probes:
    node backends/lua/probes/run.mjs

# Update the component `name` distributed with every compiler binary;
# CI only checks that a rebuild reproduces it.
update-component name: (backend-component name)
    node tools/backend-package/index.mjs update {{name}}

check-component name: (backend-component name)
    node tools/backend-package/index.mjs check {{name}}

# Ignored backend tests also run in e2e-tests now that their host is enabled
# by default. Both runners need these images in a fresh checkout.
backend-fixtures: js-component go-component
    cargo build -p flick-test-backend --features guest --target wasm32-wasip2 --release

# A real third-party backend fixture through the separate component world.
wasm-backends: backend-fixtures
    pnpm install --frozen-lockfile
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    cargo clippy -p flick-test-backend --features guest --target wasm32-wasip2 -- -D warnings
    just check-component js
    just check-component go
    just check-component lua
    cargo clippy -p flick-backend --features host --all-targets -- -D warnings
    cargo test -p flick-backend --profile e2e --features host --test component_host -- --include-ignored
    cargo clippy -p flick --features wasm-backends --all-targets -- -D warnings
    cargo test -p flick --profile e2e --features wasm-backends --bin flick --test backends -- --include-ignored
    cargo test -p flick --release --features wasm-plugins --test default_backend -- --include-ignored
    node --test tools/backend-conformance/report.test.mjs
    node tools/backend-conformance/index.mjs {{flick}} target/backends/js.wasm

# tests/kernel on both JS runtimes and as a Go executable, in debug and release,
# then in release the tests/language modules tests/kernel/language.txt lists
# (`just language` runs them in debug).
# The primitive kernel's conformance suite on JS and Go, in debug and release
kernel: e2e-compiler
    #!/usr/bin/env bash
    set -euo pipefail
    pnpm -C runtime build
    pnpm -C packages/effect-ts build
    modules=$(grep -v '^#' tests/kernel/language.txt | sed 's|.*|test/Language/&.fl|')
    for profile in debug release; do
        for target in "js --runtime rt" "js --runtime effect" go; do
            (cd tests/kernel && {{flick}} test --target $target --profile $profile)
        done
    done
    for target in js go; do
        (cd tests/language && {{flick}} test --target $target --profile release $modules)
    done
