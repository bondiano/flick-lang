# Application features

Active contributors: bondiano, Vassiliy Kuzenkov. Non-bot authors on `origin/main` for standard-library application surfaces and examples.

Flick's application facilities share types, rows, codecs, and scoped execution rather than forming separate mini-languages. This page chooses a starting point; focused pages own the detailed contracts.

## Choose a surface

| Build | Capability and boundary | Guide |
| - | - | - |
| A domain model crossing untrusted input | A `Schema` describes values; a `Codec` also describes wire input and decode/encode requirements | [Data and schema](data-and-schema.md) |
| A PostgreSQL-backed application | `sql` builds checked templates; `Database` performs I/O and owns transactions | [SQL templates and migrations](data-and-schema.md#sql-templates-and-tables) |
| A conventional HTTP service | Raw handlers/router for protocol control, or `HttpApi` for typed endpoint/client/OpenAPI descriptions | [Network services](http-rpc-and-server-functions.md) |
| Shared typed remote procedures | `Rpc` supplies grouped handlers and transports; `server fn` supplies named HTTP calls across entry targets | [RPC and server functions](http-rpc-and-server-functions.md#rpc) |
| A browser or server-rendered interface | `Ui` values, `Atom` registries, typed event policies, and instance-region setup | [User interfaces](user-interfaces.md) |
| An executable tool | `Cli.Command`/`cli` descriptions, option records, prompts, configuration | [Command-line applications](command-line-applications.md) |
| Instrumented execution | Scoped spans/loggers/exporters and typed metric values | [Observability](observability.md) |

The syntax sugar in these APIs is not all owned by the same layer. `sql`, `codec`, `cli`, `server`, and `component` are first-party plugin heads. Core `ui { ... }` context, rows, service operations, wire-codec selection, and region checking still have compiler-owned rules. The [plugin page](../packages/plugins.md) makes those boundaries explicit.

## Read complete examples

| Example | Useful files | What it demonstrates |
| - | - | - |
| First slice | `examples/first-slice/src/App/Users.fl`, `examples/first-slice/src/App/Live.fl`, `examples/first-slice/src/App/Main.fl` | Domain services, typed errors, external implementations, composition root |
| Notes CLI | `examples/cli-tool/src/App/Main.fl`, `examples/cli-tool/src/App/Notes.fl` | Subcommands, file services, configuration, interactive wizard |
| Observability | `examples/observability/src/App/Main.fl` | OTLP configuration, nested spans, a counter, scoped independent tracer |
| Conduit contract | `examples/conduit/contract/src/Conduit/Api.fl`, `examples/conduit/contract/src/Conduit/Input.fl`, `examples/conduit/contract/src/Conduit/Route.fl` | Shared typed HTTP endpoints, explicit input codecs, route constructors |
| Conduit server | `examples/conduit/server/src/Conduit/Server/Schema.fl`, `examples/conduit/server/src/Conduit/Server/Db.fl`, `examples/conduit/server/src/Conduit/Server/Routes.fl` | Migration-described SQL, persistence, and request routing |
| Conduit client | `examples/conduit/client/src/Conduit/Client/Main.fl`, `examples/conduit/client/src/Conduit/Client/View.fl` | Shared contract consumption and reactive UI |
| MTL capabilities | `examples/mtl/src/App/Capabilities.fl`, `examples/mtl/src/App/Plan.fl` | Classes/higher-kinded capabilities alongside native effects |
| Typed interpreter | `examples/typed-interpreter/src/Interp/Expr.fl`, `examples/typed-interpreter/src/Interp/Check.fl` | Typed expressions and application-level checking |

## Integration and availability

Choose the runner, entry target, and runtime backend separately from the source-level API. A module's foreign import text is not sufficient evidence of a backend default or complete target parity. See [runtimes](../packages/runtimes/index.md) and [project and CLI](../systems/project-and-cli/index.md).

Browser compiler capability flags can remove whole host-oriented modules, including mixed client APIs in `Rpc` or `HttpApi`. The playground does not gain database/network/filesystem privileges merely because SQL or UI descriptions type-check; see [browser tools](../systems/browser-tools.md).

When changing a capability, start from its focused page and preserve the common [effect and lifetime contracts](effects-and-regions.md). For Conduit-affecting SQL/full-stack changes, `docs/agents/validation.md` additionally calls for PostgreSQL and Conduit integration checks; documenting those commands here does not mean they were run.

## Key source files

| Path | Purpose |
| - | - |
| `flick-core/src/plugin/first_party.rs` | Dispatches standard syntax, derives, and watched-call checks |
| `packages/core/src/Prelude.fl` | Shared schemas, codecs, effects, layers, and UI identities |
| `packages/std/src/Layer.fl` | Application composition boundary |
| `examples/conduit/flick.toml` | Full-stack workspace configuration |
| `CONTRIBUTING.md` | Capability-reduced browser builds and supported development workflows |
