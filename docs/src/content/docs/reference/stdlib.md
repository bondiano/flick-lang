---
title: "Standard library"
---

The standard library is Flick code over the runtime adapter (see [Higher-order functions and collection traversal](../../spec/04-higher-order/), [Schema and standard data types](../../spec/07-schema/), [Compiler CLI](../cli/)).
Its modules are organized into implicit source packages. A program names them
qualified without an import (`List.map`, `Ref.make`), or imports names from them
like from any module (`import File exposing (FileSystem, OpenError)`). The
Prelude is seen by every module without qualification. A service or type of
a program shadows a built-in module of the same name for its operations:
with `service Log` declared, `Log.write` is the operation.

Signatures the specification gives are declared word for word;
`flick-core/tests/stdlib_signatures.rs` checks it.

[Generated package API](../../api/) lists the public signatures, doc comments and source links from `flick doc --std`.

For `Js.Array` and `JsError`, see [JavaScript interop](../ffi/).

## Prelude

| Name | What it is |
| - | - |
| `Eq`, `Ord`, `Hash`, `Show` | canonical equality, the total order, hashing, display |
| `Display` | text for a reader, what a hole `\{e}` of a string inserts: a `String` without quotes |
| `Into` | `into`, a conversion that loses nothing to the type its place expects: `Int` to `Float` or `BigInt` |
| `Functor`, `Foldable`, `Traversable`, `Applicative`, `Monad` | classes over `f : Type -> Type`: `map`, `fold` and `fold_map`, `traverse`, `pure` and `map2`, `flat_map` |
| `Semigroup`, `Monoid`, `Alternative` | `combine` and `empty` (`String`, `List a`, the wrappers of `Monoids`), `nothing` and `or_else` |
| `bind`, `bind_eff` | sequencing for any monad, and for `Eff` with the rows joined |
| `from_result` | `Result (Errors e) a ->{; e} a` |
| `parallel`, `parallel_all`, `parallel_results`, `parallel_exits`, `race_success`, `race_first`, `timeout`, `timeout_option` | structured concurrency, see [runtime.md](../runtime/) |
| `Failure`, `Exit`, `RawCause`, `CauseReport` | opaque outcomes |

`Option`, `Result`, `List`, `Seq`, `NonEmpty`, `Chunk`, `NonEmptyChunk`,
`HashMap`, `HashSet`, `Trie`, `PriorityQueue`, `OrderedMap`, `OrderedSet`,
`Bytes` and `Eff r e` have instances of the Prelude and `Collections` classes
whose laws they satisfy; see [collection type classes](../collections/) for
the full hierarchy, laws, instance matrix and complexity bounds.

## Modules

