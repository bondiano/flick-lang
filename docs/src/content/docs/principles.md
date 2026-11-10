---
title: Flick design principles
description: A draft manifesto with architectural consequences and explicit trade-offs.
---

**First draft.** These principles guide the design of the language, compiler,
runtime and applications. They describe the direction of Flick; the
[specification](../spec/) and [reference](../reference/types/) define the
current rules.

Flick helps you build applications whose behavior you can understand from
local contracts. You should be able to read a function's signature, see what
it requires and how it can fail, and trust the compiler to check its body
against that contract. Building, reviewing and changing a whole application
should remain practical as it grows.

We choose strict contracts at function and module boundaries, with as few
annotations as possible inside them. Developer productivity and one language
for the browser, server and command line serve this goal of predictable
behavior.

## 1. Understand behavior locally

A contract should let you reason about a call without reading every
implementation behind it. It states the accepted inputs, successful result,
allowed service requirements and expected failures. A signature is an upper
bound: a body may require less than it declares.

**Architectural consequences.** Keep top-level function signatures and
module interfaces explicit. Infer local details where the surrounding contract gives
enough information. Make dependencies available through declared services or
explicit values, and choose service implementations at the application's
composition boundary.

**Trade-off.** Signatures take effort to design and maintain. They cannot
describe every business rule, prove termination or account for defects.

## 2. Make architectural contracts checkable

Types, effects and regions should enforce the boundaries they describe.
Changing an implementation must not silently introduce a dependency or
expected failure outside its declared contract.

**Architectural consequences.** Use domain types and exhaustive matching to
make invalid combinations harder to construct. Track services, expected
errors and resource use in the checker. Keep the reference model, formal
statements and implementation tests aligned when their shared contract
changes.

**Trade-off.** Static checking can reject a safe program when it cannot
establish that safety. Explain the restriction and provide a sound way to express the program;
validate external facts at runtime where static checking cannot establish
them.

## 3. Express ordinary work directly

Sequential work should read in execution order. Calling a service, binding
its result and handling an expected failure should use ordinary control flow.

**Architectural consequences.** Lower direct effectful calls into the shared
execution model. Make deferred work and concurrency explicit. Give
evaluation order a defined meaning that optimization preserves.

**Trade-off.** Straightforward source code can require substantial compiler
and runtime machinery. Preserve source locations and useful diagnostics
through that machinery.

## 4. Separate pure logic from external effects

Data transformations should be easy to run and test using values alone.
External operations should have contracts whose implementations can be
chosen independently of that logic.

**Architectural consequences.** Keep domain calculations pure where
possible. Describe external operations with focused services, supply live
implementations at application boundaries and provide local implementations
in tests. Local implementation techniques may preserve purity when their
effects cannot be observed outside the function.

**Trade-off.** Useful service boundaries require design. A different
implementation must still satisfy the operation's contract, including its
errors and lifetime requirements.

## 5. Give work and resources an owner

The lifetime of a task or resource should follow from its owner. Success,
failure and interruption need defined cleanup behavior.

**Architectural consequences.** Use structured concurrency and scoped
resources. Define cancellation, child completion and finalizer order in the
runtime contract. Check where a resource is used: holding a handle beyond a
scope is acceptable only when it cannot be used after the resource closes.
See [regions and captures](../reference/regions/) and the
[runtime contract](../reference/runtime/).

**Trade-off.** Closing an owner may wait for children or foreign work that
cannot yet be interrupted. Cancellation does not roll back effects already
performed.

## 6. Compose a small set of general mechanisms

A new feature should fit with the language's existing types, effects,
modules and lifetime rules. Its benefit must justify the additional concepts
you need to learn.

**Architectural consequences.** Prefer libraries when existing mechanisms
express a task well. Admit specialized syntax such as `sql`, `ui` and `cli`
when it provides substantial checking or clarity and lowers to shared
mechanisms. Carry its meaning through shared analysis and IR so backends and
tools can use the same decisions.

**Trade-off.** General mechanisms can make the implementation harder.
Specialized syntax remains a commitment to documentation, diagnostics,
formatting and tooling throughout the feature's life.

## 7. Let Flick define portable semantics

Portable code should retain its specified meaning across supported targets.
Flick defines those rules, including which outcomes may legitimately vary
under concurrency.

