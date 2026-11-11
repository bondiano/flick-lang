# Effect adapter

## Dependency contract

The adapter's `package.json` owns its version and exact production dependency
pins. Flick scaffolds new Effect projects using the adapter manifest shipped
with the compiler. An installed adapter may have a different version: its own
manifest determines the dependencies checked before building, running, testing,
or after installing. `P015` reports an unreadable contract or an unsatisfied pin.
Dependency resolution follows the real adapter directory, including pnpm links.
The local `workspace:*` runtime dependency uses the workspace runtime manifest;
pnpm replaces it with that version when publishing.

The compiler still checks its runtime ABI version. RT-only builds do not need
or inspect the Effect adapter. Compile-only builds can emit code before npm
packages are installed; an adapter directory with a missing or malformed
manifest is an error. Adapter manifest contents and resolved dependency
manifests participate in Effect build cache keys.
