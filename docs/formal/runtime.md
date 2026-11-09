# λRT: the runtime contract

Extends `core.md` and `regions.md`: everything not redefined here holds as stated there. Divergences from the spec and between the two runtimes, and how each is resolved, are in §7.

References:

- JS — `runtime/src/X.ts`;
- Go — `runtime-go/flickrt/X.go`;
- Effect — `FX/…`, that is, `node_modules/.pnpm/effect@4.1.2/node_modules/effect/src/…`;
- everything else — as in `core.md`.

The probe programs of §7 are built with `flick build --emit js` and `--emit go` and run on Node and as a Go executable. Resources come from `tests/run/runtime/resources.{mjs,go}`: `open name` prints `open name` and registers a release that prints `close name`.

## 0. Scope

λRT describes what happens between evaluation steps: which fibers are alive, who owns them, how work ended and when resources are released. The core gives the steps of one thread; regions give the static bound on what reaches a resource. λRT adds:

- a pool of fibers and a nondeterministic choice of the next step;
- scope owners: admission of children, supervisor watchers, foreign Promises, closing hooks, the finalizer stack, two-phase closing;
- causes (Cause), their algebra and the "interrupted" outcome;
- interruption and masking;
- `parallel*`, `race_*`, `timeout*`;
- building layers with memoization by identity, and release after all consumers;
- atomicity of `Ref`.

