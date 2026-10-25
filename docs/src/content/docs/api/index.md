---
title: API & reference
description: Find Flick language rules, built-in libraries, runtime contracts and compiler tools.
---

Use this index when you need an exact rule, a signature or a tool's contract. The reference pages stay at their existing `/reference/` URLs; this page groups them by the question you are trying to answer.

Flick is an ML-family language that compiles to JavaScript on Effect. Its language types, built-in modules and runtime adapter are related, but they are not interchangeable API layers. In particular, a Flick runtime arrow describes services and expected errors; it is not a JavaScript `Promise` signature.

To experiment before looking up a rule, open [Play](/play/) or compare source and generated JavaScript in [Transpile](/transpile/). For a local application, start with [Getting started](../start/getting-started/).

## Language

These pages describe what the compiler accepts and how it understands a program. Start with **Kinds and types** for signatures, then **Effects, errors and services** for the runtime arrows that differ from ordinary TypeScript function types.

| Reference | Look here for |
| --- | --- |
| [Grammar](../reference/grammar/) | Lexical rules, indentation, declarations, expressions, patterns and operator precedence. |
| [Kinds and types](../reference/types/) | Built-in types, higher-kinded parameters, function signatures, records, pattern checking and polymorphism. |
| [Modules and names](../reference/modules/) | File-to-module mapping, imports, exports, visibility and name resolution. |
| [Classes and instances](../reference/classes/) | Constraints, instance resolution, deriving and dictionary passing. |
| [Effects, errors and services](../reference/effects/) | Pure versus runtime stages, service and error rows, `task`, `await`, error handling and layers. |
| [Regions and captures](../reference/regions/) | Scope ownership, resource lifetimes, captured capabilities and child fibers. |
| [Foreign functions](../reference/ffi/) | JavaScript bindings, synchronous/async/Effect modes, callbacks, cancellation and declaration-import policies. |

## Runtime and libraries

Built-in modules are Flick interfaces over the runtime adapter and its Effect foundation. They are available through the compiler; they are not separate npm packages to install one by one. The runtime reference specifies cleanup and concurrency behavior, while the library pages give the operations and domain-specific constructs.

| Reference | Look here for |
| --- | --- |
| [Standard library](../reference/stdlib/) | Prelude, collections, strings, numbers, platform services, scheduling, tests and generators. |
| [Runtime adapter](../reference/runtime/) | The pinned Effect release, owners, finalizers, fibers, parallelism, failure causes, layers and entry-runner exit codes. |
| [Effect coverage](../reference/effect-surface/) | How the pinned Effect surface is classified and exposed, including explicit exclusions. Do not assume every Effect API is directly available in Flick. |
| [Schemas and codecs](../reference/schema/) | Refinements, schema derivation, validation, wire codecs and JSON Schema export. |
| [Typed HTTP APIs](../reference/http-api/) | Endpoint descriptions, request/response codecs, handlers, clients and API documents. |
| [Typed RPC](../reference/rpc/) | Procedures with codecs, groups of handlers, servers and clients over HTTP, WebSocket, TCP and standard streams, middleware and in-memory tests. |
| [Observability](../reference/observability/) | Spans, log records and metrics, and where they go: OTLP export, Prometheus routes and the Effect DevTools. |
| [SQL](../reference/sql/) | PostgreSQL table declarations, checked query syntax, nullability, parameters and database operations. |
| [User interfaces](../reference/ui/) | The `ui` syntax, React rendering, properties, events and component contracts. |
| [Command-line applications](../reference/cli-dsl/) | The `cli` syntax, flags, arguments, subcommands and handlers. This is the application library, not the compiler's CLI. |

For the underlying library's concepts and strengths—typed errors, structured concurrency, resource safety, dependency injection and observability—see the [official Effect documentation](https://effect.website/docs/). Flick's own runtime and coverage references remain the authority for the version and surface used here.

## Compiler and tooling

These references cover the native toolchain, its project model and the boundary with JavaScript and TypeScript. If you want to consume a compiled module from TypeScript, read **Code generation** alongside **Foreign functions**: exporting declarations and importing a package's declarations solve opposite sides of that boundary.

| Reference | Look here for |
| --- | --- |
| [Compiler CLI](../reference/cli/) | Project creation, checking, builds, execution, tests, formatting, dependencies, diagnostics and exit codes. |
| [Project configuration](../reference/project/) | `flick.toml`, source and test roots, profiles and installed package discovery. |
| [Code generation](../reference/codegen/) | ES modules, JavaScript value representation, source maps, TypeScript output, `.d.ts` emission and Promise facades. |
| [Native module interfaces](../reference/iface/) | The `.iface` format used to type-check dependencies, including public signatures and package layout. |
| [Editors](../reference/editor/) | LSP capabilities, Zed configuration, TypeScript projection and Content Mapper. |
| [Testing the implementation](../contributing/testing/) | Repository test suites and contributor checks, rather than application testing APIs. |

For the available packages, integrations and their distribution boundaries, continue to [Ecosystem](../ecosystem/).
