# λFlick: elaboration

This document describes the elaboration of the surface language into the core of `core.md`. Classes and instances become dictionary passing. `do`, `for`, `?`, direct calls and `let mut` variables become core forms. Divergences from the spec, the checker and the code generator, and how each is resolved, are in §5.

References are as in `core.md`. Checker paths are relative to `flick-core/src/types/`, IR lowering paths to `flick-core/src/ir/`, code generator paths to `flick-core/src/codegen/`. The probes of §5 were built with `flick build` and run on Node (`--emit js`) and as a Go executable (`--emit go`).

## 0. Scope

Elaborated here:

- classes with one or several parameters, superclasses, functional dependencies, default methods and their default signatures;
- instances with a context, and `deriving`;
- constraints in signatures, in annotations and in higher-rank parameters;
- `do` with `<-`, `and`, an `if` guard and `let`;
- `for … yield`, `[for …]`, `for … do`;
- postfix `?`;
- direct calls;
- `let mut` variables and assignment;
- `CallSite`, the place of a use, which the checker gives.

| Not covered | Where | How the model accounts for it |
| - | - | - |
| Solving the `Record` row primitives (`HasField`, `Lacks`, `Remove`, `Project`, `UnionDisjoint`, `Map`, `Labels`) | `classes/prims.rs` | Rule (Prim) of §2.3 is a black box: it yields a `Dict::Primitive` with the labels the operation needs at run time |
| Captures and regions in dictionaries | `regions.md` | A dictionary is built from top-level values and has no captures of its own (`regions.md` §0) |
| Sets of services and errors: `...`, `-` and aliases of a row | `types/convert/sets.rs` | Expanded when a type is converted, before the stage described here: the model sees plain rows |
| Record sugar, operators, `\|>`, `sql` and `ui` blocks | `core.md` §1.4 | Elaborated before the stage described here, or as constants of Γ₀ |
| Optimizations: `opt/specialize.rs`, `opt/fold.rs`, `js/expr.rs` `direct_method`, `opt/fuse.rs` | hypothesis G2 of `core.md` | They rely on coherence (T6(b)): they replace a dictionary with a known instance or with a call of its method |
| Checking order, messages, the solver budget as a number | — | The budget enters the model only as a failure: exhausting it is the error `FLICK-C009`, not a solution |

### 0.1 Elaboration target: λFlick⁺

The core of `core.md` has `∀` at the head of a scheme, and a parameter of type `∀ᾱ. τ` (§2.3 there). A dictionary needs more:

- a method with its own variables (`map : ∀a b. (a → b) → f a → f b`) is a record field with a `∀`-type;
- a higher-rank parameter (`(∀a. a → a) → Int`, with a constraint `(∀a. Show a ⇒ a → String) → String`) is an arrow whose parameter has a `∀`-type.

