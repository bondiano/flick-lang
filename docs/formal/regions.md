# λFlick: regions and captures

Extends `core.md`: everything not redefined here holds as there. Divergences from the spec, the checker and the built-in bindings, and how each is resolved, are in §6.

Regions are typed **by use**, the only discipline of the checker, the reference model and Lean: the qualifier `C` of a function or a task is what calling or awaiting it needs live, not a bound on what it holds. A region must be live where it is used; a closure or a task that holds a capability and never uses it may outlive the region. §2.2 and §2.3 give the rules by use, and §5 the theorem they keep, T4′, which gives up T4.2 for exactly that closure. The reach bound `⌈τ⌉` of §2.1 still measures what a type shows, for (Con), (Inst), errors and `Outlives`; the held premises of (Lam) and (Task) and the lemma L4 of the former capture discipline are gone, and the divergences of §6 that speak of them are history.

References are as in `core.md`. Checker paths are relative to `flick-core/src/types/`; bindings are `builtins/X.fl` in the same directory. The probes of §6 are built with `flick build` and run on Node with a JS resource that marks a closed handle and prints `USE AFTER CLOSE` if the handle is used after close.

## 0. Scope

Added to the core:

- the kinds `Region` and `CRow`;
- `scope s { e }`: a fresh region and its owner;
- the service `Scope ϱ`: the right to acquire a resource into region `ϱ`;
- the value `Owner ϱ` (`Fiber.Owner`);
- capture qualifiers `τ^C`;
- registration of work with an owner (children and finalizers) under the constraint `Outlives C ϱ`;
- resource acquisition: a constant with `Scope ϱ` in its service row;
- region-free errors.

