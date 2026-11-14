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
JS and Go emitters remain in `flick-core`; the contract probe exercises
both with IR and plain data without `Compilation` or `Settings`.

The executable component host, registry, release components, browser
loader, atomic publication and tool execution adapters are separate
integration work. In particular, accepting a graph here does not execute
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

`flick.typescript@0.1.0` contains the shared projection's declarations,
export annotations, imports and imported module names. Its payload is CBOR
of the WIT `typescript-projection` record. JS consumes that plain data;
the checker and projection rules remain shared. Future changes to memory
or execution semantics need a coordinated IR version, independently of
this backend package version.

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
and publish atomically; this library deliberately performs no file writes.

The driver validates all of this before returning a `Generated` value. Its
cache key includes the complete ordered request, component digest, options,
environment, profile, runtime/tool pins and every side-input digest. Cache
eligibility requires two equal complete results. An unchecked generation has
no cache key; a detected difference is an error. Cache loading must revalidate
the result against the current host policy. Resource limits, cancellation and
fresh guest instances also need enforcement by the component host.

## Contract checks

Run `cargo test -p flick-backend`. The canonical WIT and native request/result
schemas have reviewable golden snapshots. Changes must review both the ABI
and schema differences, with an incompatible 0.x change requiring a new minor
version once this contract is published. Binary round trips retain zero and
non-UTF-8 bytes. Independent contract probes pin Unit's JS representation
(`undefined`), its TS annotation, source-map metadata and Go's `flickrt.Unit`.

These checks establish the boundary's shape and validation behavior. They do
not replace native/component parity, production backend-selection tests,
debug/release conformance, locked/offline checks or before/after measurements
on a real project, which remain required for R3/R4.
