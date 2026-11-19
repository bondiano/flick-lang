# Proofs: statements over the pure, total domain

Status: **contract** (RFC #647, #641). Phases 1 and 2 are implemented: §1–§6 hold in the checker, for statements over the variables of a proof and the definitions it names. Proofs across modules (§7) and the Lean development (§9) land in the phases RFC #647 lists. Each rule says where it is checked.

References as in [`core.md`](core.md). Paths are relative to `flick-core/src/`.

## 0. Scope

| Form | Meaning | Section |
| - | - | - |
| `proof name params where goal = proof` | a checked statement, erased from the program | §1 |
| `equal l r`, `holds p`, `same_type A B` | the goals | §2 |
| `PureTotalV1` | the values a statement is about | §3 |
| `refl`, `symm`, `trans`, `cong`, `funext`, `match`, a premise, `let` | the rules | §4 |
| T080–T084 | how a proof fails | §5 |

Not yet: law bundles, definitions and lemmas across project modules, `.iface` payloads.

## 1. Proofs are opt-in and erased

- `proof` is a contextual word: a declaration starts with it only when a name follows (`syntax/parser/proofs.rs`). Outside that position `proof`, and the names of goals and rules, are ordinary names (`tests/check/pass/proofs/ordinary.fl`).
- A proof is checked with its module and gives no runtime code. The lowering of the module ignores it (`ir/lower/mod.rs`), and its parts are lowered apart, for the checker only (`ir/lower/proofs.rs`).
- Code without proofs is checked as before. A proof adds no obligation to the definitions or calls around it, and analyzes nothing it does not name.
- A proof is checked only in a module that checked without errors (`project/load/checking_core.rs`). A module with errors gives no evidence.

## 2. Statements

- **Rigid quantification.** The type variables written in the parameters, and in a `same_type` goal, are rigid (skolems): the statement holds for every type they stand for, and is never checked at a convenient instance (`types/proof_decls.rs`, `statement_variables`). Value and function parameters are arbitrary values of the domain.
- **Goals.**
  - `equal l r`: `l` is inferred, then `r` is checked against its type, which the statement records.
  - `holds p` is `equal p true` for `p : Bool`.
  - `same_type A B` compares two types by the compiler's own type equality.

  Any other goal is `FLICK-T083`.
- **Premises.** A parameter `(h : equal l r)` or `(h : holds p)` is a premise: a proof of that equation the body may use. It is part of the statement, so the statement claims the goal only where its premises hold. A `same_type` premise parses but is `FLICK-T082`: a premise is an equation of values.
- **The proof is typed.** It is an ordinary expression whose rule names have types of their own, indexed by the type of the equation they prove:

| Rule | Type |
| - | - |
| `refl` | `Proof a` |
| `symm` | `Proof a -> Proof a` |
| `trans` | `a -> Proof a -> Proof a -> Proof a` |
| `cong` | `(a -> b) -> Proof a -> Proof b` |
| `funext` | `(a -> Proof b) -> Proof (a -> b)` |

  The body is checked against `Proof τ`, where `τ` is the goal's type. `Proof` belongs to no module a program can name (`logic::RULES`). Captures play no part in it (`types/variance.rs`), since the logic does not read them. Type errors in a proof are ordinary type errors.

## 3. The domain `PureTotalV1`

A statement is about the values of this domain: computations that end, without effects, over the following types and terms.

- **Types:**
  - rigid variables;
  - pure arrows;
  - `Bool`;
  - data types whose declarations do not name themselves, through their fields or through other types (`types/proof_data.rs`, `recursive`).

  - tuples of types of the domain.

  Effects, records, unknown types (metas), applied type variables and types with errors are refused (`FLICK-T082`).
- **Terms**, from the checked IR (`logic/term.rs`):
  - variables, literals, `fun`, application at the pure stage;
  - constructors, tuples, `match` without guards (tuple patterns too), `if`;
  - `let` of a name.

  - a definition or a method the statement names, once admitted (§6).

  Every other form is refused with its reason, never read as something else: a primitive operation, a mutable variable, a loop, a runtime computation.
- **Function parameters** stand for total functions of the domain at every stage. This is the interpretation of the statement, not a claim about the program's own functions.
- **Equality** is logical: the same constructor with equal fields, equal literals, and functions equal at every argument (§4, `funext`). It is not `Eq`, `==`, `TypeEq` or identity of representation.

## 4. Rules

The body elaborates to a finite certificate (`logic/cert.rs`). The checker decides it against the goal (`logic/check.rs`), computing by normalization by evaluation (`logic/eval.rs`): β, ζ (`let`) and ι (`match` on a constructor or a literal). A variable stays unknown, and a `match` on it stays as it is.

| Rule | Proves | When |
| - | - | - |
| `refl` | `l = r` | `l` and `r` have the same normal form, up to the names of bound variables |
| `symm p` | `l = r` | `p` proves `r = l` |
| `trans m p q` | `l = r` | `p` proves `l = m` and `q` proves `m = r` |
| `cong (fun x -> e) p` | `l = r` | `p` proves `a = b`, stated by a premise or a `let`, and `e[a/x]`, `e[b/x]` have the normal forms of `l`, `r` |
| `funext (fun x -> p)` | `f = g` at `A -> B` | `p` proves `f x = g x` for a fresh `x : A` |
| `match v with \| C xs -> p …` | the goal | `v` is a variable of the statement of a data type or `Bool` and not split yet; each constructor has one arm, or a final `_` arm covers the rest; in each arm `v` is `C xs` for fresh `xs` |
| `h` | `l = r` | `h` is a premise, or a proof a `let` named, of the same normal forms |
| `let h = p` then `q` | the goal | `p`'s equation is written by its rules (a premise, a lemma, `symm`, `trans`), and `q` proves the goal with `h` |
| `name args` | the lemma's goal of `args` | `name` is a proof of this module before this one, which checked; `args` give a value for each parameter and a proof of each premise, in its order; its instance is the goal with definitions folded (exact subject) |

- There is no η in normal forms: `fun x -> f x` and `f` are equal only by `funext`.
- **Lemmas are exact.** A lemma's instance is compared with the goal by β, ζ and ι with every definition folded. So a lemma about `flat_map` of `Monad Option` closes no goal about another definition, even one with the same body (`tests/check/fail/proofs/lemmas.fl`). A proof names only proofs before it, and one that failed is no lemma: the proofs of a module form no cycle.
- `refl` reads no claim: a certificate is the tree of these rules, and the checker trusts no part it did not check.

## 5. Outcomes

| Outcome | Code | Meaning |
| - | - | - |
| proved | — | the certificate checks |
| disproved | `FLICK-T080` | the two sides normalize to different constructors or literals, or a `same_type` compares different type constructors |
| unproved | `FLICK-T081` | the rule leaves the sides different; the message shows them. The statement may be true. |
| outside the domain | `FLICK-T082` | the statement names a form or a type §3 refuses |
| no certificate | `FLICK-T083` | the body is no tree of §4's rules, or a rule does not apply: a split of what is no variable, a case split twice or missed, `funext` of a non-function, a fact not written |
| budget | `FLICK-T084` | normalization took more than its steps (200 000) or depth (400). Nothing is proved. |

A split that misses a constructor is usually reported first by the ordinary check of `match` (`FLICK-T009`); either way the proof gives nothing. The budget is the same on every platform: a proof that checks natively checks in WASM.

Checked by: `logic/tests.rs` (each rule and each failure on hand-built statements), `tests/check/pass/proofs/`, `tests/check/fail/proofs/`.

## 6. Definitions

A statement may name the definitions of its module and of the built-in modules, and the methods of their instances. Each one is admitted before any proof is checked (`logic/defs.rs`):

- **Bodies.** A body is read from the IR the module is compiled to (`ir::lower` of the module; the built-in bundle for the core). It goes through the same strict extraction as a statement (§3). For a method, this is the method of the instance its dictionary resolves to: the instance the checker chose at that use, or a superclass field of one, followed through `InstanceInfo.supers`.
- **Closure.** Admission covers everything the statement, its premises and its proof name, and everything those bodies name in turn. It does so whether or not a computation reaches it. A cycle anywhere in that closure is refused: direct recursion, a `rec` group, a helper that calls back.
- **Refused:**
  - foreign code, including `foreign pure`;
  - a definition that takes dictionaries;
  - an instance with a context;
  - a default method;
  - a definition another project module compiles (phase 4).

  Each reports the definition and the reason (`FLICK-T082`).
- **Unfolding.** Normalization unfolds an admitted definition where it is evaluated (δ).

So `equal (map identity v) v` is proved by a split of `v` against the real `Functor Option`. A diverging definition never enters a proof, even as an argument a function ignores: `(fun _ -> None) (forever ())` is refused, not reduced to `None` (`tests/check/fail/proofs/definitions.fl`). Defining such functions stays legal; only the proofs that name them fail.

## 7. Proof modules

- **The proof root.** `project.proof-dir` (default `proofs`) holds proof modules. They may import the project's source modules and each other, and a source module never imports one (`project/load/imports.rs`, `Root::Proof`).
- **What reads it.** Only `flick prove [paths]` selects that root, and the LSP analyzes a proof module that is open. `flick check`, `build`, `test` and `pack` never read it, so a library's proofs cost its users nothing (`flick/tests/cli/prove_command.rs`).
- **Phase 4b.** A proof module unfolds the definitions of the built-in modules and of its own module. The definitions of other project modules, lemmas across modules and their `.iface` payloads are phase 4b. Until then such a definition is `FLICK-T082`: it is not available to this proof.

## 8. The laws of `Option`

`packages/core/proofs/Laws/Option.fl` proves the laws of the `Functor`, `Applicative` and `Monad` instances of `Option` from their definitions in `Instances`. Here `ap fs xs` is `map2 (fun f x -> f x) fs xs`, and `compose` is written out.

| ID | Law |
| - | - |
| F1 | `map identity v = v` |
| F2 | `map (compose g f) v = map g (map f v)` |
| A1 | `ap (pure identity) v = v` |
| A2 | `ap (ap (ap (pure compose) fs) gs) xs = ap fs (ap gs xs)` |
| A3 | `ap (pure f) (pure x) = pure (f x)` |
| A4 | `ap fs (pure x) = ap (pure (fun f -> f x)) fs` |
| A5 | `map f v = ap (pure f) v` |
| M1 | `flat_map f (pure x) = f x` |
| M2 | `flat_map pure v = v` |
| M3 | `flat_map g (flat_map f v) = flat_map (fun x -> flat_map g (f x)) v` |
| C1 | `map f v = flat_map (fun x -> pure (f x)) v` |
| C2 | `map2 h va vb = flat_map (fun a -> map (fun b -> h a b) vb) va` |

Each holds for every payload and every total function. `flick prove --project packages/core` checks them (`flick/tests/cli/prove_command.rs`), and no import of `Option` reads that module.

A change to a method breaks the laws it breaks. A `map2` that answers `None` to two `Some` values fails A1, A3, A5 and C2 (T080). It still satisfies A2 and A4, which hold of it.

## 9. Why the domain is total

With `ap fs xs = map2 (fun f x -> f x) fs xs`, the law `ap (ap (ap (pure compose) fs) gs) xs = ap fs (ap gs xs)` fails for a `g` that fails: with `fs = None` the left side never applies `g`, and the right side does (`tests/run/stdlib/option_ap_strictness.fl`). A proof of the law is about total functions only. It makes no claim about this program, and nothing in the program changes.

## 10. Trust boundary

The logic is host code (`logic/`). It runs no Flick program and accepts no success it did not check. The Rust checker, the extraction from IR and the compiler and runtimes that run the program are trusted. The Lean development of the calculus and of the checker's soundness is phase 6 of RFC #647. Until it lands, §4 is a contract the Rust tests check, not a theorem.
