//! A `type` with `@native when <cond>`: the backend's own where the condition
//! holds, its constructors elsewhere, which only covering bodies may name.

use flick_ir::ItemKind;

use crate::diagnostics::{Code, Diagnostic, Severity, codes};
use crate::ir;
use crate::resolve;
use crate::source::FileId;
use crate::syntax;
use crate::types::ffi::ForeignBackendSpec;
use crate::types::{self, NativeOverrideEnv, Options, StaticContext, Watched};

const BOX: &str = "\
module Probe exposing (Box, make, value)

type Box a =
  | Box a
  @native when js || go

make : a -> Box a
let make x = Box x
  @native \"./box.mjs\" \"make\" when js || go

value : Box a -> a
let value b =
  match b with
  | Box x -> x
  @native \"./box.mjs\" \"value\" when js || go
";

/// An environment of `tags`, by the official vocabulary.
fn env(tags: &[&str]) -> NativeOverrideEnv {
    let mut env = NativeOverrideEnv::default_js().clone();
    env.vocabulary.extend(tags.iter().map(|t| (*t).to_owned()));
    env.active_tags = Some(tags.iter().map(|t| (*t).to_owned()).collect());
    env.active_backend_id = None;
    env
}

fn backend() -> ForeignBackendSpec {
    ForeignBackendSpec {
        backend_id: "test/plain".into(),
        adapters: Vec::new(),
        type_form_attributes: Vec::new(),
        value_abi: None,
    }
}

/// The diagnostics of `source` and its IR, as `env` checks it.
fn check(source: &str, env: &NativeOverrideEnv) -> (Vec<Diagnostic>, Vec<ItemKind>) {
    let (diags, items, _) = checked(source, env);
    (diags, items)
}

/// The diagnostics, the IR and the typing of `source`, as `env` checks it.
fn checked(
    source: &str,
    env: &NativeOverrideEnv,
) -> (Vec<Diagnostic>, Vec<ItemKind>, types::Typing) {
    let backend = backend();
    let file = FileId::default();
    let parsed = syntax::parse(source, file);
    let mod_syntax = resolve::module_syntax(&parsed.tree, source, file);
    let module = resolve::Module {
        name: "Probe",
        file,
        text: source,
        tree: &parsed.tree,
        syntax: &mod_syntax,
        generated: resolve::NO_GENERATED,
    };
    let builtins = types::builtins();
    let lookup = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
    let (mut iface, mut diags) = resolve::resolve(&module, &lookup);
    diags.extend(parsed.diagnostics);
    let local = |name: &str| name == "Probe";
    let (checked, typing) = types::check_typed_with_static_context(
        &module,
        &lookup,
        builtins,
        Options {
            local: &local,
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
            foreign_backend: &backend,
            validate_foreign_metadata: true,
            native_overrides: Some(env),
            plugin_attributes: &[],
        },
        &mut iface,
        StaticContext {
            dictionaries: &[],
            witnesses: &[],
            #[cfg(not(target_arch = "wasm32"))]
            ir_lookup: None,
            skip_ir_types: true,
        },
    );
    diags.extend(checked);
    let errors = diags.iter().any(|d| d.severity == Severity::Error);
    let items = if errors {
        Vec::new()
    } else {
        let lowered = ir::lower(&module, &typing).expect("lowered");
        lowered.items.into_iter().map(|i| i.kind).collect()
    };
    (diags, items, typing)
}

fn constructors(items: &[ItemKind]) -> Vec<String> {
    (items.iter())
        .filter_map(|k| match k {
            ItemKind::Constructor(c) => Some(c.name.clone()),
            _ => None,
        })
        .collect()
}

fn coded(diags: &[Diagnostic], code: Code) -> usize {
    diags.iter().filter(|d| d.code == code).count()
}

#[test]
fn a_native_type_has_constructors_only_where_its_condition_fails() {
    let (diags, js) = check(BOX, &env(&["js", "rt", "node"]));
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    assert_eq!(constructors(&js), Vec::<String>::new());
    let (diags, lua) = check(BOX, &env(&["lua", "love"]));
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "{diags:#?}"
    );
    assert_eq!(constructors(&lua), vec!["Box".to_owned()]);
}

