# Phase M: memory without a collector

This is the shared memory-lowering contract for Wasm and Native, not a
description of a shipped native backend. It extends the value semantics of
[the core](core.md) and the by-use lifetime rules of [regions](regions.md).
Host JavaScript/Go garbage collection and Rust `Rc` used for syntax sharing
are not evidence for this contract.

## 1. Implementation boundary

| Obligation | Current implementation | Still required for the phase gate |
| - | - | - |
| M1: explicit ownership | Shared memory IR, Perceus insertion, source closure/dictionary conversion, guarded pattern matches and immutable record updates, pure list callbacks, left/right folds and list observers, typed failures and tail/abort handlers, counted nominal tags and constant Layer providers, owned outcomes and explicit source runners | Extend lowering/execution to every language form and outcome; run the entire corpus on this path in both profiles with zero live objects after releasing results and closing owned regions |
| M2: `Arena s` | Public `Arena.make/alloc/get`, existing by-use checker rules, counted scope/selected-owner execution, JS/Go storage retirement after finalizers, scope-release/use/retention regressions | Counted execution of arbitrary finalizers/handlers/owned work and the whole heap/region proof |
| M3: FBIP and cycles | Dynamic uniqueness/layout-checked reuse for a last borrowed receiver followed by construction; root-owned counted cells, strong reads/replacement and detach-all teardown, with closure/cell-cycle tests | Full constructor/destructor lowering and backend implementation |
| M4: Lean | `Formal/MemoryInsert.lean`: whole-function token balance for the executable cached insertion projection, before output validation, with distinct parameters and every selected-path prefix; `Formal/MemoryPass.lean`: independent token checking and computed live-in availability; `Formal/Memory.lean`: local arithmetic; `Formal/MemoryArena.lean` and `MemoryScopes.lean`: root inventories and selected-owner storage retirement | Verified Rust correspondence, machine bounds, operation/heap and exception-unwind semantics, dynamic reuse and physical reclamation remain outside these proofs |
| M5: Native split (2026-11-13) | Backend boundary below | Implementation belongs to phase N, not a replacement for M1–M4 |

`run::run_main` and the existing `tests/run/model.txt` harness still use the
substitution interpreter. Its successful run does **not** establish M1.
The separate `run_main_counted`/`run_file_counted` entry points execute
checked source IR without substitution fallback, explicitly release the
main outcome, and require `Heap::finish` to observe zero occupied slots.
The main runner applies a returned ordinary function to Unit and awaits a
resulting task, at most once each, in the same execution and cell arena.
`flick-model/tests/model/memory_source.rs` runs forty-nine selected corpus cases
against their unchanged expectations in debug and release: the seven
bootstrap cases `hello`, `inline_declarations`, `inline_module`, `inline_nested`, `inline_pack`,
`inline_pack_nominals` and `packed_module`; the codegen cases `call_site`, `stage_order`,
`eval_order`, `effect_sets`, `task_twice`, `given_shadow`, `instance_names`, `interpolation`,
`lift`, `patterns`, `repeated_superclass`, `fail`, `counted_failures` and
`counted_patterns`, `defaults`, `errors`, `exports`, `higher_rank`,
`classes`, `default_signatures`, `derive_default`, `gadt`, `gadt_derive`, `gadt_dicts`,
`handlers`, `partial_handlers`, `prototype_operation`,
`service_dictionary`, `service_place`, `services`, `closures`, `composition`, `enums`,
`generic` and `record_constructor`; `effect/row_without`;
`stdlib/effect/mutable_ref` and `stdlib/effect/optic_parts`; and
`runtime/arena_scope`, `runtime/arena_layer_scope`, `runtime/arena_owner_list`
and `runtime/counted_layer_merge`.
The new counted cases also run in the
semantic oracle, with independently specified stdout. Separate source tests
cover captures/recursion/order, tasks, mutable cells, arenas, matching,
record updates and unbounded BigInt display with its required `n` suffix.
These selected cases do not constitute the full corpus gate.
Separate fold regressions preserve accumulator aliases and escaping closures,
empty-input and cold-partial behavior, left/right callback order and fault
cleanup at every tested fuel prefix. They use closed ordinary Flick-code
templates for exact audited adapters, not host execution or generic foreign
erasure. Materializing list tails remains a quadratic-copy baseline, not a
linear-time or FBIP guarantee.
Task-valued accumulator tests additionally await the same folded tasks under
two different service providers. Distinct expected results pin cold construction,
replay and caller-time service lookup, including nested tasks, while prefix tests
release captures without sweeping external roots. This is not evidence for
lexical-default-owner selection or Arena storage retirement.
`List.length`, `NonEmpty.length` and short-circuiting `List.any` also use closed
ordinary-code templates for their exact pure adapters. An explicit shallow
`ArrayToList` intrinsic preserves the native readonly-array ABI: lists retain
their identity; tuples retain their fields in a new list. Other representations
reject, including Unit (not an empty tuple); ordinary pattern kinds stay strict.
Regressions cover empty inputs, tuples and opaque tuple aliases, left-to-right
callbacks, skipped defects, cold partial application, function-valued elements
and preserved container/element aliases. Separate prefix tests retain lists and
tuples of closures that merely hold retired Arena views: neither observer
calls those closures or dereferences their closed storage. This preserves RG4
without permitting a fresh region to escape in the result's type. Length
currently traverses materialized tails on this counted path; it is a quadratic
baseline, not the native runtime's constant-time length or an FBIP guarantee.
Dedicated counted tests also execute the existing `codegen/defect`,
`codegen/apply_order` and `codegen/composition_method` cases in both profiles.
They require the specific division-by-zero defect, unchanged preceding stdout
and zero occupied heap slots after fault cleanup. Unsafe Int division/remainder
preserve signed wrapping/truncation, and string length counts UTF-16 units with
fuel charged during traversal. These semantic outcome checks do not render or
verify JavaScript source-stack diagnostics; an arbitrary interpreter fault is
not a successful corpus result.
The memory IR is an additional public representation; it is not yet a field
of the serialized typed `Program`. The separate owner-context expression
extensions use typed IR 0.12, following the upstream module-export additions
in typed IR 0.11. Interface schema 26 adds non-public type support without exposing
those names as source exports. No corpus
case may be declared covered merely because the old interpreter runs it.

