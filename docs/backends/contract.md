# Backend component contract (0.1.0)

The public backend ABI is the separate
[`flick:backend` WIT world](../../flick-backend/wit/backend.wit). It exports
`describe`, `check-target` and `generate`. Its only import is `inputs.read`,
which accepts an opaque id from the request's content-addressed manifest.
`flick:plugin` hooks have a different purpose and ABI.

The native `flick_backend::Backend` trait uses records generated from this
WIT. It supports development and parity checks, rather than a separately
supported Rust plugin ABI. A backend component can be implemented in any
language that implements this world.

## Status

This is an initial 0.x contract and validation library. It does not freeze
IR 1.0, complete the semantic prerequisites of R3a, or switch production
`build/run/test` to components. The portable IR remains 0.11.0. The official
JS emitter lives in `backends/js`, independently of `flick-core`; the Go
emitter remains in `flick-core`. The contract probe exercises both with IR
and plain data without `Compilation` or `Settings`. Production JS component
selection is still separate work: moving the Rust emitter alone does not
establish the release component path.

The optional `flick-backend/host` feature executes this world using the
same Wasmtime configuration and compiled-code cache as the plugin host.
The literal third-party fixture exercises it as an actual WASM component.
A loaded component checks the complete request backend pin before preparing
inputs in both `check-target` and direct `generate` calls. A matching id and
version with another component digest is rejected. The driver checks the
host's loaded pin before snapshotting any input, even when that input is
unavailable; a mismatch remains a P013 diagnostic with no reader calls.
The CLI supports [explicit pinned package selection](registry.md) through
this world for build/run/test. Projects without a pin still use the temporary
native baseline. Official release components, the browser loader, atomic
publication and tool execution adapters remain separate integration work. In particular, accepting a graph here does not execute
it or establish that a tool's arguments access only declared files.

## Input boundary

A `BuildRequest` carries these independent facts:

| Data | Owner and representation |
| --- | --- |
| API, IR and language versions | Host; explicit handshake ranges in the descriptor |
| Backend selection | Project pin: id, version and SHA-256 of component bytes |
| Environment | Backend plus platform, such as `js/browser`, `js/node` or `go/native` |
| Runtime ABI and toolchain pins | Host/project; independent of backend API and IR |
| Portable program | Versioned CBOR `flick_ir::Program`, including declared lang items |
| Compilation mode | Whole program, or explicit module root with the surrounding graph and external interfaces |
| Module layout and package ownership | Plain module metadata; external interfaces are declared side inputs |
| Sources | Same order as `Program.files`; UTF-8 content input and byte line index |
| Entry/test roots and runners | Explicit symbols and entry names |
| Foreign bindings | Resolved specifier, ABI and declared content inputs |
| Checker projections | Module, named schema/version and content input |
| Package targets and root handlers | Environments rather than backend names alone |
| Remaining dependencies | Content-addressed resources and lockfiles in the input manifest |

The driver requires every declared input to be available and verifies its
size and digest before preflight, including inputs the backend never reads.
It bounds the complete serialized request before loading those inputs,
including the inline program, options and zero-byte input declarations.
Missing content rejects the request: a fallback result must not become
cacheable under a key that would also describe the content once available.
Runtime dependency pins require a package and a valid SHA-256 in both the
request and result; request membership alone does not validate a pin.
The complete module layout is checked for portable paths and collisions in
both compilation modes. Module package ownership must match the IR, and
entry, test and root-handler module references stay inside the closed graph.
Every item and nested expression span must fit the supplied UTF-8 source.
Source aliases share one retained input buffer and line index during validation.

An IR expression's optional type field is not a complete typing/effect
analysis. UI metadata and additional optimization facts are not implied
by receiving a `Program`. A consumer that needs a projection must require
its declared schema and refuse missing or incompatible data. It cannot
call the checker through this world.

