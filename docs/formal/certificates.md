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
| The tactics | `formal/FormalCert/Tactics.lean`, `Checks.lean` | Macros over `simp`, `decide` and `omega`. The kernel checks what they produce, so a tactic that fails makes its certificate fail. `Checks.lean` keeps goals they once failed on. |
| The signature | `formal/FormalCert/GenSig.lean` | Generated. `GenSig S` assumes that `S` has the generator's constructors at their declared types and that those types have no other constructors, the list constructors `Nil`/`Cons` among them. It assumes the globals at their elided schemes (`check::elided`), with their facts: the generator's values, `List.Core.get`, the region globals `open_scoped`, `use`, `Fiber.spawn`, `Resource.acquire_release` and `Scope.add_finalizer`, and the dictionary of each instance. It assumes the operations of each service without parameters, and no others, and the fields of each class's dictionary. `GenSig.inhabited` gives a signature that satisfies all of it, so the assumption is consistent. It says nothing about `Typ`. |
| The printers | `flick-model/src/cert/`, `flick-model/src/translate/lean/elide.rs`, `elide/data.rs` | They write the term, its type and the signature. A wrong printer certifies the wrong statement. |

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

- **Lists.** The core has no list. A literal `[x, y]` is the spine
  `Cons x (Cons y Nil)` of constructors only the signature has.
- **Services.** An operation `(ask_S ()).o` is the core's `perform`, at the
  arrow `a →{S} b`. The model types it `a →{} b` and asks for `S` where it
  runs. `provide (Layer.succeed S w) e` is lowered to
  `let w = ⟦w⟧ in hdl S (o x → w.o x) (λv. v) ⟦e⟧` (`Cert.typ_provide`): the
  arm reads the layer's record through a variable, so the record may hold
  anything. `Layer.effect S task` builds the record once, before the body,
  with `await ⟦task⟧`.
- **Dictionaries.** A method `d.m` or a superclass `d.$S` is (Dproj) of the
  dictionary's class, numbered by the sorted classes of the generator
  (`Cert.typ_dproj`). An instance is a global.
- **Regions.** `scope s { e }` binds its region as type variable 0 and its
  owner as term variable 0 (`Cert.typ_scope`). Its body is written in that
  binder, `Fiber.Owner s` as the core's `.owner s`, and may assume that what
  is outside outlives the region (`Cert.scope_single`), the fact work kept in
  it needs. The obligations `Provided {S} ϱ` of that work are the core's
  `.ob`/`.obv` elements, which the end of the scope discharges.
- **Facts and value bounds.** A scheme's `Outlives C s` is `Facts.single C s`,
  and a value bound `a^c` on one of its variables is `Facts.bnd`, rule 1 of
  the scheme translation. The `holds` tactic proves them at each instance. A
  bound on any other type is read as the core reads it: on data, the data; on
  a function or a computation, its use qualifier. A cell's constants
  `Ref.make`, `Ref.get` and `Ref.set` are the core's `.const` at the content
  and capacity of the cell, and `Const.ok` checks that the content reaches no
  more than the capacity.
- **Hidden storage.** Data with a function field (`Fixture.Sealed`) is, as
  `Seq`, `T C ā` in the core: its first argument is the capture row its value
  hides, and a bare `T` hides none. Its constructor takes that row as its
  first variable and qualifies each function field with it, as the model's
  (Con) checks a field against `τᵢ^q` and gives the value `(T ῡ)^q`. A
  `match` on it takes the row from the matched type, its first argument, so
  a function field is bound at the row the value hides (`PatTy.head`), and a
  bare `T` in a local signature reads a fresh row as any signature does.

Records are written in the order of their labels, and string literals are core
literals of the base type `String` (simplification 3 of
[`formal/README.md`](../../formal/README.md#assumptions-and-simplifications)).

The plan was to close side conditions with reflective deciders proved sound in
Lean. The tactics were enough, and they are fast. In the first measurement, four
certificates took about 0.24 s. Every program of a run of 2000 generated programs
has a certificate, and Lean checks all of them in about 35 s.

## What is not written yet

`flick-model/tests/cert_gaps.txt` lists the forms that certificates do not write
yet. It is computed over a fixed run of 500 programs, and each program is
counted at the first form it stops at. On that run, every program has a
certificate, so the list is empty. What a certificate still refuses, with its
reason, is what the generator does not make today: an operation used without
being called, a service with several operations or with parameters (`Within s`,
whose global `touch` the signature skips), a layer that is neither `succeed`
nor `effect`, a monomorphic `let` with facts, polymorphic recursion, a float,
an open record, and a value bound over a variable of a parameter's `∀`.

The list may only shrink. The test fails when a new gap appears, and a gap is
taken off the list once it is closed. A count can grow only by programs that a
closed gap lets through to a later form.

## Running

- `just formal-certs` regenerates `GenSig.lean`, the 32 golden certificates of
  `formal/FormalCert/Gen/Golden.lean` and the certificates of
  `formal/FormalCert/Gen/Kept.lean`: programs of the fixed run whose
  certificates were once wrong. A test of `cargo test -p flick-model` fails
  when any of them is stale, and the `lean` job of CI checks them.
- `just model-certs` certifies the fixed run and writes it to
  `target/certs/Batch/`, 50 certificates a file, which Lean then checks. With
  `CERT_PROGRAMS=n` it certifies `n` programs, but the gap list is compared only
  when `n` is 500. The nightly `certs` job runs it over 2000 programs.