Untrusted JavaScript boundary copies now use explicit work stacks, including
native Option/Result and generic bridge conversion. A source fixture checks
20,000 nested ADT/list levels across arguments, results, callbacks, Effect and
Async completion on both JavaScript runtimes. Defensive copies preserve
independent repeated aliases and reject active-path cycles; generic bridge
conversion separately preserves its existing backedge policy. These host
checks establish copy isolation/stack safety for those paths, not explicit RC
reclamation or counted execution of arbitrary FFI. See
[the boundary contract](ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary).

The additional `Handle`/`Perform` IR forms make the next dynamic boundary
explicit. Installation consumes the delayed body, each operation function,
then optional return/error functions; dispatch consumes dictionaries before
ordinary arguments. Repeated operands require separate tokens. Metadata
rejects duplicate effects/operations and invocation-count overflow, including
Control's extra resume argument, in every function before execution. Source
lowering and counted execution support tail/abort operation handlers and
return/error arms. Control metadata is representable, but source lowering
and raw counted execution explicitly reject it until real one-shot
continuation ownership is implemented; it is never substituted with tail.

## 2. Owned tokens and objects

An immediate value has no heap object. A string, arbitrary-sized integer or constructed value has
an object identity, a positive reference count, and owned fields. Every
owned root token and every owned field contributes exactly one reference.
Two equal values need not have the same identity. Cloning a syntax tree
neither allocates nor duplicates a language object.

The model's owned object tokens are opaque and non-copyable. An observable
object identity is diagnostic data, not a constructor for another owned
token: only a counted duplication or projection may create such an alias.
Comparing operand multiplicities with the total reference count alone is
insufficient, because that count also includes existing internal edges.

The current memory IR has ANF `Let` instructions and `Return`/`Branch`
exits. Each local is single-assignment on a path. Before insertion, a local
can be used any number of times. After insertion:

- each parameter starts with one owned token;
- `Dup x` requires an existing token and produces one additional token;
- `Drop x` consumes a token; a zero object count releases its fields and
  storage, recursively (implemented iteratively in the model);
- `Construct tag fields` consumes one token per field, including repeated
  occurrences, and returns a new owned object;
- `ConstructWithDicts tag fields dicts` also consumes every evidence token.
  Its physical fields are the ordinary prefix followed by the evidence
  suffix, with an explicit ordinary-field count. Projection and destruction
  still include **all** physical fields. Existing `Construct` wire shapes
  remain unchanged and mean that all supplied fields are ordinary;
- `Project object i` borrows its receiver and returns an owned field token.
  The field must be retained **before** the receiver's last token is dropped;
- `Record` consumes its values in written label order; `Field` borrows its
  receiver and returns an owned value by label. A tagged constructor's
  implicit `_tag` yields its full nominal identity string; a record's actual
  `_tag` remains an ordinary field, with no synthetic string allocation;
- `Return x` transfers one token to the caller, not to an implicit collector;
- `Branch condition yes no` consumes an immediate Boolean token and runs
  exactly one successor with the remaining tokens. It never duplicates the
  entire environment for both paths.

Returned heap values remain live until the caller explicitly drops them.
The model's `finish` check reports remaining objects; it must not sweep them
away. Invalid tokens, stale identities and double release are errors, not
successful zero-live-object results. Lifetime counters are observed from
explicit operations, never inferred only from allocations minus frees.

The static token validator is independent of insertion and checks both
branches. It proves a property of the annotation stream, not dynamic field
types, reference counts, source translation or all paths of the language.
`Formal/MemoryPass.lean` proves selected-path and prefix safety for its
token projection from fresh function parameters. `Formal/MemoryInsert.lean`
proves whole-block balance of its executable cached insertion algorithm
before output validation. The proof derives the pending-receiver successor
inventory; it does not assume output acceptance or next-state correctness.
Its function-entry corollary requires distinct parameter names, as Rust
does, but permits fresh names to be reused in disjoint branches. Arbitrary
finite local width, repeated operands/evidence and every selected-path
prefix are covered. Pending-reuse operand distinctness concerns local
names, not heap-object aliasing: runtime uniqueness and layout checks
remain necessary. This is not verified extraction from Rust or a proof of
operation, heap, exceptional-unwind or physical reclamation semantics.

### Calls, captures and execution roots

