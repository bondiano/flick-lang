# Reproducing the JavaScript guest

`just check-component js` rebuilds the guest and requires exact equality with
its distributed component and digest. `just update-component js` is the explicit
artifact update; the check never updates or accepts a different binary.

The pinned compiler and source-path remaps are necessary but insufficient for
cross-host equality. Cargo includes the host compiler identity in build-script
and proc-macro metadata, then recursively includes dependency metadata in its
`-Cmetadata` value for target crates. Those identities affect symbol names and
code/data ordering even when the final component strips symbols.

`build.mjs` first builds the native `flick-guest-rustc` wrapper with inherited
compiler wrappers, Rust flags and default target cleared. The wrapper replaces
only existing metadata arguments for `wasm32-unknown-unknown` compilations.
Host compilations, compiler queries, Cargo filenames and dependency filenames
pass through unchanged. The identity contains the canonical package source,
package name/version, crate name/types, sorted cfg/features, guest target and
pinned compiler commit. Its length-prefixed fields are hex encoded without a
separate hash; rustc hashes this identity itself.

The `flick_guest_metadata_v1` cfg invalidates existing guest artifacts: Cargo
does not rebuild them merely because `RUSTC_WRAPPER` changed. Bump this cfg
together with the wrapper identity schema when changing that algorithm.

Workspace package sources are relative to the checkout. Registry and Git
package sources retain their source-specific cache directories relative to
Cargo home. Sources outside those roots are rejected. Absolute checkout,
Cargo-home and installed rust-src paths cannot enter the guest identity; an
identity remap for `/rustc/<commit>` also prevents rustc from resolving standard
library source locations to an optional local rust-src installation.

Run `node --test tools/backend-package/*.test.mjs` and
`cargo test -p flick-component-build --bin flick-guest-rustc` for focused
regressions. Compare full `just js-component` outputs on independent hosts
before refreshing the distributed artifact.