The CLI supplies `flick.typescript@0.2.0` as CBOR of the WIT
`typescript-module-projection` record. Each logical module contains every
physical file produced by `Typing::prepare`, including lexical prefixes.
Each file identifies its physical module and output path, with declarations,
export annotations, imports and imported module names. Annotation entries
are sorted by key. The earlier `0.1.0` contract probe used a single
`typescript-projection`; consumers must check the schema version before
decoding. These plain data records do not change the world's function
signatures. The checker and projection rules remain shared.

The host prepares `.d.ts` and Promise facades with the same projection
implementation as the native baseline and reserves their output paths.
JavaScript output alone requires no TypeScript source projection. Its foreign
metadata still includes type-only imports needed by shared declarations.
`js.runtime` selects `effect` or `rt` from the effective project/CLI settings;
`js.effect-bridge` records `absent`, `available`, `available-with-ui`,
`available-with-web` or `available-with-ui-and-web`.

`flick.javascript-ffi@0.1.0` foreign bindings carry original and host-resolved
specifiers. Library files use the existing shipping checks, are retained as
compiler-owned outputs and have content-addressed `foreign` inputs. Each
library binding conservatively references the shipped input set of all
libraries in the compilation, including files from unrelated libraries. This
compilation-wide granularity is an over-approximation, not a per-binding
dependency proof; the explicit foreign binding records still resolve each
module and specifier separately.
Ordinary package imports remain imports; this does not yet provide complete
npm dependency pinning or Go foreign inputs.

Future changes to memory or execution semantics need a coordinated IR
version, independently of this backend package version.

`check-target` receives the complete request, so unsupported operations can
be refused with source spans before generation. Static handshake precedes
that call. Descriptor capabilities and the preflight response must agree.
Structural IR validation does not establish semantic equivalence; official
backends also require the language conformance corpus.

## Output boundary

A `BuildResult` contains binary artifacts, diagnostics, runtime dependencies,
launch descriptions and a DAG of toolchain steps. Artifact ids, input ids,
step ids and step-output ids have one namespace. Paths are relative portable
paths; aliases, traversal, duplicate paths and file/directory collisions are
rejected, including collisions with outputs reserved by the shared compiler.
`.iface` and shared declarations stay under compiler ownership.
Unicode filenames remain supported. Collision checks conservatively combine
canonical normalization, Unicode case folding and uppercase aliases, and
Windows device names include the reserved superscript-digit forms.
Collision keys also account for HFS+ ignorable formatting characters,
including aliases of compiler-owned files and directory/file prefixes.
Console aliases are also reserved. Path components fit 255 UTF-8 bytes and
255 decomposed UTF-16 units so they can be staged on the supported filesystems.
DOS-short-name-shaped components are refused so automatic NTFS aliases
cannot redirect an artifact to another output or a compiler-owned file.
Launch identity is `(kind, name)`: a run and a test may share a name;
duplicates within one kind are rejected.

Step outputs must name their producer as a direct dependency when consumed.
The validator sorts the graph; input order need not be topological. Tool
versions and digests must match both the request and actual project policy.
The host must additionally authorize each complete command through a tool
adapter, covering argument grammar, declared file access and network policy.
That authorization is a side-effect-free check. The host executes approved
steps; a guest receives no process API. A backend may return a finished binary
executable directly or ask a pinned host tool to produce one. Merely building
or publishing it does not execute it; launching is an explicit host action.
Tool adapters materialize only declared content and use explicit, fixed
environment values. Every input that can affect their output belongs in the
request and its cache key.

Diagnostic spans address the supplied UTF-8 source bytes, including boundary
checks. Each referenced source is loaded and UTF-8-validated once per
diagnostic validation pass, even when many diagnostics reference it.
Payload, total serialization, file, step, diagnostic and string limits
are host limits. A failure rejects the whole generation. A publication layer
must stage all files and host steps, honor cancellation, validate final outputs
and publish atomically; the generation driver performs no artifact writes.