A memory `Program` contains named zero-argument global initializers and
numbered code functions. Qualified module/binding identities remain
separate through JSON/CBOR; flattening both with a dot would be ambiguous
for generated names. Program validation checks every code body, code/global
reference and direct-call arity before execution, including unselected code.

`Closure code captures` consumes the capture tokens. Its code takes exactly
two parameters, **owned closure self and owned argument**. `Apply` transfers
both into that frame; it does not secretly clone the environment. Capture
reads are ordinary borrowed projections from self. A local recursive name
refers to the outermost closure's self token; later curried stages capture
that token instead of rebinding the name to their own inner self. No closure
owns itself through a stored field. Closure storage is distinct from tagged
constructor storage and cannot be repurposed by `Reuse`.

`Task code captures` has distinct storage and code with the owned task self
and Unit as parameters. `Await` runs its body afresh on every await; it does
not memoize a result. `Apply` rejects a task, and `Await` rejects an ordinary
closure. Task storage is also ineligible for constructor reuse.

Expression-valued conditionals use direct helper `Call`s with the condition
and the union of free branch locals as parameters. Existing `Branch` and
Perceus liveness dispose of unused parameters on the selected edge. The
caller continuation is emitted once, not cloned into both branches. Capture
discovery respects local/pattern/recursive binders and includes dictionary
references that are not ordinary `ExprKind::Local` nodes. Calls retain source
`arg_first` order and short-circuiting.

The program interpreter uses an explicit frame stack and one fuel counter.
Each initialized global has one owned cache root; `Global` retains it, and
teardown drops every initialized cache root. An initializing/ready state
detects initializer cycles. Consumed operands remain explicit scratch roots
until an operation commits. Runtime error or exhausted fuel drains scratch,
pending results, queued arguments, suspended frames and global roots by
ordinary drops, after detaching the root cell arena as specified in §5,
not by sweeping heap storage. External aliases survive.
The older standalone `execute(Function)` deliberately leaves error roots
observable; it is not the program runner's unwinding contract.

A successful returned value remains caller-owned. Code IDs are local to an
execution; returned closures are safely inspectable/droppable but cannot be
invoked by a different `execute_program` run. A persistent host runtime is
not implemented by reinterpreting those IDs. Lazy global caching alone is
not a proof of the full compiler's lexical initialization scheduling.

Currently lowered source includes ordinary globals/lambdas, curried/local
recursion, applications, value `if`, short-circuiting, data construction,
tuple/record destructuring, records, explicit/context dictionaries and
scalar primitives, tasks/await, tail/abort and return/error handlers, and
`Ref.make/get/set/modify/same`. The known
read-only `show` and Ref intrinsics discharge their defensive-copy adapters
through immutable owned-value semantics: reads retain, and replacement
changes the cell slot, not an immutable value. No value is exported to an
unmodelled host. Unknown foreign ownership adapters are not silently erased.
BigInt literals, negation, comparison and show are unbounded. The memory IR's six
BigInt arithmetic operators (`+`, `-`, `*`, `/`, `%`, `**`) also use exact signed
arithmetic without a machine-integer domain limit. Division truncates toward
zero; remainder has the dividend's sign; both operators yield zero for a zero
divisor. A negative exponent produces the specific `BigIntNegativeExponent`
defect, not a typed raise or an unsupported-operation result. This does not
assert identical native diagnostic rendering or support for other BigInt FFI.
The program runner charges digit work before performing it and keeps operands
in scratch until result delivery or ordinary error/fuel cleanup. The older
standalone function executor retains its intentionally unfueled diagnostic
contract; it is not a way to bypass the program runner's fuel accounting.

