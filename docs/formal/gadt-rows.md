# GADT row indices (E10)

Constructor results may fix and relate service, error and capture rows.
Region identities and record rows retain their previous restrictions.
The shared checker applies these equations inside a fully signed match
branch; they do not change a runtime stage into a pure one.

## Quantification over residuals

`r = {Db | r}` implies that `Db` is available. `{Db | s} = {Db}` implies
`s ⊆ {Db}`; both `s = none` and `s = {Db}` satisfy it. Choosing either
as a most general unifier would discard a solution. The implementation
retains row equations and asks which inclusions follow for every solution.

The checker decides Boolean membership implications separately for each
observed nominal instance and an unobserved instance. Its union equations
form a dual-Horn theory. Unresolved payload identities remain symbolic:
`Store Int` and `Store String` are not interchangeable. The reference
checker independently enumerates membership assignments; it does not call
the compiler's row decision procedure.

Deferred effect and capture obligations retain the assumptions in force
where they arose. A local handler must not publish a constructor's hidden
row variable into the enclosing stage. The effect solver can publish a
lexically scoped upper bound only after proving that it covers every
permitted residual. Existing visible requirements, including `Live`, stay
in that row. The ordinary binder still checks scope and occurs conditions.

The reference checker keeps the original shape of instantiated rows until
their tails are solved, and checks invariant arguments of overlapping
instances before normalization can merge them. `{Store Int | r}` is a
union, not an implicit subtraction of `Store` from `r`. A constructed
scrutinee may still have inference metas in its index: an ordinary inclusion
can choose those before the equation proves the branch requirement for
every rigid residual. Pattern existentials are never defaulted this way.
Scope bodies admit their own `Scope s`; a requirement for an outer owner
passes through. The execution fixture checks that the indexed task's
finalizer runs once when its scope closes.

## Captures in the reference checker

The translation retains capture-row arguments as `Type::Caps`, using
declared parameter kinds, including empty rows. A constructor's storage
bound is distinct from its user-declared row indices. Matching it cannot
replace a capture index with the whole value's hidden row.

Capture comparison checks row variables as well as concrete region atoms.
Inferred parameters retain their complete qualifier. A projection supplies
a bound on the declared field type rather than creating a new hidden
closure environment in that field. Once the field type is complete, the
model retains its declared storage and regions and constrains its remaining
abstract atoms by that bound. Opaque generic value bounds do not themselves
constitute a use of a region; actual callable qualifiers still require Live.
These distinctions are exercised against the existing std acceptance set
and the model's leak/property suites, without new exclusions.
The existing explicit-owner interpreter limitations for `loops_owners` and
`lambda_owners` remain listed: those programs now pass the model checker,
then expose the existing dynamic-owner interpreter behavior. This change
does not claim to fix that separate G6 boundary.

## Proof boundary and regression evidence

`formal/Formal/GadtRowExamples.lean` proves membership from absorption,
inclusion for every residual, exclusion by subtraction and transport of
capture equality. It also proves that selecting the empty residual loses
a genuine solution and retains the indexed `Store Int`/`Store Bool`
distinction. It adds no axiom or new typing rule: E11 already makes `aref`
range over all unifiers modulo type congruence.

Those examples do not certify the Rust algorithm, translation or emitter.
The formal relation, the executable reference checker and the compiler
remain distinct implementations with their existing proof boundaries.

The accepted and rejected cases are in `tests/check/{pass,fail}/types/gadt_rows.fl`.
`flick-model/tests/gadt_rows.rs` checks the accepted program and independent
capture failures; `check/row_facts/tests.rs` checks nonempty residuals,
nominal arguments and deferred assumptions. `tests/run/codegen/gadt_rows`
has independently derived stdout, and `Language.GadtRows` exercises both
runtime profiles and target implementations.

This transition does not freeze IR 1.0 or complete the backend extraction.
See the remaining plain-data inputs in [the backend baseline](../backends/baseline.md).
