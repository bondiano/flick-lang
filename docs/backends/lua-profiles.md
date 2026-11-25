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
core of a program as Lua chunks: one per reached module, each with a source
map, defined into a private namespace each chunk takes as its second
argument (see `backends/lua/src/emit/build.rs`). Everything else, the kernel
above all, is refused with `P012` at its source.

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
each needs the `luajit` dialect and a host that has it. Without `ffi`,
`Int64` and `UInt64` are refused rather than held in doubles.

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
- **Code generation.** The component generates only the pure core. The
  kernel, the value ABI and the host runtime are separate work; until they
  exist, every reachable binding of Flick's own kernel (`flick:` specifiers,
  each a row of `backends/lua/kernel.txt`) is refused on every profile.

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
| a `BigInt` literal, operator or foreign function of that type | always: the Lua kernel has no arbitrary-precision integers yet |
| an `Int64` or `UInt64` literal, operator or foreign function | the `ffi` capability is off |
| `await`, an `async` foreign function or a suspending kernel binding | the profile has no suspension (WoW) |
| a `flick:` kernel binding | always: the Lua primitive kernel is not implemented |
| a module binding (`from "m"`) | the profile has no `require` (WoW), or `m` is `ffi` without `ffi` |
| a global binding (`from global "a.b"`) | the path is not in the profile's API snapshot, or its root (`bit`, `jit` on LÖVE) needs a tag that is off |

The component declares the descriptor capability `flick.check-target`
(version `0.1.0`), so `flick check` and the editor run its `check-target`
for every project, not only for one with foreign items. In `flick check` the
request carries the project's modules without the built-in bodies, so
built-in facts are found by `flick build`, which still refuses before
writing anything. Inline Lua code is not parsed yet.