**Architectural consequences.** Share checked semantics and IR across
backends. Specify common behavior for numbers, strings, errors, resource
lifetimes and cancellation, then test each implementation against those
expectations. Expose platform-specific operations explicitly and report
statically known target restrictions when building.

**Trade-off.** Consistency can require emulation and additional runtime cost.
The portable surface has limits; host integrations keep explicitly stated
platform requirements. Runtime selection must preserve the language
contract.

## 8. Design tools with the language

Compiler messages, formatting, editor support and documentation are part of
using Flick. A language feature is incomplete while these cannot explain or
work with it.

**Architectural consequences.** Let the compiler, language server and browser
compiler share semantic analysis. Preserve source ranges through lowering.
Keep diagnostics actionable, offer safe fixes, check documentation examples
and update editor grammars alongside syntax changes.

**Trade-off.** A small syntax addition can require work across many tools.
That cost belongs in the decision to add it.

## 9. State the boundaries of every guarantee

A useful guarantee names what it covers and what it assumes. Expected
errors, defects and interruption have distinct meanings. A pure function
may still fail with a defect or never terminate.

**Architectural consequences.** Treat foreign declarations as explicit
boundaries of trust, validate external data where needed and use adapters to
access host libraries. Document which behavior signatures capture. For
example, [`Host` services](../reference/effects/#explicit-host-dependencies) identify
console, clock, randomness, environment and argument dependencies and allow tests
to replace them. Legacy ambient adapters retain empty rows, so those rows do not
identify every external capability. Keep the scope and assumptions of
[formal proofs](https://github.com/bondiano/flick-lang/tree/main/formal)
explicit: proofs of a model do not establish correctness of the Rust compiler
or either runtime.

**Trade-off.** Interoperability brings useful libraries and additional
assumptions. Checking a foreign signature against a host type declaration
does not verify the foreign implementation. Expanding a guarantee requires
implementation and evidence together.

## 10. Treat performance as a measured requirement

Flick should run ordinary application workloads efficiently. Runtime cost,
memory, startup, compilation and editor latency all matter. Regressions
require an explanation and a deliberate decision.

**Architectural consequences.** Measure representative programs and maintain
budgets for their relevant costs. Optimize within the language contract,
preserving defined evaluation order, failures, cancellation and cleanup.
Permit computationally intensive work to use external libraries through the
same explicit trust boundaries as other foreign code.

**Trade-off.** Guarantees and abstractions can have a runtime cost. Compare
that cost with their benefit; improve or redesign their implementation when
needed. External code does not automatically inherit Flick's guarantees.

## 11. Pay for the capabilities you use

The mandatory foundation should stay small. Additional capabilities should
increase the delivered program when it uses them.

**Architectural consequences.** Keep pure libraries independent of the
effect scheduler, UI and server stack. Separate platform code and large
integrations, provide removable entry points and preserve dead-code
elimination. Make observability costs controllable. Use separate size budgets
for pure libraries, small interfaces and complete applications.

**Trade-off.** Fine-grained dependencies require additional package and
entry-point design. Resource and concurrency guarantees may still require a
shared runtime foundation; each program must carry what its guarantees need.

## 12. Evolve public contracts deliberately

People invest in source code and in their understanding of its behavior.
Language evolution should protect that investment.

**Architectural consequences.** Before 1.0, minor releases may make breaking
changes to improve the design; document them and provide migration guidance
and safe automated fixes where practical. From 1.0, compatible releases must
preserve supported source code and specified observable behavior. Breaking
changes require a major release and a migration path. Internal IR, caches
and interface artifacts remain versioned and may require rebuilding.

**Trade-off.** Stability limits redesign after 1.0. Before that boundary,
users need a clear account of what changed; permission to break compatibility
still carries a migration cost.

## Use the principles to make decisions

For a proposed feature or architectural change, explain:

1. Which application problem it solves and which principles motivate it.
2. Its contract, including trust boundaries and target restrictions.
3. How it composes with existing mechanisms and affects tools.
4. Its costs in concepts, runtime work, delivered size and migration.
5. What evidence will show that the implementation meets the contract.

Preserve the stated guarantees when improving convenience or performance.
When principles pull in different directions, record the trade-off and the
reason for choosing it. A proposal to change a guarantee must name that
contract change explicitly.

The format of this manifesto was inspired by the
[Flix design principles](https://flix.dev/principles/).