Audited pure global `BigInt` bindings require genuine checked declaration facts
for both a primitive input and a `BigInt` output. Their counted conversion follows
[ECMA-262 2025](https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html#sec-bigint-constructor-number-value):
Int converts exactly, BigInt retains its owned identity, Bool becomes 0 or 1,
and finite integral Float converts its exact binary64 value, including `-0` and
the maximum finite magnitude. String uses the specified whitespace and integer
grammar, not the broader source-literal normalizer: empty trimmed text is zero,
signs apply only to decimal digits, and separators/suffixes/fractions reject.
Non-integral or nonfinite Float, invalid String, and Unit produce distinct
RangeError, SyntaxError and TypeError semantic faults, not typed raises or
unsupported-operation results. The constructor remains cold until application;
every tested fuel prefix releases execution-owned roots while preserving callers.

Input/result defensive-copy bits are discharged only after both type proofs;
metadata alone is not authority. Object/Ref/JsValue coercion still needs its real
`ToPrimitive` behavior and remains a separate lowering obligation, not a source
ban or an assertion that those values cannot convert. Existing lone-surrogate
string-representation limitations are unchanged. Go currently registers the
canonical Int conversions `Numeric.bigint_of` and `Instances.bigint_of_int`, not
arbitrary custom global declarations. These conversions do not establish whole
corpus coverage, native diagnostic rendering, or Rust/Lean correspondence.

### Owned dictionaries and structural views

Counted dictionaries are distinct from records. `Dictionary` consumes eager
values and unary getter closures in written evaluation order. `Method` borrows
its receiver: an eager entry is retained, while a getter is called with the
actual dictionary under the lookup caller's owner/evidence context. Getter
results are not cached. In particular, a returned default function may own its
dictionary without forming a dictionary-to-cached-method-to-dictionary cycle.
Construction closures capture context dictionaries, never unfinished self.
Ordinary field access, projection, record patterns and updates do not force
getters or treat a dictionary as a record.

Explicit instance methods are evaluated eagerly. Defaults and superclass slots
are getters. Default application passes actual self, signature-extra evidence,
then the method's own evidence. A default with no own evidence returns its real
value; it is not eta-expanded over an ordinary argument. Context dictionaries
retain the existing unary-curried ABI. `CallSite` uses all-eager dictionary
entries for its two class members; its source metadata is unchanged. Primitive
row evidence is an owned list of labels, not a method dictionary.

`Structure` borrows a record or exact tuple (Unit for the empty tuple), checks
its expected field count, and returns owned `(is_record, values, labels)`
tuples. Record labels use UTF-16 sort order; tuple labels are Unit. Nominal data,
lists, closures and dictionaries are not structural views. Traversal, sorting
and partial construction are metered with owned intermediates rooted explicitly.
Structural `Eq`, `Ord`, `Hash` and `Show` methods are generated as ordinary
owned closures and calls over these views. Field methods, not raw payload
comparison, determine class dispatch. Equality and lexicographic ordering
short-circuit; structural hashing wraps `h * 31 + field_hash` from seed 11.
Show preserves record labels versus tuple syntax. Ord/Hash superclass
dictionaries are getters over the field dictionaries' `$Eq`, consistently with
ordinary instances and the Go runtime. JS regression tests pin cold
construction and forcing failures at superclass lookup, not at construction.

Derived methods similarly use explicit qualified constructor tags and ordinary
payloads; held evidence, when present in the layout, is an ignored suffix, not
a payload field. Constructor declaration order determines comparison and the
initial hash seed. Field evidence is constructed once per invocation after a
matching non-nullary variant is selected; nullary and constructor-mismatch
paths do not force it. No derived-method cache creates a self-owned dictionary
cycle. An opaque alias without constructors forwards through its representation
dictionary. GADT derivation still follows the checker's existing restrictions:
it does not invent evidence for existential fields or held constraints.
Show parenthesizes a spaced constructor argument unless it already starts with
`[`, `(`, `{` or a quote.

`memory_dictionaries.rs` and `memory_source_dictionaries.rs` cover actual-self
dispatch, separate own/signature-extra evidence, value defaults, context
currying, recomputation, returned methods, caller context, typed faults and fuel
prefixes with external aliases and actual occupied-slot zero. Separate
structural/derived tests cover custom field methods, superclass reads,
short-circuiting, recursive instances, representation forwarding and interrupted
field factories. These do not constitute whole-corpus coverage.

The exact primitive compare/hash/equals adapters preserve their full declared
metadata. Comparison is NaN-last, compares strings by UTF-16, and handles
unbounded BigInts without narrowing. Hashing follows `runtime/src/rt/hash.ts`
and Go, not the older core helper; scalar and exact Ordering representations
are implemented. Other raw structural/nominal foreign hash or comparison values
still need a faithful representation/FFI contract and fail explicitly instead
of producing a dummy result. Most row operations, other foreign bindings and
whole-corpus execution remain full-M1 obligations. The generic Lean token
algebra applies to the operations'
consume/borrow lists; it does not prove getter heap acyclicity or VM execution.

The known pure `listShow` binding is lowered to a closed recursive template
using ordinary closures, matches, calls and concatenation, not an uncounted
host callback or a new VM intrinsic. Its complete callback/copy/stage/shape
adapter is checked against the declared builtin contract. Elements remain
immutable counted values, and callback arguments are independently owned;
this discharges only that known adapter's defensive-copy obligations.
Callbacks run once per element, left to right; empty input never invokes
one, and returned strings are concatenated verbatim. Nested list instances,
custom Show dictionaries, partial application, captures and error/fuel
unwinding use the existing ownership machinery. This is a quadratic-copy
baseline: bound flat-list tails cumulatively copy Θ(n²) elements, and
immutable concatenation can cost O(n × output length). Fuel meters the
existing VM and pattern work, not every copied string byte. This support
is not a linear-time or FBIP performance claim, nor support for arbitrary
foreign callbacks.

The exact pure `listMap`, `listEquals`, `listCompare` and `listHash` contracts
also lower to closed ordinary IR. Mapping invokes each callback once before
recursing, and preserves escaped callback results and captures. Equality checks
the complete lengths before any callback, then short-circuits left to right;
ordering is lexicographic and stops at the first non-EQ result. List hashing
wraps `h * 31 + element_hash` from seed 7. All six live adapter records, including
rank-two and NonEmpty mapping, are pinned with their complete metadata. These
are still quadratic-copy baselines, not general FFI or FBIP support.

`String.join` similarly lowers to a closed recursive template over immutable
counted strings. Empty input returns empty text, a singleton is unchanged,
and the separator occurs only between parts. Its complete pure, two-argument
adapter is checked, including both copy contracts. Partial application keeps
the separator owned; repeated calls and every fuel prefix through success
are checked with an independent external separator alias. It shares the
quadratic-copy limitation above and does not add a new VM intrinsic.

Constructor applications receive dictionaries before ordinary fields through
the existing unary application ABI. At saturation, counted storage places
ordinary fields first and dictionary evidence afterward. Pattern-bound
evidence is available in guards and bodies. This is deliberately not the
substitution interpreter's internal dictionary-first payload layout.
The boundary is stored explicitly, never inferred from a field's record or
function shape. Structural equality compares nominal identity and ordinary
payload only; evidence contents and widths do not participate. An ordinary
record-shaped payload is still compared normally, and ordinary closure
payloads remain unsupported. Primitive identity equality recognizes a
payload-free constructor by its ordinary arity, even when it owns evidence.
This does not add Show or Ord instances for constrained data.

Option's nonlocal `?`, local mutation, generators,
one-shot control/resume, nonconstant Layer constructors and memoization,
scoped finalizers/owned work and most host bindings still need their real memory
semantics. Lowering reports the unsupported form and original source span;
it never replaces a task with an ordinary lambda or a nonlocal return with
a helper return. No corpus-gap manifest has been loosened.

### Typed failure and exceptional roots

`Try` consumes a body closure and an error-dispatch closure. The body is
called with Unit only after installing the catch. A normal return drops the
unused handler; a raised payload removes the catch before invoking its
handler. Pattern selection, Or bindings and guarded fallthrough use the same
match helpers as ordinary matching, except unmatched error dispatch raises
the original payload instead of producing `NoMatch`. Failures in a guard or
selected handler therefore reach an outer try, not the catch that selected
them. A returned task or closure does not retain that dynamic catch.

The known `fromResult` adapter is checked in full. `Ok` and `Err` must be the
nominal Prelude Result constructors with exactly one ordinary field and no
evidence. The payload is retained before releasing its wrapper. Malformed
Results, evaluator faults and fuel exhaustion are not typed failures and
cannot be caught by an ordinary try. Result's `?` already lowers through
`from_result`; Option's source-function early return remains unsupported.

The VM roots the raised value and any pending handler before fallible entry
or frame-pop transitions. Abandoned frame locals and catch-owned handlers
are released by ordinary drops, not by executing skipped annotation streams
or scanning reachable objects. Unwind closes the exact saved Scope owner
and restores its predecessor; Within restores selection without closing the
selected owner. A failed Ref.modify callback does not commit a replacement.
A failed global initializer removes its in-progress cache marker, allowing
a caught attempt to be retried. Unwind consumes ordinary execution fuel;
final root cleanup runs even after fuel exhaustion.

`execute_program_outcome` and `execute_main_program_outcome` report stdout
with `Returned(Value)`, `Raised(Value)` or `Fault(Error)`. The first two
transfer one caller-owned token after execution roots have been released;
a fault retains no language payload. Source counterparts
`run_main_counted_outcome` and `run_file_counted_outcome` take the caller's
heap: compilation/lowering errors remain outer errors, and returned or
raised values must be explicitly dropped before `Heap::finish`. External
aliases survive. Legacy success-only wrappers retain their signatures,
release an uncaught typed payload and return a nonowning `UnhandledFailure`
error; they still do not expose stdout preceding failure.

The existing source calculus defines this error behavior. The normal-path
token-checking and whole-insertion Lean results do not prove exceptional frame
unwinding, operation contracts, source lowering or heap correctness.

### Owned operation-handler boundaries

Operation lookup selects the innermost active handler with an arm for the
requested operation, not just its effect. A source partial application owns
its dictionaries and preceding arguments in ordinary counted closures;
lookup happens only at the first runtime stage, in that call's context.
Unmentioned operations pass outward. Complete service providers are not
partial handlers and remain shadowing boundaries. Legal operation names,
including `__proto__`, are ordinary own fields in the JS runtimes.

An activation owns its operation, return and error functions. A tail call
parks its crossed frames as explicit roots and invokes the arm outside that
handler and those frames, with the handler's outer owner and evidence.
Success restores the parked body; an arm failure discontinues it before
propagating outward, bypassing catches in the body and that handler's own
error arms. Abort closes the departed scopes before invoking its arm and
does not apply the handler's return function. Normal body return removes
the activation before calling that function. At the raw memory-IR boundary,
a zero-invocation operation can return its installed owned value without an
invented Unit application. Source effects still require a runtime arrow
(`FLICK-E006`); this does not admit constant effect declarations.

Pending curried invocations, parked segments and targeted discard worklists
remain explicit owned roots. Transfers and frame unwinding consume fuel;
terminal cleanup drains active and parked roots even after exhaustion,
without sweeping heap storage or dropping caller-owned aliases. Source
regressions cover constrained operations, delayed tasks/closures, outer
contexts and independently specified outcomes across both profiles, with
fuel-cut checks after explicitly releasing returned or raised values.
The Lean insertion theorem accounts for the listed operand multiplicities,
not the dynamic correctness of these VM transfers or their physical heap.

### Constant Layers and complete providers

`ServiceTag(key)` allocates an opaque counted nominal tag, not a language
String or record. A heap-local weak index canonicalizes live tags for the
same key; the index owns no language token. Last release retires its exact
key/object entry while freeing the tag. External aliases can preserve its
identity across executions sharing that heap, without a permanent tag root.
`LayerSucceed` consumes a tag and implementation into one counted immutable
Layer allocation; ordinary iterative destruction releases both fields.
Neither tag nor Layer exposes fields or qualifies for constructor reuse.

`LayerMerge` consumes two Layer tokens into an immutable counted graph node.
Shared branches retain independently owned edges. Merge and override use
rightmost service precedence; a selected complete implementation never falls
back to another branch merely because its method is absent. Lookup traverses
right-first with an explicit worklist and charges every visited node to fuel,
including repeated shared-DAG visits. Graph destruction is iterative. Merging
does not invoke methods, build acquisitions or install nested provider owners.
All branches run under the single owner opened by their enclosing `LayerRun`.

`LayerRun` owns its Layer while installing a complete provider, opens a real
owner, and invokes the original callback with Unit. It accepts any otherwise
supported callback body, not a recognized AST template. A matching provider
stops lookup before validating the selected record field: a missing field or
nonrecord implementation faults rather than falling through to an outer
provider. Unlike tail-handler arms, Layer methods keep the caller's frames,
owner and evidence active, including the provider itself for same-service
recursion. Their dictionaries and arguments use the existing rooted unary
invocation queue.

Provider exit removes its dispatch entry, closes the exact saved run owner,
restores the previous selection, and releases the Layer before delivering
the still-owned result or failure. The same boundary is traversed during
abort and tail discontinuation; parked providers remain owned but inactive.
Terminal fault/fuel cleanup drains registrations even when callback entry
failed before installing a frame. Scope/Within restoration, abandoned global
initializers and failed Ref.modify retain their existing unwind contracts.

Only the exact internal `layerSucceed`, `layerMerge`, `layerOverride` and
`layerRun` foreign adapters are
accepted. Their complete modes, stages, callback/copy metadata and nested
shapes are checked against the live builtin declarations. LayerRun's result
copy obligation is discharged by immutable counted values with independently
owned aliases and uniqueness-gated reuse; no value crosses a mutable host
boundary. This is not generic foreign-copy erasure or support for effectful
or memoized builders, parallel build/release, finalizers or cancellation.
The dynamic provider/heap semantics are tested, not proved by the Lean
insertion theorem.

### Root services

The counted main runner first evaluates the global `Host.live` and installs
the Layer it gives as the root provider `Σ₀` of `core.md` (premise G3). No
frame holds it: an operation that no frame's provider or handler answers is
answered by it, under the root owner, keeping the performer's frames, owner
and evidence as a frame provider does. Cleanup releases it with the other
execution roots. A service outside it is still a `MissingHandler` defect.

Its host primitives are intrinsics on a fixed host, the same as the
evaluator's natives: `stdoutLine` and `stdoutWrite` write the captured
stdout, `stderrLine` borrows its text and writes nothing, `platformEnv`
allocates `None` and `platformArgs` an empty list, the wall clock reads
1 700 000 000 000 ms and the same instant in nanoseconds, the monotonic clock
0 ns, and every `flickNext` draw is 0.5. `platformEnv` and `platformArgs` allocate their result
afresh in Flick's representation, which discharges the host result copy and
shape conversion of their bindings; this is not generic foreign-copy erasure.

### Pattern selection and immutable updates

`Match object pattern` borrows its receiver and returns a counted tagged
`$match` result: a Boolean followed, on success only, by independently owned
bindings in sorted common-name order. Shared iterative validation rejects
duplicate binders on one successful path, empty/inconsistent Or patterns,
duplicate record-pattern labels and inconsistent constructor arities, even
in unselected code. It is not a type or exhaustiveness proof.

Matching first selects nonowning locations, including virtual list segments.
Failed alternatives retain no values and allocate no language objects.
Only whole-pattern success commits retained bindings and fresh counted list
tails. Every intermediate owner remains in explicit VM scratch roots until
transferred; fuel exhaustion or error drops those roots normally. Traversal,
retries, normalization and materialization use the execution's existing fuel.
Opaque handle bindings copy metadata without dereferencing closed storage.

Or patterns commit the first successful alternative's bindings; a false
guard proceeds to the next arm, not another alternative. Direct arm-suffix
helpers evaluate the scrutinee once, preserve the original outer environment
for fallthrough, and emit each body and caller continuation once. Float
patterns use numeric equality (signed zeros match, NaN never matches), not
IR literal identity. BigInt patterns use unbounded numerical equality.
`MatchFail` is a zero-argument runtime error for a missing arm, not an
invented successful value. Unsupported parameter/let destructuring remains
an explicit lowering error rather than an assumed successful match.

`Update` consumes the base and every written replacement, evaluates them in
source order, and constructs a fresh record with the original field order.
It replaces existing labels only; shared bases and unchanged fields retain
their values. Raw IR duplicates use the last replacement and still release
every overwritten input (checked source rejects duplicate labels). This
operation does not mutate or reuse record storage. Matching/update retain
the current program-only fuel/cleanup contract; standalone `execute(Function)`
rejects them with `NeedsProgram` rather than running an unmetered variant.

## 3. Insertion and reuse

For an existing local with `u` consuming occurrences in the next operation
and continuation liveness `k ∈ {0,1}`, insertion emits

```text
copies(u, k) = u - (1 - k)       (saturating natural subtraction)
discarded(u, k) = 1 if u = 0 and k = 0, otherwise 0
1 + copies(u, k) = u + k + discarded(u, k)
```

Copies precede consumption. A last borrowed receiver is released after
projection, not before it. A value live only in one successor is dropped
on the other edge. Liveness is path-sensitive; adding the syntactic uses
of both successors would over-retain it.

The current reuse opportunity is a last borrowed receiver immediately
followed by a constructor. Insertion may keep the receiver's token until
that constructor and emit `Reuse candidate tag fields`. It may not move
the release across unrelated work or a control-flow edge.

`Reuse` consumes the candidate and the new field tokens. If the candidate
is a unique constructed object with the same field capacity, its old
fields are released and the same storage holds the new tag and fields.
Otherwise it releases the candidate token and allocates normally. Tag
equality is not required; layout compatibility is. Aliases, including
aliases among new fields, must survive both cases. Reuse never changes a
shared object in place and never reinterprets a string as constructor
storage. This dynamic fallback is mandatory even when static liveness
says the candidate local is dead: another local or field may still alias
its object.

`ReuseWithDicts` carries the same explicit payload/evidence partition as
construction. Eligibility still compares total physical capacity; reuse may
change the partition but replaces its metadata together with the tag and
fields. Both reuse and allocation fallback release every old physical edge,
including evidence, rather than discarding the observationally hidden suffix.

This includes borrowed matching: whole-scrutinee bindings retain an alias
and prevent destructive reuse; selected children are retained before old
fields are released. A dead `$match` result is ordinary tagged storage and
can itself be reused when unique and layout-compatible. Reuse tests pin
both the shared fallback and these unique cases.

The Lean local-token theorem covers arbitrary natural use counts, both
continuation choices, and failed-prefix exclusion. It does not assert that
the Rust liveness calculation is correct or that reuse preserves heap
values. No theorem about the core's existing `Ref` store is a counted-heap
proof by itself.

## 4. Arena lifetime and by-use regions

The public API is `Arena s`, indexed by an existing region `s`, not by a
backend address space. Arena-backed allocations are released as part of
closing that region on success, typed failure, defect and interruption.
Close first waits for owned work and runs the registered finalizers while
their data is live; only then may backing storage be reclaimed. Closing
twice must not release storage twice.

`packages/std/src/Arena.fl` exposes opaque `Arena s` and `Value s a` with
`make : Unit ->{Scope s} Arena s`,
`alloc : Arena s -> a^c ->{} Value s (a^c)`, and
`get : Value s (a^c) ->{} a^c`. `make` acquires into the owner selected by
`Scope s`; it need not be the innermost scope. `alloc` moves an owned value
into a strong slot and returns nonowning metadata. `get` retains an ordinary
immutable value, not a borrowed pointer to arena backing storage, so a read
result may survive close. Captures of that value remain unchanged.

An arena-backed view must retain sufficient metadata to reject stale use
in the model without reading freed storage. Compiler rules require the
region live at each operation that dereferences its storage. A view which
is retained but never dereferenced after close is checked separately under
RG4: this contract does not restore a blanket `G001` for merely returning
or holding it. Dropping such a retained view must itself be safe and must
not dereference an already freed arena. Any ownership edges from arena
objects to ordinary counted objects are released during arena close.

The counted machine owns real `Data::Arena` slots and private roots per
scope. Normal return detaches all contents before dropping those roots and
resuming the caller; error/fuel cleanup drains those same explicit roots.
An internal stepwise test checks actual reclamation while the caller is
still running, not merely after program teardown. Borrowed handles contain
heap/execution provenance and never reconstruct counted ownership.
`Within` lowers to a lazy direct helper under the selected owner, restoring
the caller's owner afterward; it does not evaluate the acquisition first.
Selection validates the owner identity's heap, execution and index, not whether
its storage has retired. An inert helper may therefore return under a retired
owner, without reopening it or changing lexical evidence. Actual `ArenaMake`
checks that selected owner's admission state; `ArenaAlloc/Get` check their
actual arena handle, so an unrelated live arena stays usable. Model tests pin
both inert selection and real acquisition rejection, default-owner/evidence
restoration, foreign identities and fuel-prefix cleanup with an external alias.
Ordinary `Ref` cells remain root-owned, independently of these scopes.

The implementation separates default work/cancellation owner D from inert
lexical owner evidence C. `ReadOwnerEvidence` reads C; `WithOwnerEvidence`
changes C only around an immediate application/await and restores it on exit.
Its callee/argument/task operands are already-evaluated locals or literals,
spilled outside the bracket in source order. Optimizer substitution must retain
that boundary. Absence is Unit, not the root owner. Transporting a closed handle
does not validate, reopen, retain or register cleanup with its owner.

Source arrow/task entries save their evidence by lexical entry identity; a
fresh scope supplies fresh C. Providers change D without overwriting C. Raw
foreign stages activate D only for Scope explicitly present in the original
declaration, not a widened expected row. This carries evidence through List,
opaque Box and other ordinary containers without reconstructing their values
or changing physical callable arity. Scope-free work keeps provider-default D.
Recursive foreign-result adapters remain under correction: lower-only tests do
not prove emitted native behavior, productive/unproductive alias handling or
returned Sync/Forall calling conventions. Arbitrary host-produced executables
also require an explicit FFI protocol; this is not universal G6 completion.

JS and Go mirrors clear strong payload slots in a private owner phase after
all finalizers, before C6. They do not use an ordinary LIFO resource
finalizer. Runtime tests cover allocation during finalization, failure,
interruption and idempotent retirement; host GC is still not a proof of
counted reclamation. The substitution model separately models strong slots
and their retirement after successful finalizers. Its existing selected-owner
translation and failed-finalizer limitations are not discharged by these tests.

`tests/run/runtime/arena_scope.fl` checks copied-out values and harmless
retention; `arena_finalizers.fl` checks a finalizer registered before arena
creation reading its view through a Ref assigned later. Both use independent
stdout expectations. `tests/check/fail/regions/arena_after_close.fl` pins
`G002` at the invalid read and its inferred scope-result obligation;
`tests/check/pass/regions/arena_holding.fl` separately accepts RG4 retention.
No escape-only `G001` is introduced. `arena_layer_scope.fl` additionally
checks lexical ownership across literal/named callbacks, generic record and
tuple consumers, named tasks, callback/task results, pipes and higher-rank
scope entry. `memory_lexical_owners.rs` sweeps fuel cuts in both profiles,
retaining an independent caller root and checking actual heap/arena balance.
`arena_layer_finalizers.fl` requires the caller's finalizer to run after
the provider returns, while its arena is still readable. The counted runner
still rejects scope finalizers instead of erasing their effects; completing this path
for the whole corpus remains an M1/M2 obligation.

`Formal/MemoryScopes.lean` models one private storage-object owner per arena
and arbitrarily many strong slots, rather than one cell object per slot.
It proves exact per-object multiplicities for make/alloc/get, detach-all
and closing a selected owner in arbitrary other-owner and external-root
contexts. Closed owners carry no roots and repeated close releases nothing.
The theorem is conditional on reaching the release boundary; it does not
prove finalizer execution, handle validity or physical reclamation.

## 5. Cycles through `Ref`

Plain counted constructor/closure edges form an acyclic ownership graph.
Recursive code uses lexical code/self references, not strong self-owning
closure fields; its lowering must preserve that invariant.
Reference counting alone cannot reclaim `Ref → closure → Ref` cycles.
The phase-M policy is therefore **arena-owned mutable cells**, not a
tracing collector and not weak `Ref` contents:

1. Ordinary `Ref.make` allocates a cell in the execution's root cell arena.
   This root outlives lexical scopes, preserving existing `Ref` behavior;
   it does not silently bind an ordinary `Ref` to the nearest `scope`.
2. A cell's contents are strong owned values. Reads retain them; updates
   retain/transfer the replacement before releasing the previous contents.
   Existing invariant capture-capacity and `Live` rules remain in force.
3. A cell handle does not own the cell's contents through a counted graph
   edge. The arena owns the cell and the contents slot. Thus cycles may
   pass through cells, but are not cycles in the counted ownership graph.
   Dropping the last handle need not reclaim the cell's contents early.
4. At arena close, after work/finalization has stopped, all cell contents
   slots are detached before their owned values are dropped. Cell storage
   and stale-handle metadata are then retired. This breaks every cycle
   through cells without tracing reachability or changing strong reads.
5. Explicit region-owned cells, when added to `Arena s`, follow the same
   rule and the by-use contract of §4. Ordinary root-owned cells remain
   available until execution teardown. Foreign strong cycles require an
   explicit host ownership/release contract; they cannot be assumed safe.

The tradeoff is deliberate: unreachable mutable cells may retain memory
until their arena closes. This is deterministic region retention, not
prompt cycle collection. A zero-live-object gate must close the root cell
arena and release the program outcome and other runtime roots. It must
also demonstrate closure/cell cycles and replacement of old contents;
freeing every object unconditionally at shutdown is not that evidence.
The counted program runner implements this root-arena slice. Each cell is
an occupied `Data::Cell` heap slot with one private owner in `Machine.cells`;
the returned handle is nonowning and guarded by heap/execution identity.
`Ref.modify` calls its unary pure callback and commits the tuple
`(result, new contents)`, not the reverse. Teardown first takes every slot's
contents, then ordinarily drops the detached values and private owner
tokens. It does not scan the heap to dispose of unreachable objects.
Tests cover self/mutual cell-closure cycles, strong aliases, replacement,
callback errors and fuel exhaustion, replayed tasks and main's same-arena
function/task stages. Cell allocation/free counters and independently
checked occupied slots include the cells themselves.

`Formal/MemoryArena.lean` proves exact per-object token multiplicities for
make, strong reads, replacement in an arbitrary arena context, and close
composed with other roots. Detaching all slots preserves owners and leaves
every slot empty; close emits precisely the old owner/slot inventory.
Handles, including self/mutual handles, contribute no counted edge. These
are ledger theorems, not proofs of heap acyclicity, physical reclamation,
fresh identities, stale-handle rejection or the Rust implementation.
Arbitrary counted scope work, persistent host ownership and the whole-pass/heap
proofs remain separate obligations.

## 6. Platform and completion gates

M owns the common RC/arena contract and its model/Lean obligations.
Platform-independent portions of W2/W3 reuse this machinery. LLVM IR
lowering lives in the backend component; native code generation and linking
are host stages of N (revision 2026-11-13). Neither a native compiler nor
host garbage collection can discharge M1–M4.

Completion requires evidence on the full requested scope: all corpus
programs on counted execution with zero live objects after explicit
teardown; M2 release/use/retention tests; FBIP fallback and cell-cycle
tests; and the corresponding Lean build without extra axioms or `sorry`.
The implementation table is intentionally narrower than these gates and
must not be used to redefine them.
