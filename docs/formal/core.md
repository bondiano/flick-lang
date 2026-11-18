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
| Regions, qualifiers `T^c` typed by use, `scope`, `Live` | `regions.md` | Core types are checker types with the `Captured`/`Bounded` qualifiers and the `Live` row elements erased. The region rules (`regions/live.rs`, `live_rows.rs`) take no part in the rules. Lean's core (`formal/`) has them, a value bound as a fact of its scheme ([value bounds in the core](regions.md#value-bounds-in-the-core)) |
| Classes, dictionaries, `do`, `for`, `?` | `elaboration.md` | The core receives a program with dictionaries already inserted; schemes carry no constraints |
| `type module`, parameterized modules, `Module Sig`, `exists K : Sig. T` | source checker and lowering | Static applications become named modules; packed modules and existential witnesses become records. The core does not prove the source rules for generativity, nominal identity, or existential opening |
| Fibers, `parallel`, `race`, `spawn`, Cause, interruption, layer memoization | `runtime.md` (λRT) | The core has one thread of execution; the "interrupted" outcome appears only in λRT |
| The Sync stage | — | For typing this is the runtime arrow `R⟨ρ; ε⟩`. Intrinsic UI Sync rows may use a synchronous calling convention; ordinary runtime arrows retain λRT interruption points even without `Async`. Asynchronous waiting requires `Async` in `ρ`; scheduler preemption does not. Sync is implementation metadata, not a separate core stage |
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

ρ ::= α | α − N̄ | none | {a₁, …, aₙ} | {a₁, …, aₙ | ρ} | ρ + ρ

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

The checker also takes a type variable as an element of an error row: in `{t | e}` the variable `t` stands for one error, as `Rpc.error_case : … -> ErrorCases e -> ErrorCases {t | e}` uses it. The core has no such element, and the model rejects what needs it (`flick-model/tests/known_gaps.txt`).

**Row equality ≡** (SRow, ERow):

- `+` is associative and commutative, with unit `none`;
- `{ā | ρ} ≡ {ā} + ρ` and `ρ + ρ ≡ ρ`;
- an element is identified by its name `Module.Name`, and a row has at most one element with a given name. `{N τ̄} + {N τ̄'}` is well-formed only when `τ̄ = τ̄'`, and then equals `{N τ̄}`.

**A variable without names** (SRow, ERow). `α − N̄` is the row `α` stands for less the elements named `N̄`; the language writes it `{-N | α}`. Only a name whose element takes no arguments is taken out of a variable: the variable may hold the element at other arguments, and then nothing says which one is meant (`FLICK-T002`).

- a substitution takes the names out of what it puts there: `(α − N̄)[α := ρ]` is `ρ` without its elements named in `N̄`, and with each of its variables `β − M̄` as `β − (N̄ ∪ M̄)`;
- `(α − N̄) + (α − M̄) ≡ α − (N̄ ∩ M̄)`, so `α + (α − N̄) ≡ α`;
- `{N} + (α − N) ≡ {N} + α`: beside `N` the variable has it back.

The normal form of a row is a pair: a set of elements keyed by name and a set of variables, each with the set of names it stands without, none of which the row names itself.

**Inclusion:** `ρ ⊆ ρ'` ⇔ `ρ' ≡ ρ + ρ''` for some `ρ''`. On normal forms the elements of `ρ` are elements of `ρ'`, and a variable `α − N̄` of `ρ` is a variable `α − M̄` of `ρ'` with `M̄ ⊆ N̄`. So `α − N ⊆ α` and `α ⊆ {N | α − N}`, while `α ⊄ α − N`: the variable may hold `N`.

With `ε_b = α` and `H = {E}`, the least remainder `ε'` of (Try) is `α − E`, and so is the row a handler of `E` leaves its surroundings in (Handle-E) when its body needs `α`: `provide` gives a service to a body it knows only by a row variable.

A field row works differently. Labels are unique, order does not matter, and a repeated label is an error (`FLICK-T014`), not a merge. There is no inclusion, only equality: there is no width subtyping.

Checker:

- `unify.rs` `Store::normalize`: the normal form; `Type::Without` is a variable without names;
- `effects/solve.rs` `owed`: what a variable may hold that a row takes out of it and does not name;

Model: `Row.less` (`flick-model/src/types.rs`), with `Row::within` and `Row::add_less` for inclusion and substitution. Lean: a row variable is `Elem.rv a G`, the variable `a` without the names `G`; `Row.less` and `Row.substT` are the substitution, and `Row.back` is the third equality, read by the pointwise `Row.Sub` in the premises of `Typ.tryR` and `Typ.hdl`. A frame of a context keeps a row variable of its hole up to the names it stands without (`Row.modNames` in `FRel`, and in `Typ.handle`), since an element with a name is found by the name. `formal/Formal/Examples.lean` has the three facts above about `α` and `α − N`.

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
    | lazy e | force e
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

