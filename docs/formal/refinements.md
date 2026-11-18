# Invariants: proof-required construction and facts of paths

Status: **contract** (RFC #595, #594), implemented: §1–§6 hold in the checker, and the logic of §4–§5 is proved in Lean (`Formal/Refine.lean`, `Formal/Refine/Numbers.lean`). This page fixes what the checker accepts for `T.prove`, what a plugin's invariant must be, and what the guarantee does not cover. Each rule lists where it is checked.

References as in [`core.md`](core.md). Checker paths are relative to `flick-core/src/types/`.

## 0. Scope

| Form | Meaning | Section |
| - | - | - |
| `@invariant$of check value prove` | a block's `unboxed` type whose values its check holds for | §1 |
| `T.prove e` | `e` as a `T`, where the facts of its path prove `T.check e` | §2 |
| a fact | a Boolean expression the path ran, with the result it gave | §3 |
| the logic | what facts establish of the check | §4, §5 |

Out of scope for v1:
- facts across a join;
- facts of loop and `match` conditions;
- arithmetic, string order and `BigInt` facts;
- first-class proof-carrying functions;
- invariants of parameterized types or of non-scalar bases.

## 1. Invariants

A plugin declares the invariant of a type of its own block with `Decl::Invariant { check, value, prove, decl }`. It renders as `@invariant$of check value prove` before the type. Only generated code spells `$`, so no source writes the attribute. `refined type` gives every type it declares one.

**What the compiler checks, rejecting a broken contract.**

| Requirement | Error | Checked by |
| - | - | - |
| the attribute, the type and the `let`s of `T$check` and `T$value` are the code of one block | `FLICK-P036` | `plugin/expansion/private.rs` (`foreign_invariants`) |
| the type is `unboxed`, of one constructor whose name has `$`, with one field `Base` | `FLICK-P049` | `proofs/contract.rs` |
| `let T$value (C v) = v` exactly | `FLICK-P049` | `proofs/contract.rs` |
| no value `T$prove` exists | `FLICK-P049` | `proofs/contract.rs` |
| `T$check : Base -> Bool`, a pure arrow | `FLICK-P049` | `proofs/contract.rs` |

A broken contract gives nothing: there is no `T.prove`, and the constructor is as it was.

**Active contracts.** A contract is **active** when `T` has no parameters and `Base` is `Int`, `Float`, `String` or `Bool`. Only an active contract binds its constructor and gives `T.prove`. For an inactive one, `T.prove` is `FLICK-T077`.

**(C-Intro) the constructor discipline.** Every use of the constructor `C` of an active contract in an expression is the head of an application `C e`. Each such application is an obligation: the facts of its path prove that `T$check e` gives `true` (§4).

- A use that is not an application is `FLICK-T079`. This covers naming it alone, passing it, keeping it in a value, or applying it through a pipe.
- An obligation that is not proved is `FLICK-T077`. One whose opposite is proved is `FLICK-T078`.
- The discipline holds for the owning plugin's code too. `T.parse value = if T.check value then Ok (C value) else …` meets it through the fact `T$check value`.
- Patterns are not restricted.

Checked by: `proofs/mod.rs` (`note_construction`, `discharge_proofs`), `project/load/plugins_tests/invariants.rs`, `flick/tests/wasm/invariant.rs`.

**Invariant.** Within the trust boundary of §7, every value of `T` was made by an application of `C` whose obligation was proved. So `T$check (T$value t)` gives `true` for every `t : T`.

## 2. `T.prove`

```text
(T-Prove)  Γ; Φ ⊢ e : Base    Φ ⊩ T$check ⟦e⟧ = true
           ──────────────────────────────────────────
           Γ; Φ ⊢ T.prove e : T     ⇝  C e
```

`Φ` is the facts of the path (§3), and `⟦e⟧` is the value of `e` (§3).

- `T.prove` resolves through the companion lookup of `qualified.rs`: it is a member of a type with an active contract. A module named `T` still wins, as for any companion, and an ordinary `prove` is ordinary.
- The name records the constructor `C` in `Typing.names`. There is no `let T$prove` and no rewrite of the source.
- Lowering emits the existing `Con` of `C`, which is `Rep::Unboxed`: the identity on JS and Go.
- `e` runs once. The check does not run, and no `Result` is built.
- `T.prove` is subject to (C-Intro): applied to its argument, or `FLICK-T079`.

Checked by: `tests/check/pass/schema/prove.fl`, `tests/check/fail/schema/prove.fl`, the model's `accepted.rs` (it sees `C e`).

## 3. Values and facts

**Values.** A fact is about one of these:
- an immutable local, by the place its binder is written. A shadowing binder is another value; a `let mut` variable is no value.
- `S.value e` of a type `S` with an active contract. `e` is a value, or the one evaluation of `e` where it is no value.
- `let y = x` binds `y` to the value of `x`.

**Facts.** A fact `(φ, b)` says the Boolean expression `φ` ran to `b`. Facts come from:

| Where | Fact |
| - | - |
| the `then` branch of `if c` | `(c, true)` |
| the `else` branch of `if c` | `(c, false)` |
| the right operand of `a && b` | `(a, true)` |
| the right operand of `a \|\| b` | `(a, false)` |
| any place, for each `S.value v` it names | `(S$check (S.value v), true)`, from the invariant |

- A fact lasts as long as its branch. After the join, v1 keeps none.
- The body of a `fun`, a function `let`, a `task`, a lazy sequence or a handler arm starts with no facts (`mutation.rs`, `enter_apart`).
- Obligations are decided once the module is solved (`run_in`, `discharge_proofs`). A comparison is then known to be primitive, and a call is known to take no dictionary.

## 4. The logic

**Formulas.** A formula is `true | false | atom | not φ | φ && ψ | φ || ψ`, in the order the program runs it; anything else is **opaque**.

**Atoms.**
- `f v`, where `f` is a top-level function, keyed by its module and name, and the call takes no dictionary.
- `v`, a `Bool` value.
- `v op c` with `op` one of `<`, `<=`, `>`, `>=` and `c` a finite literal, where the comparison settled to the primitive one of `Int` or `Float` (§5). A literal on the left is flipped: `0 < n` is `n > 0`.
- `v == c` with a literal `c`, an exact atom (`!=` is its negation).
- An atom of constants alone, `8080 > 0`, gives its result by itself, as the program compares.

**Knowledge.** `K = Φ*` is what the facts establish (`proofs/logic.rs`, `Knowledge::of`):
- `(φ && ψ, true)` gives both, true. `(φ || ψ, false)` gives both, false.
- `(φ && ψ, false)` gives `(ψ, false)` once `φ` is known true. It gives `(φ, false)` once `ψ` is known true, since a pure atom gives one result wherever it runs.
- `(φ || ψ, true)` is the dual.
- An atom that is a call of an active contract's check unfolds to the check's summary, with the same result. The summary is the body of a `fun n -> φ`, or `g v` for a check that names a function `g`.
- Learning is bounded by a fixed number of steps. Past it nothing more is learned, so a proof that needed more is unknown.

**Results.** `holds_K(φ)`, from `Knowledge::holds`:

```text
holds(c)      = c
holds(a)      = K(a)                                   -- unknown when K has none
holds(not φ)  = ¬ holds(φ)
holds(φ && ψ) = false if holds(φ) = false;  holds(ψ) if holds(φ) = true;  unknown otherwise
holds(φ || ψ) = true  if holds(φ) = true;   holds(ψ) if holds(φ) = false; unknown otherwise
```

**Definedness.** An atom may be a call that never returns, so `holds` decides a part only after the parts before it, which run first.
- `true || f x` is proved.
- `f x || true` and `f x || not (f x)` are not, for an `f x` nothing observed.
- An invariant `p x || q x` does not prove `q x || p x`.

**Outcomes.** An obligation `T$check v` is:

| Outcome | When | Diagnostic |
| - | - | - |
| **Proven** | `holds(T$check v) = true`, or `holds(summary(v)) = true` | accepted |
| **Refuted** | one of them is `false` | `FLICK-T078` |
| **Unknown** | otherwise | `FLICK-T077`, naming the first part that is not proved, in the order it runs |

Unknown is no claim that the value is invalid. Both diagnostics point to `T.parse`, which checks the value when the program runs.

Checked by: `proofs/logic.rs` unit tests, `proofs/props.rs` (random formulas, facts and worlds against an evaluator of its own), `tests/check/{pass,fail}/schema/prove.fl`.

Lean: `Formal/Refine.lean`. `eval` is the three-valued evaluation, `holds` and `assume` are generic in a `Theory` of atoms, and `learn` runs rounds of facts with any budget. `sound`: in a world where every fact ran to its result, a formula `holds` decides gives that result. `unobserved_or_true` and `or_true_decides_neither` pin definedness. No `sorry`, no new axioms (`Formal/Axioms.lean`).

## 5. Numeric atoms

Comparisons `<`, `<=`, `>`, `>=` of a value with a finite literal, on primitive `Int` or `Float` (`proofs/interval.rs`):

- **Float.** The domain of a value is `{NaN?} ∪ [lo, hi]`, each end strict or not, and ±0 compare equal.
  - `x > c` true removes NaN and raises `lo`.
  - `x > c` false lowers `hi` only, so `not (x <= 0)` does not prove `x > 0`.
- **Int.** The domain is an i32 interval.
- **No arithmetic.** `x + 1` is opaque, so wrapping cannot make a proof.
- **Equality.** `==` and `!=` with a literal are exact atoms, never a substitution.
- **Not primitive.** A comparison that settled to `Ord` (`typing.generic_ordered`) is opaque. `settle.rs` notes each primitive one (`note_compared`).
- **Unbounded.** A value nothing compared has the full interval of its type: an `Int` is within i32 and never NaN.

Checked by: `proofs/interval.rs` unit tests, `proofs/props.rs` (NaN, infinities, signed zeros, the bounds of i32), `tests/check/{pass,fail}/schema/prove_numbers.fl`, `plugins_tests/refined.rs` (the lowered `as_port` applies `Port$Make` and never names `Port$check`), `tests/run/schema/prove.fl` on the model, JS, Go and Flick's runtime.

Lean (`Formal/Refine/Numbers.lean`): `Num` is a total preorder with a point NaN that every ordered comparison finds false, the model of binary64 comparisons and of `Int`'s. `Num.learn_sound` and `Num.decide_sound` are the laws of the interval. `numbers_sound`: atoms with their results together with the intervals are a sound `Theory`, so `sound` holds of the checker's knowledge. `intNum` and `Interval.int_side`: an `Int`'s open end moved to the closed one next to it holds of the same integers.

## 6. Modules

An exported type with an active contract publishes it in `.iface`:
- the type;
- the constructor;
- the companions;
- the base;
- the summary, with atoms as canonical entities.

It also passes on the contracts it learned, as instances travel, so a re-export carries them (`Interface.invariants`, since `iface-version` 28; the companions a re-export needs since 29). The fingerprint covers the contract, so a changed check invalidates its dependents.

An importer (`proofs/exported.rs`) validates each contract against the exports it sees: the type is opaque, the constructor has `$`, `check : Base -> Bool` and `value : T -> Base` with a scalar base. One that does not match gives nothing. An interface that lost declarations (`incomplete`) gives none, and a module with errors publishes none of its own. `T.prove` of an imported type records the hidden unboxed constructor; a module alias or a re-export names the same canonical entity.

Checked by: `plugins_tests/invariants.rs` (imports, aliases, a re-export alone and two deep, a second check from the kept interfaces, an edit of the check alone that breaks exactly the proofs that needed it, a module with an unrelated error, whose types give their importers no `prove`, and a re-exporter with one, which still passes on what it learned), `proofs/exported.rs` (a check reads back as written, a malformed one is opaque), `tests/run/schema/prove_imported.fl` on JS, Go, the model and Flick's runtime.

## 7. Trust boundary

The invariant of §1 holds for values made by `C`'s applications, which are `T.parse`, `T.prove` and any other the discipline checked.
- A `foreign` function returning `T` is trusted, as `foreign` contracts are, and so are the `unsafe` APIs such as `Schema.unsafe_unboxed`; either can break the invariant.
- Wire codecs and `deriving Schema` go through `T.parse` (`Schema.unsafe_refined`).
- A pure atom is assumed deterministic. A `foreign pure` function that is not deterministic breaks the assumption, as it breaks purity.
- Lean proves the logic of §4–§5; it does not prove the Rust checker or the runtimes' IEEE arithmetic, and the model sees `T.prove e` as `C e`.
