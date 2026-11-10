# Certificates of generated programs

A certificate is a Lean theorem that one program types in the core of
[`core.md`](core.md) and [`regions.md`](regions.md), as mechanized in
[`formal/`](../../formal/README.md):

```lean
theorem program7 (S : Sig) (hS : GenSig S) :
    Typ S [] Facts.none [] ⟦e⟧ (.ex ⟦τ⟧ .pure) := …
```

The programs are those of the model's property generator
(`flick-model/src/generate`). The model checks a program, and
`flick_model::cert` prints the check's typing as a proof term. `lake` checks
that term with the rules of `formal/Formal/Rules.lean`.

## What a certificate claims

A certificate says that this one closed program, written as a term of the core,
has this one type in the core, under any signature `S` that has the generator's
constructors and globals. It links the reference model to the Lean core program
by program, in the same way that the [Prelude schemes](elaboration.md) link the
Prelude scheme by scheme.

It does not prove:

- that the Rust compiler or either runtime is correct;
- that the model is correct for programs it did not check;
- that the source program and the printed core term mean the same thing. The
  printers below are trusted for that.

## What is trusted

| Part | Where | Why it is trusted |
| - | - | - |
| The core | `formal/Formal/` | The typing rules the certificates use. |
| The derived rules | `formal/FormalCert/Rules.lean`, `Match.lean` | Theorems that `lake build --wfail` checks. They add nothing to the trusted base. |
| The tactics | `formal/FormalCert/Tactics.lean` | Macros over `simp`, `decide` and `omega`. The kernel checks what they produce, so a tactic that fails makes its certificate fail. |
| The signature | `formal/FormalCert/GenSig.lean` | Generated. `GenSig S` assumes that `S` has the generator's constructors at their declared types, that those types have no other constructors, and that `S` has the globals at their elided schemes (`check::elided`). `GenSig.inhabited` gives a signature that satisfies all of it, so the assumption is consistent. It says nothing about `Typ`. |
| The printers | `flick-model/src/cert/`, `flick-model/src/translate/lean/elide.rs` | They write the term, its type and the signature. A wrong printer certifies the wrong statement. |

Each certificate depends only on the axioms `propext`, `Classical.choice` and
`Quot.sound`. No certificate uses `sorry`, `native_decide` or new axioms.

## How a certificate is written

- **Trace.** The check records, by the address of each subterm, the type it
  inferred, the type it checked against, and the instance of each variable and
  global (`check::trace`). It also records the scheme a `let` binds, the rigid
  variables of a signed right side, and the stage of a `try` body together with
  the errors that pass its arms. The metas in all of these are solved after the
  check's verdict.
- **Explicit instances.** A certificate gives every instance, every arrow at a λ
  or a call, and every row of a `try`. Lean then infers the contexts and the
  stages, and the tactics close the side conditions: row and capture-row
  inclusions, `LeLive`, and `Scheme.inst`.
- **Open metas** are printed as the least instance of their kind (`Env::ground`):
  `Unit` for a type, empty for a row, a capture row or the rest of a record. A
  typing holds at every instance of the metas it leaves open, and Lean checks it
  at this one.
- **De Bruijn indices.** An arm's pattern binds its variables leftmost
  innermost, as `Δ.map .mono ++ Γ` of (Match) does. A `let rec` is
  `let_ (fix ⟦body⟧)`. Inside the body, the function is at index 1, at the one
  type `fix` gives it, so the check may use it only at its own rigid variables.
  Polymorphic recursion is a gap.
- **Coverage.** A `match` proves `Exh` by the shape of its unguarded arms
  (`formal/FormalCert/Match.lean`). Shapes are an arm `_` or `x`; `()`;
  `true | false`; one arm `K x̄` for each constructor of a generator type
  (`exh_gen`, from the `closed` field of `GenSig`); and a record pattern
  `{ℓ = x, ..}`. A `try` arm catches all of the body's errors with `x` or `_`,
  or the error `E` with an arm `E x̄` (`catches_all`, `catches_ctor`).

Records are written in the order of their labels, and string literals are core
literals of the base type `String` (simplification 3 of
[`formal/README.md`](../../formal/README.md#assumptions-and-simplifications)).

The plan was to close side conditions with reflective deciders proved sound in
Lean. The tactics were enough, and they are fast. In the first measurement, four
certificates took about 0.24 s. A run of 2000 generated programs gives 1094
certificates, and Lean checks all of them in about 20 s (200 s of CPU time over
the cores).

## What is not written yet

`flick-model/tests/cert_gaps.txt` lists the forms that certificates do not write
yet. It is computed over a fixed run of 500 programs, and each program is
counted at the first form it stops at. On that run, 259 of the 500 programs have
a certificate. The rest stop at:

- `provide`, whose handler is the core's (Hdl) under a fresh region;
- `scope` and the region constants;
- method calls through dictionaries;
- the list literal and `List.Core.get`;
- value bounds `a^C`. The core has no value bounds, and the model's
  `Ref.make : ∀c a. a^c →{} Ref (a^c)` bounds a cell's value. Certifying the
  programs that use cells needs a core with these bounds, or a decided reading
  of them.

The list may only shrink. The test fails when a new gap appears, and a gap is
taken off the list once it is closed. A count can grow only by programs that a
closed gap lets through to a later form.

## Running

- `just formal-certs` regenerates `GenSig.lean` and the 32 golden certificates of
  `formal/FormalCert/Gen/Golden.lean`. A test of `cargo test -p flick-model`
  fails when either is stale, and the `lean` job of CI checks both.
- `just model-certs` certifies the fixed run and writes
  `target/certs/Batch.lean`, which Lean then checks. With `CERT_PROGRAMS=n` it
  certifies `n` programs, but the gap list is compared only when `n` is 500. The
  nightly `certs` job runs it over 2000 programs.