`task e` is a value for any `e`. A computation in the core is itself a value: its body is not evaluated until `await`. `lazy e` is not a value: it makes a lazy cell, and its value is the location of the cell (§3.4). `force e` forces it; the surface `await l` with `l : Lazy ρ ε τ` is `force l` ([`lazy.md`](lazy.md) §2). Runtime-only forms (`fix`, locations, `env`, `with`, `handle`, `settle`) are described in §3.1.

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
| `x \|> f`, `f <\| x`, operators | `f x`, constants |
| `f >> g` | `let f′ = f in let g′ = g in λx. g′ (f′ x)`, with fresh `f′`, `g′` of monomorphic types, as λ binds them; a chain `f >> g >> h` is one `λx. h′ (g′ (f′ x))` |
| `g << f` | `let g′ = g in let f′ = f in λx. g′ (f′ x)`: the operands bound left to right, as written |

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

With two or more elements, or with a row variable, `Errors ρ` has no other type it equals. A term of one of its errors has the type where `Errors ρ` is expected, (Errors-In) of §2.4; the `x`/`_` branch of a `try` binds one. Its values flow into `raise`, into generic code, or into a `match` that takes them apart by the errors of `ρ`, (Match-Err) of §2.4 (see [`Errors` of a row with two or more elements](#errors-of-a-row-with-two-or-more-elements)).

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

There are no other rules. `Ref`, `List`, records and ADTs are invariant; inside them types are compared by `≡` (see [only arrows and `Eff` have subtyping](#only-arrows-and-eff-have-subtyping)). The checker also subsumes a `Layer` by its rows, which the core does not (see [a layer that needs less fits where more is allowed](#a-layer-that-needs-less-fits-where-more-is-allowed)).

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

With no such `θ` the branch is dead: no value of `T ῡ` is built by `K`, and its body is not typed. The checker refines ordinary type indices by substitution and retains row equations as branch-local assumptions. Inclusions must hold for every solution of those equations; it does not choose a least residual or require one most general unifier. The rule over every unifier is the primary one, because the most general unifier is not kept by type substitution ([the most general unifier is not kept by substitution](#the-most-general-unifier-is-not-kept-by-substitution)).

- `Exh(T ῡ, Π)` counts only the constructors that can build `T ῡ`, those whose `T ρ̄` unifies with `T ῡ` under some instance, regions included. The parts of a value share its rigid variables: what a constructor fixes in one part holds in the others, so `Exh((Tagged a, Tagged a), Π)` counts no pair of different constructors (`types/gadt/columns.rs` `columns_after`). This deadness differs on purpose from that of a branch, which keeps the regions the body writes. A constructor left out of `Exh` builds no value of the type at all. A dead branch has to stay dead under the substitutions of the regions around it (scopes, `kill`), which may move the regions it does not write.

A `match` on a value of `Errors ε` takes it apart by the errors of `ε`. The type of a pattern comes from its head, as in (Try):

```text
Γ ⊢ e : Errors ε ! φ
for every branch i:   pᵢ = Eᵢ p̄ with Eᵢ ῡᵢ ∈ ε and ⊢ pᵢ : Eᵢ ῡᵢ ⇒ Δᵢ,   or pᵢ ∈ {x, _} with ⊢ pᵢ : Errors ε ⇒ Δᵢ
                      Γ, Δᵢ ⊢ gᵢ : Bool ! ψᵢ     Γ, Δᵢ ⊢ eᵢ : τ ! φᵢ
for every E ῡ ∈ ε:    Exh(E ῡ, {pᵢ = E p̄ | branch i has no when}), or a branch x or _ has no when
──────────────────────────────────────────────────────────────── (Match-Err)
Γ ⊢ match e with p₁ [when g₁] → e₁ | … : τ ! φ ⊔ ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

- The match is pure and removes no error from a row: it is not `try (raise e)`. A branch `E p̄` matches every value of the error `E`, whatever its arguments.
- The constructors of the column are the elements of `ε`. A row with a variable may stand for more errors, so only a branch `x` or `_` covers it, as for an opaque type; `Exh` and `Covers` need no premise that `ε` is closed.
- Inference: a row `ε` not known yet, as for the `x` of a `try` (whose row the checker leaves open, `raised ⊆ caught`), holds every error a pattern at its place is headed by, at the top of a branch or inside it, as `Err (A _)` of a `Result (Errors ε) τ`. Each such pattern adds its error to the row, which stays open behind it. After the branches, the row stays open if the branches without `when` cover the scrutinee with it open, and is closed to the errors the patterns named otherwise. That is the most general solution: with a branch `x` or `_` the row holds those errors and any others the rest of the program needs (`raised ⊆ caught`); without one, `Exh` needs the row closed, and the errors named are its only solution.
- An element of a closed `ε` without a branch is `FLICK-T009`, whose fix adds the missing branches (an error of another module by its qualified name); a branch of an error outside a closed `ε` is `FLICK-T003`.

A term of an error is a term of the errors of every row that holds the error, (Errors-In), which (Match-Err) takes apart:

```text
Γ ⊢ e : E ῡ ! φ    E ῡ ∈ ε
──────────────────────────── (Errors-In)
Γ ⊢ e : Errors ε ! φ
```

- The rule types any term, as (Sub) does, and changes no value: an error value carries its constructor, so `A 1` of `Errors {A, B}` is the value `A 1`.
- It is not a subtyping `E ῡ ≤ Errors ε`, so it does not reach under a type: a function `τ → E ῡ` is no `τ → Errors ε` and `List (E ῡ)` no `List (Errors ε)`. A λ checked against `τ → Errors ε` has the rule at its body, `fun x -> f x`.
- Checker: where a term is checked against `Errors ε` and its own type is an error `E ῡ` that `ε` names, the type is the element `E ῡ` of `ε` (`types/error_match.rs` `errors_in`, from `report.rs` `expect`). An error a row does not name is `FLICK-T003`, as is one of a closed row with two or more elements. A row not known yet takes the error by the projection `Errors {E ῡ} ≡ E ῡ`, the least row of (Errors-In) (see [(Errors-In) against a row not known yet](#errors-in-against-a-row-not-known-yet)).

Lean covers errors without parameters, as `HPat.ctor`: `PatTy.err` types `E p̄` at `Errors ε` from `E p̄` at `E` and `E ∈ ε`; the shapes of the values of `Errors ε` are `Shp.err`, an error whose name is in `ε`, and `Shp.errOpen`, any value when `ε` has a variable (`formal/Formal/Typing.lean`). `Ty.Opaque` no longer holds of `Errors ε`. `Typ.shape` gives a value of `Errors ε` its `Shp.err` (`Shapes.lean`), `PatTy.typ_match` types a match by `PatTy.err_match`, which `HPat.inst` shares (`Patterns.lean`), `Shp.of_substT` and `PatTy.substT` keep both under substitution (`SubstPat.lean`), and `match_progress` and the match step of preservation (`PresPure.lean`) need no change: progress reads the shape, preservation `PatTy.inst`. T2b is unchanged, since the match adds no error. `exh_errs` (`formal/FormalCert/Match.lean`) is `Exh` of a closed row covered by its errors, which the certificates of generated programs use.

(Errors-In) is `Typ.errVal` (`formal/Formal/Rules.lean`), which no longer asks for a value. As for (Conv), the steps of progress and preservation go through it to the rule below, and an inversion of a rule whose result is compared with the type it gives (`LeLive t T`) gives `LeOut t T` instead: `t` below a type that is `T` or an error of the row of `T = Errors ε` (`formal/Formal/ErrorsIn.lean`); a step builds the rule again at that type and applies (Errors-In) after it (`LeOut.typ`). L3 keeps it on the frame (`FrameTy.errVal`), and a runtime step takes the type of the error fixed by `kill` from that of its row (`Ty.YFMod.of_err`, `FreshEquivalence.lean`).

Checker: `types/error_match.rs` (`error_element` for `constructor_pattern`, `close_error_rows` for `infer.rs` `match_expr`, `row_constructors` for `exhaust.rs` `constructors`, `error_column` for `gadt/columns.rs`, `errors_in` for `report.rs` `expect`), `exhaust.rs` `show_pat` with `written_constructor`. Model: `check/match_errors.rs`, `check/pattern.rs` `signature` and `con_fields`, `check/expr/inference.rs` `check`; the generator writes such a `match` in the `x` branch of a `try`, with a branch of an error the body does not raise, and a term of one error at a row of several (`generate/handle.rs` `match_errors`); a certificate writes (Errors-In) as `.errVal` (`cert/term.rs`).

Row equations learned in one part also constrain later parts during `Exh`.
Every constructor occurrence has its own existential residuals; equal names
in two constructor schemes do not identify their witnesses. For example,
`r = {Db | r}` excludes a later constructor requiring `r = none`, while
two independent hidden rows still permit cross cases.

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
- `generalize.rs` `kind_of`: a quantified variable takes the kind of where it occurs, an argument of a type the kind of the type's parameter (`Server.Function (r : ServiceRow)` makes `r` a service row). Before, a row argument of a type other than `Eff` and `Layer` was quantified at kind `Type` (`flick-core/tests/features.rs`).
- The model (`flick-model/src/check/generalize.rs`) quantifies type metas and the row metas a type takes, as in `Errors e`; a row only of a stage is closed, as in item 5.

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
- a root environment `Σ₀`: for every `S` in `r₀` there is `Σ₀(S)` with `· ⊢ Σ₀(S) : S.Impl ! P`. The runner or the launcher assembles it from the `Host` services `main` names and those its runner adds (G3).

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
e ::= … | handle v with ā | settle ℓ e
```

A location `ℓ` is a `Ref` cell or a lazy cell; `settle ℓ e` is the frame of the lazy cell `ℓ` while its initializer `e` runs.

### 3.2 Contexts

Evaluation is strict, left to right, and respects every currying stage (spec/01, spec/02):

```text
E ::= □ | E e | v E
    | let x = E in e | let x : σ = E in e
    | match E with ā
    | {ℓ₁ = v₁, …, ℓᵢ = E, ℓᵢ₊₁ = eᵢ₊₁, …} | E.ℓ | {E with ℓ = e} | {v with ℓ = E}
    | await E | force E | settle ℓ E | try E with ā
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

Lazy cells ([`lazy.md`](lazy.md) §3). A cell holds `Pending e`, `Running e` or `Done o`, with `o ::= ok v | err v | defect`; the term of `o` is `v`, `raise v` or `todo`:

```text
⟨μ, E[lazy e]⟩                         →  ⟨μ[ℓ ↦ Pending e], E[ℓ]⟩          ℓ ∉ dom μ
⟨μ, E[force ℓ]⟩, μ(ℓ) = Pending e      →  ⟨μ[ℓ ↦ Running e], E[settle ℓ e]⟩
⟨μ, E[force ℓ]⟩, μ(ℓ) = Done o         →  ⟨μ, E[o]⟩                        o the term of the outcome
⟨μ, E[force ℓ]⟩, μ(ℓ) is neither       →  die                              a running cell: forced by its own initializer
⟨μ, E[settle ℓ v]⟩                     →  ⟨μ[ℓ ↦ Done (ok v)], E[v]⟩        if μ(ℓ) = Running e
⟨μ, E[settle ℓ (raise v)]⟩             →  ⟨μ[ℓ ↦ Done (err v)], E[raise v]⟩ if μ(ℓ) = Running e
⟨μ, E[settle ℓ (unw p ℓ' a)]⟩          →  ⟨μ[ℓ ↦ Pending e], E[unw p ℓ' a]⟩ if μ(ℓ) = Running e
⟨μ, E[Ref.get ℓ]⟩, ℓ a lazy cell       →  die
```

- `settle ℓ □` is transparent to operations. A raised error leaves it by its own step, which records it; a signal of a handler (§3.7) passes it and makes the cell `Pending e` again, so the next force runs `e` again. A defect ends the thread, so `settle ℓ die` is the step of `todo` through the frame: the cell's `Done defect` is not observed in one thread.
- The outcome, or the reset, is written only into a running cell, which every `settle ℓ` of a program finds; with the condition the laws L1 and L2 of [`lazy.md`](lazy.md) hold of every typed configuration, not only of the reachable ones (§4).
- A force of a cell that holds neither `Pending` nor `Done`, and `Ref.get` of a lazy cell, are defects. Of these only a force of a running cell is reachable: a force by the initializer of its own cell. The others keep progress free of the typing of the store and preservation free of a premise on (Loc), which types the location of a lazy cell as `Ref (Cell ρ ε τ)` too.

Lean: `RHead.lazy`, `.forcePending`, `.forceDone`, `.forceRunning`, `.settleVal`, `.settleRaise`, `.settleUnw`, `.refGetCell`, `settleCell`, `resetCell` (`formal/Formal/Semantics.lean`). The store holds a state as a term that is no value (`Tm.IsCell`): `Pending e` is `lazy e`, `Running e` is `force e`, `Done o` is `await o`.

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

`⊢ μ : Ψ` ⇔ `dom μ = dom Ψ` and, for every `ℓ`, `·;Ψ ⊢ μ(ℓ) : Ψ(ℓ) ! P`, or `Ψ(ℓ) = Cell ρ ε τ` and `μ(ℓ)` is a state of a lazy of `Lazy ρ ε τ`: `Pending e` or `Running e` with `·;Ψ ⊢ e : τ ! ψ` and `ψ ⊑ R⟨ρ;ε⟩`, `Done (ok v)` with `v : τ`, `Done (err v)` with `v : Errors ε`, or `Done defect`. `Cell ρ ε τ` is a type of the store only: no value has it, so no `Ref` cell has it and `Ref.set` never writes a lazy cell.

The rules of §2 are read with `Ψ` in the context. In addition:

```text
Ψ(ℓ) = τ                               σ = ∀ᾱ. τ₁ →φ τ₂    Γ, ᾱ, f : σ°, x : τ₁ ⊢ e : τ₂ ! φ
───────────────────── (Loc)            ──────────────────────────────────────────────── (Fix)
Γ;Ψ ⊢ ℓ : Ref τ ! P                     Γ ⊢ fix f. λx. e : σ

Γ ⊢ v : Errors ε_b ! P    v = E w̄    branches ā as in (Try)    E ∈ H + ε'
────────────────────────────────────────────────────────────────── (Handle)
Γ ⊢ handle v with ā : τ ! R⟨none; ε'⟩ ⊔ ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

```text
Ψ(ℓ) = Cell ρ ε τ                     Ψ(ℓ) = Cell ρ ε τ    Γ ⊢ e : τ ! ψ    ψ ⊑ R⟨ρ;ε⟩ ⊑ φ
───────────────────────── (LazyLoc)    ──────────────────────────────────────────────── (Settle)
Γ;Ψ ⊢ ℓ : Lazy ρ ε τ ! P               Γ ⊢ settle ℓ e : τ ! φ
```

`R⟨ρ;ε⟩` is read with the hidden qualifier of the lazy, as for (Await). A cell in `Running e` keeps the typing of its initializer, which a reset gives back to `Pending e`. Lean: `Typ.lazyLoc`, `Typ.settle` (`formal/Formal/Rules.lean`), `Ty.lazyCell`, `CellTy`, `StoreTy` (`formal/Formal/StoreTy.lean`); (T-Lazy) and (T-Force) of [`lazy.md`](lazy.md) §2 are `Typ.lazy` and `Typ.force`.

(Handle) does not require the whole row `ε_b ⊆ H + ε'`, only the error of `v` itself: after a failed branch the remainder has a smaller `H` (see [(Handle) after a failed branch](#handle-required-the-whole-error-row-after-a-failed-branch)). Coming from `try (raise v)`, the premise follows from `ε_b ⊆ H + ε'` of (Try).

(Errors-In) of §2.4 types the error values of a configuration too: `E w̄` is a value of `Errors ε` for every `ε` that holds `E ῡ`.

The remainder of a `match` after a false guard may not cover the type of the scrutinee. For `match Some 1 with | None → a | Some x when p → b | Some _ → c` the remainder `match Some 1 with | Some _ → c` fails `Exh(Option Int, …)`. For a value it is enough to cover the value itself (see [the residual match](#the-residual-match-after-a-false-guard-fails-exh)):

```text
Γ ⊢ v : τ ! P    ⊢ pᵢ : τ ⇒ Δᵢ, gᵢ and eᵢ as in (Match)    v matches some pⱼ without when
───────────────────────────────────────────────────────────────── (Match-Val)
Γ ⊢ match v with p₁ [when g₁] → e₁ | … : τ' ! ⊔ᵢ (ψᵢ ⊔ φᵢ)
```

(Match-Val) does not apply to source programs: `match 1 with | 1 → …` is still rejected.

The flick-core checker never sees runtime forms. The reference checker types them in `flick-model/src/check/runtime.rs` (`check_config`), and (Match-Val) in `handlers.rs` `match_`, only when checking a configuration. One more reading holds there only, for (Match-Err) of §2.4: once the arm of a `try` has run, the scrutinee of its `match` is the error raised, a value `E w̄` of its error `E`, which (Errors-In) types at the row of `E` and the errors the arms name (`check/match_errors.rs` `errors_of_arms`). The `x` of an arm takes a row above the body's in a program as in a configuration (`caught_row`): once the body has stepped past what it raised, (Try) still types it at the row it had, as (Sub) lets it.

### 3.7 Handlers of effects

An effect `E` declares operations `o : a → b` of a class `c`: `tail` (the arm's value is the operation's), `abort` (the arm's value is the handler's, the body does not come back) or `control` (the arm's value is the handler's, with the rest of the body as `resume`, at most once). An effect has `n` type parameters, as a service: `a` and `b` are over them, and `E ū` with `|ū| = n` is an instance. `perform_{E.o}` is a constant; `hdl E al r e` handles `E` in `e`, `al` the arms, `r` the function of the body's value. A handler's arms for errors are a `try` around it: errors pass `hdl`.

```text
S.op E o = (c, a, b)    |ū| = n
────────────────────────────────────────────────────────────── (Perform)
Γ ⊢ perform_{E.o} : a[ū] →R⟨{E ū};∅⟩ (c = abort ? τ : b[ū]) ! φ

Γ↑, ϱ ⊢ al : arms E ū (τ' at R⟨r;ε⟩, resume over ϱ)    Γ ⊢ r : τ →ψ τ' ! R⟨r;ε⟩    ψ ⊑ R⟨r;ε⟩
Γ ⊢ e : τ ! φ₀    φ₀ ⊑ R⟨{E ū} ∪ r; ε⟩    no {Provided {E} _} or {Provided v _} in φ₀
E in φ₀ only at ū, and each row variable of φ₀ without E, unless n = 0
E ū ∈ r unless every operation of E has an arm without a guard
──────────────────────────────────────────────────────────────────────────────────────────── (Handle-E)
Γ ⊢ hdl E al r e : τ' ! R⟨r;ε⟩
```

The arms are typed at `ū`: the arm of a `tail` operation has `x : a[ū]` and the type `b[ū]`; of `abort`, `x : a[ū]` and `τ'`; of `control`, `x : a[ū]`, `resume : b[ū] →R⟨r;ε⟩ τ'` holding `ϱ`, and `τ'`. `ϱ` is a region variable of the handler, as the region of a `scope`: `resume` stays in the arms. A handler answers by name at run time, whatever the arguments of the operation, so its body may perform `E` only at the arguments its arms are typed for: at `ū`, or at any when `E` has no parameters; a row variable of the body is taken without `E`, so no other instance comes in through it after a substitution. An operation without an arm goes on to the handler around, at `ū`. Whether the handler has an arm for every operation does not depend on `ū`, as `E` has the same operations at every argument (`Sig.Complete`).

Run-time forms. Installing a handler opens the region `p = |Λ|` and allocates the cell `ℓ = |μ|`, whose type `E ū →R⟨r;ε⟩ τ'` holds the instance the handler answers, its stage and the type of its value:

- `hwith p ℓ E al r e`, inside `inside p`: the handler running its body;
- `hctl p ℓ κ tl E al r a k`: the arm `a` runs while the body waits, parked at `hole p` in `k`; for `control`, `resume` is `kont p κ` with a new cell `κ` of its type;
- `hres p ℓ a b`: the resumed body `b` runs under its handler while the arm waits, parked in `a`; `hdone p a`: the arm after its `resume` finished;
- `unw p ℓ a`: unwinding to the handler `p`, which then runs `a`; it has any type, `a` the type of the handler's value.

Steps. An operation runs the arm of the innermost handler with one for it, the frames between not handling it: `abort` puts `unw p ℓ arm[v]` at the operation; `tail` and `control` park the body and run the arm at the handler. The value of a `tail` arm goes back into the body; a `control` arm that ends without resuming, or an arm that raises or unwinds, unwinds the parked body to the handler. `resume u` puts `u` into the body under its handler, once; a second `resume` dies. A signal passes every frame but `inside`, which closes first, and the frames of an arm, which let their parked part go; the handler with the cell of the signal catches it. A handler whose body has a value applies `r` to it.

A child or a finalizer keeps the handlers of the place that registered it, those older than its owner: `hkeep q ℓ E al e` around its work.

```text
Ψ(ℓ) = E ū →R⟨r;ε⟩ τ'    Γ ⊢ al : arms E ū (τ' at R⟨r;ε⟩, resume over q)    Γ ⊢ e : τ ! φ₀
φ₀ ⊑ R⟨E ū ∪ r₁; ε₁⟩ as in (Handle-E), its {Provided {E} a} with q ≥ a    R⟨r;ε⟩ ⊑ R⟨r₁;ε₁⟩ ⊑ φ
E ū ∈ r₁ if an operation of E has no arm without a guard
──────────────────────────────────────────────────────────────────────── (Keep)
Γ ⊢ hkeep q ℓ E al e : τ ! φ
```

A kept handler is the nearest handler of every operation of `E` it has an arm for, as the handler it keeps was. It runs the arm of a `tail` operation while the region `q` is live: the arm runs at the frame, while the rest of the child waits, as `hkarm q ℓ E al a k`, and its value goes back to the operation. An `abort` or `control` operation with an arm is stuck at the frame: its arm would unwind to, or capture up to, the handler in the thread that installed it, which the child is not. So such an operation leaves the row of the work as one the handler answers does, and (Keep) keeps `E` in `r₁` only for an operation without an arm, which goes on to the handler around or the root. The checker rejects `control` there (`classes/detached.rs`, `E017`), not `abort`: T1 lists an `abort` operation stuck at a kept handler as an outcome of a child, where Lean has it (`KeptStuck`). Only a child or a finalizer has a kept handler: `spawn` and `addFin` wrap the work and register it with a region, and no step moves it into the main thread (`steps_kept`). A kept handler runs no `return`: a value passes it. It catches no signal: an arm that raises lets the rest of the child go, closing its regions, and the child ends with the signal, as it would with the error.

Statements (Lean): Progress and Preservation cover every step above; T2b holds with the row `{E}` of an operation removed by its handler; the main thread is never a signal at its top (`Caught`); the regions of a handler nest as those of a scope, and what a resumed body gives back names no region younger than its handler (see [what a resumed body gives back enters the scopes of the arm](#what-a-resumed-body-gives-back-enters-the-scopes-of-the-arm)). A spawned task does not reach the handlers around the place that spawned it: an operation only they handle is stuck in the task, as an `ask_S` that nothing answers. T1 has the main thread a value, a raised error, a step or blocked, and a child of a live region also finished or stuck at a kept handler on an operation that is not `tail`.

## 4. Theorems

Hypotheses:

- **G1 (FFI).** A `foreign pure` call is a `→ₚ` step: it does not change `μ`, does not look up services, does not produce `raise`, but may produce `die`. A `foreign` call with a runtime arrow stays within that arrow's rows. Values:
  - a value the binding returns (and one the foreign code passes to a callback) has the declared type; `Never` in the result means the call does not return;
  - a trusted binding (`@flick-lang/runtime`) does not modify or retain the Flick data it receives. An untrusted one receives and returns them as copies, so for it the code generator ensures this ([Flick data copied at the foreign boundary](ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary)).

  What else at the TypeScript boundary rests on this hypothesis is listed in `ts-holes.md` §3.
- **G2 (code generation).** The generated JS and Go implement the steps of §3, and the `Lift` node in them is the identity of the model. This is checked by differential tests, not by proof.
- **G3 (runner).** `Σ₀` covers `r₀`; `Async ∈ dom Σ₀`, which the scheduler handles. The runner or the launcher builds the rest of `Σ₀` from the live `Host` layers, at least for the `Host` services `main` names (`Host.Live` for `Runner.Node` and the Go target, `Host.BrowserLive` for `Runner.Browser` and `Runner.Cloudflare`), and from the services its runner's `Main` adds, such as `Env` of `Runner.Cloudflare`; `FLICK-P026` rejects a `main` whose `r₀` needs any other. The live layers are trusted adapters over `Host.raw_*`: that they implement `S.Impl` is assumed, not proved. The model builds `Σ₀` from `Host.live` on a fixed host (`flick-model/src/native/host.rs`).

Lemmas:

- **L1 (canonical forms).** Let `·;Ψ ⊢ v : τ ! φ`. Then:
  - for `τ = τ₁ →φ' τ₂`, `v` is a λ, a `fix`, or a partial application of a constant or a constructor (see [partial constructor application](#a-partial-constructor-application-is-a-value));
  - for `Eff r ε τ'`, `task e`;
  - for `Ref τ'`, `ℓ` with `Ψ(ℓ) = τ'`;
  - for `Lazy ρ ε τ'`, `ℓ` with `Ψ(ℓ) = Cell ρ ε τ'`;
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
  - `force v`: by L1, `v = ℓ` with `ℓ ∈ dom μ`; one of the steps of a force applies to every content of the cell.
- **Preservation.** Let `·;Ψ ⊢ e : τ ! φ`, `⊢ μ : Ψ` and `⟨μ, e⟩ → ⟨μ', e'⟩`. Then there is `Ψ' ⊇ Ψ` with `⊢ μ' : Ψ'` and `·;Ψ' ⊢ e' : τ ! φ'`, where `φ' ⊑ φ`.
  - `Ψ'` differs from `Ψ` only after a step that allocates: after `Ref.make`, `Ψ' = Ψ[ℓ ↦ τ']` with a monotype `τ'` (see [store typing](#a-store-typing-of-closed-types-is-not-preserved)); after `lazy e`, `Ψ' = Ψ[ℓ ↦ Cell ρ ε τ]`.
  - The steps of a lazy cell: the initializer of a pending cell is typed in the store as the body of a `task`, so `settle ℓ e` is typed at the stage of the force; a settled value has `τ`, and a raised error has a row within `ε`, since the stage of the initializer is within `R⟨ρ;ε⟩`, so the cell keeps a state of its type.
  - The `let` case with generalization is treated in T3.
  - The case `try (raise v)`: the value `v` came from the body with `v : Errors ε'`, `ε' ⊆ ε_b`, and (Errors-In) gives `v : Errors ε_b` for (Handle).
  - The false-guard case: the remainder `match v with ā` is typed by (Match-Val).

Progress and Preservation are checked on the reference checker by the property `progress_and_preservation_hold` (`flick-model/tests/model/props.rs`, part of the model properties, `just model`). Every machine state, read as `⟨μ, E[e]⟩`, is retyped with `φ' ⊑ φ₀`; the run does not get stuck, and a final `raise v` is an error from `ε₀`. Programs with regions run the same way, with state `⟨Λ, μ, E[e]⟩` (`regions.md` §3.3). Programs with classes too (§1.6): methods of `Describe` and `Rank` of `flick-model/src/generate/Gen.fl` at the types they have instances at, through the instance of `Option` and the superclass of `Rank`, and local functions with a class constraint, bound with their signature; the source writes no dictionary and annotates the argument of a method, and `checkers_agree` also changes a program into a method without an instance, a signature without its constraint, or an argument without its type, which flick-core may find ambiguous (`FLICK-C002`, `generate::by_design`). The module of `Gen.fl` is `Fixture`, not `Gen`, which would shadow the module of std. That name found a checker bug: a project module named like a module of std exported no instances, since `own_instances` (`env_impl.rs`) left out those of the built-in modules by name, while `learn` already keeps a built-in instance out by its key (`flick-core/src/project/tests.rs` `a_module_named_as_one_of_std_*`).

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

**The laws of a lazy cell** ([`lazy.md`](lazy.md) §3, one thread):

- **L1 (once per attempt).** On any path, the starts and the resets of a cell alternate, so between two resets the initializer starts at most once: only the force of a pending cell starts it, and only a reset makes a cell pending again, a signal leaving `settle ℓ` of the running cell. Lean: `Lazy.L1`, `Lazy.L1_once`, `StartsN`, `Starts`, `Resets`, `Step.pending` (`formal/Formal/Lazy.lean`).
- **L2 (final).** On any path from a typed configuration, a settled cell keeps its outcome, and a force of it in a later configuration steps to the term of the outcome without changing the store. Lean: `Lazy.L2`, `Step.done_stays`, `Step.force_done` (`formal/Formal/Lazy.lean`). A `Ref.set` cannot write the cell, whose type no value has, and a `settle ℓ` writes only a running cell.

These are distinct from the lemmas L1–L3 above.

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
- Checker: a projection existed only for `none` and for a single element. `match e with | A _ -> … | B _ -> …` with `e : Errors {A, B}` was `FLICK-T003`, so handlers translating several errors repeated `try` branches over `from_result (Err e)`.
- Resolution (spec, checker, model, Lean): the pattern of an element, (Match-Err) of §2.4, and its introduction, (Errors-In) of §2.4: a term of one error has the type of the errors of a row that holds it, so `Exit.map_error` and `Layer.catch` can give several errors, and a value of one error goes where `Errors {A, B}` is expected. spec/03 "Notation and joining of rows" states both rules. Fixtures: `tests/check/pass/effects/errors.fl`, `tests/check/fail/effects/handlers.fl`, `tests/run/codegen/error_match.fl`, `tests/run/codegen/error_intro.fl`.
- The model typed the `x` of a `try` at the least row of its body, where the checker takes any row above it, as (Try) with (Sub) does: `try raise (A 1) with | e -> match e with | A _ -> … | B _ -> …` checked, and the model reported `B` as outside `{A}`. Resolution (model): the `x` takes the body's row and a variable, which the patterns of a `match` tell and the end of the `try` closes when nothing else does (`check/match_errors.rs` `caught_row`). The generator writes a branch of an error its `try` does not raise. Regressions: `flick-model/src/check/tests/effects_patterns.rs` `a_try_arm_takes_a_row_above_its_body`, `tests/check/pass/effects/errors.fl` `beyond_raised`.
- Not covered: errors with parameters in Lean, which waits for the arguments of named row elements in the core (#433).

### (Errors-In) against a row not known yet

- Spec: (Errors-In) of §2.4 types a term of an error `E` at every row that holds `E`.
- Checker: against `Errors ρ` with `ρ` not known yet and no element, a term of `E` makes `ρ` the row `{E}` (the projection `Errors {E} ≡ E` solved for `ρ`, `unify.rs` `unify_errors`), the least row the rule allows. A later term of another error against the same type is then `FLICK-T003`: `Exit.map_error (fun e -> match e with | X -> A 1 | Y -> B 2) exit` takes `{A}` from the first branch. Keeping `ρ` open there would also leave a row variable in the errors of every `raise` of a single error, whose argument is checked against `Errors ρ` with `ρ` unknown.
- Resolution (spec): none; the row is written where it is not known yet, by a signature of the function or an annotation of the `match`. The model does the same.

### A callee of unknown type is a pure arrow

- Divergence: `(fun h -> h ()) f` with `f : Unit ->{Log} Unit` is `FLICK-T003` "expected `Unit -> _`".
- In the model (App) admits any `φ`, but `λh. h ()` has no principal type: `(Unit →P τ) →P τ` and `(Unit →R⟨r;ε⟩ τ) →R⟨r;ε⟩ τ` are incomparable, and the core has no stage variables.
- Resolution (spec): `reference/effects.md` states that a parameter of unknown type applied to an argument is treated as a pure function; an effectful callback gets its type from the expected type or from a signature.

### Only arrows and `Eff` have subtyping

- Divergence: `List (Eff {Log} none Unit)` does not pass as `List (Eff {Log, Clock} none Unit)` (`FLICK-T003`). The model (§2.2) specifies the same.
- The spec ("A signature gives an upper bound"; `reference/effects.md` "A runtime arrow and an Eff accept a value that needs fewer services and errors") did not say that rows under other constructors must be equal.
- Resolution (spec): stated in `reference/effects.md`. Covariance of immutable constructors needs variance inference, which spec/03 defers.

### A layer that needs less fits where more is allowed

- Divergence: the checker accepts `Layer none none {Log}` where `Layer {Async} none {Log}` is expected (`effects.rs` `subsume_at`, `layer_parts`): the input and error rows are compared by `⊆`, the output row by `≡`. §2.2 has no such rule; the model follows §2.2 and rejects it (`flick-model/tests/known_gaps.txt`, `tests/check/pass/effects/async.fl` `quiet`).
- Sound for the reason (≤-Eff) is: a layer is used only through `provide`, which adds its input row to the stage of the user, and a smaller row asks less there.
- Resolution (recorded): adding (≤-Layer) to §2.2 needs the rule in the model and in the Lean development; until then the checker accepts more than the core.

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
- Resolution (model): in the reference checker the step `let x : σ = λy. e₁ in e₂` gives `e₂[x := fix x. λy. e₁]`: `x ∉ fv(λy. e₁)`, the value is the same, and (Fix) of §3.6 checks it against `σ`, like the source program (`flick-model/src/eval/mod.rs`). Test: `a_function_keeps_its_signature` (`flick-model/tests/model/props.rs`). With regions, `σ` there is the local signature with its own qualifier inferred ([Fix keeps the qualifier of a local signature](regions.md#fix-keeps-the-qualifier-of-a-local-signature)).

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
