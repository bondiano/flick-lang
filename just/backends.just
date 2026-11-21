# The official backend components, built reproducibly and pinned.

# No WASI adapter: an official backend imports only the compiler input interface.
backend-component name:
    node --test tools/backend-package/*.test.mjs
    node tools/backend-package/build.mjs {{name}}
    mkdir -p target/backends
    cargo run -p flick-component-build -- target/backend-guest/wasm32-unknown-unknown/release/flick_backend_{{name}}.wasm target/backends/{{name}}.wasm

js-component: (backend-component "js")

go-component: (backend-component "go")

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
    cargo clippy -p flick-backend --features host --all-targets -- -D warnings
    cargo test -p flick-backend --profile e2e --features host --test component_host -- --include-ignored
    cargo clippy -p flick --features wasm-backends --all-targets -- -D warnings
    cargo test -p flick --profile e2e --features wasm-backends --bin flick --test backends -- --include-ignored
    cargo test -p flick --release --features wasm-plugins --test default_backend -- --include-ignored
    node --test tools/backend-conformance/report.test.mjs
    node tools/backend-conformance/index.mjs {{flick}} target/backends/js.wasm