| Out of scope | How λRT accounts for it |
| - | - |
| The scheduler, time, the Effect operation budget, `yield_now` | Any ready fiber may take a step; a timer is an external event that can happen at any time |
| UI roots, the Bridge supervisor, atoms | Root owners with their own defect sink, like the runner's (§3.8) |
| HTTP server, RPC, queues, PubSub, `Tx*`, streams | Sources of fibers and finalizers through the same steps; divergences in them — [HTTP handlers](#go-http-handlers) |
| `FiberMap`, `FiberHandle` | As `FiberSet` (§3.6): JS and Go each have one implementation for all three |
| Internals of the `ScopedRef`, `Resource`, `ScopedCache` leases | Hypothesis G5 (`regions.md`); masking of their registration — [leases](#js-leases-register-their-release-outside-the-mask) |
| OS signals, exit codes | Interruption of the main fiber; codes — `reference/runtime.md` "Entry runner" |

## 1. Configurations

### 1.1 Causes and outcomes

```text
r ::= Fail v | Die d | Int f          reason: expected error, defect, interruption by fiber f
c ::= [r₁, …, rₙ]                     a whole cause is a list
x ::= ok v | fail c                   outcome (Exit)
```

`d` is the defect value: in JS an `Error` or any value, in Go text with Flick frames. `f` is a fiber number or `·`. Operations on `c` are in §2.

### 1.2 Fibers

A fiber is `⟨e, m, q, π, θ⟩`:

- `e` — a core term with runtime forms (§1.4);
- `m ∈ ℕ` — the mask depth; the fiber is interruptible when `m = 0`;
- `q` — the interruption request: `·` or the number of the fiber that sent it. The request is sticky and never cleared (Go `fiberState.requested`, Effect `interruptUnsafe`);
- `π` — the current owner (`CurrentOwner` in JS, `Fiber.owner` in Go): where acquisitions and foreign calls go;
- `θ` — the service environment at start, inherited from whoever started the fiber.

`Φ` is the pool: a finite map from fiber numbers to fibers, and for finished fibers to outcomes `done x`.

### 1.3 Owners

`Λ(π) = ⟨st, K, W, P, H, F, x⟩`:

- `st ∈ {open, closing, finalizing, closed}`;
- `K` — children (`Fiber.spawn`);
- `W` — watchers: they wait for children in order to call the supervisor;
- `P` — foreign Promises not yet settled;
- `H` — closing hooks, in order of addition: stopping groups, detaching listeners;
- `F` — finalizers, a stack: the last added runs first;
- `x` — the outcome of the body, known from the start of closing.

`regions.md` §3.1 distinguishes two states: `live` is `open` and `closing`, `closed` is `finalizing` and `closed`.

| | JS | Go |
| - | - | - |
| `st = open` | `Owner.open` (`owner.ts:18`) | `Owner.open` (`owner.go:13`) |
| `K`, `W` | `children`, `watchers` (`owner.ts:20-21`) | `children`, `watchers`; a `WaitGroup` counts both children and supervisors |
| `P` | `pending`, `track` (`owner.ts:31-35`) | `pending`, `track` (`owner.go:22-26`), only after a foreign call is interrupted |
| `H` | `closing` (`owner.ts:25`) | `hooks`, `onClose` |
| `F` | Effect's `Scope.Closeable`: a `Map` in insertion order, closed from the end (`FX/internal/effect.ts:4050-4073`) | the `finalizers` slice |
| Root | the runner's owner (`entry.ts`), `reportDefect` (`owner.ts:38-43`) | `Main`, `reported` (`concurrency.go:102-105`) |

### 1.4 Configuration and forms

A configuration is `⟨Φ, Λ, μ, M, D⟩`:

- `μ` — memory, as in `core.md`;
- `M` — layer memo maps (§3.5);
- `D` — the root's defect sink: causes of supervisor failures.

Runtime forms extend `core.md` §3.1 and `regions.md` §3.1:

```text
v ::= … | fib k                       child handle, of type Fiber ε τ
e ::= … | inside π e                  body of a scope (regions.md)
      | mask e | unmask m e           mask +1 / restore depth m
      | onexit e h                    after e run h under a mask; the outcome of e is kept
      | ext p                         wait for the foreign Promise p
      | close π                       closing in progress (§3.3)
      | fail c                        failure with cause c
```

`fail c` generalizes the core's `raise v` and `die`: `raise v` is `fail [Fail v]`, `die` is `fail [Die d]`. A failure propagates through frames: `F[fail c] → fail c`, except for `try`, `onexit`, `inside`, `mask` and `unmask`, which have their own rules.

## 2. The algebra of causes

### 2.1 Two operations

Effect 4.1.2 stores a cause as a flat list `reasons`; it has no Sequential/Parallel tree (spec/05 "A complex Cause"). Lists are joined in two ways:

```text
c ⧺ c'      concatenation        FX causeFromReasons over the concatenation: scope finalizers
c ⊕ c'      nub (c ⧺ c')         FX causeCombine (internal/core.ts:308-324)
∅ = []
```

`nub` keeps the first occurrence of each reason up to `≈` (`dedupeReasons`, `internal/core.ts:284-305`). If one argument is empty, `causeCombine` returns the other without normalizing it.

Some joins elect defects (`elect` of §2.3): a late failure of `forEachConcurrent` gives `elect(c ⊕ c')`, a finalizer of `onExit`, `ensuring` or `acquireRelease` that fails after a failure gives `elect(c ⊕ c')`, and `raceAll`/`raceFirst` with every branch failed give `elect(c₁ ⧺ … ⧺ cₙ)`. `Cause.combine` and the finalizers of a scope among themselves do not elect.

In Effect, `≈` is `Equal.equals` together with `Hash.hash`; annotations take part. For Flick values:

- `Fail v ≈ Fail v'` — structural equality of errors. ADT and record values are plain objects, and Effect compares them element by element: the `same 1` output of the [`parallel` probe](#go-parallel-keeps-only-the-first-failure);
- `Die d ≈ Die d'` — `Equal.equals` of the defects. A string (`Cause.die`, `Effect.die`, `Exit.die`) and a value (a typed error turned into a defect) compare by value. A thrown `Error` (a panic, `Stream.die`) compares by object identity: two panics with the same text are different. Go marks a thrown defect with the `thrown` flag ([defects by value](#go-two-defects-with-the-same-string-are-two-reasons));
- `Int f ≈ Int f'` — the same fiber and the same annotations.

Go stores a binary `Sequential`/`Parallel` tree (`cause.go:11-50`) but reads it only through `reasons()`, a left-to-right traversal (`cause.go:53-61`). The kind of a node changes nothing. Go applies `nub` as well ([combining causes in Go](#go-combining-causes-without-nub)).

### 2.2 Laws

The normal form is a list without repetitions. Every constructor except `Cause.from_reasons` produces it. On normal forms:

```text
(⊕-Unit)    ∅ ⊕ c = c = c ⊕ ∅
(⊕-Assoc)   (a ⊕ b) ⊕ c = a ⊕ (b ⊕ c)
(⊕-Idem)    c ⊕ c = c,  a ⊕ b ⊕ a = a ⊕ b
(⊕-Head)    head(a ⊕ b) = head(a) if a contains a Fail; otherwise head(b)
(Seq=Par)   Sequential and Parallel nodes are indistinguishable: both are ⊕
(⧺-Free)    ⧺ is the free monoid with unit ∅
```

`⊕` is not commutative, and the order matters: `head` depends on it, and so does recovery.

`tests/run/runtime/cause_laws.fl` checks the laws, `recover` of §2.3, and that `Cause.pretty` and `Cause.pretty_errors` depend only on the list of reasons: `Arbitrary.check` properties over causes that Effect's `Arbitrary` builds from a seed identically on Node and Go. (Seq=Par) is unreachable from the Flick surface (`Cause.combine` is always `Sequential`); `cause_test.go` checks it.

(⊕-Assoc) holds because `nub(nub(a ⧺ b) ⧺ c) = nub(a ⧺ b ⧺ c) = nub(a ⧺ nub(b ⧺ c))`. On a list with repetitions, `∅ ⊕ c` returns `c` as is, and the first non-empty `⊕` normalizes.

Definitions:

- `head(c)` — the first `Fail v` in `c` (`Cause.find_error_option`, `Cause.find_fail`);
- `primary(c)` — what recovery selects: `head(c)` if `c` has no `Die`, otherwise nothing (`cause.ts:28-29`, Go `Try`).

### 2.3 Recovery and `try`

```text
recover(c, ā) = fail (dies c)          c has a Die and a Fail
                fail c                 c has a Die and no Fail
                handle v with ā        head(c) = Fail v and v matches one of the branches ā
                fail c                 otherwise: the whole cause propagates

try (fail c) with ā   →   recover(c, ā)
```

`dies c` is `c` with every `Fail v` replaced by `Die v`. Both runtimes keep the value: JS `failsToDies` (`cause.ts:19-22`), Go `failsToDies` through `dieOf` ([typed errors as defects](#go-a-typed-error-turned-into-a-defect-loses-its-value)).

`elect c` is `dies c` if `c` has a `Die` and a `Fail`, otherwise `c` (`causeElectDefects`, `FX/internal/effect.ts:184-191`; Effect also spares its internal `Done` signals, which Flick causes do not hold). So `elect c = dies c` when `c` has a `Die`, and the first two lines of `recover` are `fail (elect c)`: Effect's `findRecoverableError` (199-200). `elect` keeps the head or removes it. Lean: `elect`, `recover_die` (`formal/Formal/RT/Recover.lean`).

A branch sees only `head(c)`. Secondary `Fail`s of other types stay in `c` if no branch matched, and disappear with `c` if one did. The "Outcomes" table of `reference/runtime.md` lists special cases of `recover`. JS: `tryWith` (`cause.ts:39-54`) over `Effect.catchCauseFilter`; Go: `Try` (`cause.go:105-125`).

### 2.4 The typed head

The type `Failure ε` (and `Exit ε τ`) promises less than the whole list: only the head.

**Invariant I_ε.** If the cause of a `Failure ε` value contains a `Fail`, then `head(c) = Fail v` and `· ⊢ v : Errors ε`. Secondary `Fail`s have no type.

I_ε is preserved by:

- `raise`, `Cause.fail`, `from_result` — a list of one reason;
- `⊕` and `⧺` at type `Failure (ε + ε')` — by (⊕-Head); `elect` keeps the head or removes it;
- `try` with residual `ε'` (the core's (Try) rule): a head that matched no branch belongs to `ε'`. This is R5;
- closing a scope: `x ⊕ c_H ⊕ c_F`, and finalizers and hooks have no typed errors (`regions.md`, [owner retention requires Outlives](regions.md#owner-retention-requires-outlives));
- `parallel`: the head belongs to the first branch that failed;
- `Failure.rethrow`, `Exit.failure`, `Effect.exit` — the same cause.

I_ε is broken by operations that give a secondary `Fail` the type `ε` ([secondary `Fail`](#a-secondary-fail-gets-the-type-of-the-residual)):

- `Cause.map`, `Exit.map_error`, `Exit.map_both` call the function on every `Fail`;
- `Cause.reasons` returns each `Fail` as `Reason ε`, and `Cause.from_reasons` after a permutation makes a secondary one the head.

spec/05 forbids both: "it is impossible to attribute the residual e to every secondary Fail or to run a callback for it under a false type".

### 2.5 Rendering and parity

```text
pretty(c)  = lines for Fail (describe v) and Die (text of d) in the order of c;
             "InterruptError: All fibers interrupted without error" if there are no lines
report(c)  = "error: main was interrupted" if c has only Int;
             "error: m" for the first Fail with AppError {message = m}, if c has no Die;
             otherwise pretty(c)
```

JS: `entry.ts:44-76` (`pretty`, `conclude`). Go: `cause.go:159-189` (`pretty`, `report`), `platform.go:145-183`.

Parity (`tests/run/portable.txt`) compares `stdout`, `stderr` and the exit code: the `report` of the main cause and everything the program prints from `Cause.*`. Hence what must agree:

- the order and number of reasons after `⊕` and `⧺`. So Go must repeat `nub` where JS goes through `causeCombine`, must not repeat it where JS goes through `causeFromReasons`, and must elect where Effect elects (§2.1) ([`parallel` in Go](#go-parallel-keeps-only-the-first-failure), [combining causes in Go](#go-combining-causes-without-nub));
- `Int`s are not visible in `pretty`, but are visible in `Cause.reasons`, `Cause.interruptors`, `Cause.has_interrupts`.

## 3. Steps

### 3.1 Nondeterminism

```text
⟨Φ[f ↦ φ], Λ, μ, M, D⟩  →  ⟨Φ[f ↦ φ'] ∪ Φ_new, Λ', μ', M', D'⟩
```

This is a step of one fiber `f`, chosen arbitrarily among the ready ones, a closing step, or an external event: a Promise settling, a timer firing. The order is unconstrained beyond the side conditions of the rules. An admissible interleaving is any sequence of such steps; the properties of §4 are claimed for all of them.

A core step lifts: `⟨μ, e⟩ → ⟨μ', e'⟩` from `core.md` gives a step of the fiber with `e`. `Ref.*` is one step (§3.7).

JS runs fibers on one thread and switches on `yield*` and on the operation budget. Go gives each fiber a goroutine. Both implementations are special cases of the model's interleavings.

### 3.2 Interruption and mask

```text
(Int-Req)      interrupt (fib k) in f  →  q(k) := f if q(k) = ·;  f waits for done(k)
(Int-Deliver)  ⟨E[e], 0, q⟩            →  ⟨E[fail [Int q]], 0, q⟩       q ≠ ·, at an interruption point
(Mask)         mask e   at depth m     →  unmask m e   at depth m + 1
(Unmask)       unmask m v              →  v   at depth m; if m = 0 and q ≠ ·, (Int-Deliver) at once
(Unmask-Fail)  unmask m (fail c)       →  fail c   at depth m
(OnExit)       onexit w h              →  mask h, then w; a failure of h with c_h gives fail c_h after a value, fail elect(c ⊕ c_h) after fail c
```

Here `w` is a value or `fail c`, `cause(v) = ∅`, `cause(fail c) = c`.

Interruption points:

- JS — any Effect step boundary with `m = 0` (`FX/internal/effect.ts:4644-4650`);
- Go — `Check` (`fiber.go:70-74`): every `Run`, the head of the effect loop, and blocking waits (`wait`, `sleep`, `all`, `race`, semaphore permits, foreign calls).

The model takes the JS points. Go has fewer, so Go admits a subset of the interleavings. The exception is (Unmask): leaving a mask with a pending request must interrupt at once ([mask exit in Go](#go-leaving-a-mask-does-not-deliver-a-pending-interruption)).

In both runtimes the following run under a mask:

- the acquisition of `acquire_release` together with the registration of the release;
- `bracket`: acquisition and release;
- finalizers, hooks, the whole closing of an owner;
- the watcher and the supervisor;
- `on_exit` and `ensuring` handlers.

### 3.3 The owner

```text
(Scope)         scope s { e } in f at depth m
                →  mask (inside π (unmask m e[s := π][s := own π]))
                   π fresh, Λ(π) = ⟨open, ∅, ∅, ∅, [], [], ·⟩
(Spawn)         spawn (own π) g t in f,  st(π) = open
                →  fib k;  Φ += k ↦ ⟨await t, 0, ·, π, θ_f⟩,  w ↦ ⟨watch π k g, 1, ·, π, θ_f⟩;
                   K(π) += k,  W(π) += w
(Spawn-Closed)  spawn (own π) g t,  st(π) ≠ open  →  fail [Die "the owner is closing"]
(Watch)         watch π k g,  Φ(k) = done x  →  K(π) -= k;  if x = fail c then g (Failure c sd),
                   where sd = (st(π) ≠ open and c is only Int);  a failure of g with c_g: D += c_g;  W(π) -= w
(Join)          join (fib k),  Φ(k) = done x  →  v if x = ok v,  fail c if x = fail c
(Fin-Add)       add_finalizer π t,  st(π) ∈ {open, closing}  →  F(π) := t :: F(π)
(Fin-Now)       add_finalizer π t,  st(π) ∈ {finalizing, closed}  →  mask (t x(π))
(Acquire)       acquire_release π a rel  →  mask (let v = a in add_finalizer π (rel v); v)
(Ext)           ext p in f  →  P(π_f) += p;  f waits for p to settle;
                   (Int-Deliver) fails f at once, and p stays in P(π_f)
(Ext-Settle)    p settled  →  P -= p;  the waiting fiber, if it still waits, gets the value or the failure
```

Closing is a sequence of steps without choice, in a fixed order:

```text
(C0)  inside π w  →  close π;  x(π) := w;  st(π) := closing          w is a value or fail c
(C1)  for each h ∈ H(π) in order, including those added during C1:  h ();  c_H := c_H ⊕ the cause of h's failure
(C2)  q(k) := f for each k ∈ K(π);  wait until K(π) = ∅
(C3)  wait until W(π) = ∅
(C4)  wait until P(π) = ∅; repeat while settling calls add new ones
(C5)  st(π) := finalizing;  for t ∈ F(π) from the top:  mask (t x);  c_F := c_F ⧺ the cause of t's failure
(C6)  st(π) := closed;  the result is w if c_H ⊕ c_F = ∅, otherwise fail (cause(w) ⊕ c_H ⊕ c_F)
```

JS — `close` (`owner.ts:54-70`), Go — `closeWithCause` (`owner.go:111-130`):

| Phase | JS | Go |
| - | - | - |
| C0 | `owner.open = false` | `o.open = false` under `o.mu` |
| C1 | a loop over `owner.closing`; again after C3 ([hooks added after C1](#a-closing-hook-added-after-c1-does-not-run)) | `hooks`, run in order; again after C3 |
| C2 | `Fiber.interruptAll([...owner.children])`: interrupts and waits (`FX/internal/effect.ts:971-982`) | `interrupt` over a snapshot of the children; the wait is in C3 |
| C3 | a loop of `Fiber.await` over `watchers` | `watchers.Wait()` |
| C4 | `settle` (`owner.ts:45-51`), then the hooks added meanwhile ([hooks added during C4](#a-hook-added-during-c4-does-not-run)) | `settle` (`owner.go:29-42`), then the same |
| C5 | `Scope.close(owner.resources, exit)`: Effect sets `Closed` before the finalizers, runs them sequentially from the end and collects the outcomes with `exitAsVoidAll`, that is, `⧺` | a loop from the end, `Sequential`; finalizer admission ends here, with `finalizing` ([late finalizers in Go](#go-a-finalizer-added-during-closing-runs-at-once)) |
| C6 | `Cause.combine`, that is, `⊕` | `Sequential`, which is `⊕` with `nub` |

Why `F` is separate from `K` (spec/05 "Fiber owner and release order"): a resource acquired after a child started is released after the child ends. A single LIFO list of children and resources cannot give this.

The body of a `scope` is interruptible (`restore` in `withOwner`, `owner.ts:85-96`); the closing is not. In Go the closing has no interruption points.

### 3.4 Structural combinators

The branches of a combinator are owned by the combinator, not by `K(π)`. Their acquisitions go into the caller's `π`: "The operation does not close an external Scope" (spec/05).

```text
(Par)       parallel* t₁ … tₙ: branches kᵢ, at most limit at a time
            all ok vᵢ       →  (v₁, …, vₙ) in input order
            first fail c₁   →  start no new ones; q(kⱼ) := f for the live ones; wait for all;
                               fail elect(… elect(c₁ ⊕ c₂') ⊕ …), where cⱼ' are the causes without Int of the branches that failed later
(Par-Res)   parallel_results: a branch's Fail → Err (head); a Die or Int stops the group, as in (Par)
(Par-Exit)  parallel_exits: the outcome of each branch; external interruption — as in (Par)
(Race)      race_first a b: the first finished branch k with any outcome; q(other) := f; wait for it; the outcome of k
            race_success a b: the first ok; if both failed — fail elect(c₁ ⧺ c₂) in order of finishing
(Timeout)   timeout ms t = race_first t (sleep ms; fail [Fail TimeoutError]);
            timeout_option ms t — the same with None instead of TimeoutError
```

The outcomes of losers do not enter the result. This is the spec/05 "Race and timeout" contract, not a loss, and R4 excludes them. A loser is interrupted and awaited before the return: its finalizers have run.

- JS: `concurrency.ts:10-45` over `Effect.all`, that is, `forEachConcurrent` (`FX/internal/effect.ts:5135-5290`; late causes without `Int` are joined with `elect(⊕)` at 5209-5217). `race.ts` is over `raceAll`/`raceFirst` (`FX/internal/effect.ts:1674-1760`, `elect(⧺)` at 1701-1703); `timeout` is over `raceFirst` (3961-3981).
- Go: `all` (`concurrency.go:159-207`), `race` (248-275), `timeoutAfter` (282-301).

In Effect a synchronous winner can win before the second branch starts; Go starts both. The model admits both behaviours: starting the second branch is a step that may not get to happen.

### 3.5 Layers and `provide`

`provide L { e }` is a lexical owner (`reference/runtime.md` "Layers"). `Layer` is a library, not a form of the core: it lowers to `Layer.run L (fun () -> e)` (`core.md` §1.4), and this section is what that library does. A known layer of `Layer.succeed` and `Layer.merge` means a chain of handlers, the first binding of an effect the innermost (`chain`, `layer_typ`, `layer_answers` in `formal/Formal/RecordHandler.lean`); what a chain does not say, the build, its scope and the memo map, is below.

```text
(Provide)  provide L e in f  →  owner πₚ by (Scope);  m := fork(M_f) or a new map;
           θ := build(m, πₚ, L);  with θ e inside πₚ;  closing πₚ — by §3.3

build(m, π, L) for a memoizable L (Layer.effect, sync, suspend, from_build_memo):
           L ∉ m:  m[L] := ⟨building, σ, n = 0⟩ with a new scope σ;  build L in σ;  m[L] := ⟨θ_L, σ, n⟩
           L ∈ m:  wait for the build to finish
           then n += 1 and F(π) += "n -= 1; if n = 0 close σ and remove L from m"
build for composite layers (merge, provide_, flat_map, …) builds the parts; fresh, or_die, constants — without the map
```

- The map key is the identity of the layer value (`FX/Layer.ts:418-469`; Go `memoized`, `layer.go:56-62`).
- `fork` gives a map that reads its ancestors. A nested `provide` of the same layer takes the outer build (`FX/Layer.ts:440, 781`; Go `currentMemo`, `layer.go:181-186`). Sibling `provide`s build anew ([nested `provide`s](#nested-provides-share-a-build)).
- The release of a build is an observer finalizer in the `F` of each consuming owner; the last one closes `σ`. So a layer is released in the C5 of its last consumer, after that consumer's children and foreign Promises.

JS: `layer.ts:9-15` (`Layer.buildWithScope(layer, owner.resources)`), the observer counter — `FX/Layer.ts:253-275`. Go: `Provide` (`layer.go:198-203`), `getOrBuild` (135-166), the counter (115-131).

### 3.6 Fiber groups

`FiberSet.make` in region `π` registers a stop hook in `H(π)` (JS `stdlib/fibers.ts:66-88`). `FiberSet.run`:

- group open → a child of the group; it acquires in `π` but is not in `K(π)`;
- group closed → an interrupted fiber; the body does not run (`fibers.ts:87`).

The hook closes the group, interrupts its children and waits for them. Since the hook runs in C1, before C4 and before the finalizers, the acquisitions of the group's children are released after those children end. Go registers the stop the same way, as an `onClose` hook ([fiber groups in Go](#go-a-fiber-group-is-stopped-by-a-finalizer)).

`join` on a group waits for the first failure. A failure after the group closed, without a `join`, goes nowhere: the group is its own supervisor (`FiberSet.fl`).

### 3.7 `Ref`

```text
Ref.modify ℓ g  →  (b, v') := g μ(ℓ);  μ(ℓ) := v';  b          one step
```

`g` is a pure arrow (`Ref.fl`). By the core's T2(a), its evaluation consists only of `→ₚ` steps and possibly `die`, with no `Ref`, `await` or services. So the step is atomic: in JS evaluating `g` does not yield the thread, in Go it runs under the cell's mutex (`sync.go:30-37`). A `die` in `g` leaves the cell unchanged. The other `Ref` operations wrap `modify` (`Ref.fl:68-160`).

### 3.8 The root and the defect sink

The runner runs `main` in the root owner `π₀` (JS `entry.ts`, Go `Main`). A supervisor failure goes to the root's `D` (`reportDefect`, Go `reported`), and the runner ends the program with code 1 (`reference/runtime.md` "Entry runner"). A signal is an `interrupt` of the main fiber followed by closing `π₀` by §3.3.

A `flick test` test is its own root: the body and the closing run in `ownedExits` (`owner.ts:108-119`), in Go in `testing.go`. The test reads its own `D` and fails on a supervisor defect ([supervisor defects in tests](#a-supervisor-defect-is-lost-in-flick-test)).

## 4. Properties

Labels:

- **[JS]** — held by the JS runtime's own code;
- **[E]** — held by Effect 4.1.2; this is a hypothesis about the library, its item in G7;
- **[Go]** — held by the Go runtime's own code.

**R1 (exactly once).** Suppose `acquire_release π a rel` returned `v`, or `add_finalizer π t` ran. Then for any outcome of the body of `π` — a value, `fail c` with any reasons, an interruption — `rel v` (`t`) starts exactly once, and before closing `π` ends.

- JS:
  - [E] `Scope.close` moves to `Closed` before the finalizers and does nothing when repeated (G7a); a finalizer added to a closed scope runs at once; acquisition and registration are under one mask (G7b);
  - [JS] closing an owner is masked (`withOwner`); lease registration runs under the mask ([leases](#js-leases-register-their-release-outside-the-mask)).
- Go: [Go] the finalizer slice is taken and cleared under `o.mu` (`owner.go:118-119`), a repeated close is empty; `add` after C5 has started runs the finalizer at once, with the scope's outcome ([late finalizers in Go](#go-a-finalizer-added-during-closing-runs-at-once)).
- Exception: the process is killed (`os.Exit`, a fatal Go error — G8).

**R2 (order).** The finalizers of `F(π)` run in the reverse order of addition. The first starts only after all of `K(π)` and `W(π)` have finished, all children of the groups of `π` have finished, and all of `P(π)` have settled.

- JS: [JS] the phases of `close`, group hooks in C1; [E] LIFO and the default sequential strategy of `Scope` (G7a), `interruptAll` waits for each (G7d).
- Go: [Go] the phases of `closeWithCause`; finalizers are admitted until C5 ([late finalizers in Go](#go-a-finalizer-added-during-closing-runs-at-once)), groups stop in C1 ([fiber groups in Go](#go-a-fiber-group-is-stopped-by-a-finalizer)).

**R3 (children do not outlive their owner).** If closing `π` has finished (C6), then no fiber of `K(π)` or `W(π)`, no child of a group of `π`, and no branch of a combinator started in `π` can take a step.

- JS: [JS] C2–C3; an interruption cannot split the registration of a child and its watcher ([`fiberSpawn`](#js-fiberspawn-is-interruptible-midway)). [E] the branches of `Effect.all` and `raceAll` are awaited, including when the parent is interrupted (G7d).
- Go: [Go] a `WaitGroup` over children and supervisors, `stop` after each branch; children of groups through the C1 hook ([fiber groups in Go](#go-a-fiber-group-is-stopped-by-a-finalizer)).
- Out of scope: foreign work after the call is interrupted. C4 waits for it only as a Promise; what foreign code does is G9.

**R4 (a child's defect is not lost).** Suppose a child `k ∈ K(π)` finished with `fail c`, where `c` has a `Die` or a `Fail`. Then `g (Failure c sd)` was called, and if `g` failed with `c_g`, then `c_g ∈ D` of the root, and the root sees it: the runner as exit code 1, a test as a test failure. For `parallel*` the result includes the causes without `Int` of all branches that failed before the group ended (`elect(⊕)`); a `Fail` may come as a `Die`.

- JS: [JS] the watcher (`owner.ts:156-168`) and `reportDefect`; every child has a watcher ([`fiberSpawn`](#js-fiberspawn-is-interruptible-midway)); tests ([supervisor defects in tests](#a-supervisor-defect-is-lost-in-flick-test)). [E] `forEachConcurrent` (G7d).
- Go: [Go] the `fork` callback in `FiberSpawn`; `all` ([`parallel` in Go](#go-parallel-keeps-only-the-first-failure)); tests and `Events.listen` ([supervisor defects in tests](#a-supervisor-defect-is-lost-in-flick-test), [listener supervisors](#go-a-failing-eventslisten-supervisor-is-dropped)).
- Excluded by contract: the outcomes of `race`/`timeout` losers; failures of group children after the group closed; `resource_auto` ([its refresh fiber](#the-refresh-fiber-of-resource_auto)).

**R5 (a covered error does not come back as the head).** Suppose `try e with ā` is typed with residual `ε'` (the core's (Try) rule) and its evaluation ended with `fail c`. Then `head(c)`, if any, is `Fail v` with `v : Errors ε'`. In particular, a branch covering `E` guarantees `head(c) ≠ Fail (E …)`. Secondary `Fail`s with `E` errors may remain in `c`, but no safe operation returns them as `Errors ε'` (I_ε, §2.4).

- JS: [JS] `tryWith` (`cause.ts:39-54`) over [E] `catchCauseFilter`; I_ε ([secondary `Fail`](#a-secondary-fail-gets-the-type-of-the-residual)).
- Go: [Go] `Try` (`cause.go:105-125`); I_ε (same).
- The stronger reading "its `fail` is absent from the channel of expected errors" holds only for the head. The cause `Fail B, Fail A` with a branch for `A` propagates whole: this is the spec/05 contract.

R6 is a property of the library: no rule of the core builds or memoizes a layer, and λRT states it of the protocol below.

**R6 (layer).** In one run of `provide L` — together with nested `provide`s that share its memo map — each memoizable layer with a given identity is built at most once. Its scope `σ` closes after all consuming owners that got services from it through this map have closed.

- JS: [E] the MemoMap by identity and the observer counter (G7e); [JS] the build goes into the `owner.resources` of the `provide` owner.
- Go: [Go] `memoMap.getOrBuild`, `observers`.
- A consumer that got the services not through the map but through the environment — a child of an outer owner started from inside `provide` ([the `provide` escape](#a-provide-service-outlives-the-release-of-its-layer)) — would use the service after `σ` closed; neither runtime prevents this. R6 rests on the premise "a fiber takes a service from the `θ` of a `provide` owner `πₚ` only before `πₚ` closes": a request `ask_S` answered by the environment is answered by a `provide` frame whose owner has not yet reached (C5). Typing gives it: `regions.md` §2.7 (`Fiber.Provided`, `FLICK-G003`), point 3 of T4. Typing gives nothing stronger: a child inherits the whole environment of the spawn site, and `Provided r ϱ` concerns only the services of its row `r`, so a fiber may keep running after a `provide` closes whose services it does not take ([the premise is per request](#the-provide-liveness-premise-is-stronger-than-typing-gives)). In Lean this is hypothesis G11 (`G11`); the core model proves it as `T4_3` (`formal/Formal/T43Main.lean`).

**R7 (`Ref.modify` is linearizable).** Each execution of `Ref.modify ℓ g` has a linearization point inside its interval, and the history of operations on `ℓ` is equivalent to a sequential one.

- JS: [JS] one thread, `g` pure and synchronous (`builtins.ts:42-60`); the cell is its own class, not `Effect.Ref`.
- Go: [Go] the cell's mutex is held for the whole `modify` (`sync.go:30-37`).

**R8 (cause laws).** The laws of §2.2, `recover` of §2.3, and that `pretty`/`report` of §2.5 depend only on the list of reasons. JS–Go parity rests on both runtimes building the same list:

- `⊕` where Effect calls `causeCombine`: closing a scope (C6), `Cause.combine`;
- `elect(⊕)` where it calls `causeElectDefects` over `causeCombine`: `parallel` (each late cause), `bracket`, `ensuring`, `on_exit` (a finalizer failing after a failure);
- `⧺` where it calls `causeFromReasons`: scope finalizers among themselves;
- `elect(⧺)`: `race_success` with every branch failed.

Effect 4.1.2 has no tree, so the Sequential/Parallel distinction reduces to a single `⊕` ("a tree of sequential/parallel causes is not part of the contract", spec/05).

- JS: [E] `FX/internal/core.ts:280-347`, `FX/internal/effect.ts:184-191` (G7f).
- Go: [Go] `cause.go`: `nub` with `Equals` for `Fail`, the fiber number for `Int`, and value equality for `Die` except thrown defects, which compare by identity ([combining causes in Go](#go-combining-causes-without-nub), [defects by value](#go-two-defects-with-the-same-string-are-two-reasons)).

## 5. Hypotheses

G1–G6 hold (`core.md`, `regions.md`). Added:

- **G7 (Effect 4.1.2).** The properties labelled [E]:
  - a) `Scope.close` is idempotent and moves to `Closed` before the finalizers; with the default strategy the finalizers run sequentially from the end; adding to a closed scope runs the finalizer at once, with the scope's outcome (`FX/internal/effect.ts:4006-4102, 4152-4161`);
  - b) `acquireRelease` registers the release under the same mask as the acquisition (4213-4231); `onExit` handlers are masked (4250-4255);
  - c) leaving a mask with a pending request interrupts at once (4644-4650);
  - d) `Fiber.interruptAll` interrupts and waits for each fiber (971-982). `forEachConcurrent` interrupts and waits for the branches and joins late causes without `Int` with `elect(⊕)` (5209-5252). `raceAll` and `raceFirst` interrupt and wait for the losers in a masked `onExit`, and if all failed — `elect(⧺)` (1674-1760);
  - e) MemoMap: the key is the layer's identity, observers are counted, the last one closes the scope; a nested map reads its parent (`FX/Layer.ts:253-275, 418-469, 781`);
  - f) `causeCombine`, `dedupeReasons`, `causeFromReasons` — as in §2.1; `causeElectDefects` is `elect` of §2.3 (`FX/internal/effect.ts:184-191`), and `onExit` joins a failing finalizer with `elect(⊕)` after a failure (4042-4048);
  - g) an interruption that skipped a handler removes the `Fail`s from the cause and adds an `Int` (`FX/internal/core.ts:613-624`). Go repeats this in every recovery handler ([skipped handlers](#an-interruption-that-skips-a-handler-erases-fail-effect)). Types are unaffected: the head stays from `ε` or disappears.

  The runtime tests check the hypothesis; the runtime trace (`FLICK_TRACE`, `just stress`) and the cause-law properties (`cause_laws.fl`) exercise it on real runs.
- **G8 (Go).** The Go memory model: unlocking a mutex and closing a channel give happens-before. `recover` catches only non-fatal panics. Stack overflow, a race on a `map`, running out of memory and `os.Exit` end the process without finalizers. `runtime.Goexit` in foreign code does not close `done` and breaks G9.
- **G9 (foreign calls).** The Promise of a foreign `async` call settles; otherwise C4 does not finish. Foreign code does not start work that `P` does not track and that holds the region — like G4 for retention. After a call is interrupted, Go closes `done` when the foreign work has finished.
- **G10 (handlers terminate).** Supervisors, finalizers and closing hooks terminate; otherwise C1, C3 or C5 does not finish. This is liveness, not safety: R1–R7 speak about steps that happened.

## 6. Theorem T5

**T5 (the runtime contract).** Let `e₀` be a program (`core.md` §2.6), let G1–G10 hold, and let `⟨{f₀ ↦ e₀}, {π₀ ↦ open}, ∅, ∅, ∅⟩ →* C` under any admissible interleaving (§3.1). Then R1–R7 hold on the path to `C`. The premise of R6 — a service is taken from the environment only of a live `provide` — is given by the typing of the program (`regions.md` §2.7, [the premise is per request](#the-provide-liveness-premise-is-stronger-than-typing-gives)).

Lean: `T5` (`formal/Formal/RT/T5.lean`) — for any core (`Core`, a parameter) and any interleaving: R1–R4 (`R1`–`R4`), R6 (`R6`), R7 (`R7`) in every reachable configuration (`Reach`), and R5 (`R5`). The hypotheses are parameters: G7(a)–(g) except (c), G8 and G9 are fields of `Hyp` (`formal/Formal/RT/Hyp.lean`); the premise of R6 is `G11`: a request for a service from the environment is answered by the `provide` of a live owner. The typing rule of `regions.md` §2.7, point 3 of T4, gives it; in the core model it is `T4_3`. G7(c) is not needed: the model has no mask, and interruption is delivered at any point of user code, which is more interleavings than §3.2 allows. G10 and the liveness parts of G8 and G9 are not part of safety. The model's simplifications are listed in `formal/README.md`.

R8 is not a statement about interleavings but a property of the functions `⊕`, `⧺`, `recover`, `pretty` and `report`. `tests/run/runtime/cause_laws.fl` checks it on both runtimes. Lean: `R8` (`formal/Formal/RT/Cause.lean`).

Invariants of reachable configurations:

- **I1 (states are monotone).** `st(π)` goes `open → closing → finalizing → closed` and never goes back. The transition out of `open` is only (C0); out of `closing` only (C5), after (C1)–(C4).
- **I2 (admission).** `K(π)` and `W(π)` grow only while `st = open` (Spawn); `H(π)` and `F(π)` only while `st ∈ {open, closing}`.
- **I3 (registration is indivisible).** If a child `k` is started by (Spawn), then in the same atomic group of steps `k ∈ K(π)` and its watcher `w ∈ W(π)`. In JS through the mask of `fiberSpawn` ([`fiberSpawn`](#js-fiberspawn-is-interruptible-midway)), in Go under `o.mu`.
- **I4 (who acquires in `π`).** The steps (Fin-Add), (Acquire) and (Ext) with `π_f = π` are taken only by a fiber whose current owner is `π`: the body of `π`, `k ∈ K(π)`, `w ∈ W(π)`, a child of a group of `π`, a branch of a combinator started by such a fiber, a finalizer or hook of `π`. For the supervisor this requires the watcher to run in the spawn owner ([supervisor owner in Go](#go-the-supervisor-acquires-into-the-scope-of-the-spawner)); for foreign regions it requires G6 (`within`).
- **I5 (head).** Every `Failure ε` and `Exit ε τ` value satisfies I_ε ([secondary `Fail`](#a-secondary-fail-gets-the-type-of-the-residual)).
- **I6 (map).** For `m[L] = ⟨θ_L, σ, n⟩`, the number `n` equals the number of observer finalizers of `L` in the `F` of owners that have not yet passed (C5); `σ` is closed if and only if `n` has dropped to 0.

Lean (`formal/Formal/RT/`): I1 — `Inv.stJ` and the `Good` conditions of the events `close`, `fins`, `closed`; I2 — `Inv.empty` and `Good` of the events `spawn`, `fin`, `gkid`, `branch`; I3 — `Inv.spawnJ`; all together — `Inv`, preserved by every step (`Inv.step`, `Inv.reach`). I4 is not modelled in Lean: an action names its owner by number, and the model checks only that owner's state; R1–R4 do without I4 (see R3).

Sketches:

- **R1.**
  - By I2 a finalizer enters `F(π)` while `st ∈ {open, closing}`, or runs at once by (Fin-Now).
  - (C5) takes the whole `F(π)` and runs each element once; by I1, (C5) happens once.
  - (C0) is reached for any outcome of the body: (C0) applies to any `w`, and interrupting the body by (Int-Deliver) gives `fail [Int …]`, which is also a `w`.
  - Closing is masked, so (C5) is not interrupted. There is no interruption point between acquisition and registration: (Acquire) is under a mask.
  - Lean: `R1`, `Inv.r1` (`formal/Formal/RT/Props.lean`). Without G7(a) and G7(b) the model has steps that break it: `Step.addDrop`, `Step.acqDrop`, `Step.acqLost`.
- **R2.**
  - (C5) comes after (C1)–(C4) by construction.
  - By I1 and I2 nothing is added to `K(π)` and `W(π)` after (C0), so waiting for emptiness ends exactly when everyone who was there has finished. Children of groups finish in (C1).
  - `F` is a stack (Fin-Add).
  - A finalizer that a still-running child adds during (C1)–(C4) goes into `F` (I2) instead of running at once. Go enforces this with its `finalizing` state ([late finalizers in Go](#go-a-finalizer-added-during-closing-runs-at-once)).
  - Lean: `R2`, `Inv.r2`. Finalizer numbers grow with registration, so "reverse order" is decreasing numbers. The condition on children is proved for everything started in `π`, including combinator branches.
- **R3.**
  - By I3 every child is in `K(π)`. (C2) sends each a request, (C3) waits for children and watchers.
  - The branches of a combinator are awaited by the combinator itself. The combinator is a fiber that `π` waits for: as the body, a child or a watcher.
  - Induction on nesting: a fiber started in `π` belongs to one of these sets (I4).
  - Lean: `R3`, `Inv.r3`. Instead of I4 there is `Guard`: every running fiber is awaited by one of those listed, and a branch by the combinator that the `par` frame holds (G7(d)). Induction on the fiber number is `Inv.noRun` (`formal/Formal/RT/NoRun.lean`).
- **R4.** By I3 every child has a watcher. The watcher is masked and (C3) waits for it, so (Watch) runs before `π` closes. A failure of `g` goes into the root's `D`; the runner reads `D` at the end (code 1), a test after its own closing ([supervisor defects in tests](#a-supervisor-defect-is-lost-in-flick-test)). For `parallel` — (Par).
  - Lean: `R4`, `Inv.r4`, `Inv.r4_closed` (with R3: after `π` closes, every child failure has reached the supervisor), `parResult_keeps` for `parallel` under G7(f).
- **R5.**
  - From the core's (Try): the residual `ε'` contains every error of the body not covered by the branches.
  - `recover` returns `fail c` with the same head only if the head matched no branch, so its type is in `ε'`.
  - If `c` has a `Die`: `dies c` has no `Fail`.
  - The rest is I5: no operation makes a secondary `Fail` the head. `from_reasons` could, but secondary `Fail`s reach it only as `Die`s ([secondary `Fail`](#a-secondary-fail-gets-the-type-of-the-residual)).
  - Lean: `R5` (`formal/Formal/RT/Recover.lean`); I_ε is `Iε`, and its preservation by `⊕`, `⧺`, closing, G7(g) and `elect` is `Iε_comb`, `Iε_append`, `Iε_close`, `Iε_keep` (with `Hyp.g7zh`, `head_elect`).
- **R6.**
  - A memoizable `L` is built only when `L ∉ m`. The entry is inserted before the build — under a lock in Go, on the single thread in JS — so a second request waits.
  - By I6, `σ` closes when the last consuming owner has passed (C5). A consumer closes after its children (R2).
  - A fiber that got `θ` not through the map takes services from it, by `regions.md` §2.7, only before `πₚ` closes (the premise of R6, point 3 of T4, [per request](#the-provide-liveness-premise-is-stronger-than-typing-gives)); it need not finish before `πₚ` closes.
  - Lean: `R6` (`formal/Formal/RT/T5.lean`): `Good6` — a rebuild only after the previous build is released, and a release only after the observers of all consumers; `GoodUse` — a service is not used after its release, under `G11`. I6 is `Inv6.obsIn`: an entry's observer is either in its `obs` (so `n` is their count) or its owner's (C5) has already run it.
- **R7.** By §3.7 the `Ref.modify` step is atomic; the linearization point is the moment of that step.
  - Lean: `R7` (`formal/Formal/RT/RefLin.lean`): each `modify ℓ` read the value of the last `make` or `modify` of the same `ℓ`; memory is a replay of the log. Without G8 the model has `Local.modRead` and `Step.modWrite`, and R7 fails.

### 6.1 Traces

T5 speaks about paths, but the runtimes show only output. So both runtimes write a trace: with `FLICK_TRACE=<file>`, each step of the owner contract is one JSON line. Events and fields are the same in JS and Go (the list is in `runtime-go/flickrt/trace.go`):

- an owner `main`, `scope` or `provide` opened, and its phases C0–C6;
- a child: registration with its watcher, end with an outcome, the supervisor call, the end of the watcher, refusal in a closed owner, the C2 request;
- a finalizer added by Flick (`acquire_release`, `Scope.add_finalizer`, `add_finalizer_exit`): onto the stack or at once (Fin-Now), and its run;
- a foreign call the owner waits for, and its settling;
- the build of a memoizable layer and its release, the end of `main`.

A disabled trace costs one check per point.

The monitor `flick-model/src/rt/monitor.rs` replays a trace and reports the first step λRT does not admit, with the preceding steps of the same owner. It checks I1–I3, R1–R4, and R6 and I6 as far as they are visible without memo maps: each build is released at most once and is released by the end of `main`. The trace does not check I4: it does not record which fiber took a step. The cause-law properties (`cause_laws.fl`) check I5 and R5; R7 is not traced, nor are the C1 hooks.

The projection `flick-model/src/rt/project.rs` is what JS and Go must agree on. Owners are named by their place in the tree: each among its siblings by order of opening. For each: the kind, how far closing got, the outcomes of children, the finalizers and the order they ran in. The projection excludes fiber numbers, the global order of steps, foreign calls (Go puts a call into `P` only after it is interrupted, §1.3) and C2 requests (only a child that has not yet finished receives one).

`flick/tests/e2e.rs` writes the trace of every run and checks each with the monitor. For the cases in `tests/run/runtime/`, the projection of both Go profiles must equal the projection of debug JS. For the other portable cases the projections legitimately differ: each runtime's standard library builds its own layers and adds its own finalizers. Their traces pass the monitor.

Limitations (`shortcut:` in the code):

- JS does not see finalizers that Effect itself adds (layers, channels, servers). So R1 and R2 are checked on Flick finalizers, and Go does not write its internal ones either;
- an owner opened by a fiber outside the `main` tree (an internal Go owner) is a root of the projection (`stdlib/http/lifecycle`);
- interruption delivery is not traced, only the C2 request;
- the JS trace counts each composite layer as a build, since Effect memoizes them too, and the Go trace counts only memoizable layers. So a case with a composite layer is not under `tests/run/runtime/`.

`just stress` runs every case of `tests/run/runtime/` N times on both runtimes with `FLICK_STRESS=<seed>` and checks every trace with the monitor. In Go, interruption points and fiber start yield or sleep at random, and turns are off. In JS, `main` runs under a scheduler that shuffles the tasks of a round within a priority and yields at random. A violation is printed with the case, the runtime, the seed and the command to reproduce it. CI does not run the recipe.

### 6.2 Conformance

Each law below names the cases that exercise it, under `tests/run/runtime/` unless a path says otherwise. They run on the JS target and as a Go executable unless the row says otherwise. "Trace" means the monitor checks the law on the trace of those runs, and on every other run of `tests/run`, under §6.1; "output" means the expected `.stdout` and exit code of the case pin it. `just conformance` runs all of them, and `just stress` runs those under `tests/run/runtime/` in random interleavings under the monitor.

| Law | Cases | Targets | Checked by |
| - | - | - | - |
| R1 | `scope_close`, `scope_releases`, `abort_closes_scopes`, `interrupt_in_finalizer`, `closed_owner` | both | trace, output |
| R1, a discontinued body | `discontinued_body`, `discontinued_fiber` | Go and `effect = "rt"`; JS on Effect waits for `control` arms (`FLICK-P012`) | trace, output |
| R2 | `close_phases`, `scope_owners`, `closing_hooks`, `settling_calls`, `closing_child_fails`, `interrupt_in_finalizer`, `abort_stops_children` | both | trace, output. Group children are not traced: output only |
| R2, a discontinued body | `discontinued_body`, `discontinued_fiber` | Go and `effect = "rt"`; JS on Effect waits | trace, output |
| R3 | `parallel`, `close_phases`, `closing_hooks`, `abort_stops_children`; `discontinued_fiber` on Go | both | trace for children and watchers; output for group children and combinator branches, which are not traced |
| R4 | `close_phases`, `closing_child_fails`, `supervisor_defect` (exit code), `cause_lists` (`parallel`) | both | trace for the call of the supervisor; output for the root seeing its failure and for `parallel` |
| R5 | `cause_laws`, `cause_lists` | both | output (properties) |
| R6 | `shared_layer`, `stdlib/effect/shared_layer_merged` | both | trace for build and release counts; output for at most one build and for the release after consumers. The projection of the merged case is not compared: see below |
| R7 | `ref_linearizable`, `handoff` | both; only Go takes steps at once | output: by its consequences (each ticket once, each fiber's tickets in order, no half-done transfer seen, a cell unchanged by a dying `modify`). Not a check of the history itself |
| R8 | `cause_laws`, `cause_lists`, `closing_causes`; `cause_test.go` for (Seq=Par) | both | output |
| C0 | `closed_owner` | both | trace (`refuse`, the phase), output |
| C1 | `closing_hooks`; a hook added during C1 or C3: `group_in_closing` | both; `group_in_closing` passes on Go and is a [known gap on JS](#js-a-group-made-while-its-owner-closes-runs-nothing) | output: hooks are not traced, only the place of the phase |
| C1, a failing hook; a hook added during C4 | not checked by a case: the hooks a Flick program adds (group stops, listener detaches) do not fail, and only a foreign callback can add one in C4. Unit tests: `runtime/test/scope.test.ts`, `owner_test.go` (a hook added during C4) | | not on a run of a program |
| C2 | `closing_hooks`, `interrupt_in_finalizer` (closing asks every child before it waits for one), `closing_child_fails` | both | trace (`interrupt`, not projected), output |
| C3 | `close_phases`, `closing_child_fails`, `supervisor_defect` | both | trace; the output of these cases can hold without the wait, so the monitor is what checks it |
| C4 | `settling_calls`, `hung_io`, `scopes` | both | trace on JS, where every call is tracked (not projected), output. A call that starts another while settling is not checked |
| C5 | `scope_close`, `closing_causes`, `closed_owner`, `interrupt_in_finalizer` | both | trace, output |
| C6 | `closing_causes`, `closing_child_fails`, `scope_close` | both | output: causes are not traced. `c_H` is not checked, as for C1 |

The law (Unmask) of §3.2 has its own case, `interrupt_after_close`.

`tests/run/known_gaps.txt` lists the cases a target does not pass yet, each with its reason. Their `.stdout` is what the contract asks for; the harness runs them and fails on one that passes, so a case joins its target as soon as the target holds the law.

`tests/run/rt.txt` lists the cases that run on the runtime of `effect = "rt"`, which has no Effect under it, in both profiles and under the monitor (`just conformance-rt`); every other case runs there too, in debug, and one that passes is reported, so that it joins the list. Where it and Go legitimately differ: [the runtime of `effect = "rt"`](#the-runtime-of-effect--rt).

A differential check: the cases of `tests/run/model.txt` also run on `flick-model`. Of this table it runs `abort_closes_scopes` and `discontinued_body`. It cannot run the others: it has no `parallel`, `Deferred`, `Effect.exit`, `Scope.add_finalizer_exit` or memo map of layers, and it runs children when a region closes, so nothing can wait for a child or a child be interrupted there. It rejects `scope_owners`, which acquires into an outer region from an inner one.

## 7. Known divergences

The divergences that come with a probe program were confirmed by the probe: the checker accepts the program, and the output on Node or Go contradicts the property or the other runtime. The `fiberSpawn` and lease divergences were found by reading the code; runtime unit tests that fail without the fix confirm them. The rest are incompleteness, a hypothesis about the library, or imprecise prose.

Every fixed divergence has a test or fixture that fails without the fix. Those resolved as "recorded" are documented here and left as they are.

The fixtures `tests/run/runtime/close_phases.fl`, `cause_lists.fl` and `cause_laws.fl` are in `portable.txt`: their output on Node and Go agrees.

Shared definitions of the probes:

```flick
module App.Main exposing (main)

type Res (s : Region)

open_res : forall s. String ->{Scope s;} Res s
foreign effect open_res from "./resources.mjs" "open"

say : String ->{} Unit
let say line = Platform.stdout_line line

nap : Float ->{Async;} Unit
let nap millis =
  match Duration.millis millis with
  | Ok d -> Effect.sleep d
  | Err _ -> ()

report : forall e. Failure e ->{} Unit
let report f = say ("supervised " ++ show (Cause.has_fails f, Cause.has_dies f, Cause.has_interrupts f))
```

### A secondary `Fail` gets the type of the residual

```flick
type error A = A Float
type error B = B String

body : Unit ->{; A, B} Float
let body () =
  let c : Failure {A, B} = Cause.combine (Cause.fail (A 1)) (Cause.fail (B "b"))
  Failure.rethrow c

handled : Unit ->{; B} Float
let handled () =
  try body () with
  | A _ -> 0

name : Option (Errors {B}) -> String
let name o =
  match o with
  | Some (B s) -> "B " ++ s
  | None -> "none"

main : Unit ->{; AppError} Unit
let main () =
  let ex : Exit {B} Float = Effect.exit (task { handled () })
  match Exit.failure ex with
  | Some f ->
    say ("primary: " ++ name (Failure.primary f))
    let flipped = Cause.from_reasons (List.reverse (Cause.reasons f))
    say ("flipped: " ++ name (Cause.find_error_option flipped))
  | None -> ()
```

- Divergence: the program is accepted, and Node prints `primary: B b`, then `flipped: undefined`. An exhaustive `match` over `Errors {B}` received `A 1` and returned `undefined` as a `String`; the core's T1 and R5 are broken.
  - `Cause.map bang f` with `bang : Errors {B} -> Errors {B}` calls `bang` on `A 1`, and so do `Exit.map_error` and `Exit.map_both`.
  - On Go, `Failure.rethrow` and `Cause.from_reasons` are unavailable (`FLICK-P012`). But `Cause.map`, `Exit.map_error`, `Exit.map_both` and `Cause.reasons` exist and visit every `Fail` (`exit.go` `CauseMap`, `ExitMapError`, `CauseReasons`). There, `parallel` produces the cause `Fail B, Fail A` at type `{B}` once it keeps late failures ([`parallel` in Go](#go-parallel-keeps-only-the-first-failure)).
- Cause: the bindings mirror Effect's `Cause.map` and `Cause.reasons`, which do not distinguish the head from secondary `Fail`s (`runtime/src/effect/Cause.ts:33-47`, `runtime/src/effect/Exit.ts`, Go `exit.go`). I_ε is broken.
- Resolution — the bindings, as spec/05 requires:
  - the function of `Cause.map`, `Exit.map_error`, `Exit.map_both` applies only to the head; secondary `Fail`s stay as they are;
  - `Cause.reasons` returns secondary `Fail`s as `Die`s with the same error: for the typed view they are defects. `from_reasons` over such a list rebuilds a cause in which recovery is blocked — conservative and safe.

  The cause itself does not change: `Failure.rethrow`, `inspect` and rendering see the original list.

  Fixed in both runtimes (`effect/Cause.ts` `flickReasons`, `mapHead`; `effect/Exit.ts`; Go `mapErrors`, `CauseReasons`). Fixture: `cause_lists.fl` (`Cause.map`, `Exit.map_error` over `Fail B, Fail A`); tests: `runtime/test/cause.test.ts`, `cause_test.go` `TestTheTypedViewOfACauseSeesItsHeadOnly`.

### Go: a finalizer added during closing runs at once

```flick
main : Unit ->{; AppError} Unit
let main () =
  scope s {
    let _child = Fiber.spawn s report (task {
      Effect.uninterruptible (task {
        nap 50
        let _r = open_res "late"
        say "child uses late"
        nap 20
        say "child done with late"
      })
    })
    nap 10
    say "body done"
  }
  say "after scope"
```

- Divergence: on Node — `body done`, `open late`, `child uses late`, `child done with late`, `supervised (false, false, true)`, `close late`, `after scope`. On Go, `close late` came right after `open late`, before `child uses late`: the child used a released resource. Breaks R2.
- Cause: `closeWithCause` set `open = false` in C0, and `add` with `!open` ran the finalizer on the spot (`owner.go:65-77, 113`). One flag closed both the admission of children (C0) and the admission of finalizers, which must last until C5 (I2). Also, `add` into a closed owner passed `exit = nil` instead of the scope's outcome (G7a).
- Resolution — Go: admission of children and admission of finalizers are separate states, as in JS; `add` runs a finalizer at once only from the start of C5 and passes the saved outcome. Fixed (`Owner.finalizing`, `exit`); fixture: `close_phases.fl` `late_acquire`. Go lacked the `supervised` line for a different reason ([mask exit in Go](#go-leaving-a-mask-does-not-deliver-a-pending-interruption)).

### Go: the supervisor acquires into the scope of the spawner

```flick
main : Unit ->{; AppError} Unit
let main () =
  scope outer {
    sup : forall e. Failure e ->{Scope outer;} Unit
    let sup _ =
      let _r = open_res "sup"
      say "supervisor ran"
    scope inner {
      let _child = Fiber.spawn outer sup (task { todo })
      nap 30
      say "inner done"
    }
    say "outer body done"
  }
  say "after"
```

- Divergence: on Node, `close sup` comes after `outer body done`; on Go it came right after `inner done`. A resource of region `outer` was released when `inner` closed, while `outer` was alive; its handle could have escaped into a cell of `outer`. Breaks R1, R2 and, through G6, T4.
- Cause: the watcher was a copy of the fiber that called `spawn` (`watcher := *fb`, `concurrency.go:119-121`), with that fiber's owner. JS runs the supervisor `inside(owner, …)` (`owner.ts:163`), as the type `Failure e ->{Scope s | r} Unit` requires.
- Resolution — Go: the watcher's owner is the `owner` passed to `spawn`. Fixed; fixture: `close_phases.fl` `supervisor_owner`.

### Go: `parallel` keeps only the first failure

```flick
type error Boom = Boom Float

late : Float -> Float ->{; Boom} Float
let late n millis = Effect.uninterruptible (task {
  nap millis
  from_result (Err (Boom n))
})

main : Unit ->{; AppError} Unit
let main () =
  let ex : Exit {Boom} (Float, Float) = Effect.exit (task { parallel (task { late 1 10 }) (task { late 2 30 }) })
  match Exit.failure ex with
  | Some f -> say ("distinct " ++ show (List.length (Cause.reasons f)))
  | None -> say "ok"
```

- Divergence: Node printed `distinct 2`, Go `distinct 1`. With `late 1 30` in the second branch both print `same 1`: Effect joins with `⊕`, and `nub` removes the equal error. `race_success` with two failures gives 2 on both (`⧺`). Breaks R4 for late defects of branches, and parity.
- Cause: `all` threw the first failure after `stop` without looking at the outcomes of the stopped branches (`concurrency.go:194-200`). Effect adds their causes without `Int` (G7d).
- Resolution — Go: like `forEachConcurrent`, `elect(… elect(c₁ ⊕ c₂') ⊕ …)` in order of finishing, with `nub` ([combining causes in Go](#go-combining-causes-without-nub)). Fixed; fixture: `cause_lists.fl` (`parallel 2 1, race 2`).

### Go: combining causes without `nub`

```flick
type error Boom = Boom Float

main : Unit ->{; AppError} Unit
let main () =
  let same : Failure {Boom} = Cause.combine (Cause.fail (Boom 1)) (Cause.fail (Boom 1))
  let other : Failure {Boom} = Cause.combine (Cause.fail (Boom 2)) (Cause.fail (Boom 1))
  let assoc : Failure {Boom} =
    Cause.combine (Cause.combine (Cause.fail (Boom 1)) (Cause.fail (Boom 2))) (Cause.fail (Boom 1))
  say (show (List.length (Cause.reasons same), List.length (Cause.reasons other), List.length (Cause.reasons assoc)))
```

- Divergence: Node printed `(1, 2, 2)`, Go `(2, 2, 3)`. Breaks R8: (⊕-Idem) and parity; through `parallel` ([`parallel` in Go](#go-parallel-keeps-only-the-first-failure)), also its output.
- Cause: `Sequential` and `Parallel` were plain tree nodes (`cause.go:39-50`).
- Resolution — Go: `Sequential` and `Parallel` are `⊕` with `nub`: `Fail`s compare by `Equals`, `Int`s by fiber number, `Die`s by identity, because in Go a defect is text while in JS two defects with the same text are different (refined for defects by value: [defects by value](#go-two-defects-with-the-same-string-are-two-reasons)). `⧺` (`concat`) stays for `race`, `race_all` (under `elect`), `Exit.as_void_all`, `failsToDies` and between scope finalizers, as in Effect. Fixed; fixture: `cause_lists.fl`; test: `cause_test.go` `TestCombineKeepsEachReasonOnce`.

### Go: a fiber group is stopped by a finalizer

```flick
main : Unit ->{; AppError} Unit
let main () =
  scope s {
    let set : FiberSet.FiberSet s none Unit = FiberSet.make ()
    let _f = FiberSet.run { propagateInterruption = false; startImmediately = true } (task {
      Effect.uninterruptible (task {
        let _r = open_res "g"
        nap 40
        say "group child done with g"
      })
    }) set
    nap 10
    say "body done"
  }
  say "after"
```

- Divergence: Node — `open g`, `body done`, `group child done with g`, `close g`. Go printed `close g` before `group child done with g`. Breaks R2.
- Cause: `newGroup` registered the group stop as a finalizer (`fibergroup.go:26-38`). The acquisitions of its children were added later and ran earlier, while the children were still working. JS registers a closing hook (C1, `stdlib/fibers.ts:66-88`). Also, a child added to a closed group was started and interrupted in Go, but not awaited (`fibergroup.go:71-75`); in JS the body does not run.
- Resolution — Go: `Owner` has C1 hooks, and a group registers its stop as a hook; `run` into a closed group does not run the body, and a child around which the group closed while it was being added is awaited by the caller. Fixed (`Owner.onClose`, `fibergroup.go`); fixture: `close_phases.fl` `group_close`.

### Go: leaving a mask does not deliver a pending interruption

```flick
main : Unit ->{Async; AppError} Unit
let main () =
  scope s {
    let f = Fiber.spawn s report (task {
      Effect.uninterruptible (task {
        nap 30
        say "block done"
      })
      say "after block"
    })
    nap 10
    Fiber.interrupt f
    say "interrupted"
  }
```

- Divergence: Node — `block done`, `supervised (false, false, true)`, `interrupted`. Go printed `block done`, `after block`, `interrupted`: the child kept working after the request and ended with success, and the supervisor was not called. The same shows in the [late-finalizer probe](#go-a-finalizer-added-during-closing-runs-at-once): Go has no `supervised` line.
- Cause: `masked` restored the depth with `defer`, without a check (`fiber.go:99-104`). The next interruption point in Go is the next `Run`, and a direct call to `say` is not one. Effect interrupts on leaving the mask (G7c), the model by (Unmask).
- Resolution — Go: `Uninterruptible` and `UninterruptibleMask`/`InterruptibleMask` run `Check` after the body if the depth is back to zero. Internal masks (finalizers, closing, `ensuring`, `on_exit`) leave the request until the next interruption point: a check there would interrupt the closing. Fixed; fixture: `close_phases.fl` `unmask`.

### A supervisor defect is lost in `flick test`

```flick
module App.Sup exposing (tests)

broken : forall e. Failure e ->{} Unit
let broken _ = todo

tests : List Test.Test
let tests = [
  Test.effect_test "a supervisor that dies" (task {
    scope o {
      let _f = Fiber.spawn o broken (task { todo })
      ()
    }
  }),
]
```

- Divergence: `flick test --emit js` and `--emit go` printed `✓ a supervisor that dies`, exit code 0. The same code in `main` ends the program with code 1 ("a defect of a supervisor"). Breaks R4.
- Cause: the defect sink was global (`owner.ts:38-43`, Go `reported`), and only the `main` runner read it (`entry.ts:164`, Go `platform.go:146`). The test runner (`ownedExits`, Go `testing.go`) did not.
- Resolution — both runtimes: a test is its own root. An owner gets its defect sink from the enclosing owner; a test has its own. Supervisor defects that arrive during the test are joined with the outcome of the body, and the test fails with kind `defect`. Fixed (`Owner.defects` in JS and Go, `stdlib/testing.ts`, `testing.go`); tests: "a supervisor that dies" in `flick/tests/test_command.rs`, `testing_test.go` `TestASupervisorThatFailsFailsItsTest`.

### Go: a failing `Events.listen` supervisor is dropped

- Divergence (found by reading the code): `l.fail` ran the supervisor in `catch` and dropped its failure (`events.go:25-33`, with a `shortcut` note there). JS passes it to `reportDefect` (`stdlib/callbacks.ts:85-91`). Also, one `supervising` fiber with a shared `mask` was used both by the handler goroutine and by the `send` goroutines on queue overflow — a race on `mask` (`fiber.go:42`) if the supervisor masks. Breaks R4.
- Resolution — Go: a supervisor failure goes to the root's sink; each supervisor call gets its own fiber state and the owner `owner`, like `within` in JS. Fixed; test: `owner_test.go` `TestAFailingListenerSupervisorIsReported`.

### JS: `fiberSpawn` is interruptible midway

- Divergence (found by reading the code): `fiberSpawn` was not masked (`owner.ts:145-172`). Interrupting the calling fiber at a step boundary after `forkDetach` (154) left the child outside `children` and without a watcher: it outlived its owner and its failure was lost. Interrupting after `children.add` (155) but before the watcher was forked left the child without a supervisor. Breaks I3 and through it R3 and R4. In Go the whole registration is under `o.mu`.
- Resolution — JS: the body of `fiberSpawn` runs under `Effect.uninterruptible`. Fixed; test: `runtime/test/spawn.test.ts` "an interrupted spawner…": the caller is interrupted at the moment of `children.add`, and without the mask the supervisor is not called.

### JS: leases register their release outside the mask

- Divergence (found by reading the code): `ScopedRef.set` (`effect/ScopedRef.ts:44-47`) first replaced `ref.current`, then called `retire` on the previous holder; between them was a step boundary where an interruption left the previous holder unreleased. `Resource` (`effect/Resource.ts:25-61`) and `ScopedCache` (`effect/ScopedCache.ts:13-21`) had the same order; an interruption pending at the exit of `acquireHolder` threw away an already acquired holder. Breaks R1. Go has none of these modules.
- Resolution — JS: registering the holder, replacing the value and `retire` run under one mask, like `acquireRelease` in Effect (G7b). The acquisition itself stays interruptible: the caller's mask is passed into `acquireHolder` through `restore`, as in Effect's `ScopedRef.set`. Fixed. The window between returning the holder and `retire` contains no user code, and the test cannot reach it; `runtime/test/leases.test.ts` pins "exactly once" when `set` is interrupted during acquisition.

### A `provide` service outlives the release of its layer

```flick
service Greeter =
  greet : String ->{} String

main : Unit ->{; AppError} Unit
let main () =
  scope outer {
    scope p {
      let layer = Layer.effect Greeter (task {
        Scope.add_finalizer (task { say "release greeter" })
        { greet = fun name -> "hello, " ++ name }
      })
      provide layer {
        let _f = Fiber.spawn outer report (task {
          nap 30
          say (Greeter.greet "child")
        })
        say "provide body done"
      }
      say "after provide"
    }
    nap 60
    say "outer done"
  }
```

- Divergence: the program was accepted; Node and Go alike print `release greeter` before `hello, child`. A child of the outer owner inherited the environment of `provide` and used the service after the layer closed. Breaks R6; through a service holding an external object closed by the layer's finalizer, this is the same "use after close" as in T4, but without a region in the type. `Scope.add_finalizer` of an outer region from inside `provide` behaves the same way.
- Cause: service rows are not part of ⌈·⌉ (`regions.md` §2.1, [service implementations hold no captures](regions.md#service-implementations-hold-no-captures)), and `Fiber.spawn` takes the task row `r` from the call site (`builtins/Fiber.fl` `spawn`). Nothing tied the lifetime of the `provide` environment to the owner the work is handed to.
- Resolution — the checker (`regions.md` §2.7). A single lexical check cannot close it: the service may come from the row of the calling function, and then the owner–`provide` relation is visible only at that function's caller. So `provide` introduces a region for the order of §2.4, the scheme of every constant with `retention owner` requires `Fiber.Provided r s` for the row of the retained work, and an obligation a stage does not discharge passes into its row and on to whoever runs it; a signature names it with the constraint `Fiber.Provided {S} s`. The runtime does not change. Fixtures: `tests/check/fail/regions/provide_escape.fl` (the program above and `Scope.add_finalizer` of an outer region), `tests/check/pass/regions/provide_owner.fl`.

### A closing hook added after C1 does not run

- Divergence (found by reading the code): `close` walked `owner.closing` once (`owner.ts:58-61`). A hook added during C3 — for example, a supervisor creating a `FiberSet` in the region of the closing owner — was skipped. The group was then stopped by its own Effect finalizer in C5, by its place in the stack, as in the [Go group divergence](#go-a-fiber-group-is-stopped-by-a-finalizer). Breaks R2 in a rare case.
- Resolution — JS and Go: after C3, run the hooks added since closing began and wait for them; no new children can enter `K` after C0 (I2). Fixed; tests: `runtime/test/scope.test.ts` "a closing hook a supervisor adds…", `owner_test.go` `TestAHookAddedWhileClosingRuns`.

### The refresh fiber of `resource_auto`

- `Effect.resource_auto` refreshes the resource with a `forkScoped` fiber (`stdlib/effect-core.ts:92`). For the owner this is a finalizer, not a child, and its failure goes nowhere.
- An incompleteness of the Effect surface, not of the language. Resolution: recorded; R4 excludes it. If `resource_auto` becomes part of the safe API, the refresh will go through `Fiber.spawn` with a supervisor.

### Nested `provide`s share a build

- A probe: the layer `dep` in `provide dep { provide dep { … } }` is built once, in two sibling `provide`s twice; the same on Node and Go.
- `reference/runtime.md` "Layers" said "two `provide`s build it twice"; spec/05 says "Independent `provide`s build independent instances". Nested `provide`s are not independent: the inner one reads the outer one's memo map (§3.5).
- Imprecise prose. Resolution: `reference/runtime.md` and spec/05 state that sibling `provide`s build anew, and a nested one takes the outer build by identity.

### Region closing as one step versus closing phases

- `regions.md` closes a region in one step: children, finalizers, then `Λ(π) := closed`, and `op_H` on a closed region gets stuck. Both runtimes set "closed" before the finalizers (Effect `Closed`, Go `open = false`) and run a finalizer added to a closed scope at once (Fin-Now), and the hooks come before the children (C1).
- Not a contradiction but a refinement: §3.3 here details that one step. Getting stuck on `op_H` remains the model of "use after close": T4 proves it unreachable, and (Fin-Now) is reachable only from a finalizer or a foreign callback. Resolution: `regions.md` §3.3 refers to λRT.

### A test closes its scope with success, and the shutdown flag is timed by the watcher

- `ownedExits` closes the owner with `Exit.void` (`owner.ts:116`), so `Scope.add_finalizer_exit` in a test sees success even when the body failed.
- `shutdown` in `Failure` is computed when the watcher sees the child end (`owner.ts:162`): a child interrupted by the program right before closing is marked as stopped by the closing.
- An incompleteness of diagnostics. Resolution: recorded.

### An interruption that skips a handler erases `Fail` (Effect)

- G7g: if a failure passes through an interrupted interruptible fiber, Effect skips the handlers, removes the `Fail`s and adds an `Int` (`FX/internal/core.ts:613-624`). Go kept the original cause.
- Types are unaffected; output can be: a program printing `Cause.reasons` of such a failure diverges.
- A probe built for the cause-law tests showed that in Go this was worse than a lost `Int`. A child fails with a `Fail` right after `Effect.uninterruptible` and is interrupted while inside the mask. The mask-exit check ([mask exit in Go](#go-leaving-a-mask-does-not-deliver-a-pending-interruption)) ran only after a successful body, so on Go `try` caught the `Fail`, the handler ran, and the child ended with success: the interruption request was lost. On Node the handler is skipped and the outcome is `[Int]`.
- Resolution — Go: `skipHandler` in `try` and `onCause` (`catch_cause`, `catch_defect`, `tap_*`, `or_die`…) does what Effect does when the fiber is interrupted and interruptible. Fixed; `cause_test.go` and `tests/run/runtime/cause_laws.fl`. The same applies in the other recovery handlers (`caught` in `cause.go`): `Effect.exit`, `match_cause`, `all` with exits and results, `eventually`, `partition`/`validate`, `first_success_of`, `retry`/`repeat` and execution plans. Probes on Node and Go: `tests/run/runtime/interrupt_skips.fl`; without the fix, Go's `Effect.exit` and `match_cause` ran the handler and the child ended with success. Boundaries that must run (`on_exit`, release, metrics) see the cause, as in Effect, where the mask stops the skip.

### A foreign call interrupted after C4 is not awaited

- A Promise enters `P(π)` when the call starts (JS) or when it is interrupted (Go). If a group child stopped in C5 (under the old order) or a finalizer that restored interruptibility interrupts a foreign call after C4, nobody waits for its Promise any more.
- Since groups stop in C1 hooks, before C4 ([Go groups](#go-a-fiber-group-is-stopped-by-a-finalizer), [hooks added after C1](#a-closing-hook-added-after-c1-does-not-run)), the case reduces to a finalizer that itself calls `interruptible`. Resolution: recorded as part of G9.

### Go: a found memo-map entry does not check observer admission

- `getOrBuild` in the "already built" branch ignores a refusal from `observe` (`layer.go:141`): a consumer with a closed scope gets the services without being counted and can outlive the build. Since the consumer's scope admits finalizers until C5 ([late finalizers in Go](#go-a-finalizer-added-during-closing-runs-at-once)), the branch is reachable only from a finalizer.
- Incompleteness. Resolution: recorded.

### Go: two defects with the same string are two reasons

- `sameReason` compared `Die` by pointer (`cause.go:78`), while Effect uses `Equal.equals` of the defect, that is, a string by value. `Cause.combine (Cause.die "x") (Cause.die "x")` gave one reason on Node and two on Go: `Cause.die` builds a new `*Cause` each time.
- Breaks (⊕-Idem) and the parity of §2.5. Resolution: `Die` compares by value, except a thrown defect: a panic, `Stream.die`, an event queue overflow and an unfinished request are an `Error` in JS, and Go marks them `thrown`. Fixed; `cause_test.go` and `tests/run/runtime/cause_laws.fl`.

### Go: a typed error turned into a defect loses its value

- JS puts the error value itself into the defect: `failsToDies` in `try`, a secondary `Fail` in `Cause.reasons`, `Effect.or_die`, `Stream.or_die`. `Cause.find_defect` describes it by its constructor (`describeDefect`), `Failure.inspect` as `die A`. Go stored only the text `describe v`, so `find_defect` gave the value's JSON, and two defects of the same value would differ by text rather than by `Equal.equals`.
- Breaks the parity of §2.5. Resolution: `dieOf` stores the value next to the text; `find_defect`, `catch_defect`, `tap_defect`, `Failure.inspect` and `Cause.pretty` read it, as JS does. Fixed; `cause_test.go` and `tests/run/runtime/cause_laws.fl`.

### Go: an empty cause is not a failure

- Go used `nil` both for "no failure" and for the empty cause `∅`. So `try Failure.rethrow Cause.empty with …` succeeded with a `nil` value of any type, and `Exit.fail_cause Cause.empty` was a successful outcome. In Effect `failCause(empty)` fails, `try` passes the cause on (§2.3, "otherwise"), and `report(∅)` is `pretty(∅)`, since `hasInterruptsOnly` of an empty cause is false.
- Breaks (Try) and parity. Resolution: a `Failure` always holds a cause: the empty one is `emptyCause`, the unit of `⊕`; `report` and `conclude` treat as an interruption only a cause that contains an `Int`. Fixed; `cause_test.go` and `tests/run/runtime/cause_laws.fl` (`recover` on the empty cause).

### A hook added during C4 does not run

Found by the Lean development `formal/Formal/RT/`.

- Divergence (found by reading the code and the model): JS and Go ran the hooks added since closing began after C3, and then only waited for foreign Promises (C4: `settle`, `owner.ts:100-101`; Go `closeWithCause`). A hook added to `H(π)` during C4 did not run: a group created at that time was stopped by its own Effect finalizer in C5, as in the [Go group divergence](#go-a-fiber-group-is-stopped-by-a-finalizer). The Lean proof of R2 and R3 depends on this point: (C5) must start with `H(π) = ∅`.
- Reachability: only a fiber whose current owner is `π` can add a hook to `π` during C4. By I4 these are the body, children, watchers, group children and branches started in `π`, and by C4 they have finished. What remains is a violation of I4 (G6) and a foreign call that adds a hook when it settles: the tests of the fix do exactly this.
- Resolution — JS and Go: after C4, run the hooks added since, and wait for foreign calls again; repeat while hooks are being added, and only then start C5. This matches the premise `H(π) = ∅` of `Step.fins` in the model. In Go the check "no hooks" and `finalizing := true` happen under one `o.mu`, so a hook added later runs at once. Fixed; tests: `runtime/test/scope.test.ts` "a closing hook added while foreign calls settle…", `owner_test.go` `TestAHookAddedWhileCallsSettleRuns`, both fail without the fix.

### The provide-liveness premise is stronger than typing gives

Found by the Lean development of the core (`formal/Formal/`).

- Divergence: R6, the sketch in §6 and hypothesis G11 in Lean (`G11`, `GoodUse`) required every fiber with a `θ` from the `provide` of owner `πₚ` to run only until `πₚ` closes. A child inherits the whole environment of the spawn site (§3.5), and the rule of `regions.md` §2.7 ties to the owner only the providers of the services in the task row. The program `provide L1 { scope outer { provide L2 { Fiber.spawn outer (task { …; ask S1 }) } } }`, where `L1` gives `S1` and `L2` gives `S2`, is accepted: `Provided {S1} outer` is discharged, since the provider of `S1` encloses `outer`. The child runs after `L2` closes and takes `S1`; `GoodUse` saw the released build of `L2` in its environment and reported a violation, although `S1` came from the live `L1`.
- Cause: the premise was stated about the whole fiber, while typing (point 3 of T4) speaks about service requests.
- Resolution — this document and the λRT model: the premise is "an operation answered from the environment is answered by the handler of a live owner". R6 and §6 state it this way; `G11` and `GoodUse` in `formal/Formal/RT/` speak of the environment entry that answers the request. The Lean core development proves the premise in this form (`T4_3`).

### Go: leaving a close does not deliver a pending interruption

- Divergence: the fiber is asked to stop while a finalizer of its `scope` runs. On Node the statement after the scope does not run and the fiber ends interrupted. On Go the statement runs, and the fiber ends with success if no interruption point follows, so its supervisor is not called. If the finalizer died, Go's cause lacks the `Int` that Node's has.
- Cause: closing runs at `mask (…)`, and (Unmask) delivers a pending request when the depth returns to zero. Go's internal masks leave the request until the next interruption point ([mask exit in Go](#go-leaving-a-mask-does-not-deliver-a-pending-interruption)), and a direct call is not one. Breaks (Unmask) and the parity of §2.5; R1–R4 hold.
- Resolution — Go: `scope` and `provide` deliver the request when they have left the close, at depth zero, not inside it (`region` in `owner.go`). Internal masks still leave it until the next interruption point. Case: `tests/run/runtime/interrupt_after_close.fl`.

### JS: a group made while its owner closes runs nothing

- Divergence: a supervisor or a stopping group child makes a `FiberSet` in the region of the closing owner and runs a task in it. On Go the group is open until its hook runs (§3.6): the task runs and has stopped before the finalizers. On Node `inOwner` refuses any child of a closing owner (`stdlib/fibers.ts`): the fiber is interrupted and the task does not run. R2 and R3 hold on both, and so a hook added during C1 or C3 cannot be observed on Node from a program.
- Resolution: recorded; §3.6 is the rule. Case: `tests/run/runtime/group_in_closing.fl`, listed in `tests/run/known_gaps.txt` for JS.

### The runtime of `effect = "rt"`

`runtime/src/rt/` is the same contract without Effect, on generators and a driver of its own. Where the runtimes differ it takes a side:

- Interruption points: as JS, a boundary between two steps of the driver: a call, a suspension, a jump of a looping generator. A plain function is one step, so a stage that runs at once cannot be interrupted, and on this runtime that is every closed row of `tail` and `abort` effects, `->{}` included: `spin : Float ->{} Float` looping for ever blocks the thread, and `codegen/release`, which stops it with `timeout_option`, does not finish. Go interrupts it at `Check`. Recorded: a function that loops for ever and must stop has a row that says so, `Async`.
- A wait that ends moves its fiber on at once, on the stack that ended it, and a fiber forked by `spawn` starts on a later turn of a queue on microtasks; branches of a combinator start at once on the stack. As Effect, so that the order of output agrees with JS on Effect. Go's order comes from goroutines and turns (`startup.go`).
- Foreign calls enter `P` when they start, as JS; Go puts them there when they are interrupted.
- Discontinuing a body throws the handler's signal into its frames, as Go panics with it, rather than `gen.return()`: every frame sees it as a failure, `Effect.ensuring` and scopes close by the same path, and the order of finalizers is Go's.
- A group made while its owner closes: §3.6, as Go ([JS](#js-a-group-made-while-its-owner-closes-runs-nothing)). `group_in_closing` passes.
- Time: a sleep, `timeout` and a schedule wait on the clock in the evidence, the host's or the virtual one of `Test.clock_test`. Moving a virtual clock wakes the sleepers due in order and waits, before the next, until what each wake scheduled has run, as Effect's `TestClock` does by its scheduler; Go waits until the fibers under the clock are parked.
- A service a layer builds is a handler: its operations run with the evidence of the place of the `provide`, as the arms of `provide S impl` do (§3.2), and a failure of one is the handler's. On Effect and Go it is a record whose operations run where they are called: `codegen/service_place` prints `inner layer` there and `outer layer` here.

### Go: HTTP handlers

- Found by reading the code. A handler without its own `scope` acquires into the server's owner (`http.go:56`); these finalizers are added after the server's finalizer and run before it, before `Shutdown` has waited for the requests. A handler's defect becomes only a response code and is reported nowhere (`http.go:65-68`). After `Close` on a wait timeout, handlers are not awaited.
- Outside λRT: the server is a source of fibers, not part of the contract. Resolution: recorded; the fixture `tests/run/stdlib/http/lifecycle.fl` wraps the handler in a `scope`.
