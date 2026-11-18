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
