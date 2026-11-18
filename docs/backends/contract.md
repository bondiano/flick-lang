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
IR 1.0 or complete every semantic prerequisite of R3a. Ordinary JS/TS
`build/run/test` select the distributed JS component by default. The portable
IR is 0.12.0. The official
JS emitter lives in `backends/js`, independently of `flick-core`; the Go
emitter remains in `flick-core`. The contract probe exercises both with IR
and plain data without `Compilation` or `Settings`. The distributed component
is checked against its Rust/WIT sources; real release-binary tests require
component selection for build/run/test. The native trait remains a development
and parity interface.

The optional `flick-backend/host` feature executes this world using the
same Wasmtime configuration and compiled-code cache as the plugin host.
The literal third-party fixture exercises it as an actual WASM component.
A loaded component checks the complete request backend pin before preparing
inputs in both `check-target` and direct `generate` calls. A matching id and
version with another component digest is rejected. The driver checks the
host's loaded pin before snapshotting any input, even when that input is
unavailable; a mismatch remains a P013 diagnostic with no reader calls.
The CLI supports [explicit pinned package selection](registry.md) through
this world for build/run/test. Projects without a pin select the shipped JS
component for JS/TS, while Go retains its temporary native baseline. The browser
loader, atomic publication and generic tool execution adapters remain integration work.
Accepting a graph here does not execute it or establish that a tool's arguments
access only declared files.

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

The CLI also supplies `flick.javascript@0.1.0`, CBOR of
`javascript-projection`. It records builtin-name classification (including
own modules replacing builtin names), independently of role or output path,
exported value names, entry names with
the defining module and pure/effect classification, and every physical file's
output path and source-map paths indexed by `Program.files`. Entry validation
and P026 spans use the same frontend function as the native development
adapter. Source byte offsets and line starts remain in the source manifest;
the component calculates UTF-16 columns from declared source bytes. These
records add no checker or host-path lookup callback to the world.

`backends/js` now implements the world through its `guest` feature. Its actual
component is built without WASI by `just js-component`. Native/WASM parity
covers JS/TS and both profiles; explicit package selection exercises the
common CLI. The component calculates release ABI and invokes the same
portable optimizer with `Program.lang`. The browser component loader remains
separate integration work.

`just js-component` remaps checkout, registry and standard-library source
paths to canonical roots. The Rust root uses the pinned compiler's full
commit hash, so installing `rust-src` does not change embedded panic paths.
`just update-component js` refreshes the distributed bytes and SHA together;
`just check-component js` requires exact bytes and the matching identity pin.

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
The host executes approved steps; a guest receives no process API. A backend
may return a finished binary executable directly or ask a pinned host tool to
produce one. Merely building or publishing it does not execute it; launching
is an explicit host action.

### Host tools and their authority

The project grants tools, not the backend: `[tools]` in `flick.toml` lists,
for each backend id or plugin package, the tools its steps may run, as
`"flick/go" = ["go"]`. The official components get no exception. A step
whose tool is not granted is `FLICK-P051` before any step runs. A launch
runtime, Node for `flick run`, needs no grant; its pin is still checked.

The host knows each tool through an adapter (`flick/src/backends/tools.rs`):
`node` and `go`. It finds the first executable on `PATH`, hashes it and
refuses a digest that differs from the pin before the candidate ever runs,
then queries the version with the adapter's own command. Only granted tools
and Node appear in `BuildRequest.toolchains`. Before each step the host
hashes the executable again, so a binary replaced during the build fails
its step. The grant is the authorization of arguments: the host does not
parse a tool's command line. An adapter decides which variables a step
inherits (the home, cache, temporary and proxy variables, and for `go` its
module settings) and which it may set (`GOFLAGS`, `GOTOOLCHAIN`,
`CGO_ENABLED`); every other variable is cleared.

Each step runs in a fresh directory beside the output directory, at the
same depth, so relative paths resolve as they do from the output. The
directory holds only the step's declared inputs: artifacts and earlier
outputs at their paths, side inputs at `.flick-inputs/<id>`. Afterwards
the host reads back only the declared outputs, each a regular file within
the artifact limit; a symlink, a directory, a missing output or a failing
exit is `FLICK-P050` with the end of the tool's output. Undeclared files are
dropped with the directory. Step outputs join the build's artifacts, so the
build cache, keyed by the request, the component, tool and runtime pins and
every input, reuses them; a changed input, component or tool runs the
steps again.

Cancellation reaches every part: SIGINT or SIGTERM, once backend work has
begun, cancels the guest call through the engine epoch, the input snapshot,
and the running tool. Each tool runs in its own process group, which gets
SIGTERM and, two seconds later, SIGKILL, so what the tool started ends too.
When a tool exits, whatever it left running in its group is killed, so no
descendant writes into a step directory or holds the tool's output open.
Off Unix there are no process groups: cancellation ends the tool itself,
and a descendant it started may outlive it.

Diagnostic spans address the supplied UTF-8 source bytes, including boundary
checks. Each referenced source is loaded and UTF-8-validated once per
diagnostic validation pass, even when many diagnostics reference it.
Payload, total serialization, file, step, diagnostic and string limits
are host limits. A failure rejects the whole generation. The generation
driver performs no artifact writes; publication is the host's.

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
artifacts retain their bytes.

### Commands

Both the backend world and the plugin world export `flick:command/command`
(`flick-backend/wit/deps/command`, copied to `flick-plugin/wit/deps`; a test
keeps the two identical). A command declares its parameters, the paths it
writes and its style; it returns a plan of files and tool steps or answers
session requests. The host checks a plan against the declaration, runs its
steps with the executor above under the contributor's `[tools]` grant, and
publishes its files in one transaction under the project's root. Help
reads the descriptors cached by component digest, never a component. See
[Contributed commands](../src/content/docs/reference/commands.md).

### Atomic publication

A build publishes its files, the removal of stale ones and the project's
output manifest as one transaction (`flick/src/publish.rs`). The files are
staged in `<out>/.flick-publish/`, which no artifact may name; a journal
records the paths; earlier files move into the journal and staged ones into
place, each by a rename on the output's filesystem. A marker file is the
commit point. A failure before it moves every earlier file back; after it,
the manifest is written and the journal removed. A publication that a crash
interrupted is finished or undone by the next one into that directory, which
takes a lock in the cache so two processes never publish into one output at
once. Unchanged files are not rewritten. Host tools that make files from the
build itself, the native Go executable and schema AOT, run on a copy beside
the output, so their results join the same transaction.

The guarantee covers failures and crashes, not readers: a process reading
the output during the commit can see some files new and others old. It does
not cover files the build does not own: they are never moved.

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
