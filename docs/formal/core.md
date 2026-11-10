# λFlick: the core without regions

The model is the source of truth for the semantics of the core. Where the spec prose or the checker diverges from it, the divergence and its resolution are recorded in §5.

References: `spec/NN` is `docs/src/content/docs/spec/NN-*.md`, `reference/X` is `docs/src/content/docs/reference/X.md`. Checker paths are relative to `flick-core/src/types/`.

## 0. Scope

The core is what the surface language reduces to once classes, regions and captures are removed. It contains:

- λ and application;
- `let` with generalization;
- ADTs and `match`;
- exact records with a row tail;
- `Ref`;
- `task`/`await`, `try … with`, handlers of effects (§3.7);
- the pure and the effectful arrow, service rows `r` and error rows `e`.

The source of an expected error is `raise`; the source of a defect is `todo`.

| Not included | Where | How the core accounts for it |
| - | - | - |
| Regions, qualifiers `T^c` typed by use, `scope`, `Live` | `regions.md` | Core types are checker types with the `Captured`/`Bounded` qualifiers and the `Live` row elements erased. The region rules (`regions/live.rs`, `live_rows.rs`) take no part in the rules |
| Classes, dictionaries, `do`, `for`, `?` | `elaboration.md` | The core receives a program with dictionaries already inserted; schemes carry no constraints |
| `type module`, parameterized modules, `Module Sig`, `exists K : Sig. T` | source checker and lowering | Static applications become named modules; packed modules and existential witnesses become records. The core does not prove the source rules for generativity, nominal identity, or existential opening |
| Fibers, `parallel`, `race`, `spawn`, Cause, interruption, layer memoization | `runtime.md` (λRT) | The core has one thread of execution; the "interrupted" outcome appears only in λRT |
| The Sync stage | — | For typing this is the runtime arrow `R⟨ρ; ε⟩` with `ρ` a closed row of effects of `tail` and `abort` operations. Sync is a way of compiling such an arrow, not a separate kind of stage; what may suspend has the effect `Async` in `ρ` |
| `foreign` | hypothesis G1 (§4) | Bindings enter Γ₀ with their declared types |

## 1. Syntax

### 1.1 Kinds

```text
κ ::= Type | SRow | ERow | Row Type | κ → κ
```

`SRow` is `ServiceRow`, `ERow` is `ErrorRow`, `Row Type` is a row of record fields. The three row kinds never mix. Below, all types are assumed well-kinded.

Checker: `kinds.rs`, `convert/kinds.rs`, `convert/rows.rs` `Checker::row`, `row_kind`.

### 1.2 Types and rows

```text
τ ::= α                           variable
    | T τ₁ … τₙ                   nominal type: ADT, error, Unit, Bool, Never, Int,
                                  Float, String, List, Option, Result, tuple
    | τ →φ τ                      arrow of stage φ
    | Eff ρ ρ τ                   computation: services, errors, result
    | Ref τ
    | {ℓ₁ : τ₁, …, ℓₙ : τₙ | ρ}    exact record; closed without a tail
    | Errors ρ                    the errors of a row as a type
    | S.Impl                      implementation record of service S
    | ∀ᾱ. τ                       polymorphic value: a parameter of higher rank

φ ::= P                           pure stage:      a -> b
    | R⟨ρ; ρ⟩                     runtime stage:   a ->{r; e} b

ρ ::= α | none | {a₁, …, aₙ} | {a₁, …, aₙ | ρ} | ρ + ρ

a ::= S τ̄                         SRow element: a service
    | E τ̄                         ERow element: a type declared with `type error`
    | ℓ : τ                       Row Type element: a field

σ ::= τ | ∀α:κ. σ
```

How this is written in the language:

- `a -> b` is `a →P b`;
- `a ->{r; e} b` is `a →R⟨r;e⟩ b`;
- `->{}` is `→R⟨none;none⟩`.

The kind of a row in `φ` and `Eff` is fixed by its position. `none` is the empty row of any kind.

**Row equality ≡** (SRow, ERow):

- `+` is associative and commutative, with unit `none`;
- `{ā | ρ} ≡ {ā} + ρ` and `ρ + ρ ≡ ρ`;
- an element is identified by its name `Module.Name`, and a row has at most one element with a given name. `{N τ̄} + {N τ̄'}` is well-formed only when `τ̄ = τ̄'`, and then equals `{N τ̄}`.

The normal form of a row is a pair: a set of elements keyed by name and a set of variables.

**Inclusion:** `ρ ⊆ ρ'` ⇔ `ρ' ≡ ρ + ρ''` for some `ρ''`. On normal forms this is inclusion of both sets.

A field row works differently. Labels are unique, order does not matter, and a repeated label is an error (`FLICK-T014`), not a merge. There is no inclusion, only equality: there is no width subtyping.

Checker:

- `unify.rs` `Store::normalize`: the normal form;
- `rows.rs` `Checker::unify_row_types`, `merge_same_names`: one element per name;
- `convert/rows.rs` `element_label`.

### 1.3 Expressions, patterns, values

```text
e ::= x | c | K
    | λx. e | e e
    | let x = e in e
    | let x : σ = e in e
    | let rec f : σ = λx. e in e
    | match e with ā
    | {ℓ₁ = e₁, …, ℓₙ = eₙ} | e.ℓ | {e with ℓ = e}
    | task e | await e
    | try e with ā
    | todo

ā ::= a | a ā
a ::= p → e | p when e → e

p ::= _ | x | K p₁ … pₙ | {ℓ₁ = p₁, …, ℓₖ = pₖ, ..} | p | p | n
```

Notation:

- `c` ranges over the constants of Γ₀ (§1.5);
- `K` ranges over constructors of declared ADTs and errors;
- `n` is a literal;
- a tuple is an ADT with constructor `(,…,)`;
- a record pattern lists a subset of the fields.

Values:

```text
v ::= λx. e | K v₁ … vₖ | {ℓ₁ = v₁, …, ℓₙ = vₙ} | task e | n    k at most the arity of K
    | c v₁ … vₖ                   k less than the arity of c
```

`task e` is a value for any `e`. A computation in the core is itself a value: its body is not evaluated until `await`. Runtime-only forms (`fix`, locations, `env`, `with`, `handle`) are described in §3.1.

### 1.4 Reduction of the surface

The reduction is specified normatively in `elaboration.md`. This table gives the correspondence used to read the rules.

| Surface | Core |
| - | - |
| `f : σ` above `let f x y = b` | `let f : σ = λx. λy. b` (`let rec` with `rec`) |
| Fresh variables of a signature | `∀` over the whole signature |
| Expression `s` in a block before `rest` | `let _ : Unit = s in rest` |
| `let p = e` with a pattern | `match e with p → rest` |
| `if c then a else b` | `match c with True → a \| False → b` |
| Annotation `let x : σ = e`, expression `(e : σ)` | `let x : σ = e in …`, `let x : σ = e in x` |
| `from_result r` | `match r with Err v → raise v \| Ok v → v` |
| Operation `S.op` of a service | `perform_{S.op}`; a service is an effect whose operations are all `tail` (§3.7) |
| `provide S impl { e }` | a handler of `S` whose arm for each operation `o` is `x → impl.o x` (§3.7, `T6c_tail`) |
| `provide layer { e }` | the library's `Layer.run layer (fun () -> e)`: a known layer of `succeed` and `merge` means a chain of such handlers (`Layer` in [`runtime.md`](runtime.md)) |
| `x \|> f`, operators | `f x`, constants |

### 1.5 Constants Γ₀

```text
Ref.make        : ∀α. α →R⟨none;none⟩ Ref α
Ref.get         : ∀α. Ref α →R⟨none;none⟩ α
Ref.set         : ∀α. Ref α →P α →R⟨none;none⟩ Unit
raise           : ∀ε α. Errors ε →R⟨none;ε⟩ α
<, <=, >, >=    : ∀α. α →P α →P Bool                       α ∈ {Int, Float, BigInt, String}
```

Where the constants come from:

