# Lua host profiles

A Lua environment names a host, not a language: `lua/wow-retail`,
`lua/wow-forever` and `lua/love` each select one pinned profile. A profile
records its host's dialect, build, API snapshot, numbers, strings, loader,
callback and suspension rules and VM limits in
[`backends/lua/profiles/`](../../backends/lua/profiles/). Each fact carries
one kind of evidence:

- `doc`: a pinned source, such as a commit, a page revision (`oldid`) or a
  versioned manual, read on the date its profile gives;
- `exec`: a probe of [`backends/lua/probes/`](../../backends/lua/probes/)
  that ran on the named host on the named date.

`flick/lua` is a pinned, non-official backend component. It describes the
environments, checks programs against their profile and generates the pure
core of a program as Lua chunks: the Lua kernel's own chunks, then one per
reached module, each with a source map, defined into a private namespace
each chunk takes as its second argument (see
`backends/lua/src/emit/build.rs`). Everything else is refused with `P012` at
its source, or for what a built-in module needs, at the project's
expression that reached it. The rows of `backends/lua/kernel.txt` say, for
each `flick:` binding, whether the Lua kernel implements it and whether it
suspends.

## Profiles

| Profile | Environment | Host and build | API snapshot | Dialects | JIT | FFI | `require` | Suspension | Evidence |
| - | - | - | - | - | - | - | - | - | - |
| `lua-5.1` | dialect `lua51` | PUC-Rio Lua 5.1.5 (nixpkgs `0bb2eedc`, `lua5_1`) | `api/lua-5.1.txt`, dumped from the host | `lua51` | no | no | yes | coroutines, no yield across `pcall` | exec and doc |
| `luajit-2.1` | dialect `luajit` | LuaJIT 2.1.1785763465 ([`1edc3e52`](https://github.com/LuaJIT/LuaJIT/tree/1edc3e52b67eaf6ce5f809be8e17d6862594b8bc)) | `api/luajit-2.1.txt`, dumped from the host | `lua51`, `luajit` | yes | yes | yes | yields across `pcall` and metamethods | exec and doc |
| `love-11.5` | `lua/love` | LÖVE 11.5 ([`6eb8d546`](https://github.com/love2d/love/tree/6eb8d546736d5915a8b5af30b2cf33456dfdcb1a)) on that LuaJIT | `api/love-11.5.txt`, dumped inside a game | `lua51`, `luajit` | yes | yes | yes, from the game's files | yes | exec and doc |
| `wow-retail` | `lua/wow-retail` | World of Warcraft 12.1.5.70961, interface 120105 ([mirror `ab5cb850`](https://github.com/Gethe/wow-ui-source/tree/ab5cb850a3f65e742959e379c4689353a2f4549f)) | `api/wow-retail.txt`, from the client's API documentation and `wow-lua.txt` | `lua51` | no | no | no | no | doc only |
| `wow-forever` | `lua/wow-forever` | World of Warcraft: Forever 1.60.1.71037, interface 16001 ([mirror `734f0653`](https://github.com/Gethe/wow-ui-source/tree/734f065329cff4992835b01a84f210a297db91b6)) | `api/wow-forever.txt`, as for Retail | `lua51` | no | no | no | no | doc only |

World of Warcraft: Forever is a separate game with vanilla-era content. Its
client shares the mainline addon API of 12.1.5, and Midnight's addon
restrictions and secret values apply to it
([Patch 1.60.1/API changes/notes](https://warcraft.wiki.gg/index.php?oldid=6921728)).

The `dialect` axis (`lua51` by default, or `luajit`) and the capabilities
`jit` and `ffi` are tags of `[target.lua]`. JIT and FFI are independent:
each needs the `luajit` dialect and a host that has it. `Int64`, `UInt64`
and `BigInt` need neither: the kernel holds them exactly on every dialect.

## Probes

`just lua-probes` runs every probe on Lua 5.1.5, on LuaJIT with the JIT on
and with `-joff`, and inside a headless LÖVE 11.5 game (`t.window = false`,
the SDL dummy drivers, OpenAL's null driver). The game runs the probe in
`love.load`; its first `love.update` reports itself and quits. Each output
must equal the probe's expectation, `<probe>.<host>.expected` or
`<probe>.expected`. The expectations were written from the Lua manuals,
the hosts' sources and Flick's [numeric policy](numeric-policy.md), not
from a run. The recipe also compares the executable API snapshots with
their hosts and writes a `FlickProbes` addon for each WoW profile under
`target/lua-probes/wow/`. Lua 5.1's `luac -p` parses that addon; no WoW
client runs it. The `lua-probes` CI job runs the recipe when a pull request
changes `backends/lua`, the flake or the workflow, and on every push to `main`.

Results of 2026-11-24, on Lua 5.1.5, LuaJIT 2.1.1785763465 and LÖVE 11.5
(37 of 37 checks):

| Probe | Shows | Lua 5.1 | LuaJIT, JIT on and off | LÖVE 11.5 | WoW |
| - | - | - | - | - | - |
| `numbers` | Int32 on doubles: `2^53 + 1` rounds; plain `max * max` rounds to `0`, a 16-bit split gives `1`; `/` and `%` give `-3.5`, floored `1`, `inf` and `nan` where Flick wants `-3`, `-1`, `0` and `0`; `fmod(min, -1)` is `-0`; a 200000-step loop checksum | pass | pass | pass | not run |
| `format` | `tostring` is `%.14g`: `2^53` prints `9.007199254741e+15`; `-0` survives a runtime negation; NaN is `-nan` on 5.1 and `nan` on LuaJIT; on 5.1 a `-0.0` literal becomes `+0` beside an equal `0` constant | pass | pass | pass | not run |
| `strings` | `#` counts bytes (`héllo` 6, UTF-16 5; `𝄞` 4, UTF-16 2); an embedded NUL counts; 5.1's `string.format("%s")` stops at it | pass | pass | pass | not run |
| `returns` | `select('#')` keeps trailing nils; `(f())` and a non-final `f()` give one value; constructors and `unpack` adjust | pass | pass | pass | not run |
| `errors` | `error` adds a position to string and number messages at level 1; values keep their identity; a failing `xpcall` handler is `error in error handling` | pass | pass | pass | not run |
| `limits` | 200 locals and 60 upvalues compile, one more does not; constants stop after 262143 (5.1, raised out of `loadstring`) or 65536 of a kind (LuaJIT); a call fails at 248 (5.1) or 247 (LuaJIT) arguments | pass | pass | pass | not run |
| `coroutines` | a yield across `pcall` or `__index` fails on 5.1 and works on LuaJIT; across `table.sort` it fails on both | pass | pass | pass | not run |
| `extensions` | `bit`, `jit` and `ffi`: absent on 5.1; on LuaJIT `jit.status()` follows `-joff`, and an FFI `int64_t` holds `2^53 + 1` exactly | pass | pass | pass | not run |

The probes are positive and negative at once: each line records what a host
does, including where it differs from Flick's contract. A kernel must
guard every such difference; the [numeric policy](numeric-policy.md) states
the results it must produce.

## Fixtures

`just lua-run` builds the fixtures of
[`backends/lua/fixtures/`](../../backends/lua/fixtures/) with the e2e
compiler through the committed `package/component.wasm`, for `lua/love` in
both dialects and for both WoW profiles, and runs what each build writes on
the pinned hosts. On plain Lua 5.1 and LuaJIT a loader shim reads
`chunks.txt` and loads each chunk with `(nil, ns)`, as a `.toc` would; LÖVE
runs the generated `main.lua` headless; for WoW, Lua 5.1's `luac -p` parses
every chunk and the shim loads them, as no client runs here. Each fixture's
`expected` output and `error` were written from Flick's semantics, not from
a run. A Lua error names its chunk and line; the harness maps the line
through the chunk's `.map` to the Flick file and line and compares that.
Output goes through `foreign pure … from global "print"`.

Results of 2026-11-26 (77 of 77 checks):

| Fixture | Shows | `love`: Lua 5.1, LuaJIT on and off, LÖVE | `love` with `luajit`: LuaJIT on and off, LÖVE | WoW Retail and Forever: `luac -p`, Lua 5.1 |
| - | - | - | - | - |
| `recursion` | a recursive count over a list, a recursive `last`, mutual recursion in `rec { … }` | pass | pass | pass |
| `init_order` | three modules initialize in dependency order: `Base`, then `Lib`, then `App` | pass | pass | pass |
| `exports` | a type, its constructors and functions exported by one module and used by another | pass | pass | pass |
| `callback` | a lambda and a partial application passed to `string.gsub`, which calls them | pass | pass | pass |
| `init_error` | an error while a top-level value initializes, at `src/App.fl:11` | pass | pass | pass |
| `callback_error` | an error inside a callback the host calls, at `src/App.fl:15` | pass | pass | pass |
| `host_utf8` | a byte the host gives that is not UTF-8 becomes U+FFFD; UTF-8 stays as it is | pass | pass | pass |

## Evidence

The executable profiles state no fact without a probe or a pinned source;
the API snapshots of Lua 5.1, LuaJIT and LÖVE are exec evidence, and every
run of `just lua-probes` compares them with their hosts again. Run
`node backends/lua/probes/run.mjs --write-api` after a host update, and
review the difference.

Both WoW profiles are documentation evidence only. Their API snapshots are
the functions the client's `Blizzard_APIDocumentationGenerated` documents
at the pinned mirror commit, plus the Lua environment that documentation
leaves out (`wow-lua.txt`, from
[Lua functions](https://warcraft.wiki.gg/index.php?oldid=6879939) and
[CreateFrame](https://warcraft.wiki.gg/index.php?oldid=6818673)). To
regenerate one, check out the mirror at the profile's commit and run
`lua backends/lua/profiles/api/wow.lua backends/lua/profiles/api/wow-lua.txt
<checkout>/Interface/AddOns/Blizzard_APIDocumentationGenerated/*.lua` from
the repository root. A snapshot lists what is documented, not everything a
client defines; a missing global needs its source added to `wow-lua.txt`.

## Release blockers

- **WoW live validation.** No WoW client ran a probe or the generated
  addon: the number type, VM limits, the absence of `require` and the
  `ADDON_LOADED` callback of both WoW profiles are inferred from documentation
  and from Lua 5.1. Each profile lists this under `blockers`, and support for
  a WoW profile cannot be claimed until a live client runs the probes.
- **Code generation.** The component generates only the pure core and
  calls host globals with scalar values and one-stage callbacks. The kernel
  implements what [the kernel](#the-kernel-and-flick-test) lists; the rest
  of it, the value ABI and the host runtime are separate work. Until they
  exist, every other reachable binding of Flick's own kernel (`flick:`
  specifiers) is refused on every profile, and `flick run` and `flick dev`
  are refused: the host runs the program.

## The kernel and `flick test`

The kernel is portable Lua 5.1, `backends/lua/kernel/*.lua`, without the
`bit` library and reading only globals every API snapshot defines. Its
chunks put the table `K` at `ns.k`; `_flick/lua-kernel/lang.lua`, which the
backend generates, gives it `Some`, `None` and `Ordering` in the
representation of the program. Generated code calls `K` for:

- `Int` arithmetic, which wraps to 32 bits, divides toward zero and gives
  `0` for a zero divisor, as the [numeric policy](numeric-policy.md) says,
  and that of `Int8`, `Int16`, `UInt8`, `UInt16` and `UInt32`, doubles
  wrapped to their width; `Float32`, a double rounded to binary32 after each
  operation; the bit operations and shifts, and the conversions between the
  numeric types;
- `BigInt`, a table of its sign and its magnitude's limbs of 24 bits, with
  schoolbook multiplication and Knuth's division; `Int64` and `UInt64` are
  `BigInt` values wrapped to 64 bits after each operation, as on JS, so
  they need no `ffi` and round nowhere;
- `%` and `**` of `Float`, as JS computes them, and the JS globals of the
  kernel the standard library binds, `Number`, `Math.floor`, `Math.sqrt`
  and `Math.log`, a port of fdlibm's `__ieee754_log` that reads the high
  word through `math.frexp`; `String.to_number` reads JS's number syntax. On Lua 5.1,
  whose compiler keeps one constant for `0` and `-0`, a `-0` that Lua would
  fold into a constant is the kernel's `K.negzero`;
- `==` of every value: SameValueZero on numbers, strings by their bytes and
  structured values by what they hold, skipping the dictionaries a tagged
  value holds at `$1`, …;
- strings, which are WTF-8 Lua strings: `++` joins the halves of a
  surrogate pair, `<` and the hooks order them by UTF-16 code units, and
  `String.length`, `char_code_at`, `slice`, `index_of` and `last_index_of`
  count UTF-16 code units;
- `++` of lists, `List.length`, `List.get` and `List.initialize`, and the
  hooks `equals`, `compare`, `hash` (that of the JS and Go runtimes, bit for
  bit) and `show` (JS spelling, `-0` kept) of the primitive values;
- the record rows `Record.get`, `extend`, `remove`, `project`,
  `merge_disjoint` and `labels`, over a record's table of labels;
- `Bytes`, a Lua string of its bytes: `Bytes.from_list`, `to_list`,
  `encode_utf8` (a lone surrogate as U+FFFD) and `decode_utf8` (strict);
- the kernel's defects: `defect` and a failed `let assert` stop with Lua's
  `error` at level 0, so without a chunk's position: `defect` with its
  message, a failed `let assert` with the message the compiler gives it and
  the debug rendering of the value, as JS writes them, which a test reports
  as JS does;
- `Test.test`, `group`, `skip` and `only`.

A string the host gives, as a result or to a callback, is UTF-8 to the
host and WTF-8 to Flick: the kernel's `from_host` keeps its UTF-8 and
replaces each ill-formed part with U+FFFD, as `TextDecoder` does, so a
string never holds bytes the kernel cannot read. A string goes to the host
as it is; a lone surrogate in it, which UTF-8 cannot hold, reaches the host
as its WTF-8 bytes.

A built-in function whose `@native` clauses name JS and Go runs its Flick
body on Lua. On `lua/love`, `flick test` builds the test modules with the
runner `_flick/lua-kernel/runner.lua`, and runs `_flick/run-tests.lua` on
Lua 5.1.5 or LuaJIT 2.1, whichever `[tools]` allows (LuaJIT when both are):
the interpreters stand for LÖVE's Lua, and no WoW client runs tests. Its
pure tests report as on the other targets. An effect test is reported
skipped, with the reason `an effect test needs the Lua runtime`, and its task
is neither generated nor checked, until the Lua runtime (#833) runs it;
property tests are refused. `just kernel-lua` runs the modules of
`tests/kernel` and of `tests/language` that their `lua.txt` lists on Lua
5.1.5 and on LuaJIT with the JIT on and off, and requires each test of the
JSON report to report as on JS, but an effect test, which only that reason
may skip; `luac -l` checks the globals each kernel chunk reads against every
snapshot. The CI `lua-probes` job runs it.

| Kernel module | Lua 5.1, LuaJIT on and off |
| - | - |
| `Kernel.Arrays` | pass (11 of 11), the same report as JS |
| `Kernel.Defects` | pass (1 of 1), the same report as JS |
| `Kernel.Hooks` | pass (32 of 32), the same report as JS |
| `Kernel.Numbers` | pass (81 of 81), the same report as JS |
| `Kernel.Records` | pass (6 of 6), the same report as JS |
| `Kernel.Strings` | pass (35 of 35), the same report as JS |
| `Language.BigInts` | pass (17 of 17), the same report as JS |
| `Language.Floats` | pass (34 of 34), the same report as JS |
| `Language.Ints` | pass (48 of 48 pure tests), the same report as JS; 1 effect test skipped |
| `Language.NumericBoundaries` | pass (10 of 10 pure tests), the same report as JS; 2 effect tests skipped |

## The synchronous WoW slice

An addon's code runs inside host callbacks (file load, `ADDON_LOADED`,
events and timers), and each callback runs to completion. The WoW
profiles therefore refuse reachable asynchronous suspension: `await`, an
`async` foreign function and the kernel bindings of the standard library
that suspend a fiber (`Effect.sleep`, `Effect.never`, `Effect.yield_now`,
the joins of `Fiber` and `Deferred.await`), each marked `suspends` in
`backends/lua/kernel.txt`. LÖVE has coroutines that yield
across `pcall`, so its profile keeps suspension open for a later runtime.

## What `check-target` refuses

The component's `check-target` walks the IR from each entry's `main`, test
root and host export (or from a library's exports), following globals,
dictionaries and the superclasses and defaults of the instances it reaches,
and the eager values of each own module it reaches. `generate` emits what
the same walk reaches. A fact found in a built-in
module is reported at the own expression that reached it, with a note of
the global it went through. Every refusal is `P012`:

| Reached fact | Refused when |
| - | - |
| the environment `lua/<p>` | `p` is not a profile of the component |
| the dialect tag, `jit` or `ffi` | the host lacks it, or `jit`/`ffi` lacks the `luajit` dialect |
| `await`, an `async` foreign function or a suspending kernel binding | the profile has no suspension (WoW) |
| a `flick:` kernel binding | the Lua kernel does not implement it (`kernel.txt`) |
| a module binding (`from "m"`) | the profile has no `require` (WoW), or `m` is `ffi` without `ffi` |
| a global binding (`from global "a.b"`) | the path is not in the profile's API snapshot, or its root (`bit`, `jit` on LÖVE) needs a tag that is off |

The component declares the descriptor capability `flick.check-target`
(version `0.1.0`), so `flick check` and the editor run its `check-target`
for every project, not only for one with foreign items. In `flick check` the
request carries the project's modules without the built-in bodies, so
built-in facts are found by `flick build`, which still refuses before
writing anything. Inline Lua code is not parsed yet.
