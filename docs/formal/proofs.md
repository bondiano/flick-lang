# Proofs: statements over the pure, total domain

Status: **contract** (RFC #647, #641). Phase 1 is implemented: §1–§5 hold in the checker for statements over the variables of a proof. Admission of definitions (§6), proofs across modules (§7) and the Lean development (§9) land in the phases RFC #647 lists. Each rule says where it is checked.

References as in [`core.md`](core.md). Paths are relative to `flick-core/src/`.

## 0. Scope

| Form | Meaning | Section |
| - | - | - |
| `proof name params where goal = proof` | a checked statement, erased from the program | §1 |
| `equal l r`, `holds p`, `same_type A B` | the goals | §2 |
| `PureTotalV1` | the values a statement is about | §3 |
| `refl`, `symm`, `trans`, `cong`, `funext`, `match`, a premise, `let` | the rules | §4 |
| T080–T084 | how a proof fails | §5 |

Not in phase 1: admission of definitions and methods, lemmas, law bundles, proof modules and `flick prove`, `.iface` payloads.

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

  Effects, tuples, records, unknown types (metas), applied type variables and types with errors are refused (`FLICK-T082`).
- **Terms**, from the checked IR (`logic/term.rs`):
  - variables, literals, `fun`, application at the pure stage;
  - constructors, `match` without guards, `if`;
  - `let` of a name.

  Every other form is refused with its reason, never read as something else: a definition or method (phase 1), a primitive operation, a mutable variable, a loop, a runtime computation.
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
| `let h = p` then `q` | the goal | `p`'s equation is written by its rules (a premise, `symm`, `trans`), and `q` proves the goal with `h` |

- There is no η in normal forms: `fun x -> f x` and `f` are equal only by `funext`.
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

## 6. Definitions (phase 2)

A statement will unfold the definitions and instance methods it names (δ), with the dictionaries the checker resolved at each use, once they are admitted: closed under what they name, without recursion, foreign code, effects or `todo`. Until then a definition or method in a statement is `FLICK-T082`.

## 7. Modules (phase 4)

Proof modules under `project.proof-dir`, `flick prove`, lemmas across modules and their `.iface` payloads.

## 8. Why the domain is total

With `ap fs xs = map2 (fun f x -> f x) fs xs`, the law `ap (ap (ap (pure compose) fs) gs) xs = ap fs (ap gs xs)` fails for a `g` that fails: with `fs = None` the left side never applies `g`, and the right side does (`tests/run/stdlib/option_ap_strictness.fl`). A proof of the law is about total functions only. It makes no claim about this program, and nothing in the program changes.

## 9. Trust boundary

The logic is host code (`logic/`). It runs no Flick program and accepts no success it did not check. The Rust checker, the extraction from IR and the compiler and runtimes that run the program are trusted. The Lean development of the calculus and of the checker's soundness is phase 6 of RFC #647. Until it lands, §4 is a contract the Rust tests check, not a theorem.
