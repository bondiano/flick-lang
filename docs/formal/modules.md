# Module boundary model

[`Formal/Modules.lean`](../../formal/Formal/Modules.lean) models the part of MD2
that precedes elaboration into the core. It is intentionally separate from the
core calculus: the core receives records and dictionary values after source
module checking.

| Source rule | Lean representation and result |
| - | - |
| `module A = Make(Arg)` gets its own abstract result types | `ResultPath` identifies the named application site; `TypePath` pairs it with a member name. `different_names_have_distinct_types` proves that two names under one parent yield distinct types, even with the same functor and argument. `reimport_preserves_type` records stable identity for repeated imports of one site. |
| Static functors may require an abstract class or service; `Module Sig` needs manifest identities | `Identity.matches` accepts any supplied identity for an abstract requirement and requires exact path equality for a manifest requirement. `manifest_requires_same_path` excludes a same-shaped declaration at another path. `abstract_identity_cannot_be_packed` rejects an abstract requirement at the packed boundary. |
| `exists K : Sig. K.Key` keeps its witness inside `unpack` | `Packed F` uses a polymorphic continuation. `unpack_pack` proves the introduction/elimination equation. `unpackTwo` requires a client polymorphic in *two* witness types; `unpackTwo_pack` proves the equation for two openings. |
| `module Child =` keeps child types distinct from the parent's types while sharing one runtime file | `InlineTypePath` records the file, optional child and member. `inline_child_is_distinct_from_parent` and `inline_siblings_are_distinct` prove path separation. The compiler's source fixture and `flick-model` test check the concrete lowering. |

This is a model of the rules, not a proof that the Rust parser, checker, `.iface`
serializer, or either backend implements them. In particular, it does not prove
source-level progress or preservation for arbitrary modules, nor correctness of
the generated record ABI. The compiler regressions check source typing and
diagnostic spans; the reference model and JS/Go tests check representative
lowered executions. The existing Lean core theorems apply after modules have
been eliminated.

Run `cd formal && lake build --wfail` to check the module model and the existing
core proofs together.