The driver validates all of this before returning a `Generated` value. Its
cache key includes the complete ordered request, component digest, options,
environment, profile, runtime/tool pins and every side-input digest. Cache
eligibility requires two equal complete results. An unchecked generation has
no cache key; a detected difference is an error. Cache loading must revalidate
the result against the current host policy. This is eligibility for caching
generation results; final tool outputs need their own reproducibility check.

The native CLI's file boundary and internal CBOR cache retain byte payloads
and artifact kinds, including executable permissions on Unix. The complete
output set is checked for portable paths, collisions and size limits before
writing or restoring files, including IR dumps, test runners, Go executables
and schema AOT results. Cache reuse checks destinations before reading them.
The host-selected project/directory prefix is resolved consistently for
publication and manifest ownership. Output roots and their ancestor traversal
below that trusted prefix must be ordinary directories;
explicit external output directories remain supported using canonical paths.
Existing output symlinks cannot redirect writes; Unix destinations with multiple
hard links are refused before writes or executable-mode changes. Stale-file lists
and manifest cleanup cannot delete through a symlink parent. Relative recorded
directories cannot contain parent traversal; the host records external directories
as normalized absolute paths. Current and stale outputs cannot be portable aliases. Cleanup validates every recorded
path before deleting any file. IR dumps are recorded for clean and stay outside
the ordinary build cache. Text emit hooks see
source/declaration/metadata text while opaque object, executable and resource
artifacts retain their bytes. Tool execution and directory publication still
need the transactional integration described above.

## Native component host

`host::WasmBackend` verifies the component SHA-256 before compilation and its
descriptor's id and version before accepting requests. Incompatible backend
imports are rejected from the compiled component's actual import types.
Compiled code is retained by digest and engine, with Wasmtime's disk cache
available for later processes. Every operation creates a fresh store and
instance; mutable guest state is never cached between operations.

The host grants only declared, digest-checked `inputs.read` data. Other
imports, including WASI filesystem, clock, random and process operations,
trap when called. The complete serialized request is bounded before input
snapshots or conversion, including metadata on direct calls. Input transfer
and call-count limits trap even if a guest
tries to ignore a read error. Instruction fuel, aggregate linear-memory and
table storage, and the budget for lifting output into host-owned values are
configured by the host. Output accounting runs during canonical ABI lifting,
so aliased guest buffers cannot first allocate an unbounded Rust reply.
Wasmtime engine/JIT and other embedder allocations are separate from the
store's memory budget and still require process-level measurement.

Cancellation uses engine epochs with a separate token for each build.
Canceling one build stops its active guest execution and refuses later
calls, while unrelated stores sharing the engine continue. Preparation checks
the token before reading inputs, between reads and before conversion; cancellation
takes priority over preparation errors. The shared driver's input snapshot
obeys the same token, including when a reader cancels and reports unavailable
data. These checks use host metadata and do not invoke another guest operation.
Host tool steps
and publication must also honor that token when integrated. Transport
failures carry the backend id, version and digest in a `P013` diagnostic;
guest diagnostics are subject to the same source-span validation as native
results.

## Contract checks

Run `cargo test -p flick-backend` and `just wasm-backends`. The latter builds
the literal component and runs the native/component tests, including resource
limits, cancellation, pin and WIT mismatches, unsupported IR, denied imports
and invalid outputs. `just wasm-plugins` includes that recipe and verifies
existing plugins against the shared host configuration.

The canonical WIT and native request/result
schemas have reviewable golden snapshots. Changes must review both the ABI
and schema differences, with an incompatible 0.x change requiring a new minor
version once this contract is published. Binary round trips retain zero and
non-UTF-8 bytes. Independent contract probes pin Unit's JS representation
(`undefined`), its TS annotation, source-map metadata and Go's `flickrt.Unit`.

These checks establish the boundary's shape and validation behavior. They do
not replace official-backend native/component parity, production selection tests,
debug/release conformance, locked/offline checks or before/after measurements
on a real project, which remain required for R3/R4.
