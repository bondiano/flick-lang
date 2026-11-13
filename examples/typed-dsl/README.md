# Typed validation DSL

The [English guide](../../docs/src/content/docs/handbook/typed-dsl.md) and
[Russian guide](../../docs/src/content/docs/ru/handbook/typed-dsl.md) explain
this example. Build and verify from the repository root:

```sh
pnpm install --frozen-lockfile
pnpm -C runtime build
pnpm -C packages/effect-ts build
cargo build -p flick-example-typed-dsl --features guest --target wasm32-wasip2 --release
cargo build -p flick --features wasm-plugins
node examples/typed-dsl/check.mjs
```

No database, network credentials or LLM are needed. The SDK uses the compiler's
pinned Rust toolchain and the `wasm32-wasip2` target. Use the Node and pnpm
versions pinned in `.tool-versions` and `package.json`. `just wasm-plugins`
builds these prerequisites and runs this verification in CI.
