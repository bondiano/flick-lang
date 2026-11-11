# Module boundary model

[`Formal/Modules.lean`](../../formal/Formal/Modules.lean) models the part of MD2
that precedes elaboration into the core. It is intentionally separate from the
core calculus: the core receives records and dictionary values after source
module checking.

[`Formal/HigherModules.lean`](../../formal/Formal/HigherModules.lean) and the
independent executable [`flick-model/src/modules.rs`](../../flick-model/src/modules.rs)
extend this boundary with static higher-order modules. The evaluator carries
opaque operation/layer code references through module structures and closures;
it has no rule that executes that code during static application.

| Source rule | Lean representation and result |
| - | - |
| `module A = Make(Arg)` gets its own abstract result types | `ResultPath` identifies the named application site; `TypePath` pairs it with a member name. `different_names_have_distinct_types` proves that two names under one parent yield distinct types, even with the same parameterized module and argument. `reimport_preserves_type` records stable identity for repeated imports of one site. |
| Parameterized modules may require an abstract class or service; `Module Sig` needs manifest identities | `Identity.matches` accepts any supplied identity for an abstract requirement and requires exact path equality for a manifest requirement. `manifest_requires_same_path` excludes a same-shaped declaration at another path. `abstract_identity_cannot_be_packed` rejects an abstract requirement at the packed boundary. |
| `exists K : Sig. K.Key` keeps its witness inside `unpack` | `Packed F` uses a polymorphic continuation. `unpack_pack` proves the introduction/elimination equation. `unpackTwo` requires a client polymorphic in *two* witness types; `unpackTwo_pack` proves the equation for two openings. |
| `module Child =` keeps child types distinct from the parent's types while sharing one runtime file | `InlineTypePath` records the file, optional child and member. `inline_child_is_distinct_from_parent` and `inline_siblings_are_distinct` prove path separation. The compiler's source fixture and `flick-model` test check the concrete lowering. |
| A factory accepts module-valued arguments and returns factories | `Expr.functor` captures a module environment. `application_uses_captured_module` excludes substitution by an equally placed caller binding; `returned_functor_retains_its_argument` proves the returned closure keeps its argument. The Rust evaluator tests the same behavior with independently chosen code references. |
| An atomic group cannot be partially applied or passed as a curried factory | `atomic_group_rejects_partial_application` checks evaluation; `matching_arrows_preserve_groups` proves that signature matching preserves the group size. |
| Higher-order inputs are contravariant and results covariant | `Subtype.arrow` declares that rule. `subtype_refl` and `subtype_trans` prove reflexivity and transitivity, including nominal shape requirements. |
| Result ascription hides unadvertised members | `project` filters public code references. `projection_hides_unadvertised` proves that a member of the projected view must be advertised. Source regressions additionally check nested parent views, result type abstraction and ascription of parameter aliases before concrete substitution. Narrowing one alias preserves the original parameter's separate view. |
| Dictionaries are explicit and scoped | `explicit_dictionary_wins`, `inner_dictionary_wins` and `dictionary_falls_back_to_global` prove resolution order. Closures capture the environment by value; extending the caller's scope does not mutate the definition environment. |

`flick-model/tests/modules/App/Hom/` is also compiled through the real source
loader, checked by the core reference checker and executed by its interpreter.
The fixture covers a returned anonymous factory, a named nested structure
functor, a captured preceding value, dependent manifest types, aliases and a captured local dictionary;
JS and Go execute the same fixture in debug and release profiles. Public source
factories in these examples declare their result signature (`FLICK-T067`).

Lexical prefix scheduling restores this existing closure contract. The Rust
compiler checks prefixes without executing them. JS places the original
bindings in physical prefix files; Go schedules top-level initializers by
value dependencies. Both preserve nominal source owners. IR version `0.6.0`
records prefix boundaries and their bindings, so an older reader
cannot silently use whole-module initialization. This placement metadata does
not change core expressions or the Lean evaluation rules. Execution regressions
also check a parent/result dependency cycle, one shared memoized layer object
and offline instantiation with a new argument after deleting package sources.

This is a model of the rules, not a proof that the Rust parser, checker, `.iface`
serializer, or either backend implements them. In particular, it does not prove
source-level progress or preservation for arbitrary modules, nor correctness of
the generated record ABI. The static shape model does not represent source
type schemes, dependent equality substitution or resource lifecycles; preserving
opaque code references is not a proof of runtime cleanup. The compiler regressions check source typing and
diagnostic spans; the reference model and JS/Go tests check representative
lowered executions. The existing Lean core theorems apply after modules have
been eliminated.

Run `cd formal && lake build --wfail` to check the module model and the existing
core proofs together.
