use std::fmt::Write;

use crate::iface::Interface;
use crate::source::SourceMap;
use crate::types::display::show_type_qualified;
use crate::types::{self, show_signature, show_type};
use crate::{resolve, syntax};

use super::{row_atoms, short_form};
use crate::types::ty::{Row, Type};
use crate::types::unify::Store;

/// The interface of module `Probe` with `text` as its declarations, whatever
/// else the checker says of them.
fn probe(text: &str) -> Interface {
    let text = format!("module Probe exposing (..)\n{text}");
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.as_str());
    let parsed = syntax::parse(&text, file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let syntax = resolve::module_syntax(&parsed.tree, &text, file);
    let module = resolve::Module {
        name: "Probe",
        file,
        text: &text,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
    let (mut interface, _) = resolve::resolve(&module, &builtin);
    // as when the built-in modules are checked: one package
    let local = |_: &str| true;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
    };
    types::check(
        &module,
        &builtin,
        types::builtins(),
        options,
        &mut interface,
    );
    interface
}

/// The foreign value `name` of type `signature`.
fn foreign(name: &str, signature: &str) -> String {
    format!("{name} : {signature}\nforeign effect {name} from \"./x.mjs\" \"x\"\n")
}

/// How `signature` is shown.
fn shown(signature: &str) -> String {
    let interface = probe(&foreign("f", signature));
    let export = (interface.exports.iter())
        .find(|e| e.entity.name == "f")
        .expect("the export");
    show_signature(export.scheme.as_ref().expect("its scheme"))
}

#[test]
fn the_outer_forall_goes_when_nothing_needs_it() {
    assert_eq!(shown("forall a. a -> a"), "a -> a");
    assert_eq!(shown("forall a. Eq a => a -> a"), "Eq a => a -> a");
    assert_eq!(shown("a -> a"), "a -> a");
}

#[test]
fn a_forall_stays_where_it_says_something() {
    // rank 2
    assert_eq!(
        shown("(forall a. a -> a) -> Int"),
        "(forall a. a -> a) -> Int"
    );
    // the variable is used only under another `forall`
    assert_eq!(
        shown("forall e. (forall a. a -> e) -> Int"),
        "forall e. (forall a. a -> e) -> Int"
    );
    // nothing would bind the variable without it
    assert_eq!(shown("forall a. Int"), "forall a. Int");
}

#[test]
fn a_stage_loses_the_qualifier_its_parameters_give() {
    assert_eq!(
        shown("forall c. (Int -> Int)^c -> Int -> Int"),
        "(Int -> Int)^c -> Int -> Int"
    );
    assert_eq!(shown("forall a. a -> Unit -> a"), "a -> Unit -> a");
    assert_eq!(
        shown("forall c. (Int -> Int)^c -> (Int -> Int)^c"),
        "(Int -> Int)^c -> Int -> Int"
    );
}

#[test]
fn a_qualifier_not_implied_stays() {
    assert_eq!(
        shown("forall c. (Int -> Int)^c -> (Int -> Int)^{}"),
        "(Int -> Int)^c -> (Int -> Int)^{}"
    );
    assert_eq!(
        shown("forall c d. (Int -> Int)^c -> (Int -> Int)^d"),
        "(Int -> Int)^c -> (Int -> Int)^d"
    );
    // what a value holds, not a stage
    assert_eq!(shown("forall a c. a^c -> a^c"), "a^c -> a^c");
}

/// `t` with the order of its binders and the form of its qualifiers set, which
/// a signature written without them may change.
fn normal(t: &Type) -> Type {
    match t {
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let mut vars = vars.clone();
            vars.sort_by(|a, b| a.name.cmp(&b.name));
            Type::Forall {
                vars,
                constraints: constraints.iter().map(normal).collect(),
                body: Box::new(normal(body)),
            }
        }
        Type::Captured {
            ty,
            captures,
            value,
        } => Type::Captured {
            ty: Box::new(normal(ty)),
            captures: Box::new(normal_row(captures)),
            value: *value,
        },
        // `{x}` and `x` are one row, which a signature writes either way
        Type::Row(row) if row.tail.is_none() && matches!(row.entries[..], [(_, Type::Var(_))]) => {
            row.entries[0].1.clone()
        }
        other => other.map_children(&mut |child| normal(child)),
    }
}

fn normal_row(row: &Type) -> Type {
    row_atoms(row).map_or_else(
        || row.clone(),
        |atoms| {
            Type::Row(Row {
                entries: atoms
                    .into_iter()
                    .map(|a| (format!("^{a}"), Type::Var(a)))
                    .collect(),
                ..Row::default()
            })
        },
    )
}

/// The short form is good only if a program can write it: for everything the
/// built-in modules export, written out and checked again it is the same type.
#[test]
fn every_builtin_signature_comes_back_from_its_short_form() {
    let mut source = String::new();
    let mut originals: Vec<(String, Type)> = Vec::new();
    for interface in types::builtins() {
        for export in &interface.exports {
            let Some(scheme) = &export.scheme else {
                continue;
            };
            if export.entity.name.contains('$') || export.parent.is_some() {
                continue;
            }
            let short = short_form(scheme, &Store::default());
            let name = format!("p{}", originals.len());
            source.push_str(&foreign(&name, &show_type_qualified(&short)));
            let what = format!("{}.{}", interface.module, export.entity.name);
            originals.push((what, scheme.clone()));
        }
    }
    let interface = probe(&source);
    let (mut checked, mut hidden, mut wrong) = (0, 0, String::new());
    for (i, (what, original)) in originals.iter().enumerate() {
        let name = format!("p{i}");
        let Some(back) = (interface.exports.iter())
            .find(|e| e.entity.name == name)
            .and_then(|e| e.scheme.as_ref())
        else {
            continue;
        };
        // a type the module does not export cannot be written here
        if back.contains(&mut |t| matches!(t, Type::Error)) {
            continue;
        }
        checked += 1;
        if show_signature(original).len() < show_type(original).len() {
            hidden += 1;
        }
        // through the short form: an alias does not give its fields their stages,
        // and the same fields written out get them, which is no difference here
        let store = Store::default();
        let normal_form = |t: &Type| show_signature(&normal(&short_form(t, &store)));
        let (want, got) = (normal_form(original), normal_form(back));
        if want != got {
            let _ = writeln!(wrong, "{what}\n  was {want}\n  now {got}");
        }
    }
    assert!(wrong.is_empty(), "{wrong}");
    assert!(checked > 1000, "only {checked} checked");
    assert!(hidden > 500, "only {hidden} shortened");
}