The elaboration target is **λFlick⁺**: the core of `core.md` with the regions and captures of `regions.md`, in which `∀ᾱ. τ` is allowed as the type of a dictionary record field and as the type of an arrow parameter. Type abstraction and application are implicit, as in the core: `Λᾱ` and `[τ̄]` are not written, and rules (Var) and (Let-Sig) apply to fields and parameters. The quantifier still ranges over monotypes only (predicativity). [This is a requirement on the mechanization](#dictionaries-need-polymorphic-fields-and-higher-rank-parameters).

Elaboration preserves captures ([captures in the elaboration](#t6a-without-captures-does-not-carry-over-to-the-core-with-regions)): a source arrow carries a qualifier, schemes also bind capture variables, and the source (Lam) requires the arrow to show what the closure holds, as (Lam) of `regions.md` does. A dictionary `D_C ᾱ` is a nominal record type that reaches nothing: `⌈D_C τ̄⌉ = ∅`. This holds because a dictionary is built from top-level values and dictionary variables (§0), and the dictionary record rule requires the record to hold nothing.

## 1. Surface

### 1.1 Classes and instances

```text
cls  ::= class (Q̄ ⇒)? C ᾱ (| fd̄)? = (m : σ)* (let m = e)*
fd   ::= i₁ … iₖ → j₁ … jₗ                positions of class parameters
inst ::= instance (Q̄ ⇒)? C τ̄ = (let m = e)*
       | type T ᾱ = … deriving C̄
Q    ::= C τ̄                              constraint
σ    ::= ∀ᾱ. Q̄ ⇒ τ                         scheme
```

Take a class `C ᾱ` with superclasses `S₁ ῡ₁, …, Sₖ ῡₖ` and a method `m : ∀β̄. Q̄ ⇒ τ`. The method itself gets the scheme `∀ᾱ β̄. (C ᾱ, Q̄) ⇒ τ`:

- the class constraint comes first;
- the class parameters come before the method's own variables.

Checker:

- syntax: `syntax/parser/decl.rs` `class_decl`, `fundep`, `instance_decl`, `context`, `deriving`;
- class: `decls.rs` `members` (method scheme), `declare_methods`, `fundep` (names → positions, `env.rs` `Fundep`), `Form::Class`;
- superclass cycles: `classes/declare.rs` `declare_classes` (`FLICK-C008`);
- instance: `classes/declare.rs` `instance_head` (canonical head, aliases expanded), `admit`; methods: `instances.rs` `instance_methods`, `method_type`, `compare_method`;
- an extra constraint in a method signature is `FLICK-C010`, a missing method is `FLICK-C011`;
- `deriving`: `classes/derive.rs` `derive`. The head `C (T ᾱ)` is not canonicalized, the context is `C α` for every parameter of kind `Type`, the field dictionaries come from `derived_fields`.

### 1.2 Opt-in conversions before elaboration

An `@implicit` instance has no context or type variables, two different ground
head types, and a class with two parameters and exactly one pure method
`a -> b`. At a mismatch between ground nominal types, the checker inserts
that method applied to the value only when exactly one visible implicit
instance matches the ordered pair. It neither infers types nor chains
conversions; ambiguity remains a mismatch.

This is a deterministic rewrite before the elaboration modeled here: its
result is an ordinary explicit method application with a known instance
dictionary. T6, including coherence, therefore applies unchanged to that
result. The model and Lean contract are unchanged: no new entailment rule,
target term, evaluation rule or Prelude scheme is introduced. This does not
constitute a proof of the Rust checker's rewrite.

### 1.3 Sugar

```text
e ::= … | do { s̄ } | for { c̄ } yield e | [for { c̄ } yield e] | for { c̄ } do e | e?
s ::= p <- e | p₁ <- e₁ and … and pₙ <- eₙ | let p = e | if e | e
c ::= p <- e | if e | let p = e
```

The last statement of a `do` is an expression. Checker: `do_notation.rs`, `applicative_do.rs`, `comprehension.rs`, `question.rs`. Syntax: `reference/grammar.md` "`do`", "`for`", `?`.

## 2. Dictionaries

### 2.1 Dictionary types

Every class `C ᾱ` with superclasses `S₁ ῡ₁, …, Sₖ ῡₖ` and methods `mⱼ : ∀β̄ⱼ. Q̄ⱼ ⇒ τⱼ` (their own schemes, without `C ᾱ`) corresponds to a λFlick⁺ record type:

```text
D_C ᾱ = { $slot₁ : D_{S₁} ῡ₁, …, $slotₖ : D_{Sₖ} ῡₖ,  m₁ : ⟦∀β̄₁. Q̄₁ ⇒ τ₁⟧, …, mₙ : ⟦∀β̄ₙ. Q̄ₙ ⇒ τₙ⟧ }

⟦∀β̄. Q₁, …, Qₗ ⇒ τ⟧ = ∀β̄. D_{Q₁} →P … →P D_{Qₗ} →P ⟦τ⟧
⟦τ⟧                  — the same elaboration inside τ (higher-rank parameters)
D_{C τ̄}              = D_C ᾱ [ᾱ := τ̄]
```

For a constrained `let` scheme, the dictionary arrows carry the qualifier `q`, which is what the right-hand side holds: `⟦∀ᾱ. Q̄ ⇒ τ⟧_q = ∀ᾱ. D_{Q₁} →P^q … →P^q ⟦τ⟧` ([dictionary arrows carry the qualifier](#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds)). For methods, constants of Γ₀ and instances, `q = ∅`.

`$slotᵢ` is the name of the superclass `Sᵢ`. If a class has two superclasses with the same name, the position is appended to the name (`classes/mod.rs` `super_slot`, [slots distinguished by position](#superclass-slots-named-by-the-bare-class-name)). The constraint `Fiber.Outlives` passes no dictionary and vanishes under `⟦·⟧` (`typing.rs` `passes_dictionary`): the region checker solves it.

At run time a dictionary is an object:

- methods are its fields;
- superclasses and default methods are lazy fields (a getter `get $S()` in JS, `d.Lazy("$S", …)` in Go).

Laziness is invisible to the model: a field is a pure value, and the recursion of instance dictionaries through superclasses is finite because the superclass graph is acyclic. Code generator: `js/items.rs` `instance`, `go/items.rs` `instance`.

### 2.2 Judgments

```text
Γ; Δ ⊢ e : τ ! φ ⇝ e'      e has type τ and effect φ; e' is its elaboration into λFlick⁺
Δ ⊩ Q ⇝ d                    constraint Q follows from the givens Δ and the instances; d is the dictionary
Δ ::= · | Δ, δ : Q            givens with dictionary variables
```

`d` is a dictionary term:

```text
d ::= δ | d.$slot | I d̄ | struct_C^τ d̄ | prim_Q ℓ̄
```

- `I` is an instance dictionary (a top-level value of the instance's module, a function of the context dictionaries);
- `struct_C^τ` is the dictionary of a record or tuple `τ` built by the compiler (`$rt.structural`). The runtime erases the type and takes the labels from the value; the term needs it for elaboration into the typed core ([dictionary terms carry their type](#the-structural-dictionary-term-carries-its-type));
- `prim_Q ℓ̄` is a row primitive with labels, or `CallSite ℓ` at the place `ℓ` of a use, rule (Site) below.

These are exactly the cases of `Dict` (`classes/mod.rs`): `Given`, `Super`, `Instance`, `Structural`, `Primitive`, and `Site`, the `prim_{CallSite ℓ} ℓ̄` of (Site). `Dict::Error` stands in for a constraint that has already been reported and does not occur in the elaboration of an accepted program.

Elaboration `⟦Γ⟧`: `x : σ` becomes `x : ⟦σ⟧`, and every given `δ : Q` becomes a variable `δ : D_Q`.

### 2.3 Constraint entailment

Closure of the givens under superclasses (`classes/solve.rs` `closure`):

```text
δ : Q ∈ Δ                     δ : Q ∈ Δ*     Q = C τ̄    Sᵢ ῡᵢ is the i-th superclass of C ᾱ
──────────  (Δ-Given)        ─────────────────────────────────────────────────── (Δ-Super)
δ : Q ∈ Δ*                    δ.$slotᵢ : Sᵢ ῡᵢ[ᾱ := τ̄] ∈ Δ*
```

If one constraint is reachable by two paths, the checker keeps the first: inner givens before outer ones, and within a given its superclasses in order (`closure` skips a repeat by type). Which path is taken does not matter, by Lemma 6.1 (§4).

Entailment rules, in the order `entail_one` tries them:

```text
d : Q ∈ Δ*
────────── (Given)
Δ ⊩ Q ⇝ d

Q = C τ̄    C is a Record row primitive    prims(Q) = ℓ̄
─────────────────────────────────────────────────── (Prim)
Δ ⊩ Q ⇝ prim_Q ℓ̄

C ∈ {Eq, Ord, Hash, Show}    τ is a record or tuple with fields τ₁ … τₙ    Δ ⊩ C τᵢ ⇝ dᵢ
───────────────────────────────────────────────────────────────────────────────── (Struct)
Δ ⊩ C τ ⇝ struct_C^τ d̄

instance Q̄' ⇒ C τ̄' ∈ 𝓘    τ̄ = τ̄'[β̄ := ῡ]    Δ ⊩ Q'ᵢ[β̄ := ῡ] ⇝ dᵢ  for each i
──────────────────────────────────────────────────────────────────────────────── (Inst)
Δ ⊩ C τ̄ ⇝ I d̄
```

Checker:

- `entail`: a budget of steps and depth (`Limits`, by default 100 000 and 256; `FLICK-C009`);
- (Given): `entail_one` over `givens`. A cache comes first: a solved constraint without variables is taken from it. Only dictionaries built without givens enter the cache (`Dict::assumes`, [the cache excludes them](#the-solver-cache-stored-dictionaries-built-from-givens)). The cache therefore yields the same dictionary as (Inst);
- (Prim): `classes/prims.rs` `primitive`;
- (Struct): `structural_instance`;
- (Inst): `by_instance`, the first head for which `head_match` (`matching.rs` `match_type`) returns `Yes`. Matching binds only the instance's variables. On `Unknown` (only metavariables are in the way) the constraint is deferred. Then `by_context` solves the context;
- 𝓘 is the set of instances the module sees: the built-in ones, those of every module it depends on, and its own (`env.rs` `instances`).

`CallSite` reads as `CallSite ς`, with an unwritten variable ς for the place. A given of the caller fixes ς, by (Given); without one the checker defaults ς to the literal ℓ of the use, as it defaults a numeric literal, and solves `CallSite ℓ` by (Prim):

```text
no δ : CallSite ς ∈ Δ*    ℓ = (Module:line of the use, Module.name of its definition)
───────────────────────────────────────────────────────────────────── (Site)
Δ ⊩ CallSite ℓ ⇝ prim_{CallSite ℓ} ℓ̄
```

Checker: `classes/call_site.rs` `call_site`, from `solve_classes` before `entail`. Each use has its own ℓ, so the solution never enters the cache; inside the context of an instance or a default signature there is no use, and `CallSite` is only a given there (`FLICK-C001` otherwise).

Functional dependencies refine types, not dictionaries:

```text
fd = ī → j̄ of class C    (C σ̄ ∈ Δ*  or  an instance head C σ̄ matches on ī)    τ̄ ≡ σ̄ on ī
───────────────────────────────────────────────────────────────────────────── (Improve)
τⱼ := σⱼ  for j ∈ j̄, then entail C τ̄ again
```

Checker: `improve_by_givens` before (Prim)/(Struct)/(Inst), `improve_by_instances` after a failed (Inst). Every refinement spends budget.

Failures: `Missing` → `FLICK-C001`, `Stuck` at the end of a definition → `FLICK-C002`, `Budget` → `FLICK-C009` (`classes/report.rs` `report_classes`).

### 2.4 Expressions

The core rules (`core.md` §2.3) carry over with the same `φ` and a structural elaboration. Three places change: the use of a name, a signature, and checking against a `∀` with constraints.

```text
x : ∀ᾱ. Q₁ … Qₗ ⇒ τ ∈ Γ      τ̄ monotypes      Δ ⊩ Qᵢ[ᾱ := τ̄] ⇝ dᵢ
────────────────────────────────────────────────────────────── (Var-Dict)
Γ; Δ ⊢ x : τ[ᾱ := τ̄] ! P ⇝ x d₁ … dₗ

m is a method of class C with scheme ∀ᾱ β̄. (C ᾱ, Q̄) ⇒ τ      Δ ⊩ C τ̄ ⇝ d₀      Δ ⊩ Qᵢ[…] ⇝ dᵢ
──────────────────────────────────────────────────────────────────────────── (Method)
Γ; Δ ⊢ m : τ[ᾱ β̄ := τ̄ ῡ] ! P ⇝ d₀.m d₁ … dₗ
```

Checker:

- `unify.rs` `instantiate` wants (`want`) every constraint of the scheme at the use site;
- `solve_classes` solves them at the end of the definition group;
- `typing.rs` `record_dict` records the dictionaries by site and order.

IR: `ir/lower/expr.rs` `name`. For `Ref::Method` the first dictionary is `ExprKind::Method { dict, name }` (a projection), the rest are `Dicts(e, d̄)` (`with_dicts`).

```text
σ = ∀ᾱ. Q₁ … Qₗ ⇒ τ₁    ᾱ ∩ ftv(Γ) = ∅    δ̄ fresh
Γ, ᾱ; Δ, δ₁ : Q₁, …, δₗ : Qₗ ⊢ e₁ : τ₁ ! φ₁ ⇝ e₁'
ᾱ ≠ ∅  or  l > 0   ⇒   φ₁ = P
Γ, x : σ; Δ ⊢ e₂ : τ ! φ₂ ⇝ e₂'
─────────────────────────────────────────────────────────────── (Let-Sig-Dict)
Γ; Δ ⊢ let x : σ = e₁ in e₂ : τ ! φ₁ ⊔ φ₂ ⇝ let x : ⟦σ⟧ = λδ₁ … δₗ. e₁' in e₂'
```

The premise `l > 0 ⇒ φ₁ = P` is needed because elaboration turns the right-hand side into a function of dictionaries. The runtime steps of the right-hand side would then run at every use of `x` rather than once. The checker enforces it for every constraint that passes a dictionary, not only when `ᾱ ≠ ∅` ([constrained signature over a runtime right-hand side](#a-constrained-signature-over-a-runtime-right-hand-side)).

Checker:

- (Let-Sig-Dict): `bindings.rs` `check_signed` → `skolemize`, `record_givens`; `generalize.rs` `polymorphic_runtime` (`FLICK-T013`);
- a higher-rank parameter and an annotation with `∀` take the same branch, `infer.rs` `check_value`.

IR:

- top level: `ItemKind::Value { dicts }` (`ir/lower/mod.rs` `givens`);
- a local signature: `Stmt::Fun { dicts }`;
- a higher-rank argument: `ExprKind::Abstract(dicts, e)`.

A constructor whose declaration has constraints carries their dictionaries, first, and a branch on it binds them as givens. The branch on a constructor of a GADT is elaborated under the most general unifier `θ` of what it builds with the matched type ([`core.md` (Branch-GADT)](core.md#24-patterns-and-exhaustiveness)):

```text
K : ∀ᾱ. Q₁ … Qₘ ⇒ τ₁ →P … →P τₙ →P T ρ̄      τ̄ monotypes      Δ ⊩ Qᵢ[ᾱ := τ̄] ⇝ dᵢ
──────────────────────────────────────────────────────────────── (Con-Dict)
Γ; Δ ⊢ K : (τ₁ →P … →P τₙ →P T ρ̄)[ᾱ := τ̄] ! P ⇝ K d₁ … dₘ

Γ; Δ ⊢ e : T ῡ ! φ ⇝ e'      θ the most general unifier of T ρ̄ and T ῡ
Γθ, y₁ : τ₁θ, …, yₙ : τₙθ; δ₁ : Q₁θ, …, δₘ : Qₘθ, Δθ ⊢ b : τθ ! φθ ⇝ b'
Γ; Δ ⊢ r : τ ! φ ⇝ r'
──────────────────────────────────────────────────────────────── (Case-Dict)
Γ; Δ ⊢ match e with K y₁ … yₙ → b | _ → r : τ ! φ ⇝ match e' with K δ₁ … δₘ y₁ … yₙ → b' | _ → r'
```

A match with more branches nests (Case-Dict) on the same value. With no unifier the branch is dropped. Under an arrow the unifier compares the rows and the captures as they are, as the checker does.

Checker: `types/gadt/refine.rs`, `types/gadt/branch.rs`; the dictionaries of the value are `Dict::Field`. IR: `ir/lower/pattern.rs` binds them after the fields. Lean: `Elab.con`, `Elab.caseK`, `Elab.caseKDead` (`formal/Formal/Elab/Source.lean`), `Unify`, `Clash` (`Elab/Unify.lean`).

A binding without a signature gets no constraints: the variables of unsolved constraints stay monomorphic (`generalize.rs` `generalize`; [unsigned bindings get no class constraints](core.md#class-constraints-and-generalization)).

**Names of givens.** In the implementation `δ` is the constraint as `show` prints it:

- `record_givens` records strings;
- `Dict::Given` stores a string;
- the code generator looks a dictionary up by string, from the nearest given outwards (`js/mod.rs` `dict`, `go/items.rs` `dict`, `opt/specialize.rs` `resolve`).

The model requires `δ̄` to be fresh. For strings this means that the givens in scope print differently. `skolemize` numbers a rigid variable if its name is already taken by a given that passes a dictionary (`unify.rs` `unshadowed`, [distinct names for givens](#givens-looked-up-by-string-confused-same-named-variables)).

### 2.5 Instances and default methods

```text
instance Q̄ ⇒ C τ̄ = let m₁ = e₁ … let mₙ = eₙ,      β̄ = ftv(τ̄)
Γ, β̄; δ̄ : Q̄ ⊢ eⱼ : ⟦…⟧ ⇝ eⱼ'        against method_type: the scheme of mⱼ with ᾱ := τ̄, without C τ̄
δ̄ : Q̄ ⊩ Sᵢ ῡᵢ[ᾱ := τ̄] ⇝ sᵢ            for each superclass
δ̄ : Q̄ ⊩ Q̄ₖ[ᾱ := τ̄, γ̄ₖ := σ̄ₖ] ⇝ d̄ₖ    for each default taken and some σ̄ₖ, Q̄ₖ the constraints of its default signature
────────────────────────────────────────────────────────────────── (Instance)
I = λδ̄. { $slotᵢ = sᵢ, mⱼ = eⱼ', mₖ = default_C_mₖ (I δ̄) d̄ₖ … }   : ∀β̄. D_{Q̄} →P D_{C τ̄}
```

Checker:

- superclasses: `classes/declare.rs` `check_instances` (`FLICK-C001`);
- methods: `instances.rs` `instance_methods`; the context becomes givens, and the methods are checked against `method_type`;
- the entry in `InstanceInfo`: `record_instance`.

Defaults:

- `class_defaults` checks the body of a default as a function with the given `C ᾱ`, and with those of its default signature when the class body repeats the method's signature with constraints `Q̄ₖ` before it: `default_C_m : ∀ᾱ β̄ γ̄. (C ᾱ, Q̄ₖ) ⇒ τ`, where the method is `∀ᾱ β̄. C ᾱ ⇒ τ` (`class_members.rs`; `Q̄ₖ` mentions no variable of `β̄`, and a method with constraints of its own has no default signature, `FLICK-C010`);
- in the IR it is the value `$default_C_m` (`typing.rs` `default_method`, `ir/lower/mod.rs`), whose first parameter is the class dictionary and the next ones those of `Q̄ₖ`;
- an instance that does not define the method derives `d̄ₖ` under its context (`instances.rs` `default_dictionaries`, `FLICK-C001` on the instance when one does not hold) and refers to the default lazily (`get m(){ return $default_C_m(this, d̄ₖ, …) }`).

`formal/Formal/Elab/Defaults.lean` proves the field typed: `this_ent` derives `I δ̄` from the context, and `T6a_default_instance` gives `default_C_m (I δ̄) d̄ₖ` the method's type at the instance. Since `d̄ₖ` are derivations like any other, T6(b) covers them as it is.

A `deriving` instance is the same (Instance), with methods the runtime builds from the field dictionaries (`InstanceInfo.derived`).

The name `I` is the head as `show` prints it (`Instance.name`; `codegen/names.rs` `instance`). Two heads of one module that print the same would get one name; in that case the head is printed with its modules (`display.rs` `show_qualified`, [module-qualified instance names](#instance-names-were-heads-without-modules)).

## 3. Sugar → core

### 3.1 `do`

Let `⟦·⟧ₘ` be the elaboration of a block in the monad `m`. Dictionaries: `dᴹ : Monad m` under the key of `<-`, `dᴬ : Alternative m` under the key of `if`, `dᴬᵖ : Applicative m` under the key of `and`.

```text
⟦do { e }⟧ₘ                     = e
⟦do { p <- e; s̄ }⟧ₘ             = dᴹ.flat_map (λp. ⟦do { s̄ }⟧ₘ) e
⟦do { e; s̄ }⟧ₘ                  = dᴹ.flat_map (λ_. ⟦do { s̄ }⟧ₘ) e
⟦do { let p = e; s̄ }⟧ₘ          = let p = e in ⟦do { s̄ }⟧ₘ
⟦do { let x : σ = e; s̄ }⟧ₘ      = let x : σ = e in ⟦do { s̄ }⟧ₘ
⟦do { if c; s̄ }⟧ₘ               = if c then ⟦do { s̄ }⟧ₘ else dᴬ.nothing
⟦do { p₁ <- e₁ and … and pₙ <- eₙ; pure e }⟧ₘ
                                = dᴬᵖ.map2 (λ(p₁, …, pₙ₋₁) pₙ. e) (… dᴬᵖ.map2 (λp₁ p₂. (p₁, p₂)) e₁ e₂ …) eₙ
⟦do { p₁ <- e₁ and … and pₙ <- eₙ; s̄ }⟧ₘ
                                = dᴹ.flat_map (λ(p₁, …, pₙ). ⟦do { s̄ }⟧ₘ) (dᴬᵖ.map2 … e₁ … eₙ)
```

- `λp. e` with a pattern is `λx. match x with p → e`.
- A signature before a `let` inside a block is (Let-Sig-Dict) of §2.4 (`do_rest`, branch `Signature` → `local_let`).
- The methods are taken from `Prelude`, whatever the names in scope are bound to (`do_notation.rs` `prelude_method`).
- The `eᵢ` of an `and` group do not see the `pⱼ` of their own group (`applicative_do.rs` `unbound_in_group`).

Checker: `do_notation.rs` `do_expr`, `do_rest`, `do_bind`, keys `bind_key`, `guard_key`; `applicative_do.rs` `do_group`, `and_keys`, `group_key`. IR: `ir/lower/do_notation.rs`, `ir/lower/applicative_do.rs`.

The continuation is an ordinary λ. Hence `?` inside `do` returns from the continuation, not from the function around the block (§3.3). If the continuation returns a type that `?` cannot return, that is `FLICK-T003`.

### 3.2 `for`

```text
⟦[for c̄ yield e]⟧ = toList (G⟦c̄ ; e⟧)          strict: List, computed at once
⟦for c̄ yield e⟧   = G⟦c̄ ; e⟧                    lazy: Seq, computed on traversal
⟦for c̄ do e⟧      = L⟦c̄ ; e⟧                    loop: Unit, run at once

G⟦· ; e⟧              = Seq.singleton e
G⟦p <- s, c̄ ; e⟧      = Seq.flat_map (λx. match x with p → G⟦c̄ ; e⟧ | _ → Seq.empty) (dᴵ.iter s)
G⟦if c, c̄ ; e⟧        = if c then G⟦c̄ ; e⟧ else Seq.empty
G⟦let p = e', c̄ ; e⟧  = let p = e' in G⟦c̄ ; e⟧

L_φ⟦· ; e⟧             = e
L_φ⟦p <- s, c̄ ; e⟧     = Seq.each_φ (λx. match x with p → L_φ⟦c̄ ; e⟧ | _ → ()) (dᴵ.iter s)
L_φ⟦if c, c̄ ; e⟧       = if c then L_φ⟦c̄ ; e⟧ else ()
L_φ⟦let p = e', c̄ ; e⟧ = let p = e' in L_φ⟦c̄ ; e⟧
```

`dᴵ : Iterable f` is the dictionary under the key of `<-`. `L_φ` is the loop at the stage `φ` of the `for … do` expression: `Seq.each_φ : ∀a. (a →φ Unit) →P Seq a →φ Unit` traverses the sequence and runs the body for each element ([the loop rule](#the-loop-rule-l-was-missing)).

The source `s` has type `f a` with `Iterable f`, and the checker rule types it through `iter : f a → Seq a` (`for_clause`). The IR does not call `iter`: the class is closed ([closed `Iterable`](#for-did-not-call-iter-while-iterable-was-open)), its instances are `List` and `Seq`, and for them `iter` is the identity at run time. This is a shortcut by a `≈`-equal dictionary, like the optimizations of §0 ([sources go through `iter`](#g-elaborated-the-for-source-without-iter)). `Seq.*` in the rules are constants of Γ₀ with lazy-sequence semantics. The body of a lazy `for` makes only Sync calls (`comprehension.rs`).

Checker: `comprehension.rs` `for_expr`, `for_clause`, `for_loop`. IR: `ir/lower/comprehension.rs`, `Generate { strict, sync }` with nested `Each`/`If`/`Block`/`Yield`; `for … do` has no `Generate`. Code generator: `js/generator.rs`. Fusion of `Seq`/`List` (`opt/fuse.rs`) is an optimization.

### 3.3 `?`

The checker picks the form from the operand type at the site of `?` (`question.rs` `question`, `typing.questions`):

```text
e : Result (Errors ε) τ,  runtime stage:    e?  ⇝  from_result e
e : Option τ:                               e?  ⇝  early return of None from the nearest function
```

- `from_result` is elaborated in `core.md` §1.4: `match r with Err v → raise v | Ok v → v`.
- Early return: the body of the nearest function (a λ, a `do` continuation, a top-level function) is put into A-normal form in evaluation order. Then `let y = e? in b` ⇝ `match e with None → None | Some y → b`. A-normal form preserves the order of the runtime steps to the left of `?`.
- The function's return type is `Option τ'` (`stage.returns`, `infer.rs`); otherwise `FLICK-T003`.

IR: `ir/lower/question.rs`, `App(Prelude.from_result, e, Runtime)` or `ExprKind::Unwrap`. Code generation of `Unwrap`: `if (v._tag === "None") return v` (`js/expr.rs`).

### 3.4 Direct calls

Calling an effectful function without `await` (spec/01) rewrites nothing in the model. The core already distinguishes the pure and the runtime arrow, and (App) gives `φ₁ ⊔ φ₂ ⊔ φ`. Elaboration only makes explicit:

- the stage of each application: `App { stage }` from `typing.runtime`, `typing.sync` (`ir/lower/expr.rs`);
- the `≤-Fun` lift: a `Lift` node from `typing.lifts`;
- `task e`, `await e`: `ExprKind::Task`, `Await`;
- a block statement: `let _ : Unit = s in rest` (`core.md` §1.4).

### 3.5 Local mutable variables

`let mut x = e` declares a variable of the function whose block holds it; `x = e` assigns it, a statement of type `Unit`. The checker (`mutation.rs`) gives the variable one monotype where it is declared, never generalized, and checks an assigned value against it, as the content of a `Ref` (`Ref.set`). An assignment is not a runtime step: a block of a pure function may assign. A function, `task`, lazy `for`, `do` continuation or arm of a handler of operations inside the function uses no variable of it (`FLICK-G005`), and the body of `try … with`, `for … do` and `[for …]` assign none declared outside them (`FLICK-T061`). Inside a `scope`, the variables from outside keep the types of the context outside, which do not name the scope's region; a value that names it is `FLICK-G002`, a closure that holds it `FLICK-G001`, as for a `Ref` created outside (`regions.md` §4).

The elaboration passes the values along. A variable is a name per assignment, the one in scope where it is read; a construct that ends in several places gives the state of the variables it assigns with its value, and the rest of the block takes it apart:

```text
⟦let mut x = e; s̄⟧             = let x₀ = e in ⟦s̄⟧[x ↦ x₀]
⟦x = e; s̄⟧                     = let xᵢ₊₁ = e in ⟦s̄⟧[x ↦ xᵢ₊₁]
⟦let y = C; s̄⟧                 = match C⁺ with (y, x̄') → ⟦s̄⟧[x̄ ↦ x̄']     x̄: the variables C assigns
(if c then a else b)⁺          = if c then a⁺ else b⁺
(match e with p̄ → ā)⁺          = match e with p̄ → ā⁺
(try e with p̄ → ā)⁺            = try (let v = e in (v, x̄)) with p̄ → ā⁺
(scope { a })⁺                 = scope { a⁺ }
(provide l { a })⁺             = provide l { a⁺ }
{ s̄; e }⁺                      = ⟦s̄; (e, x̄)⟧
```

Any other expression that assigns evaluates its parts in order, each bound by a `let` first. With the rules of the checker, this reading and the compiled code, which assigns in place, agree. Only the function that declares a variable reads it, once per read, in order. The bodies of `scope` and `provide` run once, in place. No catching construct inside the variable's scope sees a variable assigned in the code it abandons: that would take an assignment in the body of `try`, which `FLICK-T061` rejects, or in a handled body abandoned by `abort`, which is the body of a `try` too. `resume` runs the parked body at most once (§3.7 of `core.md`), so a variable that lives across an operation has one value at a time.

T7 (`formal/Formal/LetMut.lean`) is this elaboration over a fragment of blocks: `let`, `let mut`, an assignment, a `scope` and a two-armed `match` whose values the rest of the block takes, the arms giving a record `{a = v, b = {a = x₁, b = …}}` of their value and the variables. `MutOk` is the typing of the checker: a variable of a monotype, an assignment of exactly that type at any stage, the variables shifted past the region inside a `scope`. T7 states that the translation has the block's type at its stage. T1, T2a and T4 then apply to it (`T7_pure`, `T7_regions`): a block typed pure makes only pure steps, and no value assigned in a `scope` is reached after the scope ends. A lambda inside the block is a term of the core there. The rule `FLICK-G005` is not a premise of typing, since the reading types without it; it makes the reading agree with the compiled code.

IR: `Stmt::Var`, `Stmt::Assign` and `ExprKind::Var`, a read (`ir/lower/expr.rs`). Unlike a `Local`, two reads may differ, so no optimization copies or moves one (`opt/walk.rs` `droppable`, `opt/mod.rs` `atom`). JavaScript declares a variable with `let`. Go orders the calls of an expression but not the reads between them, so an operand that a later one may change or observe is bound first (`go/expr.rs` `ordered`). The model (`flick-model/src/translate/mutation.rs`) runs this elaboration, `tests/run/codegen/let_mut.fl` among its cases.

Loops (`loop`, `while`, `break`, an assignment in `for … do`) and an assignment in the body of `try` need the state of a variable where control leaves a body. They are not covered here.

## 4. Coherence and T6

### 4.1 Premises

A program is the entry module and every module it depends on. 𝓘ₚ is the set of all its instances.

| | Premise | Checker |
| - | - | - |
| K1 | **No overlap.** The heads of two instances of one class in 𝓘ₚ do not unify after alias expansion. The context does not select | `coherence.rs` `conflicts` (own against visible), `imported_conflicts` (two visible ones from modules that do not see each other, [overlap across modules](#overlapping-instances-in-modules-that-do-not-see-each-other)); `FLICK-C003` |
| K2 | **Consistent FDs.** If two heads unify on the determining positions of a dependency, the determined positions are already equal under that substitution | same place, `FLICK-C004` |
| K3 | **Covered FDs.** The variables of the determined positions lie in the least fixed point reached from the determining positions of the head by the dependencies of the context and its superclasses | `declare.rs` `admit`, `coverage_rules`; `FLICK-C005` |
| K4 | **No orphans.** An instance is declared by the package of the class or the package of the nominal constructor of the head's first parameter | `declare.rs` `ownership`; `FLICK-C006` |
| K5 | **Termination.** The context's variables occur in the head; every context constraint is structurally smaller than the head and repeats no variable more often. Superclasses are acyclic. Exhausting the budget is a failure | `declare.rs` `terminates`, `declare_classes`; `FLICK-C007`, `FLICK-C008`, `FLICK-C009` |
| K6 | **Closed classes.** Instances of the row primitives, `CallSite`, `Numeric`, `Iterable` and the structural `Eq`/`Ord`/`Hash`/`Show` of records belong to the compiler | `declare.rs` `admit`, `call_site.rs` `written_call_site`; `FLICK-C012` ([closed `Iterable`](#for-did-not-call-iter-while-iterable-was-open)) |
| K7 | **Instance slots.** The slot `$slotᵢ` of an instance `I` holds the dictionary entailed for `Sᵢ ῡᵢ[ᾱ := τ̄]` from the context of `I` | `check_instances`; slots are distinct ([slots by position](#superclass-slots-named-by-the-bare-class-name)) |
| K8 | **Names.** Givens in scope print differently ([given names](#givens-looked-up-by-string-confused-same-named-variables)); the instances of a module are named differently ([instance names](#instance-names-were-heads-without-modules)) | `unify.rs` `unshadowed`, `declare.rs` `instance_head` |

K4 and K1 together give modularity: an instance that would overlap someone else's cannot be declared in a third package. A package that owns both the class and the type checks the overlap itself. K1 for modules of one package that do not see each other is checked by a module that sees both (`imported_conflicts`). The entry module sees every instance of the program: instances of dependencies are visible transitively, however they are imported (`env.rs` `learn`). Therefore K1 is checked for the whole of 𝓘ₚ.

`FLICK-C006` does not prevent the instance from being registered (`admit` continues after `ownership`). This is harmless: the error stops the build before IR.

### 4.2 Statements

Dictionary equivalence `≈` is the least congruence on closed dictionary terms such that

```text
(I d̄).$slotᵢ ≈ sᵢ[δ̄ := d̄]                      sᵢ is slot i of instance I by (Instance)
(struct_C^τ d̄).$slotᵢ ≈ struct_S^τ (d̄.$slotᵢ)   S is the i-th superclass of the structural C
                     in both rules d̄ are closed derivations: · ⊩ Q̄'[β̄ := ῡ] ⇝ d̄, · ⊩ C τᵢ ⇝ dᵢ
struct_C^τ ≈ struct_C^{τθ}    prim_Q ℓ̄ ≈ prim_{Qθ} ℓ̄    θ a type substitution
```

The structural slot rule is explained in [structural slots](#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries), the restriction to closed derivations in [closed slot arguments](#slot-rules-of-equivalence-take-closed-derivations), and the type-substitution rules in [typed dictionary terms](#the-structural-dictionary-term-carries-its-type).

**Lemma 6.1 (superclass paths).** Let the program satisfy K1–K8, `Δ ⊩ Q ⇝ d₁` and `Δ ⊩ Q ⇝ d₂`, and let `ρ` substitute for every `δ : Q' ∈ Δ` a closed dictionary `ρ(δ)` with `· ⊩ Q'ρ ⇝ ρ(δ)`. Then `ρ(d₁) ≈ ρ(d₂)`.

Sketch. Induction on the sum of the sizes of the derivations:

- in the closed case `Q` has no variables, and by K1 exactly one instance or rule (Struct)/(Prim) entails it. The arguments follow again by the induction hypothesis;
- a path through (Given) and (Δ-Super) after `ρ` is a `.$slot` projection from a dictionary that is itself entailed closed. The `≈` rule rewrites every projection from `I d̄` into the slot, and by K7 the slot is a derivation of the same constraint;
- (Improve) produces no dictionary: by K2 and K3 the refined types are the same in any order of refinement.

The diamond (`Ord a, Hash a`, with the common superclass `Eq a`) is a special case: after `ρ`, `δ_Ord.$Eq` and `δ_Hash.$Eq` rewrite to dictionaries `Eq τ` entailed closed, and by K1 they coincide.

**Lemma 6.2 (cache).** If a constraint without variables is solved by (Inst)/(Struct)/(Prim) without givens, then in any `Δ` its derivation, after any `ρ` as in Lemma 6.1, is `≈` to that solution ([why `ρ` is needed](#lemma-62-needs-the-substitution)). Hence a cache that holds only such solutions is sound.

**T6 (elaboration).** Let the program satisfy K1–K8.

- **(a) Type preservation.** If `Γ; Δ ⊢ e : τ ! φ ⇝ e'`, then `⟦Γ⟧, ⟦Δ⟧ ⊢⁺ e' : ⟦τ⟧ ! φ` in λFlick⁺. In particular, the elaboration of an accepted program exists. The code generator does not fail on a missing dictionary (cf. [the solver cache](#the-solver-cache-stored-dictionaries-built-from-givens)), and an accepted program after elaboration satisfies T1–T3 carried over to λFlick⁺.
- **(b) Coherence.** Let `Γ; Δ ⊢ e : τ ! φ ⇝ e₁` and `⇝ e₂` be two derivations that differ only in the choice of constraint derivations. Then for any `ρ` as in Lemma 6.1, `ρ(e₁)` and `ρ(e₂)` coincide up to `≈` in dictionary positions. Hence their evaluations give the same result.

Sketch of (a): induction on the derivation.

- (Var-Dict) and (Method): `D_{Q}` is a record type, the projection `.m` gives `⟦schema⟧`, and application to `d̄` strips `D_{Qᵢ} →P`. The effect `P` does not change: applying a dictionary is a pure arrow.
- (Let-Sig-Dict): `λδ̄. e₁'` has type `D_{Q̄} →P ⟦τ₁⟧`, and the effect of the right-hand side is `P` by the premise `l > 0 ⇒ φ₁ = P`. Without it a right-hand side with a runtime step would land in a pure λ, and the core (Lam) would not be derivable ([constrained signature over a runtime right-hand side](#a-constrained-signature-over-a-runtime-right-hand-side)).
- Sugar (§3): every rule is a combination of constants of Γ₀ and core forms whose type matches the checker rule. `flat_map` gets a continuation of type `a → m b`, and `?` with `Option` gets a function returning `Option`.

Sketch of (b): by Lemma 6.1 at every dictionary site.

Lean (`formal/Formal/Elab/`; this part does not import the Lean core in `formal/Formal/`): `≈` is `Eqv`, `Δ ⊩ Q̄ ⇝ d̄` is `Ent`, `Δ*` is `Clo` (`Syntax.lean`); K1–K8 are the fields of `Coherent`; the simulation "`≈` in dictionary positions ⇒ the same result" is the hypothesis `Sim` (`Hyp.lean`), derived from the target's steps (`Run.lean`, below); Lemma 6.1 is `lemma_6_1`, Lemma 6.2 is `lemma_6_2`, T6(b) is `T6b` (`Coherence.lean`). The sketch is carried out as follows: after `ρ`, every derivation is `≈` to the closed derivation of the substituted constraint (`closed_rep`); the slot of a closed dictionary is `≈` to the closed derivation of the superclass, by induction on the order of K5 (`slot_ok`); the closed derivation is unique by K1, K5 and K6 (`closed_unique`). Simplifications: `formal/README.md`.

T6(a) in Lean (`formal/Formal/Elab/`, `T6a.lean`) is stated relative to an abstract target: λFlick⁺ is the structure `Target` (`Target.lean`), whose typing `⊢⁺` and laws are fields; the core instance below constructs it. A method scheme is `Scheme`, `Prog.methods` (`Syntax.lean`); the judgment `Γ; Δ ⊢ e : τ ! φ ⇝ e'` is `Elab`, source and elaborated terms are `Exp` and `Tm` (`Source.lean`); `⟦σ⟧` is `Scheme.tr`, `D_Q` is `TTy.dict`. Lemmas (`T6a.lean`): `Δ ⊩ Q̄ ⇝ d̄ ⇒ ⟦Δ⟧ ⊢⁺ d̄ : D_Q̄` is `ent_typed`, T6(a) is `T6a`, by induction on `Elab`. The types of instances `I : ∀β̄. D_ctx →P D_head` (by (Instance) of §2.5 and K7) and of (Struct)/(Prim) dictionaries are laws of `Target`.

The sugar of §3 in Lean (`Sugar.lean`, `SugarBase.lean`, `DoNotation.lean`, `Comprehension.lean`) desugars into source terms `Exp` over the Prelude methods and constants of Γ₀; their schemes are the hypotheses `Sugar` about the program, and (Method) supplies the dictionaries during elaboration. The checker rules are the judgments `DoOk` (`do`), `GenOk` (`for … yield`, `[for …]`), `LoopOk` (`for … do`); for `?` and a block statement they are premises of the lemmas. The elaborated term is derived with the rule's type (`do_elab`, `forSeq_elab`, `forList_elab`, `loop_elab`, `tryResult_elab`, `tryOption_elab`, `seqUnit_elab`), and T6(a) covers it (`T6a_do`, `T6a_sugar`).

`Sim` is derived from the target's steps (`Run.lean`): `Target` has a step relation and a relation `Rel` on states that relates terms differing by `≈` in dictionary positions (`TmEqv`) and is a result-preserving weak simulation (`load_rel`, `step_rel`, `final_rel`). Only typed dictionaries are evaluated, so `load_rel` requires both terms to be closed and typed at one type, and `Sim` and `T6b` speak only of fillings `Exprs.Ok`: for the target, that is the typing of the filled skeleton. From this follow `Target.sim`, by induction on the run, and T6(b) without the hypothesis `Sim`, `T6b_target`, with the typing of both fillings as premises. The result is the set of observations of terminated runs; divergence is not compared. The fields of `Rel` are proved for the core by `runTarget` (`T6b_core`).

The instance of `Target` on the Lean core with regions (`CoreTy.lean`, `CoreTr.lean`, `CoreLaws.lean`, `CoreDict.lean`, `CoreT6a.lean`). The type translation `⟦·⟧` is `trT`, `trTT`, `trS`; it commutes with substitution (`trT_subst`) and with `⌈·⌉` (`trT_reach`). The elaborated term in the core is `Tr`; the global of a structural or primitive dictionary is chosen by the type held in the term ([type-indexed families](#structural-and-primitive-dictionaries-are-families-indexed-by-type), [typed dictionary terms](#the-structural-dictionary-term-carries-its-type)), and the correspondence of contexts is `Corr`. `Has` is the core typing `Typ` of the elaboration in any corresponding context. The hypothesis `Real` is the program's declarations in the core signature: the dictionary records with slots and methods, the schemes of instance globals, of constants of Γ₀, of structural and primitive dictionaries, and the constructor `Some`. Every law of `Target` is one core rule (`has_var` … `has_prim`), and `typingTarget` collects them. T6(a) for the core is `T6a_core`, for a closed program `T6a_closed`, for the sugar `T6a_do_core` and `T6a_sugar_core`. Running dictionaries in the core: the hypothesis `RealRun` gives the bodies of dictionary globals; a closed derivation evaluates by pure steps (`ent_eval`), and `≈` preserves the values of a dictionary (`eqv_sem`, given [closed slot arguments](#slot-rules-of-equivalence-take-closed-derivations)). The target with the core's run (`CoreRel.lean` … `CoreRun.lean`) is `runTarget`: a state is a configuration, a step is `CStep`, the result is a literal of the main thread. `Rel` is the congruence `Cg` on terms generated by the base pairs `DP`: on the left a closed dictionary computation, typed in the core at stage `P` and not yet a value; on the right a closed term with the same values; on the store, children and finalizers it is pointwise (`CfgR`). A step inside a base pair is pure (`dp_calm`); otherwise the other side takes the same step (`thread_sim`, `cstep_sim`); `≈`-dictionaries give base pairs (`eqv_dsim`, `dsim_cg`; see [open dictionaries](#equivalence-relates-open-dictionaries) and [dictionaries typed in the empty context](#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)). T6(b) for the core without the hypothesis `Sim` is `T6b_core`.

Call sites (`CallSite.lean`): (Site) is (Prim) at a literal place, so `Ent`, `Eqv`, `Coherent` and T6 are unchanged. A place that is still the variable ς is no primitive (`prims = none`), and `site_given` derives `CallSite ς` only from a given: (Prim) is excluded by `none`, (Struct) by `primStruct`, (Inst) by `k6prim`. `site_caller` closes a function under its given `CallSite ς` at its caller's place (`closed_rep`, `closed_unique`): what it passes on is `≈` the caller's dictionary.

What T6 gives the optimizations:

- `opt/specialize.rs` replaces a `Numeric` given with a known instance;
- `direct_method` calls the method of a known instance directly.

Both replacements are a step along `≈`, and (b) guarantees that the result does not change.

## 5. Known divergences

Each entry states a divergence between the model and the spec, the checker or the code generator, or a correction the model needed, and how it is resolved.

The first group concerns the checker and the code generator. Six of these entries (overlap across modules, given names, the solver cache, superclass slots, instance names, `Iterable`) and the constrained signature over a runtime right-hand side violated T6. Each was confirmed by a probe: the checker accepted the program, and its build behaved as stated. All of them are fixed in the checker, each with a fixture. The opaque alias, equality, polymorphic dictionary fields, `?` and numeric defaulting entries record checker incompleteness, a design decision or a requirement on the model.

### Overlapping instances in modules that do not see each other

```flick
// App/Ty.fl
module App.Ty exposing (..)
type T = T

// App/A.fl                              // App/B.fl — the same with "from B"
module App.A exposing (..)
import App.Ty exposing (T(..))
instance Show T =
  let show _ = "from A"
from_a : T -> String
let from_a t = show t

// App/Main.fl
module App.Main exposing (main)
import App.Ty exposing (T(..))
import App.A
import App.B
main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line (show T ++ ", " ++ App.A.from_a T ++ ", " ++ App.B.from_b T)
```

- Divergence: `flick check` of the project passed. Node and Go printed `from A, from A, from B`: one type had two instances, and `Main` took the first in import order. This violated K1 and T6(b).
- Cause: `coherence.rs` `conflicts` compared an instance only with `env.instances()` of the declaring module. A sibling module that does not import it is not in that set. An importer learns instances from interfaces and compared them only by `(module, name)` (`env.rs` `learn`). The prose of `reference/classes.md` and spec/03 "Local implicit instances" ("conflicts are diagnosed when the whole accessible graph is checked") promised the check.
- Resolution (checker): a module that sees both instances checks them against each other: `imported_conflicts` before `declare_instances`, `FLICK-C003`/`FLICK-C004` on the `module` line. Only non-built-in learned instances are compared: built-in ones were checked when their modules were compiled. Fixture: the project `tests/check/projects/sibling_instances`.

### Givens looked up by string confused same-named variables

```flick
type Pair b a = Pair b a
type Color = Red

instance Show Color =
  let show _ = "Color.Red"

class Container (f : Type -> Type) =
  render : forall a. Show a => f a -> String

instance Show a => Container (Pair a) =
  let render p =
    match p with
    | Pair x y -> show x ++ " / " ++ show y

twice : forall c. (forall a. Show a => a -> String)^c -> String
let twice f = f 42

inner : Show a => a -> String
let inner x = twice (fun v -> show v ++ " & " ++ show x)
```

- Divergence: `render (Pair Red 42)` printed `Red / 42` and `inner Red` printed `42 & Red`, on Node and in Go. Expected: `Color.Red / 42` and `42 & Color.Red`. Generated JS: `render: ($d1) => (p) => … $d1.show(x) …` and `twice(($d1) => (v) => $d1.show(v) + " & " + $d1.show(x))`. `show x` took the `Show Int` dictionary instead of the dictionary of the instance context or of the signature of `inner`. This violated T6(a): the elaboration does not type.
- Cause: the instance given `Show a` and the method's own given `Show a` print the same. So do a signature given and a higher-rank parameter given. `record_givens` and `closure` record both as the string `"Show a"`, and the code generator takes the nearest given with the same string (`js/mod.rs` `dict`, `go/items.rs` `dict`). The checker distinguishes them by the identity of the rigid variable; the code generator does not.
- Resolution (checker): `skolemize` numbers a rigid variable (`a1`) if its name is already taken by a given in scope that passes a dictionary, and the variable itself occurs in a constraint of the new `∀` (`unify.rs` `unshadowed`). Names in messages change only in this case. Fixture: the e2e test `tests/run/codegen/given_shadow.fl`, on JS and Go.

### The solver cache stored dictionaries built from givens

```flick
type Foo = Foo

f : Show Foo => Foo -> String
let f x = show [x]

g : Foo -> String
let g x = show [x]
```

- Divergence: the checker accepted the module although there is no `Show Foo` instance. `flick build` failed in the code generator: `dictionary \`Show Foo\` is in scope` (`js/mod.rs`, `go/items.rs`). This violated T6(a): the accepted program had no elaboration.
- Cause: `entail_one` put every solved constraint without variables into the module cache. `Show (List Foo)` in `f` was solved by (Inst) with the given `Show Foo` as argument, and `g` got that dictionary from the cache.
- Resolution (checker): only dictionaries built without givens enter the cache (`Dict::assumes`; Lemma 6.2). Fixture: `tests/check/fail/classes/assumption_cache.fl`.

### Superclass slots named by the bare class name

```flick
class Convert a b =
  convert : a -> b

class (Convert a b, Convert b a) => Iso a b =
  label : a -> b -> String

back : Iso a b => b -> a
let back y = convert y
```

- Divergence: with instances `Convert Celsius Fahrenheit`, `Convert Fahrenheit Celsius` and `Iso Celsius Fahrenheit`, the call `back (Fahrenheit 212)` got a `Fahrenheit` instead of a `Celsius`. Node: `TypeError: Cannot read properties of undefined (reading '_tag')`; Go: `no pattern matched the value`. Two same-named classes from different modules would collide the same way. This violated K7 and T6(a).
- Cause: `Dict::Super { class }`, `InstanceInfo.supers` and the dictionary field (`get $Convert()` in JS, `d.Lazy("$Convert")` in Go) were named by the class name without arguments. The second getter replaced the first.
- Resolution (checker): a slot is the class name, and when the name repeats among the superclasses the position is appended (`classes/mod.rs` `super_slot`; used in `closure` and `check_instances`). The slots of built-in classes (`$Eq` of `Ord` and `Hash`, which the runtime builds itself in `core/data.ts` and `dict.go`) do not change. Fixture: the e2e test `tests/run/codegen/repeated_superclass.fl`.

### Instance names were heads without modules

```flick
import App.A   // type Id = Id Float
import App.B   // type Id = Id String

class Pretty a =
  pretty : a -> String

instance Pretty App.A.Id =
  let pretty _ = "A.Id"

instance Pretty App.B.Id =
  let pretty _ = "B.Id"
```

- Divergence: Node failed at module load with `SyntaxError: Identifier '$i_Pretty_Id' has already been declared`. Go printed `B.Id, B.Id`. An importer that learns both instances, equal by `(module, name)`, would have kept one (`env.rs` `learn`). `FLICK-C003` does not fire: the heads do not unify. This violated K8 and T6(b).
- Cause: `instance_head` named the instance `show(head)`, and `show` prints constructors without their module.
- Resolution (checker): if an instance of the same module already has that name, the name is the head with modules (`display.rs` `show_qualified`): `Pretty App.B.Id`. Other names and dictionaries do not change. Fixture: the e2e test `tests/run/codegen/instance_names.fl` (`HttpApi.ServeError` and `Rpc.ServeError`).

### `for` did not call `iter` while `Iterable` was open

```flick
type Box a = Box a a

instance Iterable Box =
  iter : forall d a. (Box a)^d -> (Seq a)^d
  let iter b =
    match b with
    | Box x y -> my_seq [x, y]

main : Unit ->{; AppError} Unit
let main () =
  Platform.stdout_line (show [for x <- Box 1 2 yield x * 10])
```

- Divergence: the program was accepted (`my_seq` is a binding to `seqOfList` with `retention call`). Node: `TypeError: Box is not a function or its return value is not iterable`; Go: `not a sequence`. This violated T6(a): the checker rule requires `Iterable f`, while the elaboration passed the source as is.
- Cause: `comprehension.rs` records the `iter` dictionary, but `ir/lower/comprehension.rs` does not use it, and JS does `for…of`. The class doc comment in `Prelude.fl` says `iter` costs nothing, which holds only for `List` and `Seq`.
- Resolution (checker): `Iterable` is closed, like `Numeric`. An instance outside the module `Instances` is `FLICK-C012` (K6). Calling `iter` in every `for` would be a cost paid for instances the stdlib does not have. Fixture: `tests/check/fail/classes/iterable.fl`; `reference/classes.md` and the explanation of `C012` were extended.

### An opaque alias inside its module resolves through its representation

```flick
module App.Ids exposing (UserId, make, inside)

opaque type alias UserId = String
  deriving Show

inside : UserId -> String
let inside u = show u
```

- Divergence: `inside` compiles to `Instances.$i_Show_String.show(u)`, while `show u` in another module compiles to `App_Ids.$i_Show_UserId.show(u)`. Both print the same. A hand-written `instance Pretty UserId` in its own module is `FLICK-C003` "`Pretty String` overlaps `Pretty String`": `instance_head` canonicalizes the head, and inside its module the alias expands (`unify.rs` `whnf`). Outside, `UserId` is nominal (`exports.rs`).
- Resolution (recorded): coherence holds because the derived instance of an opaque alias delegates to the instance of the representation (`derive.rs`): the dictionaries differ, the computations coincide. An opaque alias cannot have its own instance. The `FLICK-C003` message for this case should be replaced by an explanation; this is not done yet.

### Equality operators do not use `Eq`

- Divergence: `==` checks only that the operand types are equal (`calls.rs`, `settle.rs` `settle_operators`) and compiles to the structural `$rt.equals`. A hand-written `Eq` instance does not affect `==`, and `==` on a type variable does not require `Eq a`. The same holds for `!=`.
- Resolution (recorded, by design): `reference/codegen.md` defines `==` as canonical structural equality. In the model `==` is a constant of Γ₀, `∀α. α →P α →P Bool`, not a method. T6 does not concern it. The checker does not prove the laws of `Eq` (`eq` consistent with `hash`, spec/07).

### Dictionaries need polymorphic fields and higher-rank parameters

- Divergence: `core.md` §0 defers nested `∀`, while `D_C` and `⟦σ⟧` of §2.1 use it.
- Resolution (model): T6 is stated for λFlick⁺ (§0.1). The Lean core (`formal/Formal/`) has to mechanize the core with polymorphic fields and parameters (predicatively) from the start; otherwise the elaboration development (`formal/Formal/Elab/`) has nothing to build on. The Lean core provides dictionary records with `∀`-fields and parameters of type `∀ᾱ. τ` (`Ty.all`), the latter without a constraint ([a polymorphic parameter has no constraint in the model](#a-polymorphic-parameter-has-no-constraint-in-the-model)).

### A constrained signature over a runtime right-hand side

```flick
main : Unit ->{; AppError} Unit
let main () =
  let cell = Ref.make 0
  let x : Show Int => Int = Ref.modify cell (fun n -> (n + 1, n + 1))
  Platform.stdout_line (show x)
```

- Divergence: the program was accepted. `flick build` failed in the code generator: JS with `a runtime call runs inside a runtime or Sync stage` (`js/expr.rs`), Go with `effects run where a fiber is in scope` (`go/call.rs`). Had elaboration gone through, `Ref.modify` would have run at every use of `x`. This violated T6(a).
- Cause: `generalize.rs` `polymorphic_runtime` checked the premise `φ₁ = P` only when `ᾱ ≠ ∅`. A constraint without variables (`Show Int`) also turns the right-hand side into a function of a dictionary (`Stmt::Fun { dicts }`).
- Resolution (checker): `polymorphic_runtime` requires `P` also for a constraint that passes a dictionary. The message is `FLICK-T013` "cannot have the constrained type". The explanation of `T013` was extended. Fixture: `counted` in `tests/check/fail/types/generalization.fl`.

### Postfix question mark picks its form at check time

- Divergence: if the operand type is not yet known at the site of `?`, it is an error (`question.rs`), even when later use would determine it. The checking order becomes visible.
- Resolution (recorded, incompleteness): the model (§3.3) requires a known type; fewer programs are accepted, with the same guarantees.

### Numeric literal defaulting picks a type by instance heads

- Divergence: `default_numeric` takes the first numeric type for which every open constraint has a matching head (`may_have_instance`). The instance context is not checked, and a class without instances counts as matching. If the context does not hold, `FLICK-C001` follows the choice, even though another type would have fit.
- Resolution (recorded, incompleteness): this does not affect T6: the chosen type is then checked by ordinary entailment.

The remaining entries were found by the Lean mechanization (`formal/Formal/Elab/`). They correct this document and the model; the checker is not affected.

### Equivalence did not relate structural slots to superclass dictionaries

- Divergence: the given `δ : Ord τ` and the path `δ.$Eq` give `Eq τ`. For a record `τ` with fields `τ₁ … τₙ`, `ρ` may substitute `δ := struct_Ord d̄`, and the path becomes `(struct_Ord d̄).$Eq`. The same `Eq τ` is also entailed directly by (Struct): `struct_Eq ē`. The `≈` rule of §4.2 rewrote only slots of instances `I d̄`, and these two dictionaries were not `≈`: Lemma 6.1 in that form was false.
- Cause: (Struct) is the fifth kind of dictionary, and `≈` was written for four. The runtime builds the `$Eq` of a structural `Ord` from the `Eq` dictionaries of the fields, that is from `dᵢ.$Eq`: `$Eq: structural("Eq", dicts.map((d) => d.$Eq))` in `runtime/src/core/data.ts` `structural`. Its behaviour matches the new rule.
- Resolution (model): `≈` has the rule `(struct_C d̄).$slotᵢ ≈ struct_S (d̄.$slotᵢ)`. The proof relies on the superclass of a structural class being a structural class of the same parameter (`Ord ⇒ Eq`), and on row primitives having no superclasses. Lean: `Eqv.strctSlot`, the fields `structSupers`, `primSupers` of `Coherent`.

### Lemma 6.2 needs the substitution

- Divergence: the statement read "in any `Δ` its derivation is `≈` to that solution". But in `Δ = δ : Eq Int` the derivation (Given) yields `δ`, which is not `≈` to the instance dictionary: `≈` is defined on closed terms, and `δ` is a variable.
- Resolution (prose): "after any `ρ` as in Lemma 6.1". The checker is not affected: the cache holds only solutions without givens ([the solver cache](#the-solver-cache-stored-dictionaries-built-from-givens)), and elaboration substitutes the given dictionaries at the call. Lean: `lemma_6_2`.

### (Let-Sig-Dict) lost the effect of the right-hand side

- Divergence: the conclusion of (Let-Sig-Dict) was `! φ₂`. With `ᾱ = ∅` and `l = 0`, the premise on [constrained signatures](#a-constrained-signature-over-a-runtime-right-hand-side) does not require `φ₁ = P`, and `let x : Int = Ref.modify … in 1` was derived with `! P` although the right-hand side takes a runtime step. The elaboration `let x : Int = e₁' in e₂'` then does not type in λFlick⁺ at `P`, and T6(a) was false.
- Cause: the rule was copied from (Let-Sig) of `core.md` §2.3 without its conclusion `φ₁ ⊔ φ₂`.
- Resolution (prose): the conclusion is `! φ₁ ⊔ φ₂`, as in the core's (Let-Sig). The checker is not affected: a right-hand side without parameters is checked at the stage of the enclosing function (`bindings.rs` `check_signed` → `check`), and its runtime call marks that stage (`effects.rs` `perform`). Lean: the premises `φ₁ = P ∨ φ₁ = φ` of `Elab.letSig` and `Target.let_` (the stage part of (Sub)).

### G elaborated the `for` source without `iter`

- Divergence: §3.2 gave `Seq.flat_map (λx. …) s`, while `s : f a` with `Iterable f`. For `f = List` the term `Seq.flat_map k s` does not type in λFlick⁺: `Seq.flat_map` expects a `Seq a`. T6(a) for that elaboration was false.
- Cause: the rule was written after the IR, which does not call `iter` ([closed `Iterable`](#for-did-not-call-iter-while-iterable-was-open)). The checker types the source through `iter : f a → Seq a` and records its dictionary under the key of `<-` (`comprehension.rs` `for_clause`).
- Resolution (prose): `G` and `L` apply `dᴵ.iter` to the source. The IR without `iter` is a shortcut: both instances of the closed `Iterable` give the identity at run time. Checker and code generator are not affected. Lean: `Sugar.gen`, `Sugar.loop`.

### The loop rule L was missing

- Divergence: §3.2 defined `L` only in words, "the same with `Unit` instead of a sequence". It did not say which constant traverses the sequence and at which stage the body runs. Without that, T6(a) for `for … do` cannot be stated.
- Cause: the IR lowers the loop into an `Each` node without `Generate` (`ir/lower/comprehension.rs`), and `L` had no core form.
- Resolution (prose): the rules `L_φ` of §3.2 with the Γ₀ constant `Seq.each_φ : ∀a. (a →φ Unit) →P Seq a →φ Unit`. The body and the clauses run at the loop's stage: the checker checks them at the current stage (`for_loop`). There are two constants: `Seq.each_P` at `P`, which has no Prelude name, and one `Seq.each_effect : ∀c d a r e. (a →R⟨r; e⟩^c Unit) →P (Seq d a →R⟨r; e⟩^c Unit)` at every `R r e`, its rows binders of the scheme, used at the rows of the loop. Lean: `Sugar.loop`, `Sugar.each`, `eachSch`, `eachSch_R`, `loop_elab`.

### T6(a) without captures does not carry over to the core with regions

- Divergence: the Lean model of the core is extended in place with `regions.md`, and its (Lam) requires the arrow type to show everything the closure holds: `held Ψ Γ (λb) ⊆ ⌈a →ψ^C t⌉`. The elaboration source (`Elab`) and target (`Target`) did not know captures: their (Lam) allowed any `Γ`. Example: `Γ = y : α`, a constant `c : ∀b. b → Bool` from Γ₀; `λx. c y : Bool →P Bool` is derivable, while in the core this closure holds `α`, and `α` must be in `C`. The laws `var`, `let_`, `meth` and `inst` require a type translation `⟦·⟧` that commutes with substitution. Then the closed `Bool →P Bool` translates to a qualifier that no substitution changes, and such a qualifier has no variable atoms. The qualifier "all atoms" breaks under the context shift `up k` in (Let-Gen), "all variables in scope" breaks as well, and narrowing the context does not help because the parameter is contravariant.
- Cause: T6(a) was stated for λFlick⁺ of §0.1, that is for the core of `core.md` without captures; `regions.md` was not applied to elaboration (§0: a dictionary has no captures of its own, but the rest of the term does).
- Resolution (model and document): T6 goes through the core with regions. Captures are part of the elaboration source and of `Target`: an arrow has a qualifier, schemes bind capture variables, the source (Lam) carries the premise of `regions.md`, and (Var) and (Inst) instantiate capture variables like the others. λFlick⁺ is the Lean core with regions and nominal dictionary records `D_C ᾱ` with `∀`-fields; `⌈D_C τ̄⌉ = ∅` (§0.1). The mechanization design is in `formal/README.md`. Lean: in the source and `Target`, `heldS`, the premises of `Elab.lam`, `Elab.letSig`, the laws `lam`, `lamD`; in the core, dictionary records `Ty.dict`, `Typ.drec`, `Typ.dproj` and globals `Tm.glob`; the instance of `Target` on the core, `typingTarget`, `T6a_core`.

### The source arrow was a reserved constructor

- Divergence: in `Elab` the arrow `a →φ b` was the term `app (app (con c_φ) a) b`. Substituting `f := con c_P` into `f a b` gives an arrow, while in the core `tapp (tapp f a) b` does not become an arrow after substitution. The type translation `⟦·⟧` did not commute with substitution, and the law `var` for the type `∀f. f Bool Bool` was false.
- Cause: a simplification of the source model; in the checker the arrow is a separate form `Fun`, not a `Con` (`types/ty.rs`), and the checker allows no such substitution.
- Resolution (model): a separate arrow `Ty.arr φ a b q` with a qualifier `q` in `Elab`, like the checker's `Fun`.

### The dictionary function of (Let-Sig-Dict) holds what the right-hand side holds

- Divergence: `let x : Show a => String = show y` with `y : a` from the environment elaborates to `λδ. δ.show y`. The closure holds `a`, while `⌈D_{Show a} →P String⌉ = ∅`, so the core (Lam) with regions is not derivable for it: the scheme translation of §2.1 gave dictionary arrows no qualifier.
- Cause: the region checker checks the source, where `x : String`; the dictionary function appears only during elaboration. It is second-class: every use of `x` applies it to dictionaries at once (Var-Dict), so it never escapes its scope and is harmless at run time.
- Resolution (document and model): `⟦·⟧` of a constrained `let` scheme carries the qualifier `q` of the dictionary arrows (§2.1), and elaboration takes `q` to be what the right-hand side holds. Lean: `Scheme.q`, `Scheme.tr`, the premise of `Elab.letSig` and the law `lamD`.

### Value qualifiers in Prelude schemes

- Divergence: in the checker a qualifier may stand on any type. `flat_map : ∀c d a b. (∀v. a^v → (m b)^(c + v))^c → ((m a)^d → (m b)^(c + d))^c` (`Prelude.fl`); likewise `map2`, `pure`, `iter`, `Seq.flat_map`, `Seq.each_effect`, `Seq.to_list` (`Seq.fl`). In `Elab` only the arrow had a qualifier ([separate source arrow](#the-source-arrow-was-a-reserved-constructor)), and the schemes of the hypothesis `Sugar` dropped the qualifiers of non-arrow types. `pure` had no scheme at all.
- Cause: in the Lean core only the arrow and `Eff` have a qualifier (`Ty.arrow`, `Ty.eff`), and `⌈m b⌉ = ⌈m⌉ ∪ ⌈b⌉`. A value of type `m b` cannot hold the continuation `c`, so a lazy instance whose result holds the continuation (`Monad (Eff r e)`, `Seq.flat_map`) does not type with a scheme without a qualifier.
- Resolution (model): the technique the core already uses for data with a hidden store (`formal/README.md`, simplification "Qualifiers are hidden only"): a parameter of kind `CRow` on the type. `τ^q` is not added to the core; the core and T1–T4 do not change.
  - **Translating a checker scheme into a model scheme.** The `Sugar` schemes are generated by this translation.
    1. `(T ā)^ρ`, where `T` is a type with a hidden store (`Eff`, `Seq`) or a class variable over containers (`m`, `f`), translates to `T ρ ā` (`appQ`): the value qualifier is the first argument, of kind `CRow`. Classes over containers range over `m : CRow → Type → Type`. For data without a store (`Option`, `List`, `Result`, `Errors`) the qualifier is dropped: it adds nothing to `⌈·⌉`. `a^ρ` on a type variable is `a`: the value bound is not modelled (simplification "Qualifiers are hidden only"). `(A → B)^ρ` is `A →^ρ B`, and without `ρ` it is `cnil`; `ρ₁ + ρ₂` is `cun ρ₁ ρ₂`. The service and error rows of an arrow become the model's stage (`Seq.each_φ`, [the loop rule](#the-loop-rule-l-was-missing)).
    2. `∀v. a^v → …` on a parameter is instantiated with `v := a`: a value of type `a` reaches exactly `⌈a⌉`. Any other variable of a `∀` on a parameter stays a binder, `.all k t`.
    3. The `CRow` argument is invariant in the core: (≤-Cap) exists only for the arrow and `Eff`, and the qualifiers of `Elab` match exactly. So wherever the checker relies on qualifier subtyping, the model scheme takes a variable. A qualifier in a negative position (a parameter type, the result of a function parameter; for data and arrows alike) is a variable: a single checker variable stays itself at its first such occurrence, the others are fresh. A missing data qualifier in a positive position is a fresh variable (`nothing`, `empty`, `singleton`).
    4. In a positive position, every variable `q` that qualified negative positions is replaced by what those arguments reach, left to right through `cun`: for data `(T ā)^q` it is `q ∪ ⌈ā⌉`, for a function parameter `(…)^q` it is `q` and the fresh variables inside its type. A value that holds a function also holds what it returns.
    5. Binders: the class parameters, then the checker's `∀` in its order (implicit ones by first occurrence) without the instantiated and dropped ones, then the fresh variables in order of occurrence.
  - **Why `a` stands in the result of `flat_map`.** By rule 4, the `d` of the result `(m b)^(c + d)` is `d ∪ a`: the argument `(m a)^d` also reaches `⌈a⌉`. A lazy result holds the whole argument, and without `a` its body does not type. A checker probe confirms this: lazy `flat_map` and `map2` over `Eff` with a resource from `scope` are rejected (`FLICK-G001`), even when the region is visible only in the type (`m : Eff none none (Res s)`); the generalized wrapper `let bind_gen m k = flat_map k m` with the scheme `∀a b. Eff none none a -> (a -> Eff none none b) -> Eff none none b` is rejected with "this value holds `a`". On instantiation the checker grows `d` to the full reach of the argument, including the atom `a`. There is no hole, and rule 4 states the same explicitly.
  - **Schemes** (`Sugar.lean`): `flat_map : ∀m c d a b e. Monad m ⇒ (a →^c m e b) →P (m d a →^(c ∪ e) m ((c ∪ e) ∪ (d ∪ a)) b)` (`bindTy`), `map2 : ∀f c d a b x g d'. (a →^c b →^g x) →P (f d a →^(c ∪ g) f d' b →^R f R x)` with `R = (c ∪ g) ∪ ((d ∪ a) ∪ (d' ∪ b))` (`map2Ty`), `pure : ∀f c a. a →P f c a`, `nothing : ∀f a r. f r a`, `iter : ∀f d a. f d a →P Seq (d ∪ a) a`, `Seq.flat_map` is `bindTy Seq`, `singleton : ∀a r. a →P Seq r a`, `empty : ∀a r. Seq r a`, `to_list : ∀d a. Seq d a →P List a`, `Seq.each_φ : ∀c d a. (a →φ^c Unit) →P (Seq d a →φ^c Unit)`, `from_result : ∀e a. Result (Errors e) a →R⟨none; e⟩ a` is the generated scheme (`Sugar.hFromResult`, [`from_result` in the core](#from_result-could-not-raise-in-the-core)). `pair`, `unc n` and `Seq.each_P` are model constants without a Prelude counterpart (§3, [the loop rule](#the-loop-rule-l-was-missing)).
  - **Sugar.** The row of a `do` block and of `for … yield` is the one its elaboration gives: `DoOk` is indexed by the container `m` and the element type, and `do_elab`, `gen_elab` and `T6a_do` give `∃q`. The premises take any rows.
  - **The model's `Eff`** is the nominal core type `Eff q b` with the constructor `MkEff : eff ∅ ∅ b {q} → Eff q b`, and `⌈Eff q b⌉ = ⌈q⌉ ∪ ⌈b⌉` through `tapp`; a lazy body is `MkEff (task …)` by the core rule (Task). An instance of a class over containers for data without a store is a type with a phantom `CRow` parameter that the fields do not mention; the source `Option` (`optTy`, `caseOpt`) stays without it.
  - **Generation.** The schemes of the exports of `Prelude`, `Seq`, `Option`, `Result` and `List` are produced by the translation above, and an inexpressible one goes to a skip list, empty today ([generator coverage](#the-scheme-generator-translated-only-the-sugar-names)): `flick-model/src/translate/lean.rs` takes the checker's scheme (`builtins()`) and prints `formal/Formal/Elab/PreludeSchemes.lean` (`just formal-schemes`; the golden test `cargo test -p flick-model` compares the file with the checker). The "first occurrence" of rules 3 and 5 is in text order: the arguments and body of an arrow come before its qualifier. `a^ρ` on a type variable is dropped by rule 1 and takes no part in rule 4 (`pure`). A `def` takes the numbers of the method's class and of type constructors in order of occurrence. The hand-written forms `bindTy`, `map2Ty`, `eachSch` coincide with the generated ones by `rfl` (the witness, `eachSch_R`); `pair`, `unc n` and `each_P` stay hand-written.
- The checker is not affected. Lean: the schemes `Sugar`, `appQ`, `bindTy`, `map2Ty`, `DoOk`, `GenOk`; the lazy `pure`, `map2` and `flat_map` of the `Monad Eff` instance are derived by the core rules under the `Sugar` schemes (`typ_effPure_tr`, `typ_effMap2_tr`, `typ_effBind_tr`, `LazyEff.lean`). The hypothesis `RootTy` for them is a consequence rather than an assumption about the program.

### Structural and primitive dictionaries are families indexed by type

- Divergence: (Struct) gives `struct_C d̄` for `C τ` with fields `τ₁ … τₙ`, and `struct_C : D_{C τ₁} →P … →P D_{C τ}`. The number of arrows and `τ` depend on the record, so in λFlick⁺ this is not one constant with a scheme but a family `struct_C^τ`: the body for `{a, b}` compares the fields `a` and `b`. Likewise `prim_p` is the dictionary of a constraint `Q` that (Prim) solved with the label `p`. The term `struct_C d̄` (as §2.2 then wrote it) and `Dict.strct C` in Lean did not contain `τ`, and a core global cannot be chosen from the term alone.
- Cause: the runtime builds a structural dictionary dynamically, `structural("Eq", dicts)` (`runtime/src/core/data.ts`), and takes the labels from the value; a typed core cannot do that.
- Resolution (model), superseded by the next entry: elaboration into the core was a relation `Tr` that chose the global `N.strct C τ` (`N.prim p q`) together with the type, with the law `strct` taking the global of the record `τ` from its premise. Choosing by type breaks the run, so the type is now part of the term.

### The structural dictionary term carries its type

- Divergence: with the global chosen by type as in the previous entry, the term `struct_C d̄` does not show the type. Typing determines the record type, but not always: `let y = (struct_Def (I_Int)).def in 0` types for any record `τ` with fields `[Int]`, for example `{a : Int}` and `{b : Int}`, and elaborations with different `τ` are different programs whose global bodies the model does not relate. Let an instance `J` have the superclass `Def {a : Int}` with slot `struct_Def I_Int`. Then `J.$slot` and the slot are `≈`, but the body of `J` takes the global for `{a : Int}`, while the elaboration of the slot may take the one for `{b : Int}`. Its `def` body may fail (`todo`), and two `≈` terms have different results: `load_rel` is false for any `load` that chooses the elaboration, and so is T6(b) for the core. The same holds for `prim_p`: the global `N.prim p q` is chosen by the constraint `q`.
- Cause: the dictionary term had no type, and "a typed position fixes `τ`" holds only for the type of the whole term, not for its dictionaries. In addition, an instance body names the global `N.strct C τ` with the instance's variables, while a closed derivation of the same constraint names it with `τθ`; these are different globals even when the choice is unique.
- Resolution (model and document): a structural dictionary carries the record type, `struct_C^τ`, and a primitive one carries its constraint, as `prim_Q ℓ̄` of §2.2. Elaboration is a function of the term. `≈` has the rules `struct_C^τ ≈ struct_C^{τθ}` and `prim_Q ℓ̄ ≈ prim_{Qθ} ℓ̄`: the runtime erases the type (`structural` takes the labels from the value), so the run requires the bodies of the globals `N.strct C (τθ)` and `N.strct C τ` to agree (a hypothesis about bodies, like `RootTy`). Lemma 6.1 and T6(b) are unchanged: `closed_rep` relates a dictionary before and after `θ` by the new rules. The checker is not affected: its `Dict::Structural` stores no type, which is correct because the runtime does not read it. Lean: `Dict.strct C τ`, `Dict.prim p q`, `Eqv.strctSub`, `Eqv.primSub`, `TrD.strct`, `TrD.prim`.

### Slot rules of equivalence take closed derivations

- Divergence: `(I d̄).$slotᵢ ≈ sᵢ[δ̄ := d̄]` held for any `d̄`. If `sᵢ` does not mention `δⱼ`, then `dⱼ` disappears on the right but is evaluated on the left: for a diverging or stuck `dⱼ` the left side has no value and the right side has one. So `≈` did not preserve the result, and through `trans` with such an intermediate term it also related typed ends: an induction on `≈` for "`≈` dictionaries give one value" breaks at `trans`. For typed ends this is not a counterexample, but the proof would then need normalization and confluence of slot rewriting, which the mechanization avoids.
- Cause: the rule was written by substitution, while its meaning comes from evaluation. Lemma 6.1 applies both slot rules only to closed derivations of the arguments (`slot_ok`).
- Resolution (model and document): in the slot rules `d̄` are closed derivations. Lemma 6.1, Lemma 6.2 and T6(b) are unchanged. For closed derivations the arguments evaluate, and then every `≈` rule preserves the set of values of a dictionary; the induction on `≈` goes through without typing intermediate terms. The checker is not affected: `≈` is needed only by the proof. Lean: the `Ent` premises of `Eqv.instSlot` and `Eqv.strctSlot`.

### Equivalence relates open dictionaries

- Divergence: §4.2 defines `≈` on closed dictionary terms, but T6(b) also compares dictionaries under the `λδ̄` of rule (Let-Sig-Dict), and these are open; the Lean `Eqv` is defined on all terms. An open pair may be related through `trans` with an intermediate term that does not evaluate. The run compares dictionaries by value, and only a closed dictionary has a value. Reducing `≈` to closed pairs works if the instance slot rule relates closed terms: `sᵢ[δ̄ := d̄]` is closed only if the slot `sᵢ` names only the givens `δ̄`. K7 says this only about superclass slots, while the slot list of `Inst` may be longer.
- Resolution (model): `≈` is the congruence closure of its closed pairs (`DSim`, `eqv_dsim`). Slots name only givens (`SlotsBelow`): this follows from the closedness of instance bodies (`RootTy`) and their shape (`RealRun`), with no separate hypothesis (`slots_below`). The checker is not affected.

### Typing the whole term does not type its dictionaries in the empty context

- Divergence: `load_rel` gets `Has [] [] e t φ` for both terms, that is the core typing of the whole elaboration. A base pair of the run needs the typing of every closed dictionary in the empty context at stage `P`: only then is a step inside the dictionary pure. A dictionary may stand under a binder, and its typing cannot be obtained from the typing of the whole term without strengthening the context and changing the stage in the core.
- Resolution (model): in the target with a run (`runTarget`), `Has` is the core `Has` together with the typing of a dictionary by the laws of `Target` (`DTyp`, `RHas`); the laws give both. `load_rel` takes the typings of the dictionaries in the holes of a skeleton ([the dictionaries of a closed dead arm](#the-dictionaries-of-a-closed-dead-arm-need-not-be-typed)). `T6b_core` takes `RHas` for both fillings; T6(a) for the core (`T6a_core`) is unchanged. The checker is not affected.
- `RHas` of closed fillings is derived rather than taken as a premise. `T6a_close` is T6(a) with closed givens: a derivation under `Δl, Δ`, where `Δ` is closed by the types `θ` and by closed dictionaries `ρ` down to constraints without variables, has type `τθ` under `Δl θ` in any target, with the givens `Δ` replaced by dictionaries. For a skeleton whose holes stand outside dictionary abstractions (`Tm.Skel`), closing a filling is the same as filling with closed dictionaries (`fill_dsubst`). From this follows `witness_T6b_open`: T6(b) on the witness under givens, without premises about typing. It relies on [structural dictionaries typed at any instance](#structural-dictionaries-were-typed-only-at-their-own-record-type). Lean: `Elab/Close.lean`, `Elab/CloseTy.lean`, `Witness/Consts.lean`.

### Record patterns read dictionary fields without evaluating them

- Divergence: in the core `Tm.field` also reads a field of a dictionary record `drec`, so the pattern `rfield` matches a dictionary too, and the fields of a dictionary are arbitrary pure terms, not values. Matching inspects their shape without evaluating them. A typed program does not do this: a dictionary type is not a record type.
- Resolution (model): the relation `Rel` (`Cg`) places no base pairs inside `drec` (index `true`), so matching related values gives related results even without typing. The core does not change.

### Structural dictionaries were typed only at their own record type

- Divergence: the laws `strct` and `prim` of `Target` and `DTyp` gave `struct_C^τ` a type only at `τ` itself, and `prim_q` only at `q`. Closing the givens ([typing dictionaries in the empty context](#typing-the-whole-term-does-not-type-its-dictionaries-in-the-empty-context)) substitutes types `θ`, while a dictionary term stores its type ([typed dictionary terms](#the-structural-dictionary-term-carries-its-type)) and is unchanged by type substitution. Let `Δ = [Eq a]`, `d = struct_Eq^{x:a} δ₀`, `θ: a := Int`, `ρ₀ = I_EqInt`. Then `d[ρ] = struct_Eq^{x:a} I_EqInt`: the function expects `D_{Eq a}` but gets `D_{Eq Int}`. There is no `DTyp` for this term, and the dictionaries of the closed filling had no typing: for a non-identity `θ` the claim about closed fillings was false in the model.
- Cause: the law fixed the type. In the core the global `N.strct C τ` has a closed scheme `∀k` (`Real.strct`), and the run erases the type (`Eqv.strctSub`, `primSub`): the term is usable at any `τθ`.
- Resolution (model): the laws `strct` and `prim` and `DTyp` take any instance `θ` of their type, `struct_C^τ : D_{C τ₁θ} →P … →P D_{C τθ}` and `prim_q : D_{qθ}`, as the law `inst` takes any `σ`. The core proves them through `has_glob` at `θ`. The run (`DSim`, `eqv_sem`, `DTyp.core`) does not depend on the type and does not change. The checker is not affected: `Dict::Structural` stores no type. Lean: `Target.strct`, `Target.prim`, `DTyp.strct`, `DTyp.prim`, `has_strct`, `has_prim`.

### Closing the premises of a refined arm needs its typing before the closing

- Divergence: T6(a) with closed premises closes the types by `θ` and the premises by closed dictionaries `ρ`. Under `θ` the problem of a GADT arm may have unifiers in the core and no derivation in the source, which compares rows and captures as written (`cun cnil cnil` against `cnil`). The closed dictionaries have the closed types, not those the arm was elaborated at.
- Resolution (model): the law `caseK` of `Target` takes the arm typed under the most general unifier of the open problem, before the closing, and gives the closed arm. In the core each unifier of the closed problem, after `θ`, is one of the open problem, so `⟦θ_arm⟧` followed by it; the body typed under `⟦θ_arm⟧` is typed under it (T1), and then its closed premises are substituted (L2). Lean: `Target.caseK`; `has_caseK` (`Elab/CoreCaseK.lean`), `Tr.dsubst` (`Elab/CoreDSubst.lean`), `Unify.most` (`Elab/CoreUnify.lean`); `T6a_close` (`Elab/Close.lean`).

### The dictionaries of a closed dead arm need not be typed

- Divergence: `RHas` asked every dictionary of a term to be typed (`DOk`). After the closing a dead arm may hold a dictionary built for the types of the open problem from closed dictionaries of other types. With `C a ⇒` around and `a := List Int` in the arm, the arm builds `I δ` for an instance `I : C (List x) ⇒ D (List x)`; under `a := Bool` it holds `I I_{C Bool}`, which has no typing. The arm is never run.
- Resolution (model): `load_rel` relates two fillings of one skeleton by `≈` dictionaries typed in the holes; the rest of the two terms is the same and needs no typing (`fill_cg`). `RHas` types a dictionary term only, and `Exprs.Ok` of a target carries the typings of the dictionaries in the holes, which `T6b_target` gets from the closed derivations (`closed_typed`). Lean: `Target.load_rel`, `Target.exprs` (`Elab/Run.lean`); `fill_cg`, `RHas` (`Elab/CoreRun.lean`); `T6b_target` (`Elab/Close.lean`).

### `from_result` could not raise in the core

- Divergence: the witness of the hypotheses (`formal/Formal/Elab/Witness/`, see `formal/README.md`) has to give `from_result : ∀e a. Result (Errors e) a →R⟨none; e⟩ a` the body `match r with Ok x → x | Err e → raise e` (§3.3, `core.md` §1.4). In the core this did not type. The source stage `R` carried no rows, and `⟦R⟧ = rt ∅ ∅` forbade any `raise`. In addition, the core (Raise) adds to the error row what the error reaches: for `Errors e` that is the atom `e`, which neither a closed row nor the row variable `e` covers.
- Cause: the model collapsed the service and error rows of stage `R` into one stage without rows, while the core types errors by rows of atoms.
- Resolution (model): the source stage is `R r e`, its rows of services and errors written as types (`Stage` and `Ty` are mutual, `Ty.ind` is their induction): a row variable, `cnil`, a named element `rnm c t` (the row `{t}` of one service or error type `t` with constructor `c`) or a union `run r r'` (`ρ₁ + ρ₂`). `⟦R r e⟧ = rt ⟦r⟧ (errRow ⟦e⟧ ∪ ε)`. `⟦r⟧` is what the type `⟦r⟧` stands for as a row (`Ty.asRow`): a row variable of the source is one of the core, `rnm c t` the name `c` with the atoms an error of `t` reaches (`Row.err`), `run r r'` the union of both sides. `errRow t = asRow t ∪ ⌈t⌉` adds the atoms that the errors of `e` reach, and `ε` is every name (`allNames`): a body at stage `R` may raise an error of any type that reaches only what `e` reaches. The atom `e` that (Raise) adds for `Errors e` is in `errRow ⟦e⟧`, so `from_result` is one constant at the generated scheme `∀e a. Result (Errors e) a →R⟨none; e⟩ a` ([generator coverage](#the-scheme-generator-translated-only-the-sugar-names), `Sugar.hFromResult`), its body `match r with Ok x → x | Err e → raise e` is typed at every `e` (`typ_frBody`), and `e?` takes it at the error row of the type of `e`, whatever that row is (`tryResult_elab`, at the stage `R⟨none; ε⟩`). The qualifier of a translated `R r e` arrow counts the variables of its rows, as `⌈·⌉` of the source does (`Stage.qual`); a `P` arrow translates as before (`trT_arr`). An earlier resolution, without `errRow`, made `from_result` a family `from_result_n` over single error types (`frSch`) and narrowed the error row of `?` to one error type; both are gone. T6(a) and T6(b) keep their statements over these stages. The checker is not affected. Lean: `allNames`, `errRow`, `trStage`, `Stage.qual`, `Sugar.hFromResult`, `frBody`, `typ_frBody`, `tryResult_elab`.

### Source `Int` translated to a nominal type without operations

- Divergence: `⟦Int⟧` was `adt "Int"`, a type without constructors, and the `Eq Int` instance had no real body: the core had no comparison of `Int`.
- Resolution (model): the core has a constant `intEq : Int →P Int →P Bool` with a pure step on two literals (the only change to the core; T1–T4 and T4.3 were re-proved). The source `Int` is the reserved constructor `con 4` (`intTy`), as `Bool` is `con 2`, and `⟦Int⟧ = base int`. The body of `eq` of the `Eq Int` instance is `intEq`. The checker is not affected. Lean: `Const.intEq`, `PHead.intEq`, `canonical_int`, `trT_int`.

### A polymorphic parameter has no constraint in the model

- Divergence: §0.1 allows `∀ᾱ. τ` as the type of a dictionary record field and of an arrow parameter, the latter also with a constraint (`(∀a. Show a ⇒ a → String) → String`). The Lean core has dictionary fields with schemes and constraints (`Ty.dict`, `Sig.dict`, `Typ.dcons`), and a parameter of type `∀ᾱ. τ` without one (`Ty.all`).
- Cause: `Ty.all` is a type, and a type holds neither the facts of a scheme nor a dictionary. The source (`Elab.Ty`) has no dictionary type to write such a parameter with.
- Resolution (model): `Elab.allLam`, `Elab.allVar` and `Elab.varAll` are (∀I) at a lambda, (∀I-Var) and (Var) at an instance of `core.md` §2.3, and T6(a) covers them through the laws `allLam`, `allVar`, `varAll` of `Target`. A pure argument that is not a lambda or a variable is written `let g = e in f g`. A constrained parameter stays out of the model and of the schemes: the generator skips it with `rank`.

### The scheme generator translated only the Sugar names

- Divergence: `flick-model/src/translate/lean.rs` translated the 11 names that `Sugar` uses and failed on a scheme with a row outside an arrow, a tuple, a record, or a context beyond the class. The other exports of `Prelude`, `Seq`, `Option`, `Result` and `List` were not checked against the model.
- Cause: the translation of [value qualifiers](#value-qualifiers-in-prelude-schemes) was written for the sugar schemes and did not say what to do with rows outside an arrow, tuples, records and contexts.
- Resolution (model): the generator translates every export of these modules that has a scheme into a `def` of `PreludeSchemes.lean`. Inexpressible schemes do not crash the generator; they become a line `-- skip M.x (category)` at the end of the same file. Additions to the translation of [value qualifiers](#value-qualifiers-in-prelude-schemes):
  1. An arrow with rows `{r; e}` is at the stage `R r e`, so `->{r; e}` is `.R (.var i) (.var j)` and its row variables are binders of the scheme (`traverse`, `Seq.each_effect`). A row is a variable, `.cnil`, a named element `.rnm c t` (the row `{t}` of one service or error type `t` with constructor `c`) or a union `.run` (`ρ₁ + ρ₂`); `{S₁, …, Sₙ | r}` is the union of its elements and its tail, so `timeout` is at `R r ({TimeoutError} + e)`. The rows of `Eff` are dropped: `(Eff r e a)^q` is `Eff q a`, a type with a hidden store, like `Seq`. The model's `Eff` runs with empty rows.
  2. A row may be an argument of another type (`Errors e`, `Exit e a`, `FieldCodec none a`, `FieldCodec (rd + r) a`): it is written as in rule 1 and is data like any argument. `from_result` is no exception: its `e` is the argument of `Errors e` and the error row of its stage, and the model takes the generated scheme as it is ([`from_result` in the core](#from_result-could-not-raise-in-the-core)).
  3. The classes of all constraints are `Scheme.ctx`, that is the dictionary arrows of §2.1; the method's class comes first, and its parameter is the first binder. A variable of kind `Type → Type` is a variable over containers in the sense of rule 1, also outside methods (`bind`).
  4. A tuple `(a, b, c)` is `tup (tup a b) c`, like `nest` in `Sugar`. A closed record `{ℓ̄ : τ̄}` is the nominal type `rec_ℓ̄ τ̄`, with fields in label order. Their qualifier is dropped: they are data without a store (rule 1). A capture row `{a, b | c}` is the qualifier `a ∪ b ∪ c`.
  5. In rule 4, a function parameter with a fresh qualifier variable reaches the fresh variables inside its type, without that variable itself (otherwise `bind` would get `'0 ∪ '0`).
  6. Skip categories: `rank`, a `∀` on a parameter with a context ([a polymorphic parameter has no constraint in the model](#a-polymorphic-parameter-has-no-constraint-in-the-model)); `region`, a region parameter; `other`, everything else (an open record, `Symbol`, a row element that is not a constructor).
- The golden test (`translate::lean::tests`) compares the file with the checker and checks that every export of the five modules is a `def` or a skip line: a new name without `just formal-schemes` breaks the test. The schemes of the 11 `Sugar` names did not change, and the witness's `rfl` still holds. No export is skipped: the eight `Prelude` exports first skipped as `row`, `parallel_results` and `parallel_exits` (`Errors e`, `Exit e a`), `timeout` (`->{r; TimeoutError | e}`) and the field codecs (`field`, `optional_field`, `field_effect`, `on_missing`, `on_missing_effect`), are `def`s since a row may have named elements and unions and be the argument of a type.
- In the core these rows meet rules that take rows with variables (`formal/README.md`, simplifications 1, 7, 8 and 21). An `x`/`_` arm of `try` catches all of the raised row `ε_b`, its row variables and atoms too, and (Handle) asks `ε₀ ⊆ names ∪ H ∪ ε'`. An obligation `Provided r ϱ` over a row variable `r` is the row element `obv r ϱ` (`Elem.obv`), which a substitution turns into the obligations of what `r` stands for; `provide` lets it through only when its outputs are named and give no service (`Stage.ObOut`). `Layer.merge` joins outputs that are row variables: the model drops the disjointness of outputs, which soundness does not need and which an instance of a row variable does not keep, so there it accepts more than the checker, which rejects outputs that share a name it sees (`FLICK-E004`).