- `Ref.*`, `from_result`: the types from `builtins/Ref.fl` and `builtins/Prelude.fl` with captures erased.
- Order comparisons are checked by `settle.rs` `settle_operators` after inference: an unknown `α` becomes `Int`, a rigid one requires `Ord α`, anything else is `FLICK-T003` (see [order comparison on functions](#order-comparison-on-functions)).

Constructors:

- `K` from `type T ᾱ = … | K τ₁ … τₙ | …` has type `∀ᾱ. τ₁ →P … →P τₙ →P T ᾱ`.
- `E` from `type error E ᾱ = E τ̄` has type `∀ᾱ. τ̄ →P E ᾱ`. Here `E ᾱ` is at once a type of kind `Type` and an `ERow` element named `E`.
- Error fields may not hold `Ref`, `Eff`, runtime functions or `Unknown` (`services.rs` `check_error_payload`, `FLICK-E005`). The core does not need this restriction, regions do, but it always applies.

`Errors ρ` is the projection of a row to a type:

- `Errors none ≡ Never`;
- `Errors {E τ̄} ≡ E τ̄` (`unify.rs` `project`, `unify_errors`).

With two or more elements, or with a row variable, `Errors ρ` is opaque in the source language. Its values appear only in the `x`/`_` branch of a `try` and flow into `raise` or into generic code (see [`Errors` of a row with two or more elements](#errors-of-a-row-with-two-or-more-elements)).

### 1.6 Dictionaries (provisional)

This part is provisional; `elaboration.md` gives the normative account of classes. The reference checker `flick-model` checks programs into which flick-core has already inserted dictionaries. For that the core needs these forms:

```text
τ ::= …
    | C.Dict τ̄                    dictionary of class C at parameters τ̄
    | α τ                         application of a variable of kind κ → κ
    | Eff ρ ρ                     Eff without a result, of kind Type → Type

σ ::= … | ∀ᾱ. C̄ ⇒ τ              read as ∀ᾱ. C₁.Dict τ̄₁ →P … →P Cₙ.Dict τ̄ₙ →P τ
```

- `C.Dict τ̄` is a nominal type. Its fields are the class methods and `$S : S.Dict τ̄'` for every superclass `S`. A method `m : ∀ᾱ β̄. C ᾱ ⇒ υ` of class `C ᾱ` gives the field `m : ∀β̄. υ[ᾱ := τ̄]`: the class's own constraint is removed.
- `(Eff ρ ρ') τ ≡ Eff ρ ρ' τ`. Instances such as `Functor (Eff r e)`, with `f := Eff r e`, need this.
- `S.Impl ≡ {op₁ : σ₁, …, opₙ : σₙ}` is the closed record of the operations of `S`. This is how the checker defines the alias `S.Impl` (`services.rs` `impl_def`). The field of a polymorphic operation has type `σᵢ`.

```text
Γ ⊢ e : D ! φ      ℓ : ∀β̄. υ is a field of D      ῡ monotypes
──────────────────────────────────────────────────────── (Field-Impl)        D ::= S.Impl | C.Dict τ̄
Γ ⊢ e.ℓ : υ[β̄ := ῡ] ! φ
```

(Field-Impl) formalizes [calling an implementation field with a class constraint](#an-implementation-field-with-a-class-constraint-cannot-be-called-directly): both the reference checker and flick-core accept `render_impl.render 7`.

There is no instance selection: the IR has already inserted dictionaries (`Dicts`, `Abstract`, `Method`, `Instance`). An instance global has the scheme `∀ᾱ. C₁.Dict τ̄₁ →P … →P C.Dict τ̄`: its parameters are the context dictionaries. A default method `$default_C_m` has the scheme of method `m`. Coherence and instance selection are covered in `elaboration.md` §4.

Reference checker:

- `flick-model/src/check/expr.rs` `field`, `method`, `implementation`, `dictionary`;
- `check/unify.rs` `resolve`: `Eff ρ ρ'` as `EffOf`;
- `decls.rs` `finish`: schemes of default methods;
- `translate/types.rs` `scheme`, `constraint`.

## 2. Statics

### 2.1 Stage effect

The judgment `Γ ⊢ e : τ ! φ` means: `e` has type `τ`, and the runtime steps that evaluating `e` performs at the current stage fit within `φ`. The bodies of `λ` and `task` do not belong to the current stage: they are evaluated later.

```text
P ⊑ φ
R⟨r;e⟩ ⊑ R⟨r';e'⟩   ⇔   r ⊆ r'  and  e ⊆ e'

P ⊔ φ = φ ⊔ P = φ
R⟨r;e⟩ ⊔ R⟨r';e'⟩ = R⟨r + r'; e + e'⟩
```

`R⟨none;none⟩ ⋢ P`: empty rows do not make a stage pure (spec/02 "Purity is separate from error and service rows"). A runtime step where only `P` is allowed is not derivable. This is `FLICK-E001`.

Checker:

- `effects.rs` `Stage`: the stage; `pure` records why runtime is forbidden, `services`/`errors` are the upper bounds of the rows;
- `Checker::perform`: a call adds the constraint `actual ⊆ declared` or reports `FLICK-E001`;
- `sub_row`;
- the solver `effects/solve.rs` (`grow`, `close_rows`, `report_rows` → `FLICK-E002`) looks for the least solution: an unknown row that nothing required becomes `none`.

### 2.2 Subtyping

```text
                      τ₁' ≤ τ₁    φ ⊑ φ'    τ₂ ≤ τ₂'             r ⊆ r'    e ⊆ e'    τ ≤ τ'
───────── (≤-Refl)    ─────────────────────────────── (≤-Fun)    ──────────────────────────── (≤-Eff)
 τ ≤ τ                 τ₁ →φ τ₂  ≤  τ₁' →φ' τ₂'                   Eff r e τ  ≤  Eff r' e' τ'
```

There are no other rules. `Ref`, `List`, records and ADTs are invariant; inside them types are compared by `≡` (see [only arrows and `Eff` have subtyping](#only-arrows-and-eff-have-subtyping)).

`≤-Fun` with `φ = P` and `φ' = R⟨…⟩` is the lift of a pure function into a runtime position. There is no converse, because `R⟨…⟩ ⋢ P`. At run time (§3) the lift does nothing in the model. In the IR it is the `Lift` node, and its correctness is hypothesis G2.

Checker:

- `effects.rs` `Checker::subsume`, `subsume_at`: the parameter is contravariant, the result and the rows are covariant, the pair "runtime where pure is expected" is a mismatch, everything else goes to `unify`;
- `report.rs` `Checker::expect`;
- `typing/stages.rs` `lift`.

### 2.3 Rules

```text
x : ∀ᾱ.τ ∈ Γ      τ̄ monotypes                 Γ ⊢ e : τ ! φ    τ ≤ τ'    φ ⊑ φ'
────────────────────────── (Var)            ───────────────────────────────── (Sub)
Γ ⊢ x : τ[ᾱ := τ̄] ! P                        Γ ⊢ e : τ' ! φ'
```

(Const) and constructors are like (Var), using Γ₀ and the declarations. (Var) also instantiates a variable whose type is `∀ᾱ. τ`, once for each binder at its head.

```text
v a value    ᾱ ∩ ftv(Γ) = ∅    Γ, ᾱ ⊢ v : τ ! P              x : ∀ᾱ. τ ∈ Γ    ᾱ ≠ ∅
──────────────────────────────────────────── (∀I)            ───────────────────── (∀I-Var)
Γ ⊢ v : ∀ᾱ. τ ! φ                                            Γ ⊢ x : ∀ᾱ. τ ! φ
```

(∀I) is (Let-Gen) for an argument: `Γ` holds no `ᾱ`, and the premise `P` is its premise. The body of a value is typed at `P`. (∀I-Var) takes a variable of a scheme where a polymorphic value is expected. A pure term that is not a value takes the same route: `let g = e in f g`. An instance is taken only at a variable, not at the result of an arbitrary term. Subtyping (§2.2) does not look under `∀`: `∀ᾱ. τ` is below itself only, so a term that is not a variable keeps its binder until it is bound.

Checker:

- (Var): `calls.rs` `Checker::name`, `name_type`, `use_local`; `toplevel.rs` `top_value`; `qualified.rs` `qualified`, `record_entity`; `unify.rs` `Checker::instantiate`.
- (Sub): see §2.2. The checker applies subtyping where it checks an expression against an expected type (`expect`); elsewhere it uses equality.

```text
Γ, x : τ₁ ⊢ e : τ₂ ! φ                     Γ ⊢ e₁ : τ₁ →φ τ₂ ! φ₁    Γ ⊢ e₂ : τ₁ ! φ₂
────────────────────────── (Lam)           ─────────────────────────────────────────── (App)
Γ ⊢ λx. e : τ₁ →φ τ₂ ! P                    Γ ⊢ e₁ e₂ : τ₂ ! φ₁ ⊔ φ₂ ⊔ φ
```

Checker:

- (Lam): `infer.rs` `lambda`, `function`, `function_with`, `curry`; `effects.rs` `body_stage`, `open_stage`, `stage_effects`. Against an expected type, a λ takes its arrows from it. Without an expected type, the last arrow is a runtime arrow exactly when the body made a runtime call: this is the least `φ`. Every parameter is a stage.
- (App): `calls.rs` `apply`, `pipe`; `effects.rs` `call_effects`, `perform`. If the callee's type is still unknown, the checker takes `φ = P` (see [a callee of unknown type](#a-callee-of-unknown-type-is-a-pure-arrow)).

```text
Γ ⊢ e₁ : τ₁ ! P      Γ, x : Gen(Γ, τ₁) ⊢ e₂ : τ ! φ
──────────────────────────────────────────────── (Let-Gen)
Γ ⊢ let x = e₁ in e₂ : τ ! φ

Γ ⊢ e₁ : τ₁ ! φ₁      Γ, x : τ₁ ⊢ e₂ : τ ! φ₂
──────────────────────────────────────────── (Let-Mono)
Γ ⊢ let x = e₁ in e₂ : τ ! φ₁ ⊔ φ₂

σ = ∀ᾱ.τ₁    ᾱ ∩ ftv(Γ) = ∅    Γ, ᾱ ⊢ e₁ : τ₁ ! φ₁    ᾱ ≠ ∅ ⇒ φ₁ = P    Γ, x : σ ⊢ e₂ : τ ! φ₂
──────────────────────────────────────────────────────────────────────────────── (Let-Sig)
Γ ⊢ let x : σ = e₁ in e₂ : τ ! φ₁ ⊔ φ₂

σ = ∀ᾱ.τ_f    τ_f = τ₁ →φ τ₂    Γ, ᾱ, f : σ°, x : τ₁ ⊢ e₁ : τ₂ ! φ    Γ, f : σ ⊢ e₂ : τ ! φ'
──────────────────────────────────────────────────────────────────────────────── (Let-Rec)
Γ ⊢ let rec f : σ = λx. e₁ in e₂ : τ ! φ'
```

In (Let-Rec), `σ° = σ` if the signature starts with an explicit `∀` over all its variables. Otherwise `σ° = τ_f` with rigid `ᾱ`: recursive calls are monomorphic.

Checker:

- (Let-Gen), (Let-Mono): §2.5.
- (Let-Sig): `bindings.rs` `check_signed`, `polymorphic_runtime` (`FLICK-T013`); `infer.rs` `check_value`, the `Forall` branch; `unify.rs` `skolemize`. An escaping rigid variable is `FLICK-T007`, a demand for a concrete type is `FLICK-T008`. The premise `ᾱ ≠ ∅ ⇒ φ₁ = P` is checked by `polymorphic_runtime`: for a signature it is called from `polymorphic_signature`, for an annotation and a higher-rank argument from the `Forall` branch of `check_value` (see [an annotation with `forall`](#an-annotation-with-forall-bypasses-let-sig)).
- (Let-Rec): `bindings.rs` `local_let`; `recursion.rs` `new_rec`, `finish_recs` (`FLICK-T015`).
- The statement `let _ : Unit = s`: `bindings.rs` `block_then`, `statement` (`FLICK-T011`).

```text
ℓᵢ distinct    Γ ⊢ eᵢ : τᵢ ! φᵢ
──────────────────────────────────────────────────────────────────── (Rec)
Γ ⊢ {ℓ₁ = e₁, …, ℓₙ = eₙ} : {ℓ₁ : τ₁, …, ℓₙ : τₙ} ! φ₁ ⊔ … ⊔ φₙ

Γ ⊢ e : {ℓ : τ | ρ} ! φ             Γ ⊢ e : {ℓ : τ | ρ} ! φ    Γ ⊢ e' : τ ! φ'
──────────────────── (Field)        ──────────────────────────────────────────── (Update)
Γ ⊢ e.ℓ : τ ! φ                     Γ ⊢ {e with ℓ = e'} : {ℓ : τ | ρ} ! φ ⊔ φ'
```

`ρ` of kind `Row Type` is a variable or a concrete remainder: the closed `{ℓ : τ, m : υ}` is `{ℓ : τ | {m : υ}}`. `with` does not change the type of the record.

Checker: `records.rs` `record` (`FLICK-T014`), `field`, `known_field`, `record_update`; `rows.rs` `unify_row_types`.

```text
Γ ⊢ e : τ ! φ₀
for every branch i:   ⊢ pᵢ : τ ⇒ Δᵢ     Γ, Δᵢ ⊢ gᵢ : Bool ! ψᵢ     Γ, Δᵢ ⊢ eᵢ : τ' ! φᵢ
Exh(τ, {pᵢ | branch i has no when})
──────────────────────────────────────────────────────────────────── (Match)
Γ ⊢ match e with p₁ [when g₁] → e₁ | … : τ' ! φ₀ ⊔ ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

For a branch without `when`, `ψᵢ = P`.

Checker: `infer.rs` `match_expr`, `arm`; patterns and `Exh` are in §2.4.

```text
Γ ⊢ e : τ ! φ    φ ⊑ R⟨r; ε⟩               Γ ⊢ e : Eff r ε τ ! φ
──────────────────────────── (Task)        ──────────────────────────────── (Await)
Γ ⊢ task e : Eff r ε τ ! P                  Γ ⊢ await e : τ ! φ ⊔ R⟨r; ε⟩
```

`await` removes exactly one layer: `τ` may itself be an `Eff`.

Checker:

- (Task): `handlers.rs` `task`. The body is checked at its own stage, bounded by the expected `Eff` or with inferred rows.
- (Await): `calls.rs` `prefix`, the `AwaitKw` branch (`FLICK-T003` for a non-`Eff`).

```text
Γ ⊢ e : τ ! R⟨r; ε_b⟩
for every branch pᵢ [when gᵢ] → eᵢ:
    θᵢ = E ῡ         if pᵢ = E p̄ is a pattern of error constructor E
    θᵢ = Errors ε_b  if pᵢ is x or _
    ⊢ pᵢ : θᵢ ⇒ Δᵢ     Γ, Δᵢ ⊢ gᵢ : Bool ! ψᵢ     Γ, Δᵢ ⊢ eᵢ : τ ! φᵢ
ε_b ⊆ H + ε'
──────────────────────────────────────────────────────────────── (Try)
Γ ⊢ try e with ā : τ ! R⟨r; ε'⟩ ⊔ ⊔ᵢ (ψᵢ ⊔ φᵢ)

H = ε_b, if there is an x or _ branch without when;
otherwise H = { E ῡ | a branch E p̄ without when and Exh(E ῡ, {E p̄}) }
```

(Try-Pure): if `Γ ⊢ e : τ ! P`, the branches are typed the same way with `ε_b = none`, and the result is `τ ! ⊔ᵢ (ψᵢ ⊔ φᵢ)`.

Consequences of the rule:

- The body's services pass outward.
- The branches are evaluated outside the `try` frame (§3.5), so their effects belong to the enclosing stage, and this same `try` does not catch their errors.
- A branch with `when`, or with a non-exhaustive payload pattern, does not remove the error from the row (spec/05 "Expected errors").

Checker:

- `handlers.rs` `try_expr`: the body is checked in a copy of the stage whose errors are `raised`. An `x`/`_` branch (function `catch_all`, also in parentheses and with an annotation) is checked against `Errors caught` with its own row `caught` and the constraint `raised ⊆ caught`: `caught` is the `ε_b` of the rule. An error branch covers its error when `!guarded && covers_all(pat)`. Then the constraint `raised ⊆ {handled | errors}` is added. A non-error pattern is `FLICK-E007`.
- `exhaust.rs` `covers_all`.

`provide` is no rule of the core: §1.4 lowers it to a handler, typed by (Handle-E) of §3.7. The checker keeps typing it (`handlers.rs` `provide`).

```text
──────────────────── (Todo)
Γ ⊢ todo : τ ! P
```

Checker: `debug.rs` `todo` (warning `FLICK-T022`).

### 2.4 Patterns and exhaustiveness

```text
⊢ _ : τ ⇒ ·          ⊢ x : τ ⇒ x : τ          ⊢ n : T_n ⇒ ·    (T_n is the type of the literal)

K : ∀ᾱ. τ₁ →P … →P τₙ →P T ᾱ      ⊢ pᵢ : τᵢ[ᾱ := ῡ] ⇒ Δᵢ
───────────────────────────────────────────────────
⊢ K p₁ … pₙ : T ῡ ⇒ Δ₁, …, Δₙ

⊢ pᵢ : τᵢ ⇒ Δᵢ
──────────────────────────────────────────────────────────────────
⊢ {ℓ₁ = p₁, …, ℓₖ = pₖ, ..} : {ℓ₁ : τ₁, …, ℓₖ : τₖ | ρ} ⇒ Δ₁, …, Δₖ

⊢ p₁ : τ ⇒ Δ      ⊢ p₂ : τ ⇒ Δ
──────────────────────────────
⊢ p₁ | p₂ : τ ⇒ Δ
```

Rules:

- The names in the `Δᵢ` are distinct; pattern variables are monomorphic.
- The alternatives of an or-pattern bind the same names at equal types (`or_pattern` compares them with `unify`; see [or-pattern alternatives](#or-pattern-alternatives-are-compared-by-subtyping)).
- `Exh(τ, Π)`: every closed value of type `τ` matches some `p ∈ Π`.
- In `match`, `Π` is the set of branch patterns without `when`; if they do not cover, `FLICK-T009`. An unreachable branch is the warning `FLICK-T010`, which does not affect typing.
- Patterns of `let` and of λ parameters must be irrefutable: `Exh(τ, {p})`.

A constructor of a GADT builds `T ρ̄`, with result arguments `ρ̄` over its own variables, instead of `T ᾱ`. A branch on it is typed under every instance `θ` of the constructor's variables and of the type variables around it that makes `T ρ̄` the matched type and keeps the regions the branch writes:

```text
K : ∀ᾱ. τ₁ →P … →P τₙ →P T ρ̄
for every θ with (T ρ̄)θ = (T ῡ)θ that keeps the regions e writes:
    ⊢ pᵢ : τᵢθ ⇒ Δᵢ(θ)      Γθ, Δ₁(θ), …, Δₙ(θ) ⊢ e : τ'θ ! φθ
the free variables and locations of e are those of the branch
──────────────────────────────────────────────────────────── (Branch-GADT)
the branch K p₁ … pₙ → e of a match on a value of T ῡ
```

With no such `θ` the branch is dead: no value of `T ῡ` is built by `K`, and its body is not typed. The checker types the branch once, under the most general unifier, or drops it as `FLICK-T010` when there is none; both are instances of the rule. The rule over every unifier is the primary one, because the most general unifier is not kept by type substitution ([the most general unifier is not kept by substitution](#the-most-general-unifier-is-not-kept-by-substitution)).

- `Exh(T ῡ, Π)` counts only the constructors that can build `T ῡ`, those whose `T ρ̄` unifies with `T ῡ` under some instance, regions included. The parts of a value share its rigid variables: what a constructor fixes in one part holds in the others, so `Exh((Tagged a, Tagged a), Π)` counts no pair of different constructors (`types/gadt/columns.rs` `columns_after`). This deadness differs on purpose from that of a branch, which keeps the regions the body writes. A constructor left out of `Exh` builds no value of the type at all. A dead branch has to stay dead under the substitutions of the regions around it (scopes, `kill`), which may move the regions it does not write.

Checker:

- `types/gadt/refine.rs` `refine` (the unifier, `Impossible`), `types/gadt/branch.rs` (the branch under the unifier, `FLICK-T010` for a dead one);
- `patterns.rs` `bind_pattern`, `constructor_pattern` (`FLICK-T018`), `record_pattern`, `or_pattern`;
- `exhaust.rs` `exhaustive`, `useful` (Maranget's usefulness algorithm), `covers_all`;
- `irrefutable.rs`.

### 2.5 Generalization

```text
Gen(Γ, τ) = ∀ᾱ. τ,      ᾱ = ftv(τ) ∖ ftv(Γ)
```

For runtime forms `Γ` includes `Ψ` (§3.6): `ᾱ ∉ ftv(Ψ)` (see [store typing](#a-store-typing-of-closed-types-is-not-preserved)).

(Let-Gen) requires `φ₁ = P`. The result of a runtime step is monomorphic (Let-Mono), and (Let-Sig) does not allow `∀` over an effectful right-hand side. This is how the rule of spec/03 "Generalization in the presence of Ref" is formalized:

- "Provably pure construction" is an expression for which `! P` is derivable. These are values, constructor applications, records, pure calls such as `make_identity ()`, and `match` and `try` without runtime steps. `λ` and `task` are pure for any body: the conclusion of their rules has `P`.
- "Not free in the environment" is `∖ ftv(Γ)`. Rigid variables from Γ are not generalized either.
- "The result of a runtime binding is not generalized even with `->{}`": `R⟨none;none⟩ ≠ P`, so (Let-Gen) does not apply to `Ref.make None`.
- "A full signature does not cancel these checks" is the premise `ᾱ ≠ ∅ ⇒ φ₁ = P` of (Let-Sig).

The checker generalizes no more than `Gen`. Fewer generalized variables give the same T3 guarantees and only accept fewer programs. The differences:

1. "Not free in Γ" is decided by levels. `Store::level`: a meta deeper than the current level is not in Γ. The result of a runtime binding is lowered to the level of the environment (`Store::lower`).
2. Variables of numeric literals are not generalized: the literal's type is chosen later.
3. Variables of unsolved class constraints are not generalized. A scheme without a signature gets no constraints (see [class constraints and generalization](#class-constraints-and-generalization)).
4. Variables of a value with an open region obligation are not generalized. This belongs to regions.
5. Unknown call rows inside a binding are closed with `none` before generalization rather than quantified (`effects/solve.rs` `settle_rows`): the checker picks the least solution (see [unknown call rows](#unknown-call-rows-in-a-generalized-binding-close-to-none)).
6. `let (a, b) = …` and `let assert` are always monomorphic. A top-level or recursive function without a signature is `FLICK-T004`; another local one is monomorphic.

Checker:

- The condition `φ₁ ≠ P` is the flag `Checker::runtime`. Only `perform` sets it. `function_with` (λ), `task` and `for` save the flag and restore it after the body.
- `bindings.rs` `unsigned` → `generalize.rs` `Checker::generalize`; this is the only call of `generalize`.
- `bindings.rs` `check_signed` and `polymorphic_runtime` (`FLICK-T013`), `pattern_let`.

`foreign pure` atoms are created as state, so they have two separate rules (see [stateful atoms](#atommake-and-atomfresh-create-state-but-are-foreign-pure)). A local binding without a type does not generalize variables inside an atom type (`ui.rs` `lower_atoms` before `generalize`): its uses fix the type. An atom type with a quantified variable on a value is `FLICK-U005` (`ui.rs` `polymorphic_atom`): at the top level without a signature (`unsigned`), with a signature (`polymorphic_signature`), and when checked against `∀` (`check_value`).

The spec/03 examples in the model. Fixtures: `tests/check/pass/types/generalization.fl`, `tests/check/fail/types/generalization.fl`.

| Example | Rule | Outcome |
| - | - | - |
| `make_empty : ∀a. Unit ->{} Ref (Option a)` | Let-Sig, the right-hand side is a λ, `P` | accepted |
| `let numbers = make_empty ()` in `use_separate_cells` | Let-Mono: a call through `→R` | each cell has its own type |
| `identity_again : ∀a. a -> a` = `make_identity ()` | Let-Sig, a pure application, `P` | accepted |
| `fresh_cell : ∀a. Eff none none (Ref (Option a))` = `task {…}` | Task, `P` | accepted; every `await` creates its own cell |
| `let shared = task { cell }` inside `capture_cell` | Let-Gen, but `α ∈ ftv(Γ)` because `cell : Ref (Option α)` | `shared` is monomorphic |
| `let empty = logged_none ()` | Let-Mono | `(empty, empty)` as `(Option Float, Option String)` rejected |
| `invalid` | Let-Mono for `cell` | rejected |
| `shared : ∀a. Ref (Option a) = Ref.make None` at the top level | Let-Sig requires `P`, at the top level too | rejected (`FLICK-T013`, `FLICK-E001`) |

### 2.6 Module and program

Top-level declarations are nested `let`/`let rec` in dependency order. Each right-hand side is typed with `! P`, because module initialization is pure (`effects.rs` `Stage::default`, `FLICK-E001`).

Checker:

- `toplevel.rs` `check_item`, `check_definition`;
- a value whose type is still unknown at the end is `FLICK-T013` (`unknown_types`);
- recursion is explicit only (`recursion`): otherwise `FLICK-T017`; a value in a cycle is `FLICK-T012`.

A program is:

- a closed `e₀` (usually `main ()`) with `· ⊢ e₀ : τ ! φ₀` and closed rows in `φ₀ ⊑ R⟨r₀; ε₀⟩`;
- a root environment `Σ₀`: for every `S` in `r₀` there is `Σ₀(S)` with `· ⊢ Σ₀(S) : S.Impl ! P`. The runner assembles it.

## 3. Dynamics

### 3.1 Configurations

A configuration is `⟨μ, e⟩`. `μ` is the store, a finite map from locations to values. `Σ₀` is fixed for the whole run.

Outcomes:

- a value `v`;
- an expected error `raise v`;
- a defect `die`;
- divergence.

The core has no interruption: there is one thread, and `task`/`await` run on the current one (spec/02). λRT adds interruption.

Forms absent from the source language:

```text
v ::= … | ℓ | fix f. λx. e
e ::= … | handle v with ā
```

### 3.2 Contexts

Evaluation is strict, left to right, and respects every currying stage (spec/01, spec/02):

```text
E ::= □ | E e | v E
    | let x = E in e | let x : σ = E in e
    | match E with ā
    | {ℓ₁ = v₁, …, ℓᵢ = E, ℓᵢ₊₁ = eᵢ₊₁, …} | E.ℓ | {E with ℓ = e} | {v with ℓ = E}
    | await E | try E with ā
```

Contexts do not enter `λ`, `task` or branches. `F` is a single-frame context: one of the forms above with `□` in place of `E`.

An operation is answered by the innermost handler in `E` with an arm for it (§3.7); if there is none, by `Σ₀`.

### 3.3 Pure steps

```text
(λx. e) v                       →ₚ  e[x := v]
(fix f. λx. e) v                →ₚ  e[f := fix f. λx. e, x := v]
let x = v in e                  →ₚ  e[x := v]                      and likewise let x : σ = v in e
let rec f : σ = λx. e₁ in e₂    →ₚ  e₂[f := fix f. λx. e₁]
{…, ℓ = v, …}.ℓ                 →ₚ  v
{{…, ℓ = v, …} with ℓ = v'}     →ₚ  {…, ℓ = v', …}
match v with p → e | ā          →ₚ  e[δ]                           if match(p, v) = δ
                                →ₚ  match v with ā                 otherwise
match v with p when g → e | ā   →ₚ  match g[δ] with True → e[δ] | False → match v with ā
                                                                   if match(p, v) = δ
                                →ₚ  match v with ā                 otherwise
try v with ā                    →ₚ  v
```

```text
match(_, v)                          = ∅
match(x, v)                          = [x := v]
match(K p₁ … pₙ, K v₁ … vₙ)          = match(p₁, v₁) ∪ … ∪ match(pₙ, vₙ)
match({ℓ₁ = p₁, …, ..}, {…, ℓ₁ = v₁, …}) = ⋃ᵢ match(pᵢ, vᵢ)
match(p₁ | p₂, v)                    = match(p₁, v) if defined, otherwise match(p₂, v)
match(n, n)                          = ∅
undefined in all other cases
```

### 3.4 Runtime steps

```text
⟨μ, E[e]⟩                         →  ⟨μ, E[e']⟩                  if e →ₚ e'
⟨μ, E[Ref.make v]⟩                →  ⟨μ[ℓ ↦ v], E[ℓ]⟩            ℓ ∉ dom μ
⟨μ, E[Ref.get ℓ]⟩                 →  ⟨μ, E[μ(ℓ)]⟩
⟨μ, E[Ref.set ℓ v]⟩               →  ⟨μ[ℓ ↦ v], E[()]⟩
⟨μ, E[await (task e)]⟩            →  ⟨μ, E[e]⟩
⟨μ, E[perform_{S.o} v]⟩           →  ⟨μ, E[f v]⟩                 no handler in E answers it, f = Σ₀(S).o
```

What this shows:

- `await` runs the body anew on every call. `task` is neither a cache nor a Promise (spec/02 "Re-running and the value of await").
- Lexical values enter `task e` by substitution when it is built. Operations are answered at execution time, by the handlers of the current context.
- Lifting a pure function does nothing: `(λx. e) v` reduces the same way whatever the stage of the arrow.

### 3.5 Errors and defects

```text
⟨μ, E[F[raise v]]⟩                →  ⟨μ, E[raise v]⟩              F is not try □ with ā
⟨μ, E[try (raise v) with ā]⟩      →  ⟨μ, E[handle v with ā]⟩
⟨μ, E[handle v with p → e | ā]⟩   →  ⟨μ, E[e[δ]]⟩                 if match(p, v) = δ
                                  →  ⟨μ, E[handle v with ā]⟩      otherwise
⟨μ, E[handle v with p when g → e | ā]⟩
                                  →  ⟨μ, E[match g[δ] with True → e[δ] | False → handle v with ā]⟩
                                                                  if match(p, v) = δ
                                  →  ⟨μ, E[handle v with ā]⟩      otherwise
⟨μ, E[handle v with ·]⟩           →  ⟨μ, E[raise v]⟩
⟨μ, E[todo]⟩                      →  die
```

How it works:

- `raise v` propagates through frames up to the nearest `try`.
- The branches of `handle` are evaluated already outside the `try`.
- `handle v with ·` rethrows the error: this is the operational side of the remainder `ε'` in (Try).
- `die` is not caught: an ordinary handler is not a universal catch (spec/05).
- `⟨μ, raise v⟩` without a `try` is a final state.

### 3.6 Typing of runtime forms

Ψ is a store typing: `Loc ⇀ τ` with monotypes. Their free type variables are free inference variables of the configuration's derivation; (Let-Gen) does not generalize them (see [store typing](#a-store-typing-of-closed-types-is-not-preserved)).

`⊢ μ : Ψ` ⇔ `dom μ = dom Ψ` and `·;Ψ ⊢ μ(ℓ) : Ψ(ℓ) ! P` for every `ℓ`.

The rules of §2 are read with `Ψ` in the context. In addition:

```text
Ψ(ℓ) = τ                               σ = ∀ᾱ. τ₁ →φ τ₂    Γ, ᾱ, f : σ°, x : τ₁ ⊢ e : τ₂ ! φ
───────────────────── (Loc)            ──────────────────────────────────────────────── (Fix)
Γ;Ψ ⊢ ℓ : Ref τ ! P                     Γ ⊢ fix f. λx. e : σ

E : ∀ᾱ. τ̄ →P E ᾱ    Γ ⊢ w̄ : τ̄[ᾱ := ῡ] ! P    E ῡ ∈ ε
──────────────────────────────────────────── (Errors-Val)
Γ ⊢ E w̄ : Errors ε ! P

Γ ⊢ v : Errors ε_b ! P    v = E w̄    branches ā as in (Try)    E ∈ H + ε'
────────────────────────────────────────────────────────────────── (Handle)
Γ ⊢ handle v with ā : τ ! R⟨none; ε'⟩ ⊔ ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

(Handle) does not require the whole row `ε_b ⊆ H + ε'`, only the error of `v` itself: after a failed branch the remainder has a smaller `H` (see [(Handle) after a failed branch](#handle-required-the-whole-error-row-after-a-failed-branch)). Coming from `try (raise v)`, the premise follows from `ε_b ⊆ H + ε'` of (Try).

(Errors-Val) types values only. In the source language, a value of `Errors ρ` with two or more elements is introduced only by an `x`/`_` branch (see [`Errors` of a row with two or more elements](#errors-of-a-row-with-two-or-more-elements)).

The remainder of a `match` after a false guard may not cover the type of the scrutinee. For `match Some 1 with | None → a | Some x when p → b | Some _ → c` the remainder `match Some 1 with | Some _ → c` fails `Exh(Option Int, …)`. For a value it is enough to cover the value itself (see [the residual match](#the-residual-match-after-a-false-guard-fails-exh)):

```text
Γ ⊢ v : τ ! P    ⊢ pᵢ : τ ⇒ Δᵢ, gᵢ and eᵢ as in (Match)    v matches some pⱼ without when
───────────────────────────────────────────────────────────────── (Match-Val)
Γ ⊢ match v with p₁ [when g₁] → e₁ | … : τ' ! ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

(Match-Val) does not apply to source programs: `match 1 with | 1 → …` is still rejected.

The flick-core checker never sees runtime forms. The reference checker types them in `flick-model/src/check/runtime.rs` (`check_config`), and (Match-Val) in `handlers.rs` `match_`, only when checking a configuration.

### 3.7 Handlers of effects

An effect `E` declares operations `o : a → b` of a class `c`: `tail` (the arm's value is the operation's), `abort` (the arm's value is the handler's, the body does not come back) or `control` (the arm's value is the handler's, with the rest of the body as `resume`, at most once). Effects take no parameters, as services. `perform_{E.o}` is a constant; `hdl E al r e` handles `E` in `e`, `al` the arms, `r` the function of the body's value. A handler's arms for errors are a `try` around it: errors pass `hdl`.

```text
S.op E o = (c, a, b)
────────────────────────────────────────────────────── (Perform)
Γ ⊢ perform_{E.o} : a →R⟨{E};∅⟩ (c = abort ? τ : b) ! φ

Γ↑, ϱ ⊢ al : arms E (τ' at R⟨r;ε⟩, resume over ϱ)    Γ ⊢ r : τ →ψ τ' ! R⟨r;ε⟩    ψ ⊑ R⟨r;ε⟩
Γ ⊢ e : τ ! φ₀    φ₀ ⊑ R⟨{E} ∪ r; ε⟩    no {Provided {E} _} or {Provided v _} in φ₀
E ∈ r unless every operation of E has an arm without a guard
──────────────────────────────────────────────────────────────────────────────────────────── (Handle-E)
Γ ⊢ hdl E al r e : τ' ! R⟨r;ε⟩
```

The arm of a `tail` operation has `x : a` and the type `b`; of `abort`, `x : a` and `τ'`; of `control`, `x : a`, `resume : b →R⟨r;ε⟩ τ'` holding `ϱ`, and `τ'`. `ϱ` is a region variable of the handler, as the region of a `scope`: `resume` stays in the arms.

Run-time forms. Installing a handler opens the region `p = |Λ|` and allocates the cell `ℓ = |μ|`, whose type `Unit →R⟨r;ε⟩ τ'` holds the stage and the type of the handler's value:

- `hwith p ℓ E al r e`, inside `inside p`: the handler running its body;
- `hctl p ℓ κ tl E al r a k`: the arm `a` runs while the body waits, parked at `hole p` in `k`; for `control`, `resume` is `kont p κ` with a new cell `κ` of its type;
- `hres p ℓ a b`: the resumed body `b` runs under its handler while the arm waits, parked in `a`; `hdone p a`: the arm after its `resume` finished;
- `unw p ℓ a`: unwinding to the handler `p`, which then runs `a`; it has any type, `a` the type of the handler's value.

Steps. An operation runs the arm of the innermost handler with one for it, the frames between not handling it: `abort` puts `unw p ℓ arm[v]` at the operation; `tail` and `control` park the body and run the arm at the handler. The value of a `tail` arm goes back into the body; a `control` arm that ends without resuming, or an arm that raises or unwinds, unwinds the parked body to the handler. `resume u` puts `u` into the body under its handler, once; a second `resume` dies. A signal passes every frame but `inside`, which closes first, and the frames of an arm, which let their parked part go; the handler with the cell of the signal catches it. A handler whose body has a value applies `r` to it.

A child or a finalizer keeps the handlers of the place that registered it, those older than its owner: `hkeep q ℓ E al e` around its work.

```text
Ψ(ℓ) = Unit →R⟨r;ε⟩ τ'    Γ ⊢ al : arms E (τ' at R⟨r;ε⟩, resume over q)    Γ ⊢ e : τ ! φ₀
φ₀ ⊑ R⟨E ∪ r₁; ε₁⟩, its {Provided {E} a} with q ≥ a    R⟨r;ε⟩ ⊑ R⟨r₁;ε₁⟩ ⊑ φ
E ∈ r₁ if an operation of E has no arm without a guard, or is not tail
──────────────────────────────────────────────────────────────────────── (Keep)
Γ ⊢ hkeep q ℓ E al e : τ ! φ
```

It answers the `tail` operations of `E` with an arm while the region `q` is live: the arm runs at the frame, while the rest of the child waits, as `hkarm q ℓ E al a k`, and its value goes back to the operation. A kept handler runs no `return`: a value passes it. It catches no signal: an arm that raises lets the rest of the child go, closing its regions, and the child ends with the signal, as it would with the error. Other operations pass it. An `abort` or `control` operation of a child that reaches no frame of its handler is stuck; the checker rejects `control` there (`classes/detached.rs`), and these rules do not.

Statements (Lean): Progress and Preservation cover every step above; T2b holds with the row `{E}` of an operation removed by its handler; the main thread is never a signal at its top (`Caught`); the regions of a handler nest as those of a scope, and what a resumed body gives back names no region younger than its handler (see [what a resumed body gives back enters the scopes of the arm](#what-a-resumed-body-gives-back-enters-the-scopes-of-the-arm)). A spawned task does not reach the handlers around the place that spawned it: an operation only they handle is stuck in the task, as an `ask_S` that nothing answers.

## 4. Theorems

Hypotheses:

- **G1 (FFI).** A `foreign pure` call is a `→ₚ` step: it does not change `μ`, does not look up services, does not produce `raise`, but may produce `die`. A `foreign` call with a runtime arrow stays within that arrow's rows. Values:
  - a value the binding returns (and one the foreign code passes to a callback) has the declared type; `Never` in the result means the call does not return;
  - a trusted binding (`@flick-lang/runtime`) does not modify or retain the Flick data it receives. An untrusted one receives and returns them as copies, so for it the code generator ensures this ([Flick data copied at the foreign boundary](ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary)).

  What else at the TypeScript boundary rests on this hypothesis is listed in `ts-holes.md` §3.
- **G2 (code generation).** The generated JS and Go implement the steps of §3, and the `Lift` node in them is the identity of the model. This is checked by differential tests, not by proof.
- **G3 (runner).** `Σ₀` covers `r₀`; `Async ∈ dom Σ₀`, which the scheduler handles.

Lemmas:

- **L1 (canonical forms).** Let `·;Ψ ⊢ v : τ ! φ`. Then:
  - for `τ = τ₁ →φ' τ₂`, `v` is a λ, a `fix`, or a partial application of a constant or a constructor (see [partial constructor application](#a-partial-constructor-application-is-a-value));
  - for `Eff r ε τ'`, `task e`;
  - for `Ref τ'`, `ℓ` with `Ψ(ℓ) = τ'`;
  - for `T ῡ`, `K v̄` with a constructor `K` of type `T`;
  - for a record, a record with the same labels;
  - for `Errors ε`, `E w̄` with `E ῡ ∈ ε`.

  Moreover, every value is typed with `P`.
- **L2 (substitution).** If `Γ, x : ∀ᾱ.τ₁ ⊢ e : τ ! φ`, `Γ, ᾱ ⊢ v : τ₁ ! P` and `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)`, then `Γ ⊢ e[x := v] : τ ! φ`.
- **L3 (context effect).** If `·;Ψ ⊢ E[e] : τ ! φ`, then `·;Ψ ⊢ e : τ' ! φ'` for some `τ'`. Here `φ'` fits within `φ` extended by the services of the `with` frames in `E` and by the errors removed by the `try` frames in `E`. Without such frames `φ' ⊑ φ`.

**T1 (type safety).** Let `e₀`, `Σ₀` be a program (§2.6) and `⟨∅, e₀⟩ →* ⟨μ, e⟩`. Then exactly one of the following holds:

1. `e` is a value, and `·;Ψ ⊢ e : τ ! P` for some `Ψ` with `⊢ μ : Ψ`;
2. `e = raise v` and `· ⊢ v : Errors ε₀`: an expected error from the row `ε₀`;
3. the configuration takes a step `⟨μ, e⟩ → ⟨μ', e'⟩` or `⟨μ, e⟩ → die`.

λRT adds the "interrupted" outcome.

T1 follows from two statements:

- **Progress.** Let `·;Ψ ⊢ e : τ ! φ` and `⊢ μ : Ψ`, and let the effects of `φ` be in `Σ₀`, taking the handlers of `e` into account. Then `e` is a value, or `e = raise v`, or the configuration takes a step (possibly to `die`). The main cases:
  - `match`: by L1 and `Exh` a branch exists. A `False` guard passes the value to the next branch, and `Exh` over the branches without `when` guarantees that a matching one is found.
  - `perform_{E.o} v`: by L3, a handler of `E` answers it or `E ∈ r₀ ⊆ dom Σ₀`, whose record has a function for `o`.
  - `await v`: by L1, `v = task e`.
  - `Ref.get ℓ`: by L1 and `⊢ μ : Ψ`, the location `ℓ ∈ dom μ`.
- **Preservation.** Let `·;Ψ ⊢ e : τ ! φ`, `⊢ μ : Ψ` and `⟨μ, e⟩ → ⟨μ', e'⟩`. Then there is `Ψ' ⊇ Ψ` with `⊢ μ' : Ψ'` and `·;Ψ' ⊢ e' : τ ! φ'`, where `φ' ⊑ φ`.
  - `Ψ'` differs from `Ψ` only after a `Ref.make` step: then `Ψ' = Ψ[ℓ ↦ τ']` with a monotype `τ'` (see [store typing](#a-store-typing-of-closed-types-is-not-preserved)).
  - The `let` case with generalization is treated in T3.
  - The case `try (raise v)`: the value `v` came from the body with `v : Errors ε'`, `ε' ⊆ ε_b`, and (Errors-Val) gives `v : Errors ε_b` for (Handle).
  - The false-guard case: the remainder `match v with ā` is typed by (Match-Val).

Progress and Preservation are checked on the reference checker by the property `progress_and_preservation_hold` (`flick-model/tests/props.rs`, part of the model properties, `just model`). Every machine state, read as `⟨μ, E[e]⟩`, is retyped with `φ' ⊑ φ₀`; the run does not get stuck, and a final `raise v` is an error from `ε₀`. Programs with regions run the same way, with state `⟨Λ, μ, E[e]⟩` (`regions.md` §3.3).

**T2 (effect rows).**

- **(a) Purity.** If `·;Ψ ⊢ e : τ ! P`, then every evaluation `⟨μ, e⟩ →* ⟨μ', e'⟩` consists only of `→ₚ` steps:
  - `μ' = μ`;
  - there is no `Ref.*`, `await` or operation step;
  - `e'` is not `raise v`.

  `die` and divergence are possible. Justification: the steps of §3.4–3.5 happen on redexes that the rules type only with `R⟨…⟩`: constants with `→R`, operations, (Await) and (Handle-E). By L3 the redex's effect would then be `⊑ P`, which is impossible, and Preservation keeps `P`.
- **(b) Rows.** Let `·;Ψ ⊢ e : τ ! R⟨r; ε⟩`. Then in every evaluation from `⟨μ, e⟩`:
  - every operation of `S` answered by `Σ₀`, rather than by a handler of the evaluation itself, has `S ∈ r`;
  - if the evaluation ends in `raise v`, then `· ⊢ v : Errors ε`, that is, the constructor of `v` is an element of `ε`.

  For `· ⊢ v : Eff r ε τ` the same holds for `await v`. Formally this is a statement about a semantics where an operation of `S` answered by `Σ₀` is labelled `S`.

  Corollary: with `ε = none` the evaluation does not end in an expected error. A defect and divergence remain possible (spec/05 "Expected errors").

**T3 (generalization).**

- **Lemma 3.1.** While a subterm with `Γ ⊢ e : τ ! P` is being evaluated, that is, while the redex lies inside `e`, the store `μ` does not grow. This follows from T2(a).
- **Theorem.** For every reachable configuration `⟨μ, e⟩` of a program there is `Ψ` with `⊢ μ : Ψ` and `·;Ψ ⊢ e : τ ! φ`, where each `Ψ(ℓ)` is a monotype fixed when `ℓ` is allocated, and `Ψ` only grows along subsequent configurations. Hence:
  1. `Ref.get ℓ` returns a value of type `Ψ(ℓ)`, and `Ref.set ℓ v` writes a value of type `Ψ(ℓ)`: a single cell is never used at two types.
  2. The value of a binding with scheme `∀ᾱ.τ₁`, `ᾱ ≠ ∅`, never reaches a cell whose type depends on `ᾱ`. Cells allocated before the binding have types without `ᾱ`, because `ᾱ ∉ ftv(Γ) ∪ ftv(Ψ)`. Allocating a new cell while the right-hand side is evaluated is ruled out by the premise `P` (Lemma 3.1).

The key to the proof is the `let` case of Preservation:

- a step inside the right-hand side under (Let-Gen): by Lemma 3.1 `Ψ' = Ψ`, `e₁'` is again typed with `P`, and `Gen(Γ, τ₁)` does not change;
- the step `let x = v in e₂ → e₂[x := v]`: L2, where `ᾱ ∉ ftv(Ψ)` because `Gen` does not generalize `ftv(Ψ)`;
- (Let-Sig): the same.

Without the premise `P` the first case breaks. `Ref.make None` steps to `ℓ` with `Ψ(ℓ) = Option α`, where `α` is generalized, and the body uses `ℓ` at two types. The program `invalid` from spec/03 reaches `String.trim 42` and gets stuck; in JS it is a `TypeError` (the failure shown in [an annotation with `forall`](#an-annotation-with-forall-bypasses-let-sig)).

T3 is about `Ref`. State created through the FFI is covered by hypothesis G1; for atoms that violate it the checker enforces a separate rule, `FLICK-U005` (see [stateful atoms](#atommake-and-atomfresh-create-state-but-are-foreign-pure)).

## 5. Known divergences

The first four divergences below and the or-pattern one broke T1–T3 or hypotheses G1 and G2, and the numeric-literal one broke T1. Each was confirmed by a probe: the checker accepted the program, and its JS build behaved as described. The remaining divergences are incompleteness of the checker relative to the model or imprecise prose. Every checker fix has a fixture.

Shared definitions for the two `try` divergences:

```flick
type error A = A Float
type error B = B String
type error C = C String

f : Bool ->{; A, B, C} Unit
let f b =
  if b then from_result (Err (C "not a number")) else from_result (Err (B "x"))
```

### An annotation with `forall` bypasses (Let-Sig)

```flick
hole : Unit ->{} String
let hole () =
  let cell : forall a. Ref (Option a) = Ref.make None
  Ref.set cell (Some 42)
  match Ref.get cell with
  | Some value -> String.trim value
  | None -> ""
```

- Divergence: the program was accepted. In Node it failed with `TypeError: self.trim is not a function`. The same held for `(Ref.make None : forall a. Ref (Option a))` and for `use_poly (Ref.make None)` with the higher-rank parameter `use_poly : (forall a. Ref (Option a)) ->{} String`. This violated T3.
- Cause: the premise `ᾱ ≠ ∅ ⇒ φ₁ = P` was checked only by `polymorphic_runtime`, and only for signatures. Neither `unsigned` with an annotation nor `check_value` checking against `Forall` checked it.
- Resolution (checker): `check_value` in the `Forall` branch saves `Checker::runtime` around the check and, if the current stage made a runtime call, calls `polymorphic_runtime` (`FLICK-T013`), the same check as for signatures. Fixture: the three forms in `tests/check/fail/types/generalization.fl`.

### A catch-all try branch typed by its use discharges the whole row

A branch `x` of a `try` whose use gives it the type `Errors {A, B}` removed the whole row of the body.

```flick
rethrow : Errors {A, B} ->{; A, B} String
let rethrow e = from_result (Err e)

g : Unit ->{; A, B} String
let g () =
  try
    f true
    "ok"
  with
  | e -> rethrow e
```

- Divergence: `g` was accepted although the body raises `C`. A function `h : Unit ->{} String` with `try g () with | A _ -> "a" | B _ -> "b"` ended in Node with error `C`. This violated T2(b).
- Cause: if the branch type had the form `Errors …`, `try_expr` marked the branch as catching everything without checking that type against `Errors raised`.
- Resolution (checker): fixed together with the next divergence.

### A try variable typed by its use counts as a full branch

A variable in a `try` whose use gives it the type `A` counted as a complete branch for `A`, but at run time caught any error.

```flick
payload : A -> Float
let payload e =
  match e with
  | A x -> x + 1

g : Unit ->{; B, C} Float
let g () =
  try
    f true
    0
  with
  | e -> payload e
```

- Divergence: the program was accepted. In Node `g ()` received `C "not a number"` and returned `undefined` as a `Float`. This violated T1.
- Resolution (checker): the type of an `x`/`_` branch is fixed by the pattern syntax, as in (Try): the branch is checked against `Errors caught`, and the body's errors must be contained in `caught` (`raised ⊆ caught`). `payload e` makes `caught = {A}`, and the pattern reports `FLICK-E002`: "the expected type `Errors {A}` does not allow errors `B`, `C`"; `rethrow e` from the previous divergence reports the same for `C`. If the body raises only `A`, `payload e` is accepted, as in the model (`Errors {A} ≡ A`). Fixtures: `tests/check/fail/effects/handlers.fl`, `tests/check/pass/effects/errors.fl`.

### `Atom.make` and `Atom.fresh` create state but are foreign pure

`Atom.make` and `Atom.fresh` are declared `foreign pure` but create state, and a local binding of one was generalized.

```flick
poke : Unit ->{Atom.Read, Atom.Write} String
let poke () =
  let s = Atom.fresh None
  Atom.set s (Some 42)
  match Atom.value s with
  | Some v -> String.trim v
  | None -> ""
```

- Divergence: the program was accepted. This violated hypothesis G1, and through it T3. `FLICK-U005` (`ui.rs` `polymorphic_atoms`) checked only the top level.
- Resolution (checker): a local binding without a type does not generalize variables inside an atom type (`ui.rs` `lower_atoms`), so in the example above `Some 42` and `String.trim v` give `FLICK-T003`. An atom type with a quantified variable is `FLICK-U005` (`ui.rs` `polymorphic_atom`) at the top level and for a signature, an annotation or a higher-rank argument at any level; `polymorphic_atoms` was removed. Reporting U005 for a local binding without a type would be a false positive: the atom type there is often refined by the following lines (`let model = Atom.make None`). Fixture: the local forms in `tests/check/fail/ui/atom_poly.fl`.
- Why `make`/`fresh` are not runtime arrows: top-level atoms are built during module initialization, and that is pure.

### `Errors` of a row with two or more elements

- Spec: spec/03 "Notation and joining of rows" called such a type "a sum of the nominal errors".
- Checker: a projection exists only for `none` and for a single element. `match e with | A _ -> … | B _ -> …` with `e : Errors {A, B}` is `FLICK-T003`.
- The model does not need the sum: in the source language values of `Errors ρ` come only from an `x`/`_` branch, and for the proof (Errors-Val) on runtime values suffices.
- Resolution (spec, spec/03 "Notation and joining of rows"): "`Errors ρ` with two or more elements or with a variable is an opaque type. Its value comes from the `x`/`_` branch of a `try` and flows into `from_result`, `Failure.rethrow` and generic code. To take it apart by error, use a `try` with error branches over `from_result (Err e)`." A sum with patterns on elements is a possible extension: an introduction rule and a pattern on an element of a closed row.

### A callee of unknown type is a pure arrow

- Divergence: `(fun h -> h ()) f` with `f : Unit ->{Log} Unit` is `FLICK-T003` "expected `Unit -> _`".
- In the model (App) admits any `φ`, but `λh. h ()` has no principal type: `(Unit →P τ) →P τ` and `(Unit →R⟨r;ε⟩ τ) →R⟨r;ε⟩ τ` are incomparable, and the core has no stage variables.
- Resolution (spec): `reference/effects.md` states that a parameter of unknown type applied to an argument is treated as a pure function; an effectful callback gets its type from the expected type or from a signature.

### Only arrows and `Eff` have subtyping

- Divergence: `List (Eff {Log} none Unit)` does not pass as `List (Eff {Log, Clock} none Unit)` (`FLICK-T003`). The model (§2.2) specifies the same.
- The spec ("A signature gives an upper bound"; `reference/effects.md` "A runtime arrow and an Eff accept a value that needs fewer services and errors") did not say that rows under other constructors must be equal.
- Resolution (spec): stated in `reference/effects.md`. Covariance of immutable constructors needs variance inference, which spec/03 defers.

### Or-pattern alternatives are compared by subtyping

The alternatives of an or-pattern were compared by subtyping, and the IR did not apply the lift recorded by that comparison.

```flick
type Job = Loud (Unit ->{} String) | Quiet (Unit -> String)

run : Job ->{} String
let run job =
  match job with
  | Loud f | Quiet f -> f ()
```

- Divergence: `run (Quiet hello)` crashed in Node with `Fiber.runLoop: Not a valid effect`. `patterns.rs` `or_pattern` compared the alternatives through `expect`. In the model the lift is the identity, so this violated hypothesis G2, not the rules.
- Resolution (checker): the alternatives bind names at equal types (`unify`), as in §2.4. Fixture: `tests/check/fail/types/matching.fl`.

### Class constraints and generalization

- Spec: spec/03 excluded from generalization only the variables of "ambiguous constraints".
- Checker: the variables of any unsolved constraint and of numeric literals stay monomorphic; a scheme without a signature never gets constraints.
- Resolution (spec, spec/03 "Generalization in the presence of Ref"): "a binding without a signature has no class constraints: variables of unsolved constraints and of numeric literals stay monomorphic; constrained polymorphism requires a signature".

### Unknown call rows in a generalized binding close to none

Unknown call rows inside a generalized binding are closed with `none`, whereas `Gen` would quantify them.

- This is the choice of the least solution and incompleteness relative to the model. The model accepts it as a narrowing of `Gen` (§2.5, item 5).
- Resolution (spec): a sentence in `reference/types.md` "Generalization", analogous to what `reference/effects.md` already says about the end of a declaration.

### The type of `Some 42` in spec/03

- Spec: "`Some 42` fixes the cell's content as `Option Float`" and "`numbers` … stores only `Option Float`".
- In fact an integer literal without context is `Int` (`reference/types.md` "Numbers and operators"), and the fixture expects "expected `String`, found `Int`".
- Resolution (spec): `Option Int` in both places, in the English and the Russian versions.

### Which example spec/03 rejects under variance

- Spec, "What variance additionally allows": "The example `logged_none` above is deliberately rejected".
- In fact `use_logged_none` is rejected, while `logged_none` itself is accepted.
- Resolution (spec): corrected.

### The residual match after a false guard fails Exh

- Divergence: `match Some 0 with | None → a | Some x when false → b | Some _ → c` steps to `match Some 0 with | Some _ → c`, and (Match) requires `Exh(Option Int, {Some _})`. Preservation under the rules of §2.3 fails, although the step is safe: the value matches the remaining branch.
- Resolution (model): the rule (Match-Val) of §3.6 for a value in scrutinee position. The reference checker applies it only to configurations; without it the property `progress_and_preservation_hold` fails.

### A numeric literal variable accepted an arrow

```flick
literal_call : Float -> Float
let literal_call n = 2 n

literal_element : Unit -> Int
let literal_element () =
  match [0, fun x -> x] with
  | [_, b] -> b + 1
  | _ -> 0
```

- Divergence: both were accepted, and the model rejects both: a literal is a number, not a function. In the second, `b : Int` is a function; in Node `literal_element ()` returned `0` (`b + 1 | 0`). This violated T1.
- Cause: a literal variable can only become a number, and unifying it with an arrow fails, but the error was discarded by `calls.rs` `apply` (a callee of unknown type) and `infer.rs` `curry` (a λ with an unknown expected type).
- Resolution (checker): `FLICK-T003` "this is not a function" and "expected `Int`, found a function". Fixtures: `tests/check/fail/types/misc.fl` `literal_call`, `literal_element`.

### Order comparison on functions

```flick
let program () =
  match { a = fun x -> 0 } with
  | { a = f } -> f < f
```

- Divergence: `flick-core` rejects it (`FLICK-T003` "`<` works on `Int` or `Float` or `BigInt` or `String`, not on `_ -> Int`"), while the reference checker accepted it: §1.5 did not name the comparison types, and `flick-model` gave `<` the type `∀α. α → α → Bool`. The generator produces `<` only over `Int`; the contradiction was found by a mutation that substituted a function variable.
- Resolution (model): the `<, <=, >, >=` line in §1.5 and a deferred operand check in `flick-model` (`check/constants.rs` `ordered_operand`), like `settle_operators`; test `comparisons_order_numbers_and_strings_only`.

### A partial constructor application is a value

- Divergence: the value grammar of §1.3 gave `K v₁ … vₙ` without fixing `n`, and L1 for arrows listed only λ, `fix` and a partial application of a constant. But a `K` of arity above zero and `K v₁` are functions: `let f = Some in f 1` substitutes `Some` as a value. Without this, L1 for arrows is false, and Progress gets stuck on `let f = Some in …`.
- Resolution (model): the grammar has `K v₁ … vₖ` with `k` at most the arity of `K`, and L1 for arrows includes a partial constructor application. Lean: `Value.ctorApp`, `canonical_arrow` (`formal/Formal/CanonForms.lean`).

### (Handle) required the whole error row after a failed branch

- Divergence: for `try e with | A _ -> 0 | B _ -> 1`, where the body `e` raises `A` or `B`, `ε_b = {A, B}`, the branches remove `H = {A, B}`, and `ε' = none`. Suppose the body raised `B "x"`: `handle (B "x") with A _ → 0 | B _ → 1` steps to `handle (B "x") with B _ → 1`, whose remainder has `H = {B}`, and the premise `ε_b ⊆ H + ε'` of (Handle) fails. Preservation under the rules of §3.6 breaks, although the step is safe: the error of the value itself is `B`. This is the `try` counterpart of [the residual match](#the-residual-match-after-a-false-guard-fails-exh).
- Resolution (model): the premise of (Handle) is `E ∈ H + ε'` for the constructor `E` of the value `v`. Coming from `try (raise v)`, it follows from the premise of (Try); after a failed branch `E p̄` without `when` with `Exh(E, {E p̄})`, it holds because the branch did not match, so `v` is not an `E`. The reference checker already checked it this way: `check/runtime.rs` types `handle v` as `try (raise v)`, and `raise v` carries only the error of `v`. Lean: `Typ.handle` with `HCov` (`formal/Formal/Typing.lean`, `Rules.lean`), `HCov.tail` (`formal/Formal/PresRt.lean`).

### A store typing of closed types is not preserved

- Divergence: §3.6 required closed `Ψ(ℓ)`, and T3 required the same type `τ` for all reachable configurations. The closed term `Ref.make (λx. x)` has type `Ref (α → α)`: (Let-Gen) does not apply because of the runtime stage, and `α` is free. After the step, `Ψ(ℓ)` is either open or, after substituting a closed type, the type of the configuration is no longer `τ`. "Without loss of generality" relied on a type-substitution lemma that the document did not have.
- Resolution (model): the `Ψ(ℓ)` are monotypes, possibly with free inference variables, and `Gen` with `Ψ` in the context does not generalize `ftv(Ψ)`. So the premise `ᾱ ∉ ftv(Ψ)` of L2 holds by construction. Lean: (Let-Gen) shifts `Γ` and `Ψ` past the generalized variables (`Typ.letGen`, `formal/Formal/Rules.lean`); the type-substitution lemma is `Typ.substT` (`formal/Formal/TySubst.lean`).

### The most general unifier is not kept by substitution

- Divergence: a rule that types a GADT branch only under the most general unifier of `T ρ̄` with the matched type is not closed under type substitution in the core, where rows and captures are sets. For `Refl : TypeEq a a` at `TypeEq (Int ->{c} Int) (Int -> Int)`, every unifier sends `c` to a type that reaches nothing (`Int`, `Unit`, `caps ∅`), and none of them is most general. A substitution of the variables around a branch can leave such a problem, and the branch cannot be typed again.
- Resolution (model): (Branch-GADT) types the branch under every unifier that keeps the regions the branch writes. The checker's typing under the most general unifier and its dropped dead branch are derived rules. Lean: `Typ.aref` (`formal/Formal/Rules.lean`); `Typ.aref_substT` (`TySubstBase.lean`), `Typ.aref_subst` (`SubstArm.lean`), `aref_match` (`SpineMatch.lean`); `Mgu`, `Typ.aref_mgu`, `Typ.aref_dead` (`ArefMgu.lean`).

### A dead branch keeps no typing of its body

- Divergence: weakening and substitution need the free variables and locations of a branch within its context, also for a dead one. A typing of the body under some instance does not survive type substitution: `match (x : Expr a) | IntLit n → n` is typed under `a := Int`, and under `a := Bool` the branch is dead and `n : Int` has no typing at `Bool`.
- Resolution (model): (Branch-GADT) asks for the free variables and locations of the body directly, beside the typings under the unifiers. Lean: the premises of `Typ.aref`.

### Stepping a signed let loses its signature

After the step on `let x : σ = v` the state loses `σ`, and the reference checker cannot infer it. The case arose in generated programs with services, `provide` and regions (`regions.md` §3.3).

```text
v1 : forall a b r e. (a ->{r; e} b) -> a ->{r; e} b
let v1 g x = (g x)
… (v1 (try (try (fun x2 -> ((), 0)) with | _ -> (fun x3 -> ((), 0)))
       with | _ -> as4) …)
```

- Divergence: the step of §3.3 gives `(λg. λx. g x) (try … with _ → as4)` with `as4 : Unit →R⟨none;none⟩ (Unit, Int)`. In the program the `try` was checked against `a →R⟨r;e⟩ b` from `σ`, but in the state `λg` has no signature. The reference checker infers the argument without an expected type: `λx2` takes `P` (see [a callee of unknown type](#a-callee-of-unknown-type-is-a-pure-arrow)), and the branch with `as4` fails `(⊑)` at step 32 of `progress_and_preservation_hold`.
- This is incompleteness of the reference checker, not of the document: under the rules of §2, `λg` is typed with a runtime-arrow parameter, and Preservation holds. The reference checker infers, and `λg. λx. g x` has no principal type.
- Resolution (model): in the reference checker the step `let x : σ = λy. e₁ in e₂` gives `e₂[x := fix x. λy. e₁]`: `x ∉ fv(λy. e₁)`, the value is the same, and (Fix) of §3.6 checks it against `σ`, like the source program (`flick-model/src/eval/mod.rs`). Test: `a_function_keeps_its_signature` (`flick-model/tests/props.rs`). With regions, `σ` there is the local signature with its own qualifier inferred ([Fix keeps the qualifier of a local signature](regions.md#fix-keeps-the-qualifier-of-a-local-signature)).

### A handler that waited for its body to unwind was typable and stuck

- Divergence: a first design kept the value of an `abort` arm, or of a `control` arm that let the body go, at the handler, while the body unwound with a bare signal `unw p`. A signal of the handler `p` typed anywhere, as it has any type, could pass the frame that waited for it inside another handler of the same effect, and a state typed by the rules did not step.
- Resolution (model): the signal holds what the handler runs, `unw p ℓ a`, and the handler `hwith p ℓ` with the cell `ℓ` catches it, as the signal of the Go runtime holds the arm. The cell types `a` at the type of the handler's value, and `Caught` shows that a signal passes no frame of its own handler. Lean: `RHead.hwithCatch`, `Step.unw` (`Semantics.lean`); `Caught.lean`, `CaughtStep.lean`.

### What a resumed body gives back enters the scopes of the arm

- Divergence: `resume u` inside a `scope` the arm opened gives that scope the value of the resumed body, and its error when it raises. Values move into a deeper place, which `⊢ Λ` allows only for what names no region of the deeper places.
- Resolution (model): the type and the errors of a handler's value are fixed when the handler is installed, and name only regions older than the handler; the scopes of an arm are younger. `⊢ Λ` records it of the cell of the handler (`HCell`), and the step moves the value by its type. Lean: `HCell`, `Tm.Within` (`Within.lean`); `old_move`, `Nest.hresVal` (`NestHdlPure.lean`, `NestHdlRt.lean`).

### Outside the core

#### A service operation with a class constraint loses its dictionary

- Divergence: with `service Render = render : Show a => a ->{} String`, the call `Render.render 42` compiled to `$rt.operation($s_Render, "render", 1)(42)`, without the `Show` dictionary. Cause: `services.rs` `operation_type` dropped the constraints of the scheme.
- Resolution (checker and code generator): `operation_type` requests the dictionaries of the constraints, like `instantiate`. The IR produces `Dicts(Operation)`, and the code generator passes the dictionaries to the runtime: `$rt.operation(tag, name, arity, [d…])` in JS, `flickrt.OperationGiven(key, name, arity, d…)` in Go. The runtime applies them to the implementation after the service lookup. Fixture: e2e `tests/run/codegen/service_dictionary.fl`, on JS and Go.

#### An implementation field with a class constraint cannot be called directly

- Divergence: `render_impl.render 7` with `render_impl : Render.Impl` was `FLICK-T003` "this is not a function: it has type `forall a. Show a => a ->{} String`". `records.rs` `field` returned the field type without instantiating `∀`. This was incompleteness, not a soundness hole: the program was rejected.
- Resolution (checker): `records.rs` `field` instantiates the field's scheme, as `instantiate` does for a name, and requests the dictionaries of its constraints; the IR wraps the field in `Dicts`, and the code generator passes them as for any value with dictionaries. Fixture: e2e `tests/run/codegen/service_dictionary.fl` (`render_impl.render 7`, `render_impl.pair`), on JS and Go.