#[test]
fn a_constructor_outside_a_covering_body_is_refused() {
    let source = format!(
        "{BOX}
bad : a -> Box a
let bad x = Box x

partly : a -> Box a
let partly x = Box x
  @native \"./box.mjs\" \"make\" when js
"
    );
    let (diags, _) = check(&source, &env(&["lua"]));
    assert_eq!(coded(&diags, codes::F017), 2, "{diags:#?}");
}

#[test]
fn a_native_type_derives_nothing() {
    let source = BOX.replace("  | Box a\n", "  | Box a\n  deriving Eq\n");
    let (diags, _) = check(&source, &env(&["lua"]));
    assert_eq!(coded(&diags, codes::F017), 1, "{diags:#?}");
}

#[test]
fn a_native_type_without_constructors_is_refused() {
    let source = "module Probe exposing (Handle)\n\ntype Handle\n  @native when js\n";
    let (diags, _) = check(source, &env(&["js", "rt", "node"]));
    assert_eq!(coded(&diags, codes::F016), 1, "{diags:#?}");
}

/// Where the type is the backend's own, its values cross a binding as an
/// opaque type's do: `make`'s result is not copied, its argument is.
#[test]
fn a_native_type_crosses_a_binding_uncopied() {
    let (_, _, typing) = checked(BOX, &env(&["js", "rt", "node"]));
    let make = (typing.native_overrides.iter())
        .map(|(_, info)| &info.info.adapter.copies)
        .find(|copies| copies.args.len() == 1)
        .expect("make's clause");
    assert_eq!((make.args.clone(), make.result), (vec![true], false));
}

/// A collection whose representation keeps a function: typing reads it as
/// the type without constructors it is on JS.
const COLL: &str = "\
module Probe exposing (Coll, Wit, remap, union)

type Wit a = Wit (a -> Bool)
  @native when js || go

type Coll a v = Coll (Wit a) (List v)
  @native when js || go

remap : Coll a v -> Coll a b
let remap c =
  match c with
  | Coll w _ -> Coll w []
  @native \"./coll.mjs\" \"remap\" when js || go

union : Coll a v -> Coll a v -> Coll a v
let union l r =
  match (l, r) with
  | (Coll w xs, Coll _ ys) -> Coll w (xs ++ ys)
  @native \"./coll.mjs\" \"union\" when js || go

instance Semigroup (Coll a v) =
  let combine left right = union left right
";

/// The body that stands for a binding moves what the representation holds
/// between type arguments: G4 holds it, as the binding, to the regions it
/// keeps. A user of the type reads it with no hidden row, so `combine`
/// passes a `Coll` through as an opaque value.
#[test]
fn typing_reads_a_native_type_as_opaque_and_trusts_its_bodies() {
    for tags in [&["lua"][..], &["js", "rt", "node"]] {
        let (diags, _) = check(COLL, &env(tags));
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{tags:?}: {diags:#?}"
        );
    }
}

/// The same data without `@native` has hidden storage: `combine` is held to
/// the bound of its result, which the hidden row of `union`'s does not fit.
#[test]
fn a_type_that_is_not_native_keeps_its_hidden_storage() {
    let source = COLL
        .replace("  @native when js || go\n", "")
        .replace("  @native \"./coll.mjs\" \"remap\" when js || go\n", "")
        .replace("  @native \"./coll.mjs\" \"union\" when js || go\n", "");
    let (diags, _) = check(&source, &env(&["lua"]));
    let combine = u32::try_from(source.find("union left right").expect("combine")).expect("u32");
    assert!(
        (diags.iter()).any(|d| d.code == codes::G001 && d.span.is_some_and(|s| s.start == combine)),
        "{diags:#?}"
    );
}

/// The representation of a native type is its package's: a covering body
/// of another package may not forge a witness with it.
#[test]
fn a_native_type_of_another_package_keeps_its_constructors() {
    let source = "\
module Probe exposing (forged)

import EqHash

forged : EqHash.EqHash EqHash.Canonical Int
let forged = EqHash.Witness (fun _ _ -> true) (fun _ -> 0)
  @native \"./forge.mjs\" \"forged\" when js || go || vm
";
    let (diags, _) = check(source, &env(&["lua"]));
    let refused: Vec<_> = (diags.iter())
        .filter(|d| d.code == codes::F017 && d.message.contains("another package"))
        .collect();
    assert_eq!(refused.len(), 1, "{diags:#?}");
}