| Not covered | Where | How the model accounts for it |
| - | - | - |
| Concurrent execution of children, cancellation, two-phase close | `runtime.md` (λRT) | A child of region `π` is work that may take a step at any moment while `π` is live; closing `π` waits for it to finish (§3.3). Step order and cancellation are given by λRT |
| The reader region of a lazy `for` (`reader_region`) and the region of a UI instance (`Setup s`) | — | The same rules with a different source of the region: a function or a component instance instead of `scope`. `Setup s`, like any service indexed by a region, keeps that region live where its row has it, and a handler of it is installed only where the region is live ([Scope keeps its region live](#scope-keeps-its-region-live); Lean `hdl_err_live`) |
| The internals of leases in `ScopedRef`, `Resource`, `ScopedCache` | hypothesis G5 (§5) | The value is available only inside `use`, with the lease region; release waits for the leases to end ([leases](#leases-instead-of-early-close)) |
| Classes, `do`, dictionaries | `elaboration.md` | A dictionary is built from top-level values and has no captures of its own |

## 1. Syntax

### 1.1 Kinds

```text
κ ::= … | Region | CRow
```

`CRow` is a capture row (spec/05 "CaptureRow"). Checker: `kinds.rs`, `Kind::Region`, `Kind::CaptureRow`.

### 1.2 Regions, captures, types

```text
ϱ ::= s                          variable of kind Region
    | π                          runtime region (§3)

C ::= ∅ | {k₁, …, kₙ} | c | C ∪ C        capture set, c is a variable of kind CRow
k ::= ϱ | α                       atom: a region or a variable of kind Type

τ ::= … (core)
    | τ^C                         qualified type
    | H ϱ̄ τ̄                       nominal type with parameters of kind Region: File s, Server s
    | Owner ϱ                     owner of a region

SRow element:  … | Scope ϱ

σ ::= τ | ∀κ̄. Θ ⇒ τ               Θ are constraints Outlives C ϱ
```

How this is written in the language:

- `T^{s, t}` is `τ^{s,t}`, `T^c` is `τ^c`, `T^(c + d)` is `τ^(c ∪ d)`;
- a bare `T` is `τ^∅`: no hidden captures beyond what `T` shows, except data with hidden storage written bare in a signature, whose hidden row elision reads (§1.4);
- `Fiber.Outlives c s` is `Outlives c s`.

Equality of capture sets: `∪` is associative, commutative and idempotent, with unit `∅`. The normal form is a set of atoms and a set of variables `c`, as for core rows. The atom `α` is not written in the source language (`FLICK-T002`): it appears when ⌈·⌉ (§2.1) meets a type variable.

Qualifiers come in two sorts, by what they are attached to:

- **hidden**: on a function, on `Eff`, and on data with hidden storage (a function field, an opaque type, a foreign type marked `@stores`); `C` is added to what the type shows;
- **value bound**: on a type variable and on data that stores only its parameters; `C` bounds the whole value (reference/regions "For a generic value, `a^c` bounds the whole value").

Checker:

- `ty.rs` `Type::Captured` is the hidden qualifier, `Type::Bounded` the value bound;
- `regions/data.rs` `value_bound`, `bound_head`, `hidden_fields` decide which of the two applies;
- `regions/data.rs` `scalar`: scalars have no captures under any qualifier.

Lean: a hidden qualifier is the `C` of `Ty.arrow` and `Ty.eff`, or a parameter of kind `CRow` of data with hidden storage. A value bound on a variable of a scheme is a fact of the scheme, `Facts.bnd i D` (`formal/Formal/SchemeOps.lean`), and the body writes `α` for `α^{≤D}` ([value bounds in the core](#value-bounds-in-the-core)).

### 1.3 Expressions

```text
e ::= … | scope s { e }
```

`scope s { e }` binds `s` in the types of the body and `s : Owner s` in its values. `scope { e }` is the same with a fresh invisible name.

### 1.4 Desugaring the surface

| Surface | Model |
| - | - |
| `scope { e }` | `scope s { e }`, `s` fresh |
| A curried stage `a -> b -> c` in a signature | elision: each runtime arrow and `Eff` of the signature needs `Live` of what the earlier parameters reach, `a →R⟨r;ε⟩ (b →R⟨Live ⌈a⌉ + r;ε⟩ c)`; a pure stage needs nothing. Interfaces omit `Live`, and every use of the signature elides it again (`regions/live.rs` `elide`, `live_scheme`; Lean `Ty.elide`) |
| Data with hidden storage written bare in a signature: `G s a` for `type G s a = G (s ->{FileSystem} a)` | elision of its hidden row, by where it is written: in a parameter, a fresh variable of kind `CRow` the scheme quantifies; in a result (after the arrows of the parameters), `⌈·⌉` of the parameters before it, its atoms of kinds `Region` and `CRow`, as for `Live` above. `make : Handle r -> G Unit Int` reads `∀r. Handle r → (G Unit Int)^{r}`, and `compose : G a b -> G s a ->{FileSystem} G s b` reads `∀a b s c d. (G a b)^c → (G s a)^d →R (G s b)^{c, d}`. A written qualifier turns it off: `(G Unit Int)^{}` holds nothing, and the body is checked against it ([the hidden row of data is not what its arguments show](#the-hidden-row-of-data-is-not-what-its-arguments-show)) |
| A capture-row variable of a signature, `^v` (not a region `^{t}`), that only the arrow of one parameter mentions, not on that parameter's own qualifier, and that bounds a parameter of that arrow: `(a^v -> b)^c -> (f a)^d -> (f b)^c` | a `∀` of the callback, at its arrow inside its qualifier, binding such variables in the order they are first written: `(∀v. a^v → b)^c → (f a)^d → (f b)^c`. The checker rewrites the signature when it converts it, before the hidden rows and `Live` are read (`types/convert/callbacks.rs` `localize_callback_rows`), so the model, the generated `PreludeSchemes.lean` and `ElideSchemes.lean` and every interface see the explicit scheme. A variable mentioned anywhere else stays quantified over the whole scheme (`c` and `x` of `Ref.modify`, `c` of `Rpc.listen`'s `hs^c`), and a region binder (`forall t.` of `ScopedRef.use`) is written |
| `fun`, `task` without an expected type | the qualifier is a fresh variable of kind `CRow`; the solution is the least one (§2.6) |
| A call with `Scope ϱ` where `ϱ` is not yet known | `ϱ` is the region of the `Scope` named by the stage row (a task against `Eff {Scope t \| r} e a`), otherwise of the nearest `scope` (`regions/scopes.rs` `lexical_owner`, `declared_scope`) |
| A call with `Scope ϱ` of an outer region inside a nested `scope` | the requirement travels through the nested `scope`s to the one whose region is `ϱ` (`settle_needs`); codegen passes the owner lexically: `$rt.within(owner, …)` |

The hidden rows of data in signatures (the row of data with hidden storage in the table above):

- **Which data.** Data with hidden storage as Lean and the model have it (#217; `regions/mod.rs` `hidden` of the model): an ADT with a field that is a function or a computation, transitively through the fields of the ADTs it holds, opaque Flick types of std included. A foreign type without constructors gets a row when its declaration says its values keep functions, `@stores` (`FieldCodec`, `Codec`, `PureCodec` of `Prelude`, `Fields` of `Codec.Derive`): foreign code makes them and keeps the functions it is given, so the reading lets a bare result hold them, as G4 asks (§5.1). Another type without constructors gets no row: what a foreign binding can hide is the business of G4, not of elision ([a foreign type that keeps functions shows no hidden row](#a-foreign-type-that-keeps-functions-shows-no-hidden-row)). A native type, a `type` with `@native when` (the FFI reference, native types), is read as such a type on every target, whatever its constructors hold (`env.rs` `TypeDef::typing`): no row, invariant parameters. The Flick bodies that cover it stand for its bindings, so G4 holds them as it holds a binding, and the checker reports no bound of a value of that type in them (`ffi/represented.rs` `covers_native`, `solve.rs` `report_hold`). The model and Lean have no native types: to them a covering body is foreign code.
- **Where.** At the outer level of a parameter or of a result, of the signature's own stages and, as elision of `Live` reads it, of the stages of a parameter that is a function (Lean `Ty.fresh`, `Ty.elide`, `Scheme.elide`): in a parameter of a callback a fresh variable the scheme quantifies, in the result of a callback what the callback's own parameters reach. `apply : (G a b -> Int) -> G a b -> Int` reads `∀a b c d. ((G a b)^c → Int) → (G a b)^d → Int`, and `with_file : (File s -> G String) -> …` reads `(File s → (G String)^{s}) → …`. A written qualifier on an arrow does not stop the reading, as the core keeps it as a field of the arrow: `lift : (a -> b)^c -> (G a -> G b)^c` reads `∀a b c d. (a → b)^c → ((G a)^d → (G b)^{c, d})^c`, as its curried form would. Under a `∀` of the signature (`forall t. …` in a parameter or a result) elision goes on as `Ty.elide` of `.all` reads it (`formal/Formal/Live/Elide.lean:39`), but `Ty.fresh` does not enter it: a parameter there keeps the row `∅`, and a result holds what the parameters before it reach of the atoms outside the `∀`. `Caps.under` counts none of its binders, for `Live` as for the hidden row. `(forall a. File s -> a -> G a) -> …` reads `(∀a. File s → a → (G a)^{s}) → …`, while `(forall t. File t -> G String) -> …` keeps `G String` at `^∅`, so a callback for any region returns no data over the file it is given. A binder named as an atom the row carries is renamed: `File s -> (forall s. s -> G String)` holds the outer `s` ([a binder of a `∀` in a signature counts for nothing](#a-binder-of-a--in-a-signature-counts-for-nothing)). A constant without arrows (`x : G s a`) and data inside a type argument (`List (G s a)`, `Option (G s a)`) get the row `∅`, as elision of `Live` restarts inside a type argument.
- **When.** The checker reads the rows once, when it converts the signature, and keeps the converted scheme; each use instantiates it.
- **What a user sees.** Like `Live`, a row the checker read is not shown in diagnostics or hover: messages show the signature as written. When a read row is why a value cannot leave its `scope`, the message names the region it holds: `FLICK-G001` at the value of the `scope` ("this value holds region `s`, but the expected type `G Unit Int` holds no captures"), because a bare result after parameters that reach nothing reads as `^∅` and the value is compared with it before the `scope` ends. A read variable is named "the hidden row of a parameter's data".

### 1.5 Constants Γ₀

```text
open_H    : ∀s. τ_a →R⟨{Scope s}; ε⟩ H s               acquisition
op_H      : ∀s. H s →R⟨r; ε⟩ τ                         use of a capability

Fiber.spawn :
  ∀s c r e a. Outlives c s ⇒
  Owner s → (Failure e →R⟨{Scope s} + r;none⟩ Unit)^c → (Eff ({Scope s} + r) e a)^c →R⟨r;none⟩ Fiber e a

Scope.add_finalizer :
  ∀s c r. Outlives c s ⇒ (Eff r none Unit)^c →R⟨{Scope s} + r; none⟩ Unit

Scope.run :
  ∀r e a c. (∀s. Owner s →R⟨{Scope s} + r; e⟩ a)^c →R⟨r; e⟩ a

Ref.make  : ∀c α. α^c →R⟨none;none⟩ Ref (α^c)
Ref.get   : ∀c α. (Ref α)^c →R⟨none;none⟩ α^c
Ref.set   : ∀c α. (Ref α)^c →P (α^c →R⟨none;none⟩ Unit)^c
```

Sources:

- `open_H`, `op_H` are the scheme for `File.open`, `File.read_handle` (`builtins/File.fl`) and any `foreign` binding with `Scope s` in its row. Acquisition registers the release with the owner of `s`, and only closing `s` releases the capability (G4). The `retention` annotation speaks about the arguments, not the result: for `File.node_open` it is `call`.
- `Fiber.spawn` is in `builtins/Fiber.fl`, `Scope.add_finalizer` in `builtins/Scope.fl`. The finalizer's `Outlives c s` is required because [owner retention requires `Outlives`](#owner-retention-requires-outlives); the child's rows name `Scope s` because [children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner).
- `Resource.own : Close a => a^{} ->{Async, Scope s} a^{s}` of std adds no constant: it is Flick code over `add_close`, a foreign of `flick:Scope` that `Resource.fl` declares privately and `Scope.defer` exposes, which registers a finalizer as `Scope.add_finalizer` does, with `retention owner`, but parametric in the outcome `Exit e b` of the body. It registers `close_exit token exit value` with the token `Closing s`, whose constructor only `Resource` sees and which a `Close` instance receives at a region of its own (`forall t. Closing t -> …`), so it cannot keep it. Its argument holds no region by (Inst), so what the finalizer holds is `s`, and `Outlives {s} s`.
- `Scope.defer` and `Resource.make` add no constant either: the first is that foreign under a public name; the second is Flick code that acquires under `masked` and registers its release through it.
- `Ref.*` are in `builtins/Ref.fl`: the capacity `c` sits inside the invariant argument of `Ref`, so it cannot be widened through the outer bound of the reference.

Lean: `Const.ty`, `Const.ok` (`formal/Formal/Syntax.lean`). A cell type is `Ref τ` with `τ` the contents, as in `core.md`, and the capacity `c` is the instance `ι.C` of the constant, fixed for each use: `Const.ok` of `Ref.*` is the premise (Inst) of the bound `α^c`, `⌈τ⌉ ⊆ c` up to the facts, and the arrow `Ref.set ℓ` has the qualifier `c`, so storing needs the capacity live. `LiveRules.lean` checks both: `Ref.make` rejects a capacity smaller than its contents, and `refSet_needs_capacity`.

## 2. Statics

### 2.1 The reach bound ⌈τ⌉

`⌈τ⌉` is the set of atoms a value of type `τ` can reach: the regions of its capabilities and the captures of the values substituted for variables.

```text
⌈α⌉            = {α}
⌈B⌉            = ∅                     B is Int, Float, BigInt, String, Bool, Unit, Never
⌈H ϱ̄ τ̄⌉        = ϱ̄ ∪ ⋃ ⌈τᵢ⌉
⌈T ā⌉          = ⋃ ⌈aᵢ⌉ over arguments of kinds Type, Region, CRow, ERow    arguments of kind SRow are excluded
⌈f ā⌉          = {f} ∪ ⋃ ⌈aᵢ⌉ over the arguments ⌈T ā⌉ counts      f is a variable of a higher kind
⌈Owner ϱ⌉      = {ϱ}
⌈Ref τ⌉        = ⌈τ⌉
⌈{ℓᵢ : τᵢ | ρ}⌉ = ⋃ ⌈τᵢ⌉
⌈τ₁ →φ τ₂⌉     = ⌈τ₁⌉ ∪ ⌈τ₂⌉
⌈Eff r e τ⌉    = ⌈τ⌉
⌈Lazy r e τ⌉   = ⌈τ⌉ ∪ ⌈e⌉
⌈Layer ri e ro⌉ = ⌈S.Impl⌉ = ∅
⌈Errors ρ⌉     = ⌈ρ⌉
⌈ρ⌉            = ⋃ ⌈ῡ⌉ over elements E ῡ ∈ ρ, and the row variables of ρ     ρ of kind ERow

hidden qualifier:        ⌈τ^C⌉ = ⌈τ⌉ ∪ C
value bound:             ⌈τ^C⌉ = C ∪ (regions of τ)       τ is not a type variable
bound on a variable:     ⌈α^{≤C}⌉ = {α^{≤C}}
```

`⌈Errors ρ⌉` is the union over the elements because `Errors {E ῡ} ≡ E ῡ` ([`Errors` reaches its payloads](#errors-reaches-its-payloads)). A row variable `e` of kind ERow is an atom, as a type variable is: an instance `e := {Wrapped (File s)}` reaches `s`, so without it `⌈τ[e := ρ]⌉ = ⌈τ⌉[e := ⌈ρ⌉]` fails and a closure over a value of `Errors e` would claim to hold nothing ([an error-row variable reaches what its errors reach](#an-error-row-variable-reaches-what-its-errors-reach)). The error row of `Eff r e τ` and of an arrow is a row of its stage and is not in ⌈·⌉.

**Bounded atom**: `α` together with the fact "`α` is no wider than `C`" ([bounded variable atoms](#a-bounded-variable-reaches-what-the-variable-reaches)). A bound on a variable appears only together with the premise (Inst) `⌈υ⌉ ⊆ C` for the value `υ` substituted for `α`: the premise is checked by whoever instantiates the scheme, or becomes an obligation if `C` grows. Hence `α^{≤C}` reaches the same as `α`: under substitution both give `⌈υ⌉`. How such an atom compares:

- `K ⊆ X` with the atom `α^{≤C}` in `K` holds if `α ∈ X` or `C ⊆ X`;
- in `X`, the atom `α^{≤C}` admits only `α`: `C` may be wider than `⌈υ⌉`;
- in the other judgments (§2.4, §2.5) `α^{≤C}` reads as `C`; this is not weaker.

Lean: `α^{≤C}` is the variable `α` under the fact `Outlives α C` (`Facts.bnd`), and `Outl Θ K (.caps X)` is `K ⊆ X` up to the facts (`Outl.caps_iff`). Facts are not chained (`formal/README.md`), so the atom fits into `X` if `α ∈ X` or `X` is `C` itself; (Inst) asks no more, as it compares an instance with the instance of its bound.

Why service rows are excluded from ⌈·⌉: a value of type `Eff r e τ`, `τ₁ →R⟨r;e⟩ τ₂` or `Layer ri e ro` is used only through `await`, application or `provide`, and these add `r` (`ri`) to the stage of the user. A region from the row, for example `Scope s`, appears in the type of every place where the value runs; an unused value reaches nothing.

Why the result is included: the result can be discarded (`ignore (await t)`), and a region the type showed only in the result would vanish from the type of the enclosing value. Hence `⌈Eff r e τ⌉ ⊇ ⌈τ⌉`, as for a function. Treating `Eff` as opaque is unsound ([the reach bound of `Eff` includes its result](#the-reach-bound-of-eff-includes-its-result)).

Why the error row of `Lazy` is included ([`lazy.md`](lazy.md) §2): a lazy keeps the outcome of its initializer once it is forced, a typed failure too, so it holds the errors of its row, as a value of `Errors e` does; a computation `Eff r e τ` holds no error, which each `await` raises anew. Its captures are its hidden qualifier, as for `Eff`: `⌈Lazy r e τ^C⌉ = ⌈τ⌉ ∪ ⌈e⌉ ∪ C`. So a lazy that holds a `File s`, or may fail with an error that does, does not leave `scope s` (`FLICK-G001`/`G002`), and no lazy is forced after its region closes. Lean: `Ty.lazy` and its `Ty.reach` (`formal/Formal/TyBase.lean`); the store type of a cell, `Ty.lazyCell`, reaches the same.

Why the parameter is included: a closure whose parameter type shows `s` may omit `s` from its qualifier. Narrowing the parameter by subtyping (§2.2) then compares ⌈·⌉ of the whole value, not just the qualifier.

**Substitution.** `⌈τ[α := υ]⌉ = ⌈τ⌉[α := ⌈υ⌉]`, `⌈τ[e := ρ]⌉ = ⌈τ⌉[e := ⌈ρ⌉]` and `⌈τ[c := C]⌉ = ⌈τ⌉[c := C]`. For a value bound, substitution is allowed only if its condition holds (§2.3, (Inst)).

Checker:

- `regions/mod.rs` `captures_of`, `collect_bounded_captures` compute ⌈·⌉: `Layer`, `ServiceTag`, `ServiceImpl` are opaque, arguments of kind `ServiceRow` are excluded, and an argument of kind `ErrorRow` other than the row of an `Eff` reaches what its errors reach and its row variables (`error_row_captures`) ([the reach bound of `Eff`](#the-reach-bound-of-eff-includes-its-result), [service row variables](#service-row-variables-are-not-captures)); a curried stage retains the error-row variables of earlier parameters as it retains their type variables (`stages`); a qualifier on a value that can hold nothing (a scalar, data without hidden storage and without captures in its type) adds nothing (`regions/data.rs` `holds_nothing`, [value bounds in unknown positions](#a-value-bound-survives-an-unknown-type-position));
- `regions/mod.rs` `shown_by` is what a type shows openly: exactly ⌈·⌉;
- `regions/compare.rs` `expanded` replaces the atom of a variable already solved to a type by ⌈that type⌉.

### 2.2 Subtyping

A call of `(τ₁ →ψ τ₂)^C` runs at the use stage `use(ψ, C)`: `ψ` with `Live C` added to its row of services when `ψ` is a runtime stage, and `P` when `ψ = P`. Because `use(P, C) = P` is substitution-invariant (a pure arrow `(τ₁ →P b)^C` over a type variable `b` must not move `C` onto `b` after instantiation), an outer surface qualifier on a syntactically curried or rank-2 callable that starts with a pure arrow and syntactically reaches a runtime stage (`(∀ᾱ. τ₁ →P … →P τₙ →R⟨r;ε⟩ τ)^C`, `(∀ᾱ. τ₁ →P … →P Eff r ε τ)^C`, `(∀ᾱ. τ₁ →P … →P Lazy r ε τ)^C`) is placed onto that inner callable stage during signature/annotation normalization, before type-variable instantiation (unioning with any existing inner qualifier), matching Lean's `LeLive.arrow` and `ElideSchemes.lean`. Whole-value bounds (`τ^{≤C}` from qualified records, `Resource.acquire_release`, or `Ref.get`) carry `C` across pure projections and applications onto inner runtime stages on the actual/elimination side without affecting expected positions or all-pure functions. Subtyping compares use stages:

```text
τ₁' ≤ τ₁    τ₂ ≤ τ₂'    use(ψ, C) ⊑ use(ψ', C')    ⌈(τ₁ →ψ τ₂)^C⌉ ⊆ ⌈(τ₁' →ψ' τ₂')^C'⌉
──────────────────────────────────────────────────────────────────────── (≤-Use)
(τ₁ →ψ τ₂)^C ≤ (τ₁' →ψ' τ₂')^C'
```

and likewise for `Eff r ε τ`. A qualifier may move into the runtime row of a supertype, but not disappear: `(Unit →R⟨∅;∅⟩ Unit)^{s}` is not a subtype of `Unit →R⟨∅;∅⟩ Unit`, even if a parameter or the result shows `s`. Lean: `Stage.use`, `LeLive`, `LeLive.arrow_use`, `LeLive.eff_use` (`Live/Mode.lean`), checked in `LiveRules.lean`. Consequences:

- widening a qualifier is allowed, narrowing is `FLICK-G001`;
- a function or task value, a pure one too, fits only where the reach of the position covers what it holds (the last premise, `Caps.Sub … reach` of `LeLive.arrow`, `Live/Mode.lean`; `Ty.reach` of an arrow includes its qualifier, `TyBase.lean`); for one with effects the use stages are compared besides;
- what a function position's type shows, and what a value bound's type shows (`a` of `List a`), counts as visible, not held;
- inside `Ref` types are compared by `≡` together with their qualifiers, mutable storage is invariant (`FLICK-T003`); inside lists and other data they are compared by the variance of each parameter ([§2.2.1](#221-variance-of-data-arguments)); a record is immutable and compares field by field, covariantly, with the same labels: `{ℓ : τ, … | ρ} ≤ {ℓ : τ', … | ρ'}` when `τ ≤ τ'` for each field and the tails compare likewise, keeping what it reaches (Lean: `LeLive.rcons`, `Live/Mode.lean`);
- the hidden row of data with hidden storage is one of its arguments, and subtyping may only widen it: `(T C ā) ≤ (T C' ā)` when `C ⊆ C'`, the other arguments by `≡` (Lean: `LeLive.hide`, its condition `Sig.hidden_ok`; `Formal/Widen.lean`). Data with a smaller row passes where a larger one is expected, never the reverse, as for the qualifier of a function. Only the row the reading of §1.4 adds widens, and only while it is covariant: a well-formed signature has it in the fields only as the qualifier of a stored function or computation, or as the hidden row of stored data, at the top of a field, never under `Ref`, inside an argument of another type, in a contravariant position, or fixed by the result of a GADT constructor. A capture row a declaration names (`type K (c : CaptureRow)`) is invariant ([a capture row a declaration names is invariant](#a-capture-row-a-declaration-names-is-invariant)). Data inside an argument of another type (`List (G s a)`) follows the variance of that parameter ([§2.2.1](#221-variance-of-data-arguments)); a region another argument shows does not stand in for it: `(G (File s) a)^{s}` is no `G (File s) a`, because code generic in that argument no longer sees the region, and the use the row records would be lost ([the hidden row of data is not what its arguments show](#the-hidden-row-of-data-is-not-what-its-arguments-show));
- `α^{≤C} ≤ α` and `α ≤ α^{≤C}` by §2.1: both reach `⌈υ⌉`. Hence `Option (a^{≤v}, b^{≤w}) ≡ Option (a, b)` ([bounded variable atoms](#a-bounded-variable-reaches-what-the-variable-reaches)).

Checker:

- `effects.rs` `subsume_at` → `regions/compare.rs` `subsume_captured`, `held_fits`; `regions/data.rs` `subsume_bound`, `bounded_callable`;
- the ⌈·⌉ check is not done by `subsume` itself but by the `Hold` obligation (`regions/mod.rs` `hold`) with the `visible` part of the expected type: what the type shows is subtracted from what is held (`regions/solve.rs` `hidden`); subtracting it from the hidden row of data is a divergence ([the hidden row of data is not what its arguments show](#the-hidden-row-of-data-is-not-what-its-arguments-show));
- `regions/compare.rs` `unify_captured`: qualifiers under an invariant constructor are equal.

#### 2.2.1 Variance of data arguments

Each parameter of a data type has a variance, `+`, `−`, `inv` or `bi` (unused), and subtyping compares the arguments of data by it:

```text
for each i:  +: uᵢ ≤ uᵢ'    −: uᵢ' ≤ uᵢ    inv: uᵢ ≡ uᵢ'    bi: any        ⌈uᵢ⌉ ⊆ ⌈uᵢ'⌉
──────────────────────────────────────────────────────────────────────────── (≤-Data)
T ū ≤ T ū'
```

The last premise is that of (≤-Use), argument by argument: subtyping never hides what a value reaches (`LeLive.reach`, `Live/Order.lean`), and a field built at one argument is read at the other, so each argument keeps what it reaches. A `−` parameter, whose argument narrows, therefore changes only what does not reach a region, for example the stage of an arrow. So `List ((Unit →P Unit)^{s})` passes where `List ((Unit →P Unit)^{s,t})` is expected, and the widening of the hidden row (§2.2) is the case of its `+` parameter. The variance is inferred from the fields, not written: the least fixed point over a group of declarations, starting from `bi`, where a field contributes the polarity of each position a parameter occupies:

- an arrow: its parameter `−`, its result and qualifier `+`; its stage does not hold the parameter;
- `Eff r ε τ C`: `τ` and `C` `+`; its rows `r` and `ε` do not hold the parameter;
- a record: its fields `+`; another data type: the variance of its parameter, composed;
- `Ref τ`, a handle, an owner and a dictionary: `inv` (mutable or shared storage);
- a stage and `Errors ρ`: `inv`.

A `+` parameter stands in no contravariant position, not even under a second flip: there its argument would grow what the field reaches where subtyping asks it to shrink. Lean: `Ty.Pol`, `Sig.var_ok` (`Syntax.lean`).

Indices of GADT constructors. Matching on a constructor refines an argument to equal `res K[j]σ`. Widening that argument would make the branch assume a type the value does not have:

```flick
type Tag a =
  | Pure : Tag (Unit -> Unit)

use : forall a. Tag a -> a -> Unit -> Unit
let use t f =
  match t with
  | Pure -> f
```

If `Tag` were covariant in `a`, `Pure : Tag ((Unit -> Unit)^{s})` would reach the branch, which returns `f` as a pure function, and a callback using `s` would leave its `scope`. A parameter `j` that some constructor fixes, with `res K[j]` other than `v j`, gets a sign only when `res K[j]` is closed in that direction (Scherer and Rémy, *GADTs meet subtyping*, ESOP 2013): upward closed for `+`, downward for `−`, checked syntactically, and the fields of `K` do not hold `v j`. The check takes types subtyping moves neither way: scalars and nominal types without arguments; a tuple, `List a`, an arrow or a qualifier is not closed (Lean: `Ty.Closed`, `Ty.Closed.le_eq`, `Sig.builds_vary`, `VaryArgs.lean`). Otherwise the parameter is `inv`, so `Tag` and the `Expr` of `tests/check/pass/types/gadt.fl` are invariant in their index, and `TypeEq a a` too.

Classes. A class parameter has a variance from the types of its methods and superclasses by the same rules (`+` producer-only, `−` consumer-only, `inv` mixed/higher-kinded or below a double flip where reach preservation forbids `+`, `bi` unused). A wanted `C τ̄` is served by an instance with head `H` (`Cn.Serves`) when for each parameter `j`: `τⱼ ≤ Hσⱼ` and `⌈Hσⱼ⌉ ⊆ ⌈τⱼ⌉` for `−` (the class only consumes it, as `Show`, `Eq` and `Ord`), `Hσⱼ ≤ τⱼ` for `+` (it only produces it, as `Default` and `Decode`), and `τⱼ ≡ Hσⱼ` for `inv` and `bi` (unused parameters stay exact so marker classes cannot coerce unrelated types). Dictionary records compare arguments in `LeLive.dict` by the same signs (`Sig.dictVar`, `Sig.dictVar_ok`), so a dictionary built for `Hσ` transports soundly to `C τ̄`. Coherence (K1′): two instances of a class must not be able to serve any common query `Q` under any substitutions (strictly stronger than pairwise head comparability so product heads such as `(R, P)` and `(P, R)` are rejected too); the check extends the existing overlap error (`FLICK-C003`) from unification to common-query satisfiability. Improvement by functional dependencies stays by `≡`: it infers types, it does not choose a dictionary.

Inferred parameter variances of both data declarations and classes are exported in `.iface` and shown in hover, `flick doc` and `flick inspect <M> --emit variance`.

Lean: data variance is `Sig.var` with `Sig.var_ok` (`Syntax.lean`), `LeLive.data` (`Live/Mode.lean`), `Sig.builds_vary` (`VaryArgs.lean`), `SpW.ctor` (`VarySpine.lean`), and `LiveVariance.lean`. Class variance and B1/K1′ elaboration are formalized in full: `Sig.dictVar` with `Sig.dictVar_ok` and `Ty.PolD` (`Syntax.lean`), `LeLive.dict` (`Live/Mode.lean`) and `dict_field` (`DictCanon.lean`) preserve `T1` and `T4'` (`SafeUse`); `Ty.Le`, `Cn.Serves`, computable `inferClassVar` with soundness (`inferClassVar_sound`, `Prog.ClsVarOk.super_serves`), and `Ent.inst_le` (`Elab/Syntax.lean`, `Elab/Variance.lean`); strengthened coherence K1′ (`Coherent.k1'`, `k5inst`, `k5mono`, `Elab/Hyp.lean`) with `closed_unique_common` and `T6b` (`Elab/Coherence.lean`); dictionary transport (`Target.dictLe`, `dict_transport`, `Cn.Serves.tr`, `has_dictLe`) through `T6a`, `T6a_core`, `T6b_core` and `T6a_safeUse` (`Elab/ClassVariance.lean`), plus non-trivial `Eq` (`−`) and `Option`/`List` (`+`) variances on `Elab/Witness/`. Model: `flick-model/src/variance.rs` reads both data signs (`signs`) and class signs (`class_signs`, a two-phase least fixed point matching `inferClassVar` / `Ty.PolD`), `check/rows.rs` `subsume_data` compares data by data signs (and records and tuples field by field), `check/subtyping.rs` `subsume_dict` and `check/widen.rs` compare dictionaries by class signs with reach preservation (`SafeUse`), and `flick-model/src/elab.rs` provides executable B1 resolution (`serves`, `entail`) and symmetric K1′ overlap checking (`instances_overlap`, `check_coherence`). The certificates of its programs state both data and class signs in `GenSig.lean` (`genVar` proven to meet `Sig.var_ok`, `genDictVar` proven to meet `Sig.dictVar_ok`), and `le_live` proves `LeLive.data` and `LeLive.dict`. Checker: `flick-core/src/types/variance.rs` and `variance/class_solve.rs` compute data and class parameter signs, export both on `TypeDef.variance` in `.iface` (`iface-version` 32), and compare data and explicit `$Dict` arguments by them; records and tuples compare field by field. Imported opaque data types and imported classes use their validated serialized signs when present, while missing/unknown metadata, abstract types, opaque type aliases and non-`Type` / higher-kinded parameters remain conservatively invariant; `closed` remains restricted to built-in scalars and data types with visible constructors so opaque, abstract and module-parameter types are never assumed closed in GADTs. Signature elision preserves the capture rows shown by data arguments as well as their hidden storage row. `classes/variance_match.rs`, `classes/solve.rs`, `classes/coherence.rs` and `classes/mod.rs` implement B1 variance-directed instance and given resolution (preserving non-empty hidden capture qualifiers on class constraints for `SafeUse`, waiting on `Match::Unknown` metas so refinement never switches a chosen dictionary, and keeping functional-dependency improvement strictly equality-based) together with symmetric K1′ common-query overlap checking (`FLICK-C003`).

### 2.3 Rules

The judgment is as in the core, `Γ ⊢ e : τ ! φ`. `Γ` additionally holds variables of kinds `Region` and `CRow`, facts `Θ` (§2.4), and the list of open `scope`s: the regions bound by `scope`s around the current position.

```text
Γ, x : τ₁ ⊢ e : τ₂ ! use(φ, C)
────────────────────────────── (Lam)
Γ ⊢ λx. e : (τ₁ →φ τ₂)^C ! P

Γ ⊢ e : τ ! use(R⟨r; ε⟩, C)
──────────────────────────── (Task)
Γ ⊢ task e : (Eff r ε τ)^C ! P

Γ ⊢ f : (τ₁ →ψ τ₂)^C ! φ    Γ ⊢ e : τ₁ ! φ    use(ψ, C) ⊑ φ
──────────────────────────────────────────────────────── (App)
Γ ⊢ f e : τ₂ ! φ
```

A body is checked where its qualifier is live, and a call or an await needs the qualifier live in its own stage (`Live C`, `FLICK-G002` "this uses `s`, which … does not keep live"). Nothing bounds what a closure or a task holds: one that holds a capability and never uses it gets the qualifier `∅` and may leave the `scope` of its region. Services the body looks up at run time are in the row `r`.

Applying a local `f : (∀v. τ^v → σ)^C` with `v ∉ σ` adds nothing of the argument to the result: the result is parametric in what the argument holds. So `Option.map` returns `(Option b)^c` for a function qualified `c`.

Checker: `regions/live.rs` (`live_call`, `live_await`, `qualifier_live`), `regions/applied.rs` `parametric_in_argument`. Lean: (Lam), (App), (Task) of `Typ` (`Rules.lean`).

```text
K : ∀ᾱ. τ̄ →P T ᾱ      Γ ⊢ eᵢ : τᵢ' ! φᵢ      τᵢ' ≤ τᵢ[ᾱ := ῡ]      ⋃ ⌈τᵢ'⌉ ⊆ ⌈(T ῡ)^C⌉
──────────────────────────────────────────────────────────────────────────── (Con)
Γ ⊢ K ē : (T ῡ)^C ! ⊔ φᵢ

Γ ⊢ e : (T ῡ)^C ! φ    field i of type τᵢ[ᾱ := ῡ]
──────────────────────────────────────────── (Proj)
a field extracted by match or .ℓ has type (τᵢ[ᾱ := ῡ])^C
```

For data that stores only its parameters, `⌈T ῡ⌉` already covers the fields and `C = ∅`. A function field or an opaque type requires `C`. A constructor does not erase captures, and neither do nominality and opacity (spec/05 "Existential/opaque packaging"). The premise `τᵢ' ≤ τᵢ` is read without the field qualifier ([the field premise of (Con)](#the-field-premise-of-con-ignores-the-field-qualifier)).

Checker: `regions/data.rs` `constructor_field`, `data_value`, `data_field`; records and tuples in `records.rs`, `patterns.rs` through the same functions.

```text
Γ ⊢ e₁ : τ_f ! φ₁    τ_f = τ₁ →P τ₂    Γ ⊢ e₂ : τ₁ ! φ₂
───────────────────────────────────────────────────── (App-Pure)
Γ ⊢ e₁ e₂ : τ₂^{≤ ⌈τ_f⌉ ∪ ⌈τ₁⌉} ! φ₁ ⊔ φ₂
```

`τ^{≤C}` is a value bound. The refinement is sound by T2(a): a pure application neither allocates nor acquires, and its result is built from the function, the argument and constants.

Checker: `regions/data.rs` `application_result` (called from `calls.rs` `apply`); the reader region is excluded from the bound.

```text
x : ∀ᾱ c̄. Θ ⇒ τ ∈ Γ    τ̄, C̄ a substitution    Γ ⊢ Θ[ᾱ := τ̄, c̄ := C̄]
for each occurrence αᵢ^D in τ:   ⌈τᵢ⌉ ⊆ D[c̄ := C̄]
───────────────────────────────────────────────────────── (Inst)
Γ ⊢ x : τ[ᾱ := τ̄, c̄ := C̄] ! P
```

A value bound bounds the whole substituted value. `a^{}` with `a := File s` is rejected at the call site: `FLICK-G001` "the expected type `File s` holds no captures".

Checker: `unify.rs` `Checker::instantiate`; the bound condition is a `Hold` from `regions/data.rs` `subsume_bound`, which holds every region of the value for a bound on a variable (`Visible::Regions`, [a bound on a variable bounds the whole value](#a-bound-on-a-variable-bounds-the-whole-value)); `Outlives` constraints are `classes/mod.rs` `want`.

Lean: (Var), a global and the projection of a dictionary field ask `Scheme.Holds`, the facts of the scheme at the instance; a bound is the fact `Facts.bnd i D`, so this is the premise `⌈τᵢ⌉ ⊆ D[c̄ := C̄]` (`Scheme.holds_bnd`). (Let-Gen), a global and a field of a dictionary assume the facts of their scheme in its body. `Examples.lean` types a generic `let` whose body makes a cell under the bound `α^{≤c}`, and shows an instance beyond the bound rejected.

```text
s ∉ fr(Γ)    Γ, s : Region, s : Owner s ⊢ e : τ ! φ    φ ⊑ R⟨{Scope s} + r; ε⟩
s ∉ fr(τ) ∪ fr(r) ∪ fr(ε)
──────────────────────────────────────────────────────────────── (Scope)
Γ ⊢ scope s { e } : τ ! R⟨r; ε⟩
```

- `fr(τ)` are the free regions of `τ`, including its hidden qualifiers and rows. A value bound is a fact of its scheme (`⌈τ⌉ ⊆ C`, [value bounds in the core](#value-bounds-in-the-core)), not part of `τ`: on a value that can hold nothing (a scalar, or data without hidden storage and without captures in its type, `holds_nothing`), a value bound carries no region, whereas on an opaque foreign capability such as `a^{s}` of `Resource.acquire_release` (G4) the region remains part of the value's reach.
- `Scope.run f` lowers to the same `inside` form as `scope s { f s_owner }` for a function `f` of any form (a lambda or a variable), with its region bound by the rank-2 parameter of `Scope.run` (§1.5), so the three conditions on `s` hold by scoping. Lowering agreement does not prove source typing equivalence (E12): a core `scope` knows that outer resources and providers outlive `s` (§2.4, `Fiber.Provided`), allows nested acquisition into an outer `Scope` alongside `Scope s`, binds `s` as a region type name, and keeps lexical `let mut`, `break`/`continue`, and `defer` across its body, whereas a rank-2 `∀s` callback to `Scope.run` cannot express any of those.
- `R⟨r; ε⟩` is always part of the result: `scope` is a runtime step, and a pure function cannot open one (`FLICK-E001`).
- `scope` discharges only the `Scope s` of its own region; every other `Scope ϱ` passes outwards in `r`.
- A `Scope ϱ` requirement that no `scope` discharges and that the signature does not list is `FLICK-G004`.

Checker:

- `regions/scopes.rs` `scope_expr`: a fresh rigid region one level deeper, `Owner`, a separate stage for the body;
- `effects.rs` `scope_row`: the body's requirements without `Scope s`;
- `unify.rs` `Store::leave` and `regions/mod.rs` `region_escape`: `s ∉ fr(τ)`, `FLICK-G002`;
- for a qualifier still unsolved at the end of the `scope`, `s ∉ fr(τ)` is checked by the solver: a variable at a level outside the `scope` does not accept a rigid region of a level inside it (`regions/solve.rs` `grow_holds`, `report_hold`);
- `effects/solve.rs` `no_owner`: `FLICK-G004`.

### 2.4 Outlives: `Θ ⊢ k ≥ ϱ`

```text
k ≥ k          k ≥ ϱ, ϱ ≥ ϱ' ⇒ k ≥ ϱ'          Outlives C ϱ ∈ Θ, k ∈ C ⇒ k ≥ ϱ

s₁ ≥ s₂,  if scope s₂ is lexically inside scope s₁
k ≥ s,    if s is bound by a scope in the declaration body, and k is a region of the signature or a type variable

Θ ⊢ Outlives C ϱ   ⇔   ∀k ∈ C. Θ ⊢ k ≥ ϱ, and each variable c in C is covered by a fact Outlives c ϱ ∈ Θ
```

The second rule is sound because signature regions and the captures of substituted values come from the caller: they are live for the whole call, while a `scope` inside the body closes earlier.

```text
Γ ⊢ Outlives (C ∪ ⌈a⌉) ϱ    Γ ⊢ o : Owner ϱ    Γ ⊢ f : (Failure e →R⟨{Scope ϱ} + r;none⟩ Unit)^C    Γ ⊢ t : (Eff ({Scope ϱ} + r) e a)^C
──────────────────────────────────────────────────────────────────── (Spawn)
Γ ⊢ Fiber.spawn o f t : Fiber e a ! R⟨r; none⟩
```

This is (Inst) with `Θ = Outlives c s` plus the core (App), and `Outlives` is taken over everything the task holds: `⌈(Eff … a)^C⌉ = C ∪ ⌈a⌉`. A region shown only by the result is also held by the child until it ends ([(Spawn) bounds everything the task holds](#spawn-bounds-everything-the-task-holds)). The rows of the child and of the supervisor name the owner's `Scope ϱ`: the runtime runs them under the owner (`packages/effect-ts/src/owner.ts` `inside`), so they acquire into its region, and the row of the call is only `r` ([children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner)). Any constant that holds an argument until its owner closes has the same shape: a finalizer, an event handler, a background stream (G4).

Checker: `regions/solve.rs` `check_outlives` (`FLICK-G003`): the region of a `scope` must enclose the owner's region (`encloses`); an atom not from a `scope` fits if the owner is a `scope` of the body, or if a given `Outlives` covers it.

### 2.5 Region-free errors

```text
declaration type error E ᾱ = E τ̄:
    E has no parameters of kinds Region and CRow;
    τ̄ contain no Ref, Eff, Layer, Unknown, runtime arrows or non-empty qualifiers      (Err-Decl)

a step with effect R⟨r; ε⟩ under scope s:   s ∉ ⌈E ῡ⌉ for each E ῡ ∈ ε                       (Err-Raise)
```

The second rule applies even when a `try` inside the same `scope` catches the error (spec/05 "Expected errors"). The model would only need the check on errors that leave the `scope`; the spec chooses the simpler and stricter rule, and the model adopts it.

`Exit e a` stores the success value or a failure: `⌈Exit e a⌉ = ⌈e⌉ ∪ ⌈a⌉`, and `⌈Failure e⌉ = ⌈e⌉`. `RawCause` is opaque and reaches no regions: foreign JS values are sealed (G4).

Checker:

- (Err-Decl): `services.rs` `check_error_payload`, `live_part` (`FLICK-E005`);
- (Err-Raise): `effects.rs` `perform` puts the error row of each call into `regions.raised`, and `regions/solve.rs` `check_raised` looks for the region of the `scope` in it (`FLICK-E005`).

A signature region in the error row is not checked: `∀s. File s →R⟨none; {Wrapped (File s)}⟩ Unit` is accepted. Each call site inside a `scope` substitutes the region of that `scope` and is checked (`FLICK-E005` at the call), so at the level of a program this is equivalent to (Err-Raise). A value that keeps such an error shows `s` (§2.1): a closure over it is `(Unit → Unit)^{s}`, and leaving a `scope` with it is `FLICK-G001`/`FLICK-G002` by its type.

### 2.6 Qualifier inference and generalization

A qualifier without an expected type is a variable of kind `CRow`. Each `Hold` obligation yields a constraint `hidden ⊆ C`. The solver grows variables to the least solution, as for effect rows, then closes the remaining ones to `∅`.

- A variable at a level outside a `scope` does not accept the region of that `scope`. Such a constraint stays unsatisfied and yields `FLICK-G002`.
- An obligation with a written bound reports `FLICK-G001`.

Generalization (`core.md` §2.5) does not quantify the variables of a value whose obligation is still open. Otherwise `let f = fun () -> use h` would get `∀c. (Unit → Unit)^c`, and instantiating `c := ∅` would erase the capture (`FLICK-G002`).

A local signature need not name the regions its function holds or uses: the outer qualifier is inferred, and the later stages need what elision gives them (§1.4), `Live` of what the earlier parameters reach. An instance method without a signature is checked against the class signature, elided before the instance arguments are substituted.

Checker:

- `effects/solve.rs` (the `grow_holds` loop together with rows, `close_holds`, `report_regions`);
- `regions/solve.rs` `grow_holds`, `blocked_capture_rows`, `close_holds`, `report_regions`, `report_hold`;
- `regions/compare.rs` `compare_deferred`: qualifiers of a type that is inferred later;
- `generalize.rs`: the variables of a value held by an open obligation are monomorphic; `without_value_captures`.

### 2.7 Services and the region of `provide`

This section states [provide services must not outlive their layer](runtime.md#a-provide-service-outlives-the-release-of-its-layer). `provide L { e }` is the lexical owner of the build of `L` (`runtime.md` §3.5). For the order of §2.4 it introduces a region `ϱₚ`, like `scope`: `ϱₚ ≥ s` for a `scope s` inside the body and `s ≥ ϱₚ` for a `scope s` around it. `ϱₚ` does not appear in the types of values: only the service provider of the body has it.

A child of owner `ϱ` started from inside `provide` inherits the environment `θ` of the call site. It uses the services of `L` until `ϱ` closes, while the layer is released when `ϱₚ` closes. Hence retaining a task with an owner requires its services to outlive the owner:

```text
Provided R ϱ         constraint (like Outlives, §2.4): the provider of each service of row R outlives ϱ
{Provided {S} ϱ}     service row element: the same obligation, not yet discharged in the stage

Γ ⊢ Outlives C ϱ    Γ ⊢ Provided r ϱ    Γ ⊢ o : Owner ϱ    Γ ⊢ t : (Eff ({Scope ϱ} + r) e a)^C
────────────────────────────────────────────────────────────────────── (Spawn-Svc)
Γ ⊢ Fiber.spawn o f t : Fiber e a ! R⟨r; none⟩
```

The scheme of every constant with `retention owner` (G4) requires `Fiber.Provided r s` and `Detached r` (no `control` or `abort` operations) for the row of the retained task or finalizer, as it requires `Fiber.Outlives c s` for captures; with one region in the signature the checker adds them (`ffi/retention.rs` `with_owner_constraints`), and a foreign function otherwise missing `Outlives`, `Provided` or `Detached` is `FLICK-F002` (`ffi/retention.rs` `owner_retention`).

The call site splits `Provided r ϱ` into the services `S ∈ r` (except `Scope`, `Async`, which the root gives every fiber and no owner provides, and the obligations themselves) and discharges each in the stage `X` that contains the call, lexically, like `Scope ϱ` (`regions/scopes.rs` `lexical_owner`):

```text
S is given by a provide ϱₚ inside X, the nearest to the call:   discharged if ϱₚ ≥ ϱ; otherwise FLICK-G003
otherwise, if ϱ is a scope inside X:                             discharged: the provider of X covers the whole run of X
otherwise, if a given Provided R ϱ with S ∈ R is in scope:      discharged
otherwise:                                                       element {Provided {S} ϱ} in the row of X                (Provided)
```

In the second case `S` comes from the row of `X`, hence from whoever runs `X`, and lives for the whole run of `X`, including after `ϱ` closes inside `X`. In the last case it cannot be decided: the element moves with the rest of the row to whoever runs `X` (`await`, application, `provide` of a layer) and is discharged there by the same rules. A signature does not write such an element; it writes a constraint: `start : forall s. Fiber.Provided {Greeter} s => Fiber.Owner s ->{Greeter} Unit`. Inside `start` the constraint is given, and the call site receives it at instantiation and discharges it with its own provider.

A row has one `Provided {S}` element per service: two obligations for the same service under different owners in one stage unify the owners into one region, otherwise `FLICK-T003`. This is incompleteness, not a hole: the program is rejected.

Layers use the same constraint, in the schemes of the standard library rather than in a rule. The body of `Layer.from_build` and `from_build_memo` has the type `forall s. Fiber.Provided ri s => MemoMap ->{Scope s | ri; e} Context ro`: its region is rigid, so an obligation `{Provided {S} s}` of a finalizer it registers could not move into the row `ri` of the layer, and the constraint is given instead. Whatever builds a layer into a region `s` of its caller needs `Fiber.Provided ri s` there: `Layer.build`, `build_with_memo_map`, `LayerRef.make`, `LayerMap.make` and `from_record`. `provide L { e }` (`Layer.run`) and `Layer.launch` build into a region of their own, younger than every provider of the `ri` of their call, so for them the constraint holds by the second case and their schemes do not state it. The fix of `FLICK-G003` adds `Fiber.Provided` only to a signature that names its region (`regions/provided.rs` `signed`), not for the region of a `forall` in an argument's type. The acquisition of `Resource.manual`, `Effect.resource_auto`, `ScopedRef.from_acquire`, `ScopedRef.set`, `Pool.make`, `Pool.make_with_ttl`, `RcRef.make`, `RcMap.make`, `ScopedCache.make` and `ScopedCache.make_with` assumes `Fiber.Provided r t` in the same way for its region `t`, of a sub-owner of the resource that closes no later than `s`, so a release it registers there may use the services `r` of the call, whose `Provided r s` the scheme of the foreign function gives (either through `retention owner` for `Resource.manual`, `Effect.resource_auto` and `RcRef.make`, or explicitly for the rest, including `ScopedRef.set`, which has no `Scope s` in its row; §5.1, G4).

Checker:

- `types/handlers.rs` `provide`: the region `ϱₚ` in `regions.scopes`, the service providers of the body in the stage;
- `regions/provided.rs`: `Fiber.Provided` obligations: splitting by service at the end of a call (`calls.rs` `apply`), discharge, moving into the stage row and discharge at the `perform` of whoever runs it (`FLICK-G003`); an owner that will be inferred from rows waits for the end of the declaration;
- `effects/solve.rs`: an obligation that reaches a written signature row (`FLICK-G003` with a `Fiber.Provided` hint).

In Lean (`formal/`) `provide S impl` is a handler of `S` (`core.md` §1.4), and the rules below are those of a handler of an effect: an obligation is an element `ob S ϱ` of the service row (`Elem.ob`); `Fiber.spawn` and `Scope.add_finalizer` put them into the stage of the call (`Row.obs`); the end of a region discharges them by the second case (`Row.disch`); a handler of `S` does not let them through (`Stage.ObOut` in (Handle-E)); and at run time a handler discharges them when its region is older (`Row.hP`). A child and a finalizer keep the handlers of their registration site older than their owner (`withs`, `Tm.hkeep`); one younger than the owner ends first, and the obligation of the work keeps its effect out of the row of the work (`kid_dropped`). An operation of a kept handler whose region has closed is stuck (`DeadUse`), and `T4'` shows that this does not happen.

The reference model (`flick-model/src/check/regions/provided.rs`) implements the same from this document. The `Provided R ϱ` of a scheme is an element `{Provided R ϱ}` in the row of its last runtime arrow, and the call splits it at `perform`. Elements move up with the stage row: `provide` discharges them by the first case, the `scope` of their region by the second, the end of a λ or task by a given `Provided`. Difference: a given constraint discharges at the end of the λ or task around the call, not at the call itself, so a λ with a written row in between rejects what this section accepts (`shortcut:`). The `with θ` of a configuration discharges the elements of its services without comparing regions: the layer is already built, and use after its end is caught by the machine (§3.3). In an inclusion `ρ ⊆ ρ'` the services `ρ'` lacks go into its meta before a given settles the obligations, so a given over `ρ'` itself, as that of a `from_build` body over the layer's inputs, names them (`check/rows.rs` `within_deferred`).

## 3. Dynamics

### 3.1 Configurations

A configuration is `⟨Λ, μ, e⟩`. `Λ` maps a runtime region `π` to:

- a state: `live` or `closed`;
- a list of finalizers `F(π)`;
- a set of children `K(π)`.

Runtime forms:

```text
v ::= … | cap π n | own π
e ::= … | inside π e
```

- `cap π n` is capability `n` of region `π`, of type `H π`;
- `own π` is the owner, of type `Owner π`;
- `inside π e` is the body of a `scope` running in region `π`.

Arguments of kind `Region` are explicit in the model. `open_H π v` receives its region rather than looking up the nearest one: codegen passes the owner lexically (`$rt.within`), hypothesis G6.

### 3.2 Steps

```text
⟨Λ, μ, E[scope s { e }]⟩       →  ⟨Λ[π ↦ live, F = [], K = ∅], μ, E[inside π e[s := π][s := own π]]⟩     π fresh
⟨Λ, μ, E[open_H π v]⟩          →  ⟨Λ[F(π) += close_n], μ, E[cap π n]⟩                  Λ(π) = live, n fresh
⟨Λ, μ, E[op_H (cap π n) v̄]⟩    →  ⟨Λ, μ, E[v']⟩                                       Λ(π) = live
⟨Λ, μ, E[op_H (cap π n) v̄]⟩    — stuck                                                Λ(π) = closed
⟨Λ, μ, E[add_finalizer π t]⟩   →  ⟨Λ[F(π) += t], μ, E[()]⟩
⟨Λ, μ, E[spawn (own π) f t]⟩   →  ⟨Λ[K(π) += (f, t)], μ, E[fiber]⟩
```

While `Λ(π) = live`, a child from `K(π)` may take a step between any two steps of the main computation; its error goes to `f`. The context `E` of `core.md` §3.2 is extended with the frame `inside π E`.

### 3.3 Close

```text
⟨Λ, μ, E[inside π w]⟩   →  the children K(π) run to completion or are interrupted;
                           the finalizers F(π) run in reverse order;
                           Λ(π) := closed;
                           ⟨Λ', μ', E[w]⟩
```

Here `w` is a value or `raise v`. `die` and interruption close the region the same way; λRT describes this. A region closes only when its body is a value. Hence the region of a `scope` nested in the body, and the regions of `scope`s inside the children, are closed earlier (L5).

`op_H` getting stuck on a closed region is "use after close". In the JS runtime this is a step of a resource whose `release` has already run; the probes detect it.

The individual phases of close, hooks, and a finalizer added to a closed scope are described by λRT (`runtime.md` §3.3, [region closing refined into closing phases](runtime.md#region-closing-as-one-step-versus-closing-phases)).

The reference model `flick-model` (`src/eval/regions.rs`) runs §3.2–§3.3 in one admissible order:

- the children `K(π)` take steps only when `π` closes, in start order. This is the latest possible moment: a child that holds an already closed region gets stuck on it;
- the region of `open_H` is the nearest `inside` around the step. The generator does not run a step of an outer `Scope` inside a nested `scope`, so by G6 this is the same region;
- a capability and a region-free object given by a foreign binding (`make_conn`, G4) are one form `cap`; `op_H` also gets stuck on a released object;
- an error of a child or a finalizer stops the machine: the supervisor and the order of causes are λRT. The children of generated programs have no errors;
- a child and a finalizer remember the environments `θ` of all `provide`s around the registration site and run under them (`runtime.md` §3.5). A `provide` ends together with its `with θ`, and a service operation from a finished `θ` gets stuck: that is a violation of T4.3.

The model properties (`just model`) check (Inside) and `⊢ Λ` on every state. There `⊢ Λ` is stronger than §3.4: the work of a live `π` reaches (`reach_μ`, §5.2) only `π` and the regions it was created in. The property `leaks_reach_a_use_after_close` checks the machine itself: the generator's leaks, after which the leaked value is used, reach the stuck state of §3.2. There are nine such paths, plus path 10, in which a child, a finalizer or a `start` with a given `Provided` gets stuck on a service of a finished `provide`. A callback passed as a parameter and an error caught inside the `scope` use the value only before close.

### 3.4 Typing runtime forms

```text
──────────────────── (Cap)        ───────────────────── (Own)
Γ ⊢ cap π n : H π ! P             Γ ⊢ own π : Owner π ! P

Γ ⊢ e : τ ! φ    φ ⊑ R⟨{Scope π} + r; ε⟩    π ∉ fr(τ) ∪ fr(r) ∪ fr(ε)    Λ(π) = live
──────────────────────────────────────────────────────────── (Inside)
Γ ⊢ inside π e : τ ! R⟨r; ε⟩
```

`⊢ Λ`: the finalizers and children of each live `π` are typed at stages whose `Live` regions outlive `π` in the nesting order of live regions (Lean `LCfgTy`, `Row.Outlives`).

(Lam) and (Task) ask nothing of what a runtime body holds, as of a program ([(Lam) and (Task) count runtime forms](#lam-and-task-count-runtime-forms) records the premise of the capture discipline). The bodies of `λ`, `fix` and `task` contain no `inside`.

## 4. Leak paths

The table of spec/05 "Static regions and captures": the model rule, the counterexample it rejects, and where that counterexample is checked. Fixtures are in `tests/check/fail/regions/`.

| Path | Rule | Counterexample | Check |
| - | - | - | - |
| Direct result, nested record or ADT | (Scope): `s ∉ fr(τ)`; ⌈·⌉ is transitive over arguments | `scope { File.open_scoped path }`, `Wrapper (File s)`, a record with a closure | `escape.fl` `result`, `wrapper`, `in_record` (`FLICK-G002`) |
| Closure or `Eff` | (Lam), (Task), (≤-Cap); generalization does not quantify an open obligation | `bad_callback`, `bad_deferred`; `let f = fun () -> use h` after `scope`; a task that hides a capture behind the region of its result or row | `escape.fl` `bad_callback`, `bad_deferred` (`FLICK-G001`); `eff_result.fl` ([reach bound of `Eff`](#the-reach-bound-of-eff-includes-its-result)); a probe |
| Passing, including through a contravariant parameter | (≤-Cap) on the whole value | a callback with a bare parameter as `((Unit → Unit)^{s} → Unit)`; a closure with `s` in its parameter as a bare parameter | probes (`FLICK-G001`/`G002`) |
| Module interfaces | a top-level signature is mandatory for functions and is checked as written; `.iface` stores qualifiers | a call of `reader` from another module that escapes the `scope` | `tests/check/projects/regions` (`FLICK-G001`, `FLICK-G002`); outside the Lean model, which has no modules (`core.md` §2.6) |
| Existential and opaque packaging | (Con), (Proj) | `Sealed (File s)`; `opaque type Sealed = Sealed (Unit ->{} Unit)` around a closure over `h` | `escape.fl` `sealed`; a probe (`FLICK-G002`) |
| Through a helper generic in an argument of data with hidden storage | §2.2: the hidden row only widens, and what the other arguments show does not stand in for it; §1.4: a bare result row is what the earlier parameters reach | `compose (G (fun f -> File.read_all f)) (G (fun _ -> file))` returned from the `scope` that opened `file` ([the hidden row of data is not what its arguments show](#the-hidden-row-of-data-is-not-what-its-arguments-show)) | `hidden_through_helper.fl`: `through_run` and `through_c2` are `FLICK-G001` at the value of the `scope`; the body of `compose2` is `FLICK-G002` where `c2` takes the functions taken out of the data; `through_function` is `FLICK-G002` |
| Write to an outer `Ref` or cache | invariance of `Ref (α^c)`; the cell type is closed at allocation | `Ref.set cell (Some file)`; `Ref (Unit ->{} String)` against `Ref ((Unit ->{} String)^{s})`; a closure that uses a region of the signature stored in a cell the caller passes; a top-level atom | `escape.fl` `outer_cell`, `outer_callback`; `qualifiers.fl` `invariant` (`FLICK-T003`); `ref_capacity.fl`; `instance.fl`; `stored_closure.fl` ([a value bound allows no use](#a-value-bound-allows-no-use)) |
| Assignment to a `let mut` variable from outside | the variable's type is fixed where it is declared, outside the region (`elaboration.md` §3.5); T7 reads a `scope` that assigns as one whose value holds the variables | `last = Some file`, `reader = fun () -> path_of file` inside `scope` | `let_mut.fl` `outer_file` (`FLICK-G002`), `outer_callback`, `written_callback` (holding without use is allowed); `T7_regions` |
| Error, `Exit`, diagnostics | (Err-Decl), (Err-Raise); `Failure`, `RawCause` are inert | `Wrapped file` under `try` inside `scope`; `type error Leak (s : Region)`; a generic `boom : ∀a. a →R⟨none;{Wrapped a}⟩ Unit` with a handle | `escape.fl` `raised`; `qualifiers.fl` `Leak` (`FLICK-E005`); a probe |
| Child and finalizer | (Spawn), §2.4 | a child of an outer owner holds an inner region; a given `c` without `Outlives` | `qualifiers.fl` `spawn_outer`, `spawn_given` (`FLICK-G003`); `retention.fl` ([owner retention](#owner-retention-requires-outlives)); `children.fl` ([children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner)); a probe |
| A layer service in a child of an outer owner | (Spawn-Svc), (Provided), §2.7 | `Fiber.spawn outer` or `Scope.add_finalizer` of an outer region from inside a `provide` with a layer service ([provide services must not outlive their layer](runtime.md#a-provide-service-outlives-the-release-of-its-layer)) | `provide_escape.fl` (`FLICK-G003`); path 10 of the model's program generator (`flick-model/src/generate/provides.rs`) |
| Pure foreign call over a resource | G4: a foreign function whose parameter reaches a region has a runtime stage, so elision needs that region live where it is called | `peek : File s -> Int` called by a pure closure that only holds the file and leaves its `scope` | `fail/ffi/pure_over_capability.fl` (`FLICK-F009`); a layer of `ErrorReporter.layer` returned from the `scope` of its reporters, `fail/regions/reporter_layer.fl` (`FLICK-G001`) |
| Foreign call | G4: the signature is a retention contract; what is retained under `retention owner` requires `Outlives` (`FLICK-F002`) | an acquired value outside its `scope`, including through a generic function; a lease value from `use` | `acquired.fl` ([acquired values](#acquired-values-carry-their-region), [value bounds](#a-value-bound-survives-an-unknown-type-position)), `leases.fl` ([leases](#leases-instead-of-early-close)), `fail/ffi/owner_retention.fl` |

## 5. Theorem T4′

### 5.1 Hypotheses

G1–G3 of `core.md` hold, and the following are added:

- **G4 (FFI retention contract).**
  - `retention call`: after returning, the function holds no arguments except those reachable from the result within ⌈result type⌉.
  - A foreign type whose values keep functions they are given under `retention call` is marked `@stores` (§1.4): its result holds what the parameters before it reach, which covers what it keeps. A result that is a function shows its own qualifier instead, and an application of it is bounded by what the function holds: `Codec.transform_effect` returns `(PureCodec Json a -> Codec r none Json b)^c`, so the codec it makes holds the `c` of its decoding without a mark of its own.
  - `retention owner` with owner `ϱ` (from `Scope ϱ` or `Owner ϱ`): the function holds its arguments until `ϱ` closes, and its scheme requires `Outlives c ϱ` for each capture row `c` of a retained argument that is not shown by what follows it, and `Provided r ϱ` with `Detached r` for the service row `r` of retained work (§2.7), which a signature with one region gets without writing them (`ffi.rs` `with_owner_constraints`, `owner_retention`, `FLICK-F002`). The call site checks `Outlives` for the whole ⌈·⌉ of the retained argument, including what its result shows ([(Spawn) bounds everything the task holds](#spawn-bounds-everything-the-task-holds)).
  - An acquisition `∀s. τ_a →R⟨{Scope s} + r; ε⟩ τ` returns capabilities of region `s`, and `s ∈ ⌈τ⌉` unless `τ` is a scalar. Only closing `s` releases them.
  - A function without `retention` counts as `call`. `FLICK-F002` requires the annotation only on parameters with variables of kinds `Region` and `CRow` (`ffi.rs` `foreign_info`). A parameter that is a variable of kind `Type` (`∀a. a →R Unit`) needs no annotation even though it can receive a capability: such a binding is covered only by this hypothesis.
  - A foreign function one of whose parameters reaches an atom of kind `Region` that its result does not carry uses that region when called, so it has a runtime stage, and elision needs the region live where it is called (§1.4). A pure foreign arrow there would let a closure that only holds the capability leave its `scope` (by use, §2) and read the closed resource with nothing live, and is `FLICK-F009` (`ffi.rs` `foreign_info`). A row of kind `CRow` is no use of its own: a pure callback uses nothing (`Stage.use .pure = .pure`), and an effectful one makes the call run a stage. A result that carries the region (`File s -> File s`) is no use either: its type shows the region, and its users are checked.
  - Pure operations on a capability that return what was fixed when it was acquired do not depend on whether the region is live, and keep a pure stage as reviewed exemptions of the item above (`ffi.rs` `PURE_OVER_CAPABILITY`): `ChildProcess.pid`, `HttpApi.port`, `Rpc.port`, `Http.Server.address`. A function that returns a value using the region later carries the region in its result instead: `ErrorReporter.layer` and `LayerRef.get` return `(Layer …)^{s}`, so the layer cannot leave the `scope` of `s`.
- **G5 (leases).** `ScopedRef`, `Resource`, `ScopedCache` hand out their value only in `use`, with lease region `t`, and release the previous value when no lease holds it (`packages/effect-ts/src/effect/lease.ts`); acquisition goes into a region of its own, of higher rank. `RcRef`, `RcMap`, `Pool` likewise quantify a fresh acquisition region, independently of the container's owner. They lend the value to the calling scope (`a^{t}`) and do not revoke it before that scope closes (Effect 4.1.2).
- **G6 (lexical owner).** A step that needs `Scope ϱ` receives the owner of region `ϱ`, not the nearest `scope` at run time (`$rt.within`). A constant does not run its `Eff` argument under an owner other than the one named in that argument's row: an argument that a combinator runs names the combinator's region, of higher rank for sub-owners ([leases](#leases-instead-of-early-close)) and the owner's `Scope s` for children ([children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner)).

### 5.2 Lemmas

`reach_μ(e)` are the regions `π` that occur in `e` (in `cap π n`, `own π`, and inside the bodies of `λ` and `task`), together with `reach_μ(μ(ℓ))` for each location `ℓ` in `e`, transitively. In a substitution semantics this is exactly reachability.

Typing by use does not bound `reach_μ(v)` by the type of `v`: a closure may hold a capability its type does not show. The lemma L4 of the capture discipline (`reach_μ(v) ⊆ ⌈τ⌉`) is therefore gone, and with it T4.2 (§5.3).

- **L5 (nesting).** If `π₂` is created inside `inside π₁` (in the body or in a child of `π₁`), then `π₂` closes before `π₁`. Follows from §3.3: `inside π₁ w` closes only when the body is a value and the children of `π₁` have finished.
- **L6 (old cells).** If `ℓ` was allocated before `π` was created, then `π ∉ fr(Ψ(ℓ))`: `Ψ(ℓ)` is closed at allocation, and `π` is fresh.

  In Lean: `reach_μ` is `Reaches`; L5 is `LNest.nested`; L6 is `lemma_6`. Freshness of `π` is kept by renaming: each step renames the derivation through `kill |Λ|`, which replaces regions `≥ |Λ|` with `Unit`, and the configuration mentions only existing regions (`CfgBelow`). The facts of a configuration are the creation order (`Facts.age`): a region outlives all regions created after it.

  `⊢ Λ` in Lean is `LNest` (`Live/LNest.lean`): each region has ancestors `anc π` (the regions it was created in, and itself); the `inside` forms and the handlers of each thread are at the place of their region, and the arms of a handler's cell name only ancestors of its region (`LCell`). `inside` forms occur only where the term is being evaluated, so a step neither copies nor loses them. Every step preserves the invariant (`LNest.step`). What a thread names is not bounded; instead `LCfgTy` types the children and finalizers of each region `p` at stages naming only ancestors of `p`, and every region atom of the stage of a hole is the thread's or a frame's, and so live (`hole_atm`, `LNest.atm_live`).

### 5.3 Statement

**T4′ (regions, by use).** Let `e₀` be a program (`core.md` §2.6) without `Scope` in `φ₀`, let G1–G6 hold, and let `⟨∅, ∅, e₀⟩ →* ⟨Λ, μ, e⟩` under any admissible order of child steps. Then:

1. (T4.1) no thread — the main thread, or a child or finalizer of a live region — is at a step that needs a region that is not live: `open_H`, `op_H`, registering a finalizer or a child, or closing a region;
2. (T4.2, given up) nothing is said of what threads *reach*: a value may keep a closed capability reachable;
3. (T4.3) no fiber performs an operation of a service given by a `provide` with region `ϱₚ` after `ϱₚ` closes.

T4.2 of the capture discipline said that at a close step of `π` nothing that can still step reaches `π`. Typing by use gives it up on purpose: a closure or a task that holds a capability of `π` and never uses it is accepted, may outlive `π`, and runs without a use after close. What it costs is memory, not safety: the runtime contract takes only T4.3 (G11 of `runtime.md`), so T5 is unaffected.

Sketch. Invariant: every reachable state is well typed, `⊢ μ : Ψ`, `⊢ Λ`, and the stage of every thread names only live regions of its place. A step that needs a region has that region's atom in the stage of its redex — a call needs its qualifier live (`use(ψ, C) ⊑ φ`), `op` needs `Scope s`, registered work needs every region of its `Live` to outlive the owner — and every region atom of the stage of a hole is the thread's or a frame's, hence live. Foreign calls are covered by G4.

Item 3. The environment `θ` of a `provide` is received by the body of the `provide` and by what it starts. The body and the children of owners inside `ϱₚ` finish before it closes (L5). Retained work of an owner `ϱ` from the body received `Provided S ϱ`, which is discharged only when `ϱₚ ≥ ϱ` (§2.7): `ϱ` closes no later than `ϱₚ`, and its children finish before that. An obligation that moved to the runner of a stage is discharged there by the same rule. This discharges the premise of R6 in `runtime.md` ([provide services must not outlive their layer](runtime.md#a-provide-service-outlives-the-release-of-its-layer)).

**Corollary (progress for capabilities).** The stuck state of §3.2 is unreachable, and T1 of the core stays valid with the constants `op_H`.

In Lean, T4′ is `SafeUse` (`formal/Formal/Live.lean`), proved of the typing `Typ` as `T4'` (`formal/Formal/Live/T4Live.lean`): its first two conjuncts are item 1 for the main thread and for the children of live regions (`DeadUse`, `Regions.lean`), the third is item 3, and the corollary is `T1_regions` (`Live/T1Regions.lean`). Item 3 is stated in the form of [the answering provide must be live](runtime.md#the-provide-liveness-premise-is-stronger-than-typing-gives): in every reachable state, an operation answered by a handler of region `ϱₚ`, its own or kept, occurs only while `ϱₚ` is live. A thread holds only handlers of its ancestors, which are live (`LNest`). What the obligation contributes is that nothing is lost by a child keeping only those: the row of the work names no effect whose handler is younger than the owner, because `Provided {S} ϱ` of the registration is discharged only by a handler of an older region (`kid_kept`, `kid_dropped`), so an operation of a child is answered by a handler it keeps or by the root, unless it is not `tail` and its nearest handler is one the child keeps, where it is stuck (`KidCov`, `KeptStuck`, `T1_regions`).

`Fiber e a` is not a capability of the owner's region: a child's handle may outlive the owner, and `join` then gets an interruption. T4′ does not forbid this, and neither does spec/05.

## 6. Known divergences

Some divergences broke T4, the theorem of the former capture discipline. [The reach bound of `Eff`](#the-reach-bound-of-eff-includes-its-result) broke it in the checker's rules. [Owner retention](#owner-retention-requires-outlives), [background work](#background-work-has-owner-retention) and [acquired values](#acquired-values-carry-their-region) broke hypothesis G4 in the built-in bindings, and through it T4. [Leases](#leases-instead-of-early-close) broke G6 and the "no early close" promise of spec/05. Each of these is confirmed by a probe: a program the checker accepted that prints `USE AFTER CLOSE` when run on Node, or a program accepted where the analogous form is rejected (`FLICK-G003`). The others are checker incompleteness or imprecise prose. All are resolved in the checker, the bindings, the runtime, the spec or this document, as each item states; every checker fix has a fixture.

Common definitions for the probes:

```flick
type Handle (s : Region)

open : forall s. String ->{Scope s} Handle s
foreign effect open from "./res.mjs" "open"

use : forall s. Handle s ->{} Unit
foreign effect use from "./res.mjs" "use"
  retention call

type Raw

make_conn : String ->{} Raw
foreign effect make_conn from "./res.mjs" "raw"

close_conn : Raw ->{} Unit
foreign effect close_conn from "./res.mjs" "close"

use_conn : Raw ->{} Unit
foreign effect use_conn from "./res.mjs" "use"

report : Failure e ->{; none} Unit
let report _ = ()
```

`open` acquires into region `s` and registers the release with its owner; `make_conn` creates a region-free object that `close_conn` releases.

### The reach bound of Eff includes its result

Divergence: `Eff` was opaque to ⌈·⌉, while its result was visible to `visible_in`.

```flick
forget_result : forall s. Eff none none (Handle s) -> Eff none none Unit
let forget_result t = task { ignore (await t) }

main : Unit ->{} Unit
let main () =
  let later =
    scope {
      let h = open "inner"
      forget_result (task { use h; h })
    }
  await later
```

- Symptom: the program was accepted, and Node printed `USE AFTER CLOSE inner`. The top-level signature is checked as written.
  - The argument task holds `h`, but against the bare `Eff none none (Handle s)` its qualifier was empty: `visible_in` sees `s` in the result.
  - The body of `forget_result` captures `t`, but `captures_of` of `Eff` was empty: the result task was bare.
  - The same hole without a signature: `let t : Eff none none (Handle inner) = task { use h; h }`, then `Fiber.spawn outer report t` (`c` is empty, so `Outlives` passes) or `Ref.set cell (task { ignore (await t) })` into an outer cell. Both printed `USE AFTER CLOSE`.
  - The same shape with a function instead of a task, `(Unit ->{} Handle s) -> Eff none none Unit`, was rejected: `FLICK-G001` "this task uses `f`, which holds `s`".
- Cause: `regions/mod.rs` `collect_bounded_captures` did not enter `Eff` (the `opaque` list), while `regions/solve.rs` `hidden` subtracted `visible_in`, which does. L4 was violated.
- Resolution, checker: `⌈Eff r e τ⌉ = ⌈τ⌉`; `collect_bounded_captures` collects the result argument of `Eff` but not its rows (§2.1). `Layer`, `ServiceTag`, `ServiceImpl`, `Errors` stay opaque: their regions occur only in rows, and rows return to the type on use. Fixture: `tests/check/fail/regions/eff_result.fl` (the three shapes above and a task with `Scope inner` in its row).

### Service row variables are not captures

- Symptom: the wrapper `rpc_listen o g hs = Rpc.listen o g hs` with a signature repeating that of `Rpc.listen` was `FLICK-G001` "this function uses `g`, which holds `r`". Here `g : Group r hs`, and `r` is a service row.
- Cause: `collect_bounded_captures` skipped arguments of kind `ErrorRow` but not `ServiceRow`; the tail variable entered ⌈·⌉ as an atom.
- Incompleteness, not a hole. Resolution, checker: `ServiceRow` arguments are skipped too, as in §2.1. Fixture: `listen_on` in `tests/check/pass/regions/leases.fl`.

### Owner retention requires Outlives

Divergence: `retention owner` without `Outlives c s`.

```flick
defer_to : forall s c. Fiber.Owner s -> (Eff none none Unit)^c ->{Scope s} Unit
let defer_to _owner work = Scope.add_finalizer work

main : Unit ->{} Unit
let main () =
  scope outer {
    scope inner {
      let h = open "inner"
      defer_to outer (task { use h })
    }
  }
```

- Symptom: accepted. Node printed `close inner`, then, when `outer` closed, `USE AFTER CLOSE inner`. The same with `Resource.acquire_release` whose `release` holds `h` (`USE AFTER CLOSE inner`).
- Wrappers with given `s`, `c` and no `Outlives` over `Fiber.spawn` and `Stream.to_pubsub` are `FLICK-G003`. Over these constants they were accepted:
  - `Scope.add_finalizer`, `Scope.add_finalizer_exit` (`builtins/Scope.fl`);
  - `Effect.add_finalizer` (`builtins/Effect/Errors.fl`);
  - `Resource.acquire_release`: `release` (`builtins/Resource.fl`);
  - `Events.listen`: the handler and the supervisor (`builtins/Events.fl`).
- By signature the same held for `Rpc.listen`, `listen_websocket`, `listen_tcp`, `handler`, `handler_websocket` (`builtins/Rpc.fl`): the handlers `hs^c` live until `s` closes.
- Resolution, bindings: `Fiber.Outlives c s =>` on each of them, and on `HttpApi.listen`, which the check below found. To keep it from recurring, checker: for a `foreign` with `retention owner`, a parameter's capture row not shown by what follows the last such parameter requires `Outlives` (`FLICK-F002`). Fixtures: `tests/check/fail/regions/retention.fl`, `tests/check/fail/ffi/owner_retention.fl`.

### Background work has owner retention

- Symptom: `Channel.to_queue`, `to_queue_array`, `to_pubsub`, `to_pubsub_array` (`builtins/Channel/Run.fl`) run the channel "in the background of the innermost `scope`" until it closes, but were declared `retention call` without `Outlives`.
- A wrapper over `Channel.to_queue` with given `s`, `c` was accepted. The same wrapper over `Stream.to_pubsub`, which has `retention owner` and `Outlives c s`, is `FLICK-G003`. A channel `Channel.from_subscription sub` of an inner region escaped this way into the owner of an outer one.
- Resolution, bindings: `retention owner` and `Fiber.Outlives c s =>`, as for `Stream`. Fixture: `channel_queue` in `retention.fl`.

### Acquired values carry their region

Divergence: an acquired value had no region in its type.

```flick
leak : Unit ->{} Unit
let leak () =
  let conn =
    scope {
      Resource.acquire_release (task { make_conn "db" }) close_conn
    }
  use_conn conn
```

- Symptom: accepted; Node printed `close db`, then `USE AFTER CLOSE db`. `acquire_release : … ->{Scope s | r; e} a` returned `a`, in which `s` does not occur. The same for `ScopedRef.get r` after `scope` (`USE AFTER CLOSE ref`).
- By signature the same held for `Resource.get`, `RcRef.get`, `RcMap.get`, `RcMap.get_option`, `Pool.get`, `ScopedCache.get`, `get_option`, `get_success`. `LayerMap.get : k -> LayerMap s k e ro -> Layer none e ro` lost `s` on a layer whose services are built in `s`.
- `Layer.build` let it through as well: `Context ro` has no region, and the implementation can capture a value from `acquire_release`, which has no captures.
- Resolution, bindings: the result is `a^{s}`; for a lease with `Scope t` (`RcRef`, `RcMap`, `Pool`) it is `a^{t}`; `LayerMap.get` returns `(Layer none e ro)^{s}`. A scalar `a` still escapes, but a scalar is a copy, not a capability (G4). A function that receives such a value accepts it with a qualifier: `close_conn : forall c. Raw^c ->{} Unit`. Fixture: `tests/check/fail/regions/acquired.fl`. The remaining path through a generic function is closed by [value bounds in unknown positions](#a-value-bound-survives-an-unknown-type-position).

### Leases instead of early close

Divergence: sub-owners released a resource inside a live region.

```flick
early : Unit ->{} Unit
let early () =
  scope {
    let r = ScopedRef.from_acquire (task { open "first" })
    let h1 = ScopedRef.get r
    ScopedRef.set (task { open "second" }) r
    use h1
  }
```

- Symptom: accepted. Node printed `close first` at `set`, then `USE AFTER CLOSE first`, although the region of the `scope` is open and the type of `h1` is `Handle s`. The same for `Resource.refresh` (`USE AFTER CLOSE res`).
- By signature the same held for `RcRef.invalidate`, `RcMap.invalidate`, `Pool.invalidate`, and `ScopedCache` eviction and `idleTimeToLive`.
- Two causes:
  - spec/05 promises "the safe scoped API has **no early close**", and these modules were early closes;
  - a task that a combinator runs itself acquired into the combinator's own Effect `Scope`, not into the region its type names: `open` inside `from_acquire` received the owner of the `ScopedRef`. G6 was violated: lexical owner passing (`$rt.within`) works only where Flick code performs the `Scope` requirement.
- In Effect 4.1.2 `RcRef.invalidate`, `RcMap.invalidate`, `Pool.invalidate` do not revoke a value already lent, so `a^{t}` from [acquired values](#acquired-values-carry-their-region) suffices for them. The early closes are `ScopedRef.set`, `Resource.refresh` and `ScopedCache` eviction.
- Resolution, API with leases:
  - `use : ScopedRef s a -> (forall t. a^{t} ->{r; e} b)^c ->{r; e} b`, likewise `Resource.use`, `ScopedCache.use`, `use_option`, `use_success`; `get`, `get_option`, `get_success`, `ScopedCache.values` and `entries` are removed, and `ScopedCache.refresh` returns `Unit`;
  - acquisition is of higher rank in the region: `from_acquire : (forall t. Eff {Scope t | r} e a^{t})^c ->{Scope s | r; e} ScopedRef s a`, likewise `set`, `Resource.manual`, `Effect.resource_auto`, and `lookup` of `ScopedCache`; a task against such a type acquires into `t` (`lexical_owner`, `declared_scope`);
  - runtime: `packages/effect-ts/src/effect/lease.ts` keeps a lease count, and replacement, refresh or eviction release the previous value after the last lease ends; `ScopedRef` and `Resource` are Flick's own, not Effect's;
  - the name of a top-level function passed at a higher-rank type holds nothing (`infer.rs` `names_top_level`): `Resource.use res ping` passes just like `fun c -> ping c`.

  A value of a type that names its region (`File t`) cannot be stored this way: `t` would escape the `forall`. Fixtures: `tests/check/fail/regions/leases.fl`, `tests/check/pass/regions/leases.fl`, e2e `tests/run/runtime/leases.fl`.

### Service implementations hold no captures

- Spec (spec/05 "Services and type classes"): "The hidden captures of an implementation record are kept in its type and in the Layer".
- Checker: the fields of an implementation are bare positions. `Layer.effect Probe (task { let h = open "layer"; { poke = fun () -> use h } })` is `FLICK-G001` "the expected type `Unit ->{} Unit` holds no captures". `Layer` and `S.Impl` have no qualifiers.
- Incompleteness relative to the prose, not a hole. Resolution, spec: a service implementation holds no captures; a layer that needs a resource acquires it while being built. spec/05 states this.

### Local signatures infer only the outer qualifier

```flick
local : Unit ->{} Unit
let local () =
  scope {
    let h = open "poly"
    wrap : forall a. a -> (Unit ->{} Unit)
    let wrap _ = fun () -> use h
    (wrap 1) ()
  }
```

- Symptom: `FLICK-G001` "the expected type `(Unit ->{} Unit)^{a}` allows only `a`", although the closure does not leave the `scope`.
- reference/regions promised: "a local signature need not name the regions its function holds". The outer qualifier was inferred, but the capture discipline made the qualifiers of later stages written bounds.
- Incompleteness of the capture discipline. Resolution, checker: typing by use asks nothing of what the closure holds, and a later stage needs only what elision gives it (§1.4), so `wrap` is accepted; a later stage that uses a region the earlier parameters do not reach still needs it in its row.

### Children acquire in the region of their owner

Divergence: a child acquired into its owner's region while its type named another.

```flick
elsewhere : Unit ->{} Unit
let elsewhere () =
  scope inner {
    let cell = Ref.make (Some (open "first"))
    let t = task { Ref.set cell (Some (open "child")) }
    scope inner2 {
      ignore (Fiber.spawn inner2 report t)
    }
    match Ref.get cell with
    | Some h -> use h
    | None -> ()
  }
```

- Symptom: accepted; Node printed `close child` when `inner2` closed, then `USE AFTER CLOSE child`. By its type the task acquires into `inner`, but the runtime runs the child under the owner `inner2` (`inside(owner, …)`), and the cell of `inner` holds a released handle. The same for `FiberSet.run`, `FiberHandle.run`, `FiberMap.run` (`inOwner`) and `Events.listen`.
- G6 was violated.
- Resolution, bindings and runtime: the rows of the task and of the supervisor are the owner's `{Scope s | r}`, and the row of the call is `r`; the runtime runs the task, the supervisor and the event handler under the owner. A task written directly in the call acquires into `s` (`declared_scope`); a task with another region is `FLICK-T008`. Fixtures: `tests/check/fail/regions/children.fl`, `eff_result.fl` `in_row`.

### A cell created empty accepts a resource of its scope

- Symptom: `scope inner { let cell = Ref.make None; Ref.set cell (Some (open "x")); … }` was `FLICK-G002` "this value holds region `inner`, so it cannot leave the `scope`", although the cell does not leave `inner`. With `Ref.make (Some (open "first"))` the same program was accepted.
- Incompleteness. Cause: `Some x` carries a hidden value qualifier `^{c}` not yet inferred, while a cell made from `None` stores a bare `Option α` under its bound. (Inst) with a value bound removed the hidden qualifier only if the bound already admitted it; otherwise the variable's type was compared with it, the uninferred `c` was closed to empty, and the capture of `open` then did not fit in `c`. With `Ref.make (Some …)` the element type itself carries `^{c}`, and the comparison passed.
- Resolution, checker: `regions/data.rs` `subsume_bound_at` also removes a value qualifier that is not yet inferred when the bound is not inferred either: the hold of the same substitution grows the bound by its captures (§2.6), so (≤-Cap) is checked as before. Fixtures: `tests/check/pass/regions/scopes.fl` `empty_cell`; `tests/check/fail/regions/escape.fl` `empty_cell_out`: such a cell still cannot leave the `scope`.

### A value bound survives an unknown type position

Divergence: a value bound was lost in a position of unknown type.

```flick
pass : forall a. a -> a
let pass x = x

through : Unit ->{} Unit
let through () =
  let leaked =
    scope {
      let c = Resource.acquire_release (task { make_conn "db" }) close_conn
      pass c
    }
  use_conn leaked
```

- Symptom: since [acquired values carry their region](#acquired-values-carry-their-region), `c : Raw^{s}`, but `pass c` gave `Raw` without a bound, and the value escaped the `scope` (Node: `USE AFTER CLOSE db`). Likewise `ScopedRef.use r (fun c -> c)` returned the lease value.
- Cause: `regions/data.rs` `subsume_bound_at` dropped the value bound when the expected type was still a variable.
- Resolution, checker: such a position receives the value together with its bound if the bound names a region and the type can store something; a bound on a value that can hold nothing is always dropped (`holds_nothing`); a value qualifier the bound already admits is not given to the bound variable; a closure qualifier stays part of the type. Fixtures: `acquired.fl` `generic`, `leases.fl` `returned`.

### Scope region of a task argument is resolved after the call

Divergence: `FiberSet.run` received the task before the set.

- Symptom: `FiberSet.run options (task { … open … }) set` in a nested scope was `FLICK-T003`: the task is checked before `set`, its region is not yet known, and the acquisition went to the nearest scope.
- Incompleteness. Cause: `calls.rs` `apply` checks arguments left to right, and `regions/scopes.rs` `lexical_owner` gave an uninferred `Scope` region of the task the nearest `scope` immediately.
- Resolution, checker: while an argument of type `Eff` is being checked, such a region waits for the end of the call. If no later argument names it, it gets the nearest `scope`, as before. Fixture: `tests/check/pass/regions/set_after_task.fl` (`FiberSet.run`, `FiberHandle.run`, `FiberMap.run`).

The next three were found by the model's program generator (`just model`): programs without regions that the core model accepts.

### Bounded qualifiers are compared after the type is inferred

Divergence: a `Bounded` qualifier was compared before its type was inferred.

```flick
empty : Bool ->{} { b : Option Int }
let empty c =
  await (if c then task { { b = None } } else task { { b = None } })
```

- Symptom: `FLICK-T003` "expected `{ b : Option Int }`, found `{ b : Option Int }`". Also rejected: a record with a function `{ a = fun x -> x }` in two branches, and a `match` on `Some s` with a literal pattern when `s` comes from `None`.
- Cause: on `Bounded` against another type, `unify.rs` compared the bound with the value's captures immediately, without the deferral `unify_captured` performs inside subtyping (`compare.rs` `unknown_captures`). The bound `{}` against `^a` of a not yet inferred `a` was a label mismatch, although `a` later becomes `Int` and holds nothing.
- Resolution, checker: both branches compare qualifiers through the shared `unify_qualifiers`, which defers comparing the captures of an uninferred type. A type the declaration never infers (`None` with unused contents) holds nothing at the end of the declaration: no values of that type were created, and `compare_deferred` drops its captures. Fixtures: `tests/check/pass/regions/late_captures.fl` `literal`, `pick`, `empty`, `unknown`.

### Forall-bound variables are not captures

- Symptom: `go : forall a. Nested a -> Float -> Float` with `let rec go` inside a function was rejected: `go` uses itself at its scheme, and the `forall` variables counted as captures of the value.
- Cause: `regions/mod.rs` `collect_bounded_captures` collected the captures of a `Forall` body together with the variables it binds.
- Resolution, checker: variables bound by `forall` are not captures; each use instantiates them. Fixture: `tests/check/pass/types/recursion.fl` `nested_depth`.

### A scalar bound is not compared

Divergence: the bound of a scalar was compared with the empty one.

```flick
dropped : Unit ->{; Oops} Unit
let dropped () =
  ({ a = from_result (Err (Oops 0)); b = from_result (Err (Oops 1)) }).a
  ()
```

- Symptom: `FLICK-T011` "has type `Unit^{_}` and would be discarded"; with `match … with | () -> 0`, `FLICK-T003` "expected `Unit^{_}`, found `Unit`". The projection takes the bound of the record, which contains the captures of the uninferred type of field `b`.
- Resolution, checker: on `Bounded` with a scalar (`regions/data.rs` `scalar`), `unify.rs` does not compare the bound: a scalar holds nothing, as `value_bound` already assumes. Fixtures: `tests/check/pass/regions/late_captures.fl` `dropped`, `matched`.

The next three were found by the checker's differential tests against the reference model (`flick-model/src/check/regions/`, the rules of §2 as written here), on generated programs with regions (`flick-model/src/generate/regions.rs`: the paths of §4 except the fourth) and on the corpus `tests/run/model.txt`. On generated programs the model and the checker agree: both accept them and reject every leak.

### Errors reaches its payloads

Divergence: `⌈Errors ρ⌉ = ∅` contradicted `Errors {E} ≡ E`.

- Symptom: `raise : ∀e a. Errors e →R⟨none; e⟩ a`, and the argument `Leaked h` with `h : Handle s` is compared with `Errors {Leaked (Handle s)}` by (≤-Cap). With `⌈Errors ρ⌉ = ∅` this is `{s} ⊆ ∅`, and the program is rejected by this premise rather than by (Err-Raise), even outside a `scope`. Likewise `∀s. File s →R⟨none; {Wrapped (File s)}⟩ Unit`, which §2.5 accepts, would be rejected.
- Imprecise document. Resolution, document: `⌈Errors ρ⌉` is the union of ⌈·⌉ of the arguments of the elements of `ρ` (§2.1), like `E ῡ`, with which `Errors {E ῡ}` coincides. The error row is checked only by (Err-Raise). The model computes exactly this; the checker rejects an error leaving a `scope` with `FLICK-E005`, as the model does.

### An error-row variable reaches what its errors reach

Divergence: ⌈·⌉ left out the row variables of an error row, and the checker every argument of kind `ErrorRow` and `Errors` itself.

```flick
keep : forall e. Errors e -> (Unit -> Unit)
let keep x = fun () -> ignore x

hide : forall s. File s ->{} (Unit -> Unit)
let hide file =
  try from_result (Err (Wrapped file)) with
  | err -> keep err
```

- Symptom: both were accepted with types that hold nothing, although the closure `keep` returns holds `x`, and `keep err` holds `s` through `Wrapped (File s)`. `⌈τ[e := ρ]⌉ = ⌈τ⌉[e := ⌈ρ⌉]` failed for `e := {Wrapped (File s)}`, and only the check at a call of `hide` inside a `scope` (`FLICK-G001` "this value holds region `s`") kept the closure in.
- Cause: `regions/mod.rs` `collect_bounded_captures` skipped arguments of kind `ErrorRow` and treated `Errors` as opaque ("an error cannot carry a region"); (Err-Decl) and (Err-Raise) keep the region of a `scope` out of a raised error, not a signature region or a variable out of a kept one.
- Imprecise document and checker incompleteness. Resolution, document, checker, model and Lean: an error row reaches what its errors reach and its row variables (§2.1), except the row of an `Eff`, which belongs to its stage; a curried stage retains them as it retains type variables, so `keep` reads as `Errors e -> (Unit -> Unit)^{e}`; a field or another bare position names them through a capture row, `(Errors e)^c -> { run : (Unit -> Unit)^c }`. Lean: `Ty.reach` of `Ty.errors` (`formal/Formal/Syntax.lean`). Fixtures: `tests/check/pass/regions/error_rows.fl`, `tests/check/fail/regions/error_rows.fl`.

### The field premise of Con ignores the field qualifier

Divergence: the premise `τᵢ' ≤ τᵢ` of (Con), read with (≤-Cap), would reject a hidden field.

- Symptom: in `Sealed (fun () -> use h)` the field is declared as bare `Unit ->{} Unit`. (≤-Cap) on the whole value (§2.2) requires `{s} ⊆ ∅`, and (Con) does not derive. The table of §4 and the checker accept the constructor and reject only the escape from the `scope` by `⌈Sealed^C⌉ ∋ s`.
- Imprecise document. Resolution: the premise of (Con) reads as `≤` without the field qualifier: the capture of the field value goes into the data's `C`, which is what the last premise requires. The model checks a field with hidden storage against `τᵢ^{c}` with a fresh `c` (`regions/mod.rs` `construct`); the checker's constructor schemes state the same explicitly: `Some : ∀a c. a^{≤c} → (Option a)^{c}`.

### A bounded variable reaches what the variable reaches

Divergence: the document did not say how the bound `α^{≤C}` relates to the atom `α`.

- Symptom: in `tests/run/codegen/do_notation.fl` `pair_optional`, the value `Option (a^{≤v₁}, b^{≤v₂})` is checked against `Option (a, b)`. By the earlier §2.1, `⌈a^{≤v}⌉ = {v}` while `⌈a⌉ = {a}`, so (≤-Cap) fails; the same for `carry` and `shadowed` in `applicative_do.fl`. The checker accepts them.
- Incompleteness: the document did not connect variable atoms `α` (§1.2) with the value bounds that substitution (Inst) checks only at the call site. The model compared only regions in obligations and merely grew capture row variables for `Outlives`. This suffices for T4 as long as generic code does not hide a type variable.
- Resolution, document and model. §2.1: `⌈α^{≤C}⌉ = {α^{≤C}}`, the atom `α` with the fact "no wider than `C`"; it fits into a set containing `α` or `C`, and in an admitting set it gives only `α`. Then `a^{≤v} ≡ a` (§2.2), and `pair_optional`, `carry`, `shadowed` derive. The model (`flick-model/src/check/regions/mod.rs` `reach_bounded`, `solve.rs` `excess`) keeps the atom together with its bound and also compares atoms `α` in obligations: a type variable that generic code hides behind a qualifier is `FLICK-G001`, like a region. The corpus `tests/run/model.txt` and the model properties (`just model`) show no divergence from the checker.

### Value bounds in the core

Decision: Lean has value bounds as facts of schemes, not as a type former.

- Before: Lean had only hidden qualifiers. `Some`, `Ok`, `Err`, `fold` and `pure` were skipped in `formal/Formal/Live/ElideSchemes.lean` ("a value bound, which the core does not have"), the elaboration dropped their bounds, and `Ref.*` instantiated the cell type with no capacity.
- Decision: a bound `αᵢ^{≤D}` on a variable of a scheme is the fact `Outlives αᵢ D` of the scheme, `Facts.bnd i D` (`SchemeOps.lean`), and the body of the scheme writes `α`. `Outl Θ C (.caps D)` is `C ⊆ D` up to the facts, so `Scheme.Holds` at an instance is (Inst) `⌈τᵢ⌉ ⊆ D[c̄ := C̄]` (`Scheme.holds_bnd`): (Var), a global and the projection of a dictionary field check it, and (Let-Gen), a global and a field of a dictionary assume it in their bodies. By §2.1, `α^{≤C}` and `α` reach the same, so a type former would add a case to every analysis of `Ty` (substitution, ⌈·⌉, `LeLive`, canonical forms, shapes, preservation, T4′) and no judgment that reads it.
- The capacity of a cell is the instance `ι.C` of `Ref.*` (§1.5): a cell type stays `Ref τ`, `Const.ok` asks `⌈τ⌉ ⊆ c`, and the arrow `Ref.set ℓ` has the qualifier `c`, so storing needs the capacity live. The store typing and canonical forms are unchanged.
- Left out: a bound on a type that is not a variable is read as `⌈υ⌉ ⊆ C`, not `C ∪ regions(υ)` (`formal/README.md`, simplification 15); and since facts are not chained, `K ⊆ X` with `α^{≤C}` in `K` follows from `C ⊆ X` only for `X = C` (§2.1). No rule or theorem of Lean needs the chained comparison: (Inst) compares an instance with the instance of its bound, and the elaboration compares what types show.
- The model's schemes: a bound `a^c` on a variable of a scheme is a fact of the core scheme in `formal/Formal/Live/ElideSchemes.lean`, which `Scheme.elide` keeps (`flick-model/src/translate/lean/elide.rs`), and a bound `(a, c)` of the scheme of the elaboration in `formal/Formal/Elab/PreludeSchemes.lean` (`tr.rs`, rule 1 of [the scheme translation](elaboration.md#value-qualifiers-in-prelude-schemes)). `Some`, `Ok`, `Err`, `fold` and `pure` are no longer skipped. A bound the core cannot read so is skipped with its reason: on a variable of a `∀` of a parameter, on a type that is not a variable, or, in `ElideSchemes.lean`, in a scheme with a runtime stage, whose elision the model reads through the bound and `Ty.elide` through the variable. None of the exports has one.
- The elaboration (T6): (Var-Dict), (Method) and (Const) check the bounds of their scheme at the instance, and the core reads them as facts (`elaboration.md`, [value qualifiers in Prelude schemes](elaboration.md#value-qualifiers-in-prelude-schemes)).

### A Scope in a local function row is not a capture

Divergence: a local function with `Scope s` in its row was considered to hold `s`.

```flick
scoped_row : Unit ->{} Unit
let scoped_row u =
  scope s1 {
    v3 : Int ->{Scope s1} Unit
    let v3 n = ()
    let t : Eff {Scope s1} none Unit = task { v3 0 }
    await t
  }
```

- Symptom: `FLICK-G001` "this task uses `v3`, which holds region `s1`". `v3` captures nothing, and `Scope s1` is in its service row, which ⌈·⌉ excludes (§2.1). With `v3 : Int ->{} Unit` the program was accepted. The model accepts both.
- Checker incompleteness, not a hole: it rejected too much. Found by the model's program generator: a recursive local function with a stage from the budget inside a `scope`.
- Resolution, checker: for a function under a qualifier, `collect_bounded_captures` (`regions/mod.rs`) walked its whole type including arrow rows; now `without_rows` removes the rows of arrows and `Eff`, as ⌈·⌉ of §2.1 does. Fixture: `tests/check/pass/regions/scope_row.fl`; the generator puts `Scope` in the rows of local functions.

### A qualifier on a meta does not bind the meta to itself

Divergence: the bound `a^c` on a variable of the same `a` gave an infinite type.

```flick
in_cell : Unit ->{; Bad} Int
let in_cell () =
  let cell =
    Ref.make (try
        { a = from_result (Err (Oops 1)); b = await (from_result (Err (Bad ""))) }
      with
      | Oops _ -> { a = from_result (Err (Bad "")); b = () })
  0
```

- Symptom: `FLICK-T016` "expected `{ a : _; b : Unit }`, found `{ a : _; b : Unit }`: the type would contain itself". Without `Ref.make` the program was accepted; the model accepts both.
- Cause: on `α^c ≡ α`, where `α` is a not yet inferred meta, `unify.rs` bound `α := α^c`, and the occurs check found `α` in its own solution. The bound does not make the type different: this is `α ≡ α` plus a qualifier comparison, which for an uninferred type is deferred ([bounded qualifiers](#bounded-qualifiers-are-compared-after-the-type-is-inferred)).
- Checker incompleteness, not a hole. Resolution, checker: a meta is not bound to a qualifier on itself, and the pair proceeds to the qualifier comparison. Found by the model's program generator. Fixture: `tests/check/pass/regions/late_captures.fl` `in_cell`.

### Spawn bounds everything the task holds

Divergence: (Spawn) bounded only the hidden qualifier of the task.

```flick
shown_by_result : Unit ->{} Unit
let shown_by_result () =
  scope outer {
    scope inner {
      let h = open "a"
      ignore (Fiber.spawn outer report (task { use h; h }))
    }
  }
```

- Symptom: under the earlier wording of §2.4, the premise of (Spawn) was `Outlives C ϱ` for the qualifier `C` of the task `(Eff … a)^C`. The task holds `h`, but the region `inner` is shown by its result `Handle inner`, and (Task) admits `C = ∅`. By the document's rule the program was accepted, although the child of `outer` holds `h` after `inner` closes. G4 said the same: `Outlives` is needed for captures not shown by what follows the argument, and the result `Fiber e a` shows them.
- The checker does not accept the program: `FLICK-G003` "the child holds region `inner`, which may end before its owner region `outer`" (`regions/solve.rs` `check_outlives` checks everything the task holds). Imprecise document, not a hole.
- Resolution, document: the premise of (Spawn) is `Outlives (C ∪ ⌈a⌉) ϱ`, everything the retained argument holds; likewise for a finalizer and for any constant with `retention owner` (G4). Lean: `Const.ok`. Found while proving T4 in Lean (`formal/Formal/Regions.lean`).

### Lam and Task count runtime forms

Divergence: (Lam) and (Task) did not see runtime forms in the body.

- Symptom: the premise of (Lam) counted only the free variables of the body. The term `λx. (λy. ()) (cap π n)` has none and gets type `(Unit → Unit)^∅`, although the body holds a capability of region `π`, and L4 fails for it. Programs contain no such terms, but a step creates them: substituting a value for a free variable leaves `cap π n` inside a `λ`.
- Imprecise §3.4. Resolution, document: for runtime forms, the premises of (Lam) and (Task) count the regions of `cap π n`, `own π` and `⌈Ψ(ℓ)⌉` of the body's locations on a par with `⌈Γ(y)⌉` of the free variables; for programs the rule is unchanged. Substituting a value preserves the premise by the syntactic part of L4: a value holds no more than its type shows. Typing by use has dropped the premise and that lemma (Lean keeps `held`, `Reach.lean`, as a measure only). Found while proving T4 in Lean (`formal/Formal/Regions.lean`).

### A cell's content and the cell both bound what is stored

Divergence: the model checked a value stored by `Ref.set` only against the outer of two qualifiers.

```flick
leak : Unit ->{} Unit
let leak () =
  let cell = Ref.make (task { () })
  let _ = scope s { let h = open "x"; Ref.set cell (task { use h }) }
  await (Ref.get cell)
```

- Symptom: the model's checkers, captures and live, accepted the program, and its machine stopped with "use after close". `Ref.set` takes `α^c` with `α` the cell's content `τ^{c₀}`, so the task was checked against `(τ^{c₀})^c`. (Lam), (Task) and (≤-Cap) took only `c`, which is free and grew to `s`; `c₀`, the capacity the cell was made with, was never asked. flick-core rejects the program in both modes.
- Model hole, not a hole of the document or of Lean, where `Ref.set` takes exactly the cell's type. Resolution, model: a value checked against nested qualifiers is within each (`qualifiers`, `layers`, `close_within` in `check/regions/mod.rs`), so `c₀` grows to `s` and `s` would leave its `scope` (`FLICK-G002`). Found while porting the live discipline to flick-core. Test: `flick-model/tests/model/live_core.rs` `live_core_rejects_a_task_in_an_outer_cell`.

### A value bound allows no use

Divergence: the checker and the model let a function or a task use what a value bound of its position allows.

```flick
stash : Handle s -> Ref (Option (Unit ->{} Unit)) ->{} Unit
let stash h cell = Ref.set cell (Some (fun () -> use h))
```

- Symptom: the checker accepted `stash`. Called inside `scope s` with a cell made outside it, the closure read from the cell after the `scope` used the closed region (#660, `USE AFTER CLOSE` on Node). The closure was inferred against the bound `^c` of `Some`, so its row needed `Live c`; unifying that row with the closed `{}` of the cell's arrow left the `Live` out while `c` was not known, and `c` then grew to `s`. Written directly, `Ref.set cell (fun () -> use h)` and `Ref.set cell (task { use h })` into a cell of `Unit ->{} Unit` or `Eff none none Unit` were checked against the type of the cell under the bound `c` of `Ref.set`, and took `c`, which nothing bounds, as what they may use. The model rejected `stash` by (Con), but accepted the direct writes, and its machine stopped with "use after close".
- Checker and model hole, not a hole of the document or of Lean: a value bound is a fact `⌈τ⌉ ⊆ c` of the scheme and the body of the scheme writes `α` ([value bounds in the core](#value-bounds-in-the-core)), and (Lam) and (Task) check a body at the use stage of its own arrow or `Eff`, so `Ref.set` stores in a cell of `Unit →R Unit` only a function that uses nothing. Resolution, checker: a function or a task checked against a value bound uses it only while its own type is inferred, whose rows then record it (`regions/live.rs` `expected_live`); a `Live` that unification leaves out against a closed row without it empties its capture rows not inferred yet, so a region they would gain is reported where it is used (`regions/live_rows.rs` `unify_live`). Model: (Lam), (Task) and (≤-Cap) take only the hidden qualifiers of the expected type as what a use may need (`hidden_qualifiers` in `check/regions/mod.rs`), and a function or a task that a bounded variable takes records the innermost bound in its row, as the checker's inferred one does (`subsume_bare` in `check/rows.rs`), so a cell made from one closure still takes another that uses the cell's capacity (`tests/run/runtime/arena_finalizers.fl`). Tests: `tests/check/fail/regions/stored_closure.fl` (`FLICK-G002`), `tests/check/pass/regions/stored_closure.fl`; `flick-model/tests/model/live_core.rs` `live_core_rejects_a_use_in_a_typed_cell`.
- Symptom after that resolution (#723): the order of the arguments decided the verdict, and in fact both orders were rejected. With `keep2 : List (a^c) -> a^c ->{} List (a^c)`, `k : Unit ->{} Unit` and `h` a handle of `s`, the checker rejected `keep2 [k] (fun () -> use h)` and `keep2 [fun () -> use h] k`. In the first, `k` fixed `a := Unit ->{} Unit`, so the closure checked against it could use nothing. In the second, the closure fixed `a := Unit ->{Live c} Unit`, and `k` was unified with it; that left `Live c` out against the closed `{}` and emptied `c`, as the resolution above does for a cell's written type.
- Checker incompleteness, not a change of the document, of Lean or of the model: a value bound is still the fact `⌈τ⌉ ⊆ c` of the scheme, and (Lam), (Task) and (≤-Cap) still check a body at the use stage of its own arrow or `Eff`. Lean types both calls with `a := (Unit → Unit)^{s}` (`LeLive.arrow` widens `k`), and the model accepts both. Resolution, checker (`regions/data.rs` `subsume_bound_at`, `regions/live.rs` `callable_bound_fits`): a function or a task that fixes a bounded variable gives it its own type, qualifier kept, whose row also keeps live a use row of its own, `Live w` for a fresh `w`; a closure fitted later grows `w`, and a written type without it empties `w`, not the bound, which says what the value reaches, not what using it needs. Where the bound names a variable of an inner `forall` the type cannot take, the value fixes it as before. One fitted into a bound whose type is known fits that type by subsumption, as two arrows are compared. Each only widens a row, so a closure still uses only what its own row keeps live, and the list stored in a `Ref (List (Unit ->{} Unit))` or kept past `scope s` is still `FLICK-G002`. Two consequences: `grow_live` (`regions/live_rows.rs`) tries each `Live` variable not inferred yet of a row, since the qualifier of a function made before the region's `scope` cannot take the region and another can; and instance matching ignores `Live` (`types/classes/matching.rs`), which no instance head names, so `throw_error` of such a function or of a closure still selects `MonadError (ExceptT e m) e`. Model: it records the innermost bound in the row instead of a fresh `w` (`subsume_bare` in `check/rows.rs`), unchanged; both give these verdicts. Tests: `tests/check/pass/regions/bound_order.fl`, `tests/check/fail/regions/bound_order.fl`; `flick-model/tests/model/live_core.rs` `live_core_keeps_bounded_functions_in_either_order`; `flick-core/tests/transformer_captures/mtl.rs` `except_errors_of_runtime_callbacks_find_their_instance`.
- Symptom for services and errors (#762): the same order sensitivity hit closed service and error rows even without regions or value bounds (`keep2 [quiet] (fun () -> Log.line "x")` and `if b then quiet else (fun () -> Log.line "x")`, with plain `keep2 : List a -> a ->{} List a` or value-bounded `keep2 : List (a^c) -> a^c ->{} List (a^c)`). Both `flick-core` and `flick-model` let `quiet : Unit ->{} Unit` fix the type variable to the closed row `{}`, rejecting a later function needing `Log` (`FLICK-E002`), whereas Lean's `LeLive.arrow` and `LeLive.eff` already permit widening closed rows covariantly.
- Checker and model incompleteness, not a change of the document or of Lean: when a value fixes an open type variable under subsumption (or a covariant argument of a data type), closed (or rigid-tailed) service and error rows in covariant positions — arrows, `Eff`, `Layer`, tuples, closed records, and `+` data parameters — are widened with fresh open row tails (`flick-core`: `types/effects/widen.rs` `bind_subsumed_meta`, `widen_for_meta`; `flick-model`: `check/widen.rs` `widen_for_meta`). Contravariant parameters, invariant positions (`Ref`, GADTs), opaque aliases, pure arrows, `@sync` service/error rows, and rows already carrying an inference tail (`-S | ?r`) stay untouched; remaining open widened tails are elided in diagnostics (`types/display.rs` `strip_widened`) and closed to empty at generalization and end of declaration (`effects/solve.rs` `settle_rows`, `close_rows_above`; `flick-model` `check/generalize.rs`, `check/widen.rs` `close_widened_rows` from `check/mod.rs` `finish`). Unifying a widened tail with `Without` (`-S | ?s`) preserves the exclusion (`types/rows.rs`). `FLICK-E002` fixes only target written rows owned by the enclosing declaration (`types/toplevel.rs` `owner_of`, `types/effects.rs` `editable_place`), so a caller never receives a suggestion to edit another binding's signature. Tests: `tests/check/pass/effects/service_order.fl`, `tests/check/fail/effects/service_order.fl`, `flick-core/src/types/effects/ownership_fix_tests.rs`, `flick-model/tests/model/live_service_order.rs`.

### Scope keeps its region live

Divergence: typing by use in the model and in flick-core kept `Live C` as a row element of its own, apart from `Scope s`.

```flick
group : Unit ->{Scope s, Async;} FiberSet.FiberSet s none Unit
let group () =
  let set : FiberSet.FiberSet s none Unit = FiberSet.make ()
  let _ = FiberSet.run { propagateInterruption = false; startImmediately = true } (task { () }) set
  set
```

- Symptom: both checkers rejected the helper (`FLICK-G002` "this uses `s`, which the signature of `group` does not keep live"). Its body uses region `s`, its row has `Scope s` and no `Live s`, and no program can write `Live s`. The same held for `tests/run/runtime/{closing_child_fails,closing_hooks,group_in_closing}.fl`, which the former capture discipline accepted and which run.
- Stricter than Lean, not a hole: in `formal/Formal/Live/Mode.lean`, `Stage.use (.rt r ε) C` is `.rt (Row.union r (Row.ofCaps C)) ε`, and `Row.scope s = Row.ofCaps s.reach` (`Syntax.lean`), so `Live C` and `Scope s` are the same row atoms. Resolution, model and checker: where `Live C` is checked within a row, the regions of that row's `Scope ϱ` count as live. Work an owner keeps needs the regions of its `Live` to outlive the owner; its `Scope` is the owner's own ([children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner)), which adds nothing to that. Only one direction: `Live ϱ` gives no `Scope ϱ`, so an acquisition into `ϱ` still needs `Scope ϱ` (`FLICK-G004`). Model: `check/regions/live.rs` `scope_live`, `check/rows.rs` `within`; flick-core: `types/regions/live_rows.rs`. Tests: `flick-model/tests/model/live_scope.rs`; `tests/check/pass/regions/scope_helper.fl`, `tests/check/fail/regions/acquire.fl`.
- Extended to `Setup s` of a component, like any service indexed by a region: a row with `Setup s` keeps `s` live, and a handler of it is installed only where `s` is live, so a `provide` of `Setup s` outside the instance of `s` is rejected. The runtime gives `Setup s` only while it sets up the instance of region `s` (`setUp` in `runtime/src/rt/ui/views.ts` and `runtime/src/ui/react/views.ts`). Checker: `regions/live_rows.rs` `live_install`; model: `check/regions/live.rs` `indexed_live`; Lean: `hdl_err_live` (`formal/Formal/Live/Install.lean`): (Handle) removes only the name of the effect from the body's row, so every region its index reaches stays in the stage where the handler is installed. Tests: `tests/check/pass/ui/component_setup.fl`, `tests/check/pass/regions/setup_region.fl`, `tests/check/fail/regions/install.fl`.

### Found by checking the corpus in the model

`flick-model/tests/model/accepted.rs` checks every pass fixture and every docs example flick-core compiles in the model; what the model still rejects is `flick-model/tests/known_gaps.txt`, each with its reason. The first run rejected 100 of 652. Each item below made the model stricter than the checker, none accepted a leak:

- a row written as the argument of a type (`Atom.Async a none`) is `Errors ρ`, as instantiating a row variable there gives it; an argument of kind SRow reaches nothing (§2.1, `Server.Function {Server | r}`);
- `ask_S` and the layers of a service with parameters (`Repo a`) take them: `∀p̄. Unit →R⟨S p̄⟩ S.Impl p̄`, and an operation of `S τ̄` its scheme at `τ̄`;
- a list literal against `List τ` checks each element against `τ`, so (Sub) may widen it (`[text "x"] : List (Ui^c)`);
- the `Scope` region of a step outside the `scope`s of its λ or task is the one the stage names ([leases](#leases-instead-of-early-close)), and in a task argument it waits for the end of the call ([after the call](#scope-region-of-a-task-argument-is-resolved-after-the-call));
- work two owners keep that uses one service is kept by the outer owner when one `scope` encloses the other: the provider outlives both (§2.7).
- `Async` of work an owner keeps makes no `Provided` obligation: the root gives it to every fiber, as the checker reads it (§2.7); the model had rejected `Resource.own`, whose close runs with `{Async}`.
- a `Fiber.Provided` constraint over a task, `forall t. Fiber.Provided r t => Eff {Scope t | r} e a^{t}` of `Resource.manual`, is given to the task as to the body of a λ; the model had dropped it, as it placed the constraint only in the row of a runtime arrow (`Type::with_provided`).
- (Scope) in the model (`check/regions/scope.rs` `free_regions`, `unify.rs` `lower`, `check/regions/data.rs` `project_part`) ignores vacuous value bounds on types that hold nothing (`holds_nothing`: scalars and parameter-only data with empty reach, such as `String^{≤s}` from `RcMap.get`), while keeping bounds on opaque/hidden types (`Raw^{≤s}` from `Resource.acquire_release`, G4) and type variables; and `take_open_region` (`check/regions/solve.rs`) binds only an unsolved `Kind::Region` meta in a written qualifier (`qualifiers` / `allowed`), never an error-row, type, or argument meta in the underlying type (which previously bound the error-row meta of `Stream.partition_effect` to the scope's region and prevented the stream's capture-row meta from growing before leaving `scope`).

Fixtures: the cases of `accepted.rs`; `flick-model/tests/model/live.rs` `live_on_std` lists the std values the model still cannot type.

### The step of scope names its region in signatures

Divergence: the reference machine took the step of `scope` as `inside π e[s := own π]` and left `s` in the signatures of `e`, bound again by (Inside).

```text
scope s {
  let h = open "x"
  let _ = Fiber.spawn s report (task {
    v : ((Unit ->{} Unit)^{s}) ->{} Unit
    let v f = f ()
    v (fun _ -> use h) })
  0 }
```

- Symptom: `FLICK-G001` "this value holds {π0}, which `` does not allow" at the step of `Fiber.spawn` in `progress_and_preservation_hold`, and `FLICK-G002` when the argument of `v` holds nothing. The task became a child `K(π)`, and `⊢ Λ` types a child outside the `inside` that bound `s`; a finalizer and the value of a cell leave the same way.
- Incompleteness of the reference machine, not of the document: §3.2 has `e[s := π][s := own π]`, and under it the child's signature names `π`.
- Resolution, model: the step puts `π` in place of `s` in the signatures of the body, but for a signature that binds `s` itself and for a nested `scope s` (`flick-model/src/subst.rs` `subst_in_region`). Tests: `a_child_keeps_the_region_its_signatures_name`, `work_of_a_region_names_its_scope` (`flick-model/tests/model/props.rs`).

### Fix keeps the qualifier of a local signature

Divergence: (Fix) of the reference checker took the signature of `fix f. λx. e` bare, although (Let-Sig) had inferred its outer qualifier ([local signatures infer only the outer qualifier](#local-signatures-infer-only-the-outer-qualifier)).

The program the model properties found, as the generator wrote it and its choices shrank it:

```text
scope s1 {
  let h2 = open "x"
  v3 : Unit ->{} Unit
  let v3 x7 =
    ({ a = use h2
       b = scope s4 {
         let h5 = open "x"
         let cell6 = Ref.make 0
         0 } }).a
  let cell10 : Ref (Int -> Int) = Ref.make ((fun x9 -> 0) : Int -> Int)
  let v11 : (Eff none none Unit)^{s1} = task { v3 () }
  0 }
```

- Symptom: `FLICK-G001` "this value holds {π0}, which `` does not allow" in `progress_and_preservation_hold`, one step after `let v3` (the step that makes it `fix v3. λx7. …use (cap π0 n)…`, see [stepping a signed let](core.md#stepping-a-signed-let-loses-its-signature)): the λ needs `Live {π0}`, and a bare `Unit →R⟨none;none⟩ Unit` allows none.
- Incompleteness of the reference checker: the program is accepted with `v3 : (Unit →R Unit)^{s1}`, and the state is typed by the same rule once (Fix) infers the qualifier as (Let-Sig) did.
- Resolution, model: (Fix) checks the λ against the local signature with its own qualifier inferred, and gives `fix` that type (`flick-model/src/check/bindings.rs` `fix`). Tests: `a_function_keeps_the_qualifier_of_its_local_signature`, `a_stepped_signature_keeps_its_qualifier` (`flick-model/tests/model/props.rs`).

### The work of a region is typed inside it

Divergence: `⊢ Λ` of the reference checker typed a child or a finalizer of `π` outside every region, so a `scope` written in it was nested in nothing.

```text
scope s1 {
  let _ = Fiber.spawn s1 report (task {
    (scope s4 {
      let _ = Fiber.spawn s4 report (task {
        let _ = Fiber.spawn s1 report (task { () })
        () })
      { a = (); b = 0 } }).a })
  0 }
```

- Symptom: `FLICK-G003` "`π0` may end before region `s4`" in `progress_and_preservation_hold`, at the step that makes the outer task a child of `π0`: the child of `s4` holds the owner of `s1`, which needs `s1 ≥ s4`. The program has it by the first rule of §2.4, `s4` being written inside `scope s1`; in `K(π0)` the task stands apart from that `scope`.
- Incompleteness of the reference checker: a child runs in the region of its owner ([children acquire in the region of their owner](#children-acquire-in-the-region-of-their-owner)), so a `scope` it opens is inside `π0` and the regions `π0` was made in, as the machine runs it.
- Resolution, model: `⊢ Λ` types the work of `π` as nested in `π` and in every region `π` was made in (`flick-model/src/check/runtime.rs` `lambda`, `check/regions/scope.rs` `nested_in`). Test: `a_scope_in_a_child_is_inside_the_region_of_its_owner` (`flick-model/tests/model/props.rs`).

### The hidden row of data is not what its arguments show

Divergence: the checker and the model counted the hidden row of data with hidden storage as covered by what the other arguments of its type show, and read such a type written bare in a signature as `^∅`.

```flick
type G s a = G (s ->{FileSystem} a)

run : G s a -> s ->{FileSystem} a
let run (G f) x = f x

compose : G a b -> G s a ->{FileSystem} G s b
let compose inner outer = G (fun x -> run inner (run outer x))

leaked : Path ->{Async, FileSystem; OpenError} G Unit String
let leaked path = scope {
  let file = File.open_scoped path
  compose (G (fun f -> File.read_all f)) (G (fun _ -> file))
}
```

- Symptom: `G (fun f -> File.read_all f)` is `(G (File s) String)^{s}`: its field uses `s`. Passed as a bare `G a b`, it lost the row because `File s` shows `s` (`regions/solve.rs` `hidden`, `regions/data.rs` `subsume_bound_at`); inside `compose`, generic in `a`, nothing shows `s` any more, and the composition leaves the `scope`. Run on js and Go, it reads the released resource after the `scope` ends, without an error. The model accepted it too. The direct form, `G (fun _ -> File.read_all file)` out of the `scope`, is `FLICK-G001` at the value of the `scope`, as it was before this resolution: the bare result `G Unit String` reads as `^∅`, and subtyping against it comes before the end of the `scope`.
- Lean rejects it, because `LeLive` does not narrow the row. With the bare row read as `∅`, at the `app` of `compose`: the field of the argument needs `s` in its stage and its declared row does not name it, so the argument is `G {s} …` (`LeLive.arrow_use`), and `¬ LeLive (G {s} u v) (G ∅ u v)`. With the row read by elision (§1.4), at the `scope`: the result `(G Unit String)^{s}` reaches `s`, and the body of a `scope` has a type `t.up 1`, which does not (`Ty.up_avoid_v0`). Checked by lemmas in a scratch file outside the library (2026-11-12), against `LeLive` without the rule for data; with the rule, which only widens, both rejections are checked again as regression lemmas of `LiveRules.lean`.
- A pure foreign function, `peek : File s -> Int`, reads a released file through the same composition built with a pure helper. Typing by use accepts it: a pure stage uses no region (`Stage.use .pure _ = .pure`). What breaks is G4: a pure foreign arrow over a resource claims its call needs nothing live.
- Resolution, document: §1.4 reads the hidden row of a bare data type in a signature by elision, §2.2 lets subtyping only widen it, and G4 gives a foreign function over a resource a runtime stage. The contract of typing by use (§0, T4′) is unchanged. For programs: the row of a result holds what the earlier parameters reach, so `make : Handle r -> G Unit Int` that returns a fresh closure holds `{r}` and does not leave the `scope` of `r` unless written `(G Unit Int)^{}`; `cf (F f) (F g) = F (fun x -> f (g x))` for `type F s a = F (s -> a)` is accepted, its result row being `{c, d}`.
- Resolution, checker: the reading of signatures (`regions/hidden_rows.rs`, once when `convert.rs` converts a signature; a read row is marked and not shown), the hidden row compared by inclusion without what the other arguments show (`regions/data.rs` `subsume_bound_at`), and no parameter atom in `constructor_field`. Lean rejects `leaked` at the `scope` (`Ty.up_avoid_v0`); the checker reports the same program as `FLICK-G001` at the value of the `scope`, because the bare result of `leaked` reads as `^∅` and the value is compared with it first. A composition through a pure helper that takes the functions out of the data, `G (c2 f g)` for `c2 : (a ->{…} b) -> (s ->{…} a) -> s ->{…} b`, is `FLICK-G002` in its body: the functions use the hidden rows of the data, and a bare callback parameter of `c2` keeps none of them live. Tests: `tests/check/fail/regions/hidden_through_helper.fl` (`through_run`, `through_c2`, `through_function`), `tests/check/fail/regions/hidden_rows.fl` (`make` leaving the `scope`, a named row `K`), `tests/check/pass/regions/hidden_rows.fl` (`cf`, `pre`, a composition through `run`, a written `^{}`).
- Resolution, Lean: the reading is formalized in the core, as the elision of `Live` is (#295): `Ty.bare`, a mark for a bare row distinct from a written `^{}`, read by `Ty.elide` and `Scheme.elide` (fresh binders of kind `CRow` after those of the scheme), the parameters of a parameter that is a function included; `Ty.reach_le_elide`, and `Ty.reach_elide` for a type without `bare`; `Live/ElideRows.lean` reads `compose` and stops `leaked` at its `scope`. The rule of `LeLive` that widens the hidden row of data is #281.
- Resolution, model: `live_scheme` reads the rows as `Scheme.elide` does (`check/regions/live.rs` `fresh_rows`, `elide`; `bare_hidden` in `check/regions/mod.rs`), and (≤-Cap) holds the hidden row of such data within the expected one, with only the variables its type shows beside it (`check/rows.rs` `subsume_unfolded`). No export of `Prelude`, `Seq`, `Option`, `Result` or `List` wrote such data bare then, so `Live/ElideSchemes.lean` was unchanged; the codecs of `Prelude` marked `@stores` later do ([a foreign type that keeps functions shows no hidden row](#a-foreign-type-that-keeps-functions-shows-no-hidden-row)). Tests: `bare_data_in_a_signature_reads_its_hidden_rows` (`flick-model/src/check/tests.rs`); `the_model_refuses_leak6` (`flick-model/tests/model/leaks.rs`): the model refuses `leaked` by (≤-Cap) on its own, and checks the same composition run inside the `scope`.
- Resolution, certificates: a `match` on data with hidden storage types its fields at the row the matched value hides, as (Pat-Adt) takes the arguments of its type (`PatTy.head`). The model gives the fields of such data that row instead of projecting the value's qualifier onto them (`check/pattern.rs` `bind_pattern`, `con_fields`), and the writer puts the row first among the arguments of the pattern's type (`cert/pat.rs`). The generator follows a bound `Fixture.Sealed` with a `match` on it and a call of a local function whose signature writes it bare (`generate/sealed.rs`); `formal/FormalCert/Gen/Kept.lean` keeps both forms, and the fixed run of 2000 programs has no gap.
- Open: how data with hidden storage substituted for a higher-kinded variable of a class signature (`Functor (StateT s m)`) gets its row; settled with the checker by `tests/check/pass/regions/transformers.fl`.

### A binder of a `∀` in a signature counts for nothing

Divergence: the checker did not read the hidden rows of data under a `∀` of a signature, and the model counted the region binders of such a `∀` among what a stage needs and a row holds (`check/regions/live.rs` `elide`, through `regions_of`). The core counts neither: `Ty.elide` of `.all k t` reads `t` at `Caps.under k K` and `Caps.under k H`, which shift the outer atoms and leave out the `k` new binders.

```flick
tagged : (forall a. File s -> a -> G a) -> File s ->{Async, FileSystem; ReadError} String
with_file : (forall t. File t -> G String) -> File s ->{Async, FileSystem; ReadError} String
```

- Symptom: the checker read `G a` in `tagged` as `^∅`, where the core reads `(G a)^{s}`, and refused a callback that returns data over `f : File s`. The model read `G String` in `with_file` as `(G String)^{t}`, where the core reads `^∅`, so it accepted a callback the core refuses.
- Resolution, checker: under a `∀` the reading goes on with the rows held so far, counts no binder of it, and renames a binder named as an atom those rows carry (`regions/hidden_rows.rs` `rows_of_stages`). Tests: `tests/check/pass/regions/hidden_rows_rank_n.fl` (`tagged`, the shadowing `shadowed`), `tests/check/fail/regions/hidden_rows_rank_n.fl` (`with_file`'s callback, `FLICK-G001`).
- Resolution, model: `elide` under a `Forall` counts none of its binders, for `Live` as for the hidden row, and renames a shadowing one (`elide_under`). Test: `under_a_forall_only_outer_atoms_are_held` (`flick-model/src/check/tests_forall.rs`). The corpus, `tests/run`, `examples/` and 500 generated programs read the same.
- Lean: unchanged; the rule is `Ty.elide` of `.all`.

### A foreign type that keeps functions shows no hidden row

Divergence: G4 asks a `retention call` function to keep nothing its result does not show, but a foreign type without constructors read bare as `^∅`, and `on_missing_effect` of `Prelude` keeps its default in the codec it returns.

```flick
later : Path ->{Async, FileSystem; OpenError} Codec {Async, FileSystem} none Json Row
let later path = scope {
  let file = File.open_scoped path
  codec Row.schema {
    size = field_effect "size" (Codec.general Float.json_finite)
      |> on_missing_effect (fun () ->
        try Int.to_float (String.length (File.read_all file)) with | ReadError _ -> -1);
  }
}
```

- Symptom: the codec leaves the `scope` that opened `file`; decoding `{}` with it after the `scope` runs the default, which reads the closed file. On js, ts and Go the program prints `decoded false` without an error.
- Resolution, document: `@stores` before a type without constructors or a body makes it data with hidden storage (§1.4), and the types of std whose values keep functions are marked; T4′ and its hypotheses are unchanged, as the mark makes the signatures say what G4 already asked. A result that is a function shows its qualifier and needs no mark (G4, §5.1).
- Resolution, checker: `syntax/validate.rs` places the attribute (`FLICK-S014`), `types/decls.rs` `declare_types` sets `Form::Nominal { hidden }`, which the interface already carries, and the reading of §1.4 (`regions/hidden_rows.rs` `data_hides`) gives the type a row. The code of a `codec` block names a field's decoded type with `Codec.Derive.Decodes`, not by writing the type of the field's codec, which, bare in an annotation, holds nothing. Tests: `tests/check/fail/regions/codec_stores.fl` (a block, one field and `transform_effect` leaving the `scope` are `FLICK-G001`), `tests/check/pass/regions/codec_stores.fl` (a block used inside its `scope`, one that holds nothing leaving it).
- Resolution, written qualifiers: the reading did not enter a result that is a function with a written qualifier, `(PureCodec Json a -> PureCodec Json b)^c` of `Codec.transform`, while the core's `Ty.fresh` and `Ty.elide` do, its qualifier being a field of the arrow; the short form of the signature, which omits that qualifier, read a row the signature did not (`every_builtin_signature_comes_back_from_its_short_form`). The checker (`hidden_rows.rs` `rows_of_stages`) and the model (`check/regions/live.rs` `fresh_rows`, for a parameter too) now enter it. Tests: `tests/check/pass/regions/hidden_rows_qualified.fl` (`lift` over data that reads a file, inside its `scope`), `tests/check/fail/regions/hidden_rows_qualified.fl` (its result leaving the `scope`, `FLICK-G001`).
- Resolution, model: `Decls.hidden` keeps the types without constructors an interface says hide captures, an opaque ADT of std included, and `bare_hidden` reads them (`check/regions/mod.rs`); the Lean translation shows their row as it does `Seq`'s (`translate/lean/elide/data.rs`), and `Live/ElideSchemes.lean` is regenerated. Test: `a_nominal_type_with_hidden_storage_reads_its_hidden_rows` (`flick-model/src/check/tests_stores.rs`).
- Resolution, Lean: `Sig.hidden_ok` speaks only of the constructors of a type, so a signature that builds no `T` may give it a hidden row (`storesSig`); `on_missing` reads its result's row (`on_missing_reads_hidden_rows`) and a codec whose default holds a scope's region does not leave it (`on_missing_scope_rejected`), in `Live/ElideRows.lean`.

### A capture row a declaration names is invariant

Divergence: a rule that widens every capture-row argument of data, `(T C ā) ≤ (T C' ā)` for `C ⊆ C'`, breaks T4′.

```flick
type K (c : CaptureRow) = K ((Unit ->{} Unit)^c ->{} Unit)
```

- Symptom: a `K` whose field stores its callback in a cell made outside the `scope` is a `K ∅`. Widened to `K {s}` inside `scope s`, its field takes a callback that uses `s`; after the `scope` the callback, read from the cell as `Unit ->{} Unit`, runs and uses the closed region (T4.1). In a proof of preservation the field, matched as `(Unit ->^{s} Unit) ->{} Unit`, has a value typed `(Unit ->^∅ Unit) ->{} Unit`, and getting from one to the other narrows the qualifier of its parameter.
- Resolution, document: only the hidden row the reading adds (§1.4) widens, under the condition of §2.2 that it is covariant in the fields; a row a declaration names keeps `≡`. Lean: the condition is part of a well-formed `Sig`, and `K` is a regression of `LiveRules.lean` (`contra_row_not_hidden`, `named_row_invariant`). The model compares the row by `≡` and refuses `K {} ≤ K c` on its own (`the_model_refuses_widening_a_named_row`, `flick-model/tests/model/leaks.rs`).

### An index a GADT constructor fixes is not widened by its fields

Divergence: inferring the variance of a parameter from the fields alone makes a GADT index covariant when no field uses it, and breaks T4′.

- Symptom: with `Tag` of [§2.2.1](#221-variance-of-data-arguments), `a` occurs in no field, so the fields give it `bi`. `Pure` typed `Tag ((Unit -> Unit)^{s})` reaches `match t with | Pure -> f`, the branch refines `a` to the pure `Unit -> Unit`, and returns `f`, a callback using `s`, as a pure function out of its `scope`.
- Resolution, document: a parameter some constructor fixes keeps a sign only when `res K[j]` is closed in that direction, otherwise it is `inv` ([§2.2.1](#221-variance-of-data-arguments)). Lean: the closure lemma of the index, with `Tag` a regression of `LiveRules.lean`.

### A region of an instance's head was taken for a type variable

Divergence: an instance method used a region of its head that the class signature keeps no live, and a closure that called the method read a handle after its `scope`.

```flick
type Handle (s : Region) = Handle Raw

class Read a =
  read : a ->{} String

instance Read (Handle s) =
  let read (Handle raw) = name_of raw

leak : Unit ->{} String
let leak () =
  let f = scope s {
    let h = Handle (Resource.acquire_release (task { make_conn "x" }) close_conn)
    fun () -> read h
  }
  f ()
```

- Symptom: accepted; Node printed `close x`, then `USE AFTER CLOSE x`. A generic `later : Read a => a -> (Unit ->{} String)` around the method leaked the same way.
- Cause: `instances.rs` `instance_methods` made the skolems of the instance's variables without their kinds (`rigid_kinds`), so `live_atoms` read the region `s` as a type variable, which is no use (§1.4), and the body, checked against the class signature elided before the instance's types replace its parameters ([found while porting](#rules-of-typing-by-use-found-while-porting)), used `s` with nothing live.
- Resolution, checker: the skolems of an instance carry their kinds, so a method uses a region of its head only where the class signature keeps it live (`FLICK-G002`). The rule is the first of [the rules found while porting](#rules-of-typing-by-use-found-while-porting), so the model and Lean, where a dictionary field has the type of the class signature at the instance (`elaboration.md`), are unchanged. Fixtures: `tests/check/fail/regions/instance_regions.fl`, `tests/check/pass/regions/instance_regions.fl`.

### A bound on a variable bounds the whole value

Divergence: (Inst) bounds the whole value substituted for a variable (§2.3), but the checker took what the value's type shows for visible, not held.

```flick
free : a^{} -> Unit
let free _ = ()

shown : String ->{Async, FileSystem; OpenError} Unit
let shown path = scope {
  let file = File.open_scoped path
  free file
}
```

- Symptom: accepted, as was `free (Holder conn)` for data `Holder` over a value `conn` its `scope` owns. `Resource.own`, which takes `a^{}` so that a scope owns only a value that holds no region, accepted such a `Holder` in an inner `scope`, whose end closed the outer one's value while the outer one still used it.
- Cause: `regions/data.rs` `subsume_bound_at` read every value bound with `Visible::Late` of the bounded type, which for a variable is what the variable comes to stand for.
- Resolution, checker: a bound on a variable holds every region of the value, while a type variable in it stays no use (`Visible::Regions`); a bound on a type that is not a variable, `(List a)^c`, keeps what its type shows visible, as a type argument carries its own qualifier. Lean and the model ask `⌈τᵢ⌉ ⊆ D` (`Scheme.holds_bnd`) as before. Fixtures: `tests/check/fail/regions/value_bound.fl`, `tests/check/pass/regions/value_bound.fl`, `tests/check/fail/regions/close_token.fl`.

### An outer qualifier on a syntactic curried or rank-2 runtime callable is placed on its inner stage

Divergence: calling or partially applying a qualified rank-2 or curried parameter lost its qualifier after the pure outer stage (`#769`), so work kept by `Scope.defer`, `Scope.add_finalizer`, or `Fiber.spawn` bypassed `FLICK-G003` and escaping closures bypassed `FLICK-G002`.

```flick
rank2 :
  (Detached r, Fiber.Provided r s) =>
  a -> (forall e b. a -> Exit e b ->{r} Unit)^c ->{Scope s | r} Unit
let rank2 v f = Scope.defer (fun exit -> f v exit)
```

- Symptom: accepted by the checker (reporting only `FLICK-T052`), while the 1-arg form `(Exit e b ->{r} Unit)^c` was rejected with `FLICK-G003`. In the model, `apply` also dropped outer qualifiers when instantiating `Type::Forall`, losing `^c` even for 1-arg rank-2 calls. Symmetrically, value-bounded (`Type::Bounded`) curried/rank-2/`Eff`/`Lazy` callables could drop their region bound on application or `Ref.set` / bare subsumption, and `Forall` subsumption/unification could drop or swap constraints (`Fiber.Outlives`, class constraints).
- Cause: `use(P, C) = P` cannot consume `C` at a pure stage, and dynamically pushing `Type::Captured` across pure arrows based on `is_callable(result)` violates substitution invariance (`b -> (Int -> b)^c ->{} b` is accepted for rigid `b` and would let `?c` absorb a region after instantiating `b` to a runtime callable). In addition, `regions/data.rs` `subsume_bound_at` stripped `Type::Bounded` from actual callables without preserving live regions on runtime stages (`with_live_bound`) and guarded only `Fun` instead of `is_callable`, `lazy.rs` `force` did not call `live_await`, `effects.rs`/`unify/compare.rs` (and `flick-model/src/check/unify.rs`) ignored `Forall` constraints on mismatch/unification, and `bindings.rs` wrapped written-qualified local callables in fresh capture metas.
- Resolution, checker: `regions/live.rs` (`place_qualifiers`, called from `live_scheme` and `convert.rs` `annotation`) syntactically places any outer `Type::Captured` on a curried/rank-2 callable that starts with a pure arrow and reaches a runtime stage (`Fun { effects: Some(..) }`, `Eff`, `Lazy`) onto that inner callable stage before instantiation (unioning with any existing inner qualifier, capture-avoiding under `Forall`). `open_function` and `with_live` follow `Stage.use` directly without dynamically pushing `Type::Captured` across pure arrows, preserving substitution invariance. For whole-value `Type::Bounded` callables on actual/elimination positions, `carry_bound` (`applied.rs`) carries bounds across pure stages and `with_live_bound` (`live.rs`, `compare.rs`, `data.rs`) adds `Live` only onto reachable runtime stages (leaving all-pure functions unchanged). `infer.rs` (`check_forall`, `check_lazy`), `lazy.rs` (`lazy_expr`, `force`), `effects.rs` (`subsume_suspended`, `subsume_forall`), `unify/compare.rs` (`Forall` constraint unification), and `bindings.rs` (`infers_local_captures`) close the remaining gaps. Fixtures: `tests/check/fail/regions/rank2_qualifier.fl`, `tests/check/pass/regions/rank2_qualifier.fl`.
- Resolution, model: `check/regions/live.rs` (`place_qualifiers` in `live_scheme`), `check/regions/mod.rs` (`is_callable`, `rewrap_qualifiers` unioning `Type::Captured` without stacking, `rewrap_bounds`, `qualify_hidden`, multi-layer `qualifier`, `need_outlives`), `check/expr/applications_records.rs` (`apply` preserving qualifiers across `Forall` and `Type::Bounded` across `Stage::Pure`), `check/expr/inference.rs` (`check_qualified_forall`), `check/handlers.rs` (`await_`), `check/rows.rs` (`subsume_bare`, `subsume_forall`), and `check/unify.rs` (`unify_forall` checking `outlives` constraints). Tests: `a_called_rank2_parameter_keeps_its_qualifier_in_a_child` (`flick-model/tests/model/live_kept.rs`), `qualified_rank2_and_curried_keep_their_qualifier`, `bounded_curried_callable_keeps_its_region_bound`, `forall_subsumption_enforces_kinds_and_outlives`, `pure_arrow_with_type_variable_result_does_not_absorb_region`, and `dual_qualifier_curried_and_rank2_are_reflexive_and_checkable` (`flick-model/src/check/tests_forall.rs`).
- Lean: core rules (`formal/Formal/Live/Mode.lean` `Stage.use`/`LeLive.arrow`, `formal/Formal/Live/Elide.lean` `Ty.elide`) are unchanged and preserved. Placing surface qualifiers onto the inner runtime stage before elision aligns `List.fold_effect` and `Seq.fold_effect` in `formal/Formal/Live/ElideSchemes.lean` and `formal/Formal/Elab/PreludeSchemes.lean` (regenerated via `just formal-schemes` and verified with `just lean` / `lake build --wfail`).

### Rules of typing by use found while porting

- An instance method without a signature is checked against the class signature, elided before the instance arguments are substituted (`tests/check/pass/regions/instance_elided.fl`); a signature an instance method writes does not add the `Live` its own arrows would need (`tests/check/fail/regions/instance_signature.fl`).
- A function or task value is held against a written qualifier like any value, pure ones too: what it holds must be within the reach of the position, as `LeLive.arrow` requires, and a qualifier does not hide what its use needs live either (`tests/check/fail/regions/call_qualifier.fl`, `tests/check/fail/regions/callback_reach.fl`). The checker had compared only the use stage, so a pure `(Float -> Float)^{d}` passed for `(Float -> Float)^{c}`; the model and Lean rejected it.
- What a function position's type shows, and what a value bound's type shows (`a` of `List a`), counts as visible, not held.
- Applying a local `f : (∀v. τ^v → σ)^C` with `v ∉ σ` adds nothing of the argument to the result (`regions/applied.rs` `parametric_in_argument`), so `Option.map` returns `(Option b)^c`.

Outside regions, found along the way:

### flick build stops at an unknown module

- Symptom: `import App.Missing exposing (thing)` in a file passed to `flick build` panicked with `the checker resolved` in `ir/lower/expr.rs:208`. `flick check` on the same file reports `FLICK-R003`.
- Resolution, compiler: `build` does not proceed to lowering if loading produced errors (`project/load/checking.rs`). Test: `flick/tests/cli/build_command.rs` `build_stops_at_an_unknown_module`.