| Module | Contents |
| - | - |
| `Collections` | shared collection capability classes (`ToSeq`, `Sized`, `Indexed`, `Keyed`, `Member`) and minimal-capability generic consumers (`take`, `find`, `any`, `all`, `each_effect`, `fold_effect`, `to_list`, `count`, `lookup_or`, `lookup_all`, `filter_members`, `all_in`, `endpoints`, `nth_from_end`); see [collection type classes](../collections/) |
| `List` | `map` (pure callback), `map_effect` (runtime callback, one element after another), `map_indexed`, `each_effect`, folds, `filter`, `find`, `sort` (stable, by `Ord`), `sort_by`, `get`, `slice`, `range`, `zip`, … |
| `Seq` | lazy sequences that start over on each iteration: infinite `from`, `iterate`, `repeat`, `cycle`, `unfold`; `range`, `map`, `filter`, `flat_map`, `take`, `take_while`, `drop`, `zip`, `enumerate`, `scan`, `chunks`, `windows`, `find`, `first`, `to_list`, `fold`, `each_effect`; a `Monad` for `do`; `Stream.from_seq` streams one |
| `Option`, `Result` | `map`, `bind`, `with_default` and conversions |
| `Monoids` | `Sum` and `Product` of numbers, `First` and `Last` of an `Option`, `Min` and `Max` by `Ord`: `fold_map Sum xs` says how numbers combine |
| `Generic` | `Generic a rep` with `to_rep` and `from_rep`, which `deriving Generic` gives a type, and its representation `Sum`, `Con`, `Field` (see [Deriving](../classes/#deriving)) |
| `Validation` | `Validation e a`, a `Result` whose `map2` combines the errors of both with their `Semigroup`: an `and` group of `do` reports every error; `valid`, `invalid`, `from_result`, `to_result` |
| `Flick.Defect` | `defect`, which stops pure code with a message the nearest handler of defects receives, as `Int.unsafe_div` by zero does (see [the kernel](../kernel/#defects)) |
| `Form` | forms of a record type that derives `Form`: `draft`, `input`, `validate`, `report` (see [Forms](../form/)) |
| `Route` | URLs of a type that derives `Url`: `print` and `parse`; `Atom.route` holds the current page (see [Routes](../route/)) |
| `NonEmpty` | lists with at least one element |
| `Chunk` | sequences for repeated concatenation |
| `String` | `split`, `join`, `slice`, `index_of`, `to_number`, … over UTF-16 code units |
| `Markdown` | `to_html`: the everyday CommonMark subset (headings, paragraphs, lists, quotes, fenced code, rules, `code`, strong, emphasis, links) into HTML in which every text is escaped and links go only to `http`, `https`, `mailto` and relative URLs, so a script in the source stays text; `escape` |
| `Int` | `to_float`, `from_float`, `truncate`, `parse`, bitwise `bit_*` and `shift_*`, checked `checked_add`, `checked_div`, … returning `Result ArithmeticError Int`, and `unsafe_div`, `unsafe_rem` |
| `Float` | `strict_equal` (`===`), `same_value` (`Object.is`), rounding, `parse`, `is_nan`, `is_finite`, … |
| `BigInt` | conversions and checked `divide`, `remainder` |
| `Ref` | `make`, `get`, `set`, `modify`, and `same` for identity; no `Eq` or `Hash` |
| `FuncRef` | `fun`, the function a `FuncRef` refers to, a function without captures for a host that takes a plain function pointer (see [ffi.md](../ffi/#functions-without-captures-and-trampolines)) |
| `Redacted` | `make`, `reveal` (runtime); `show` prints `<redacted>` |
| `BigDecimal` | decimal numbers: `from_string`, exact `add`, `subtract`, checked `multiply`, `divide_to_scale` |
| `Duration` | lengths of time: checked `nanos`, `millis`, `seconds`, `minutes`, explicit `infinity` |
| `Bytes` | [packed, immutable byte sequences](#packed-bytes) and conversions for `List Int` APIs |
| `Crypto` | the `Crypto` service over Web Crypto (`Crypto.system`): `random_bytes`, `digest` (SHA-1, SHA-256, SHA-384, SHA-512), `hmac` over `digest` (RFC 2104), random numbers, UUIDs and ULIDs; `pbkdf2` (RFC 8018), and `hash_password` / `verify_password`: PBKDF2-HMAC-SHA256 with 600000 rounds and a random salt, stored as `$pbkdf2-sha256$i=<rounds>$<salt>$<hash>` in URL-safe base64 and checked in constant time |
| `Jwt` | JSON Web Tokens signed with HMAC-SHA256 (`alg` `HS256`): `sign` and `verify` over a JSON codec of the claims; `verify` refuses another `alg`, a wrong signature and an `exp` the clock has passed with `JwtError` (`Malformed`, `BadSignature`, `Expired`); `now` and `expires_in` give `iat` and `exp` in seconds since the epoch |
| `EqHash`, `HashSet`, `HashMap` | hash collections that keep their witness |
| `Loop` | `loop` over a `Step` and `Trampoline`, for stack-safe pure recursion |
| `Host` | the six host services `Console`, `Clock`, `Random`, `Environment`, `Arguments` and `Process`, their sets `Live` (all six) and `BrowserLive` (the first three), and the trusted live layers `console`, `clock`, `random`, `environment`, `arguments`, `process`, `live` and `browser_live` an entry chooses; see [Explicit host dependencies](../services/#explicit-host-dependencies) |
| `Platform` | `stdout_line`, `stderr_line`, `stdout_write` through `Host.Console`, `args` through `Host.Arguments`, `env` through `Host.Environment` |
| `Log` | the `Log` service with `info`, `warn`, `error`, `debug`; `from_console` makes a `Log` layer of a `Host.Console` layer, and `Log.console`, made of `Host.console`, needs nothing |
| `File` | the `FileSystem` service: `open_scoped`, `read_all`, `read_text`, `write_text`, and `File.node` |
| `ChildProcess` | child processes: a `Command` (`make`, `set_cwd`, `set_env`, `set_stdin`, `prefix`, `pipe_to`, `inherit_output`) run to its end by `string`, `lines` and `exit_code`, or started by `spawn` as a `Process` of a scope, all through `Host.Process`, with `stdout`, `stderr`, `output_string`, `output_lines`, `wait`, `is_running`, `kill` and `kill_with` |
| `Resource` | `own` of a value of `Close`, which its scope closes once with the outcome of the body, and `make`, which acquires a value its scope releases with that outcome (with `Scope.defer` for cleanup without a value), see [resources.md](../resources/); `acquire_release` into a scope's owner, and `bracket` |
| `Events` | `queue` policies and the `log_failures` supervisor |
| `Ui`, `Ui.Event` | user interfaces rendered with React, see [ui.md](../ui/) |
| `Sql`, `Database` | typed PostgreSQL queries and the `Database` service, see [sql.md](../sql/) |
| `Rpc` | typed remote procedure calls — procedures with codecs, groups, servers and clients over HTTP, WebSocket, TCP and standard streams, see [rpc.md](../rpc/) |
| `Metric`, `Tracer` | metrics of the program's registry and spans around its tasks, see [observability.md](../observability/) |
| `Otlp`, `Prometheus`, `DevTools` | where spans, log records and metrics go: an OTLP collector, a Prometheus route, the Effect DevTools, see [observability.md](../observability/) |
| `Fiber`, `Failure`, `Exit`, `RawCause` | children of a scope and outcomes |
| `Test`, `Gen` | tests and generators, see below |
| `Schedule` | schedules for `Effect.repeat` and `Effect.retry`: `recurs`, `spaced`, `exponential`, `fibonacci`, `up_to`, `while`, `tap`, … |
| `Effect` | operations on tasks beyond the language: `retry` (typed failures only), `repeat`, `schedule`, `sleep`, … |

### More modules

Each of these modules is documented in its source under
`packages/<package>/src/`, in the
[implicit source package](../project/#flick-dependencies) that owns it.
For example, `Clock` is in `std`, `Stream` in `stream`, and `HttpApi` in `http`:

| Module | Contents |
| - | - |
| `Clock`, `Random` | reads of the time through `Host.Clock`, from the fiber's clock, which `Test.clock_test` makes virtual, and draws through `Host.Random`; `Random.with_seed` repeats the draws of a task |
| `DateTime` | instants in UTC or in a time zone, with checked constructors; `now`, `is_past` and `is_future` read `Host.Clock`, `now_in_current_zone` also `CurrentTimeZone` |
| `Config`, `ConfigProvider` | configuration read by path: `Config.get` reads the current `ConfigProvider` (from `ConfigProvider.layer`) or else `ConfigProvider.from_env` over the variables `Host.Environment.vars` lists, as `ConfigProvider.layer_add`, `Cli.run` and the `Otlp` exporters do, and `get_with` takes that fallback provider from a function of your own |
| `Console`, `Logger`, `LogLevel`, `ErrorReporter` | `log`, `info` and `debug` to stdout and `warn` and `error` to stderr through `Host.Console`, what happens to each log message, log levels ordered by severity, and where unhandled failures are reported |
| `Cause` | build, combine and read how a computation failed, over Flick's opaque `Failure e` |
| `Cron` | sets of times, optionally in a time zone; `Schedule.cron` runs by one |
| `ByteSize` | an exact, non-negative number of bytes in decimal or binary units |
| `Cache` | values looked up by key on demand, at most once at a time per key, with a capacity and a time to live |
| `Semaphore`, `PartitionedSemaphore` | permits that limit how many tasks run at once; the partitioned one shares a pool fairly by key |
| `PubSub` | messages published to every subscriber; a subscription belongs to a `scope` |
| `SynchronizedRef`, `SubscriptionRef` | references whose updates may run tasks; the subscription one publishes each new value |
| `FiberHandle`, `FiberMap` | at most one child at a time, or one per key, owned by a `scope` |
| `LayerMap`, `LayerRef` | layers built on first use and shared while in use: by key, or one refreshable layer |
| `ExecutionPlan` | steps that each provide a task's services, tried in order until one succeeds |
| `Request`, `RequestResolver` | requests answered in batches by a resolver |
| `Scheduler` | how fibers take turns |
| `Sink`, `ChannelSchema` | what consumes a stream, and channels that decode or encode each chunk with a codec |
| `Tx`, `TxRef`, `TxQueue`, `TxPriorityQueue`, `TxPubSub`, `TxHashMap`, `TxHashSet`, `TxChunk`, `TxDeferred`, `TxSemaphore`, `TxReentrantLock`, `TxSubscriptionRef` | transactional values, read and changed inside `Tx.atomically` |
| `Order`, `Equivalence`, `Predicate`, `Filter`, `Function` | orders, equivalences, predicates, filters and function helpers as values |
| `Combiner`, `Reducer` | ways to combine two values, and with a start value to fold a list |
| `Optic` | lenses, prisms and other optics that read a part of a structure and rebuild it |
| `Graph`, `Trie`, `HashRing` | immutable graphs with their algorithms, maps from string keys with prefix queries, consistent hashing onto weighted nodes |
| `PriorityQueue` | an immutable priority queue for pure algorithms: the least priority first, equal priorities in insertion order |
| `OrderedMap`, `OrderedSet` | persistent maps and sets in the order of `Ord`, with ascending traversal and inclusive ranges of keys |
| `JsonPatch`, `JsonPointer` | the differences between two JSON values (RFC 6902) and JSON Pointers (RFC 6901) |
| `Net` | network addresses and DNS records as JSON |
| `Terminal`, `PlatformError` | the terminal as a service, and how the platform services fail |
| `Formatter`, `Inspectable` | the text and JSON form of inert data |
| `Version` | the release of Effect the program runs on |

Every export of the pinned Effect is wrapped or decided in a manifest that
a linter checks, see [Effect coverage](../runtime/#effect-coverage).

## Numbers

`Ord Float` is a total order: `-Infinity` < finite numbers < `Infinity` <
`NaN`, signed zeros equal and all NaNs equal, so `List.sort` puts `NaN` last.
The operators `<`, `<=`, `>`, `>=` keep their JS meaning.

`BigDecimal.divide_to_scale scale mode dividend divisor` computes the
quotient with `scale` digits after the point from the coefficients as BigInt,
without a global precision:

| `mode` | `5 / 2` | `-5 / 2` | `11 / 4` | `-11 / 4` |
| - | - | - | - | - |
| `TowardZero` | 2 | -2 | 2 | -2 |
| `Floor` | 2 | -3 | 2 | -3 |
| `Ceiling` | 3 | -2 | 3 | -2 |
| `HalfEven` | 2 | -2 | 3 | -3 |
| `HalfAwayFromZero` | 3 | -3 | 3 | -3 |
| `Exact` | `DecimalError` | `DecimalError` | `DecimalError` | `DecimalError` |

A zero divisor and a scale that is not a safe integer are `DecimalError`
too. `Duration.millis (-1)`, `Duration.millis Float.nan` and a conversion
that overflows are `Err (DurationError …)`; an unbounded duration is
`Duration.infinity`, and `Duration.to_millis` gives `None` for it.

## Packed bytes

`Bytes.Bytes` stores one byte per element in the JS and Go runtimes.
`Bytes.empty` is empty and `Bytes.length` counts bytes. `Bytes.get` takes a
zero-based index. `Bytes.slice start end` takes a half-open byte range.
Both return `None` for invalid bounds. An empty range within the bounds
returns `Some Bytes.empty`.

`Bytes.from_list` copies a `List Int` and reports `Err (Bytes.ByteOutOfRange
index value)` for the first integer outside `0..255`. `Bytes.to_list` makes
a fresh list. Slices can share private storage, which stays immutable and
has no public mutable view.

Use `Bytes.encode_utf8` and `Bytes.decode_utf8` with packed bytes. Decoding
returns `None` for invalid UTF-8 and preserves a byte order mark. Use
`Bytes.encode_base64url` for unpadded output and `Bytes.decode_base64url`
for input with optional valid padding. `Bytes.equal_constant_time` visits
every byte when the lengths match.

`Bytes.from_string`, `Bytes.to_string`, `Bytes.to_base64url`,
`Bytes.from_base64url` and `Bytes.equal` still take or return `List Int`.
Use `from_list` and `to_list` at boundaries with existing binary APIs.

## Hash collections

A `HashSet w a` or `HashMap w k v` keeps the witness of equality and hashing
it was made with, and its identity `w` is part of the type. `HashSet.empty`
uses the canonical instances (`w` is `EqHash.Canonical`); `EqHash.with eq
hash body` makes a witness of a fresh identity and passes it to `body`,
which cannot return anything mentioning it. `HashMap.from_seq` and
`HashSet.from_seq` consume a finite sequence once without building a
temporary list, and `to_seq` returns a lazy, restartable traversal as a
source of `for`. Insertion and lookup use the
set's own witness, and `HashSet.union` of sets of two witnesses is a type
error: `HashSet.rehash witness set` moves a set to another witness first.

```flick
case_blind : Int
let case_blind =
  EqHash.with
    (fun a b -> String.to_lower a == String.to_lower b)
    (fun s -> hash (String.to_lower s))
    (fun witness -> HashSet.size (HashSet.insert "ADA" (HashSet.insert "ada" (HashSet.with witness))))
```

## Child processes

A process's `stdout` and `stderr` are pipes. Both targets bound unread buffering
and drain unread output on `wait` and `exit_code`:

| | JS target (Node) | Go target |
| - | - | - |
| before `wait` / reading | Node pauses the stream once the pipe buffer (~64 KiB) fills | `outputQueue` buffers up to 64 KiB per stream, then pauses the writer |
| on `wait` / `exit_code` | concurrently drains `stdout` and `stderr` and returns exit code without deadlocking | discards queued/incoming output (`drain()`) and returns exit code in bounded memory |

Cases:

```flick
// a. spawn, then only wait: 1 MiB nobody reads
waits : Unit ->{Host.Process, Async; PlatformError.PlatformError} Int
let waits () = scope {
  let p = ChildProcess.spawn (ChildProcess.make "sh" ["-c", "head -c 1048576 /dev/zero"])
  ChildProcess.wait p // JS: 0; Go: 0 (drains unread output)
}

// b. exit_code of a command with much output: the same
chatty : Unit ->{Host.Process, Async; PlatformError.PlatformError} Int
let chatty () =
  ChildProcess.exit_code (ChildProcess.make "sh" ["-c", "head -c 1048576 /dev/zero"]) // JS: 0; Go: 0

// c. endless output nobody reads, e.g. `yes`: bounded to ~64 KiB before `wait`;
//    during `wait`, both targets drain output in bounded memory until `kill` or scope close
```

Read what a child writes (`stdout`, `stderr`, `string`, `lines`), or let it
write to the program's own streams with `inherit_output`; both targets behave
alike. This is pinned by `tests/run/stdlib/process/wait_large_output.fl` and
`TestChildProcessUnreadOutputDoesNotBlock` in `runtime-go/flickrt/process_test.go`.

## Tests

A module of the test root exports `tests : List Test.Test`, which `suite`
and `test` blocks declare (see [testing](../testing/)), or which it builds
by hand:

```flick
module Test.Text exposing (tests)

tests : List Test.Test
let tests =
  [
    Test.test "trims" (fun () -> Test.equal "a" (String.trim " a ")),
    Test.effect_test "reads" (task { Test.check (Test.equal 1 1) }),
    Test.property "reverse twice" (Gen.list 10 (Gen.int 0 9)) (fun xs ->
      Test.equal xs (List.reverse (List.reverse xs))),
    Test.group "numbers" [Test.test "adds" (fun () -> Test.equal 2 (1 + 1))],
  ]
```

| Function | Test |
| - | - |
| `Test.test name check` | pure: `check ()` is an `Assertion` |
| `Test.expect actual \|> Test.to.equal expected`, `to.be_true`, `to.be_greater_than`, `to.contain`, `to.fail_with` | a matcher of the subject `actual`, which fails the test with `AssertionFailed` when it does not match, `to.equal` with a line diff of long values (see [testing](../testing/#assertions)) |
| `Test.effect_test name task` | the task runs in a scope of its own, which owns what it acquires, with the live host services, which outlive that scope: what it acquires may use them when it is released; `Test.check` fails it on an assertion |
| `Test.clock_test name task` | as `effect_test`, on a clock of its own that starts at 0 and moves only by `Test.adjust duration` and `Test.set_time millis`: a sleep, a timeout or a retry schedule waits for virtual time, so a test of an hour's backoff runs at once |
| `Test.property name gen check` | `check` on 100 values of `gen`, drawn from the run's seed; `Gen.function result` (for a `Hash` argument) draws pure functions, as a law over functions needs |
| `Test.group name tests` | `name >` before the names of `tests` |
| `Test.skip test`, `Test.only test` | the test, and each test in it, skipped or marked `only` (see [testing](../testing/#full-names-skip-and-only)) |
| `Test.before_all task`, `before_each`, `after_each`, `after_all` | a hook of the group, or the module, whose list it is in, run around the tests of it that the run selects (see [testing](../testing/#hooks)) |
| `Test.timeout millis test` | the test, and each test in it without its own, given `millis` for its `before_each` hooks and its body (see [testing](../testing/#timeouts)) |
| `Test.case_name name index parts` | the name of the case `index` of a `test.each "name"`: `name [index]`, or with `{0}`…`{n}` replaced by `parts` |
| `Test.serial test` | the test, and each test in it, run one at a time, also under `--jobs` |
| `Test.fixture key init`, `Test.shared key init`, `Test.share fixture`, `Test.use fixture` | a fixture of each test, or shared by the run of the group whose list `share`s it, and its `Lazy` for the code that uses it: what `fixture` of a suite expands into (see [testing](../testing/#fixtures)) |
| `Fixture.cleanup release value` | `value`, which `release` releases when the scope of the fixture that made it closes |
| `Test.suite name layer specs` | tests that share what `layer` makes: built once, before the first of them that runs, and released after the last, the `beforeAll` and `afterAll` of the suite; each spec, `Test.it name task`, is an `effect_test` with the layer's services, and a failed build fails each of them |

A suite of end-to-end tests starts what they need once, such as a database
in a container, and sends requests to the server's own router with
`HttpApi.Inject.call` (see [HttpApi](../http-api/#clients-and-documents)):

```flick
# import Conduit.Api as Api
# import Conduit.Server.App as Server
# import Conduit.Server.Postgres as Postgres
# import Conduit.Server.ApiTest exposing (send, at, ada)
#
# let example = task {
let services = Layer.merge Postgres.layer (Layer.merge (Server.keys "test") Crypto.system)

tests : List Test.Test
let tests = [
  Test.suite "the API" services [
    Test.it "signing up answers a token" (task {
      let user = (send (at None () { user = ada }) Api.Public.endpoints.register).user
      Test.check (Test.is_true "a token" (user.token != ""))
    }),
  ],
]
# ()
# }
```

`examples/conduit/server/test` runs the RealWorld API this way, its
PostgreSQL started from Flick with `ChildProcess`.

`flick test` builds the project with its test root into `.flick/test/` and
runs every test; the full name of a test is `Module > group > name`.

| Option | Meaning |
| - | - |
| `--filter <text>` | only tests whose full name contains `text` |
| `--jobs <n>` | at most `n` tests at a time, `1` by default |
| `--seed <n>` | the seed of the generators; a failed run prints the one it used |
| `--reporter human\|json` | lines per test and a summary, or one JSON object |
| `--watch` | run again when a file of the project changes |
| `--target js\|go` | run on Node or as one Go executable, `.flick/test/flick-test`; by default by `build.target` |

Each result is `passed` or `failed` with what failed: `assertion`, `failure`
(a typed error), `defect`, `cleanup` (the test's scope did not close
cleanly; a test whose body passed fails too) or `interrupted`. The exit code
is `0` when every test passed, `1` otherwise. SIGINT interrupts the running
tests, and every one of them closes its scope before the runner exits; under
`--watch` a change stops the run the same way (the watcher closes the
runner's stdin, so it works on every platform) and starts the next run only
after that.
