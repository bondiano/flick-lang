use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "native-project")]
use std::sync::atomic::AtomicUsize;

use super::super::checking::omitting;
use super::super::{Compiled, Mode, ModuleCache, analyze, run};
use super::{Memory, library};
#[cfg(feature = "native-project")]
use super::{Saved, memory_store, stored_count};
use crate::source::{SourceMap, Span};
use crate::{ir, types};

/// An inline module with each form that `relocate_ir` must move outside
/// expressions and their patterns: a top-level destructuring, the pattern
/// of a `for`, an instance that takes its class's default and a derived
/// one that does, a derived `Generic` and the `catch` of a foreign function.
/// The parent reaches the child through the same forms and exports a value
/// of a child type for an importer.
const FORMS: &str = "module App.Parent exposing (Describe(..), answer, pair)

import Generic exposing (Generic)

class Describe a =
  describe : a -> String
  describe : Show a => a -> String
  let describe x = \"<\" ++ show x ++ \">\"

module Child exposing (Pair(..), Box(..), Broken(..), left, sums, fail) =
  type Pair = Pair Int Int
    deriving Generic, Show, Describe
  type Box = Box Int
    deriving Show
  instance Describe Box
  type error Broken = Broken String
  let Pair left right = Pair 1 2
  sums : List Pair -> List Int
  let sums pairs = [for Pair a b <- pairs yield a + b + right]
  to_broken : JsValue -> Broken
  let to_broken _ = Broken \"mapped\"
  fail : String ->{; Broken} Unit
  foreign fail from \"./bindings.mjs\" \"fail\"
    catch to_broken

let Child.Pair low high = Child.Pair 5 6

pair : Child.Pair
let pair = Child.Pair Child.left high

answer : Unit ->{; Child.Broken} String
let answer () =
  Child.fail \"boom\"
  describe (Child.Box low) ++ describe pair ++ show (Child.sums [pair])
";

const PARENT: &str = "/p/src/App/Parent.fl";

/// The visits of `relocate_ir` the forms of [`FORMS`] need, by the names
/// its test hook leaves them out by.
const VISITS: [&str; 5] = ["destructure", "for", "defaults", "generic", "catch"];

/// `Pair` of the child as the parent's file holds it.
fn pair() -> types::Constructor {
    types::Constructor {
        module: "App.Parent".into(),
        ty: "Child$Pair".into(),
        name: "Child$Pair".into(),
        arity: 2,
        rep: types::Rep::Tagged,
        dicts: 0,
    }
}

/// A top-level name of the parent's file.
fn global(name: &str) -> ir::ExprKind {
    ir::ExprKind::Global {
        module: "App.Parent".into(),
        name: name.into(),
    }
}

/// An instance of the parent's file, with no context.
fn instance(name: &str) -> types::Dict {
    types::Dict::Instance {
        module: "App.Parent".into(),
        name: name.into(),
        args: Vec::new(),
    }
}

/// The first expression of `expr`, itself included, that `wanted` picks.
fn find<'a>(expr: &'a ir::Expr, wanted: &impl Fn(&ir::Expr) -> bool) -> Option<&'a ir::Expr> {
    if wanted(expr) {
        return Some(expr);
    }
    let mut found = None;
    expr.each_child(&mut |child| {
        if found.is_none() {
            found = find(child, wanted);
        }
    });
    found
}

/// What `module` breaks of what each form of [`FORMS`] expects of it, by
/// the visit that moves the form; nothing when it breaks none.
fn broken(module: &Compiled) -> Vec<(&'static str, String)> {
    VISITS
        .into_iter()
        .filter_map(|visit| expect(visit, module).err().map(|why| (visit, why)))
        .collect()
}

/// What the form moved by `visit` expects of the parent's IR, written from
/// [`FORMS`] alone: the names the parent's file holds the child's under,
/// the places in the parent's file and that each reference of the child's
/// names a declaration of the file.
fn expect(visit: &str, module: &Compiled) -> Result<(), String> {
    let ir = &module.ir;
    let at = |span: Span| (span.file == module.file).then(|| &FORMS[span.range()]);
    let item = |source: &str| {
        ir.items
            .iter()
            .filter(|item| at(item.span) == Some(source))
            .collect::<Vec<_>>()
    };
    let one = |source: &str| match item(source)[..] {
        [item] => Ok(item),
        ref items => Err(format!("{} items at `{source}`", items.len())),
    };
    let body = |source: &str| match &one(source)?.kind {
        ir::ItemKind::Value { body, .. } => Ok(body),
        other => Err(format!("`{source}` is {other:?}")),
    };
    let calls = |source: &str, kind: &ir::ExprKind| {
        find(body(source)?, &|e| e.kind == *kind)
            .map(|_| ())
            .ok_or(format!("`{source}` names no {kind:?}"))
    };
    let answer = "let answer () =";
    let answer = &FORMS[FORMS.find(answer).expect("answer")..FORMS.len() - 1];
    if one("Pair Int Int")?.kind != ir::ItemKind::Constructor(pair()) {
        return Err("`Pair` is not declared as the child's".into());
    }
    match visit {
        "destructure" => {
            for (source, names) in [
                (
                    "let Pair left right = Pair 1 2",
                    ["Child$left", "Child$right"],
                ),
                ("let Child.Pair low high = Child.Pair 5 6", ["low", "high"]),
            ] {
                let names = names.map(|name| ir::Pat::Bind(name.into())).into();
                match &one(source)?.kind {
                    ir::ItemKind::Destructure { pattern, .. }
                        if *pattern == ir::Pat::Con(pair(), names) => {}
                    other => return Err(format!("`{source}` is {other:?}")),
                }
            }
            calls(
                "let pair = Child.Pair Child.left high",
                &global("Child$left"),
            )
        }
        "for" => {
            let sums = "let sums pairs = [for Pair a b <- pairs yield a + b + right]";
            let each = find(body(sums)?, &|e| {
                matches!(e.kind, ir::ExprKind::Each { .. })
            })
            .ok_or("`sums` has no `for`")?;
            let ir::ExprKind::Each { pat, .. } = &each.kind else {
                unreachable!()
            };
            let bound = ["a", "b"].map(|name| ir::Pat::Bind(name.into())).into();
            if *pat != ir::Pat::Con(pair(), bound) || at(each.span) != Some("Pair a b <- pairs") {
                return Err(format!("the `for` of `sums` is {each:?}"));
            }
            calls(answer, &global("Child$sums"))
        }
        "defaults" => {
            for (source, name, show) in [
                (
                    "instance Describe Box",
                    "Child$Describe Box",
                    "Child$Show Box",
                ),
                (
                    "deriving Generic, Show, Describe",
                    "Child$Describe Pair",
                    "Child$Show Pair",
                ),
            ] {
                let shown = item(source).into_iter().chain(item("deriving Show"));
                if !shown.clone().any(|item| {
                    matches!(&item.kind, ir::ItemKind::Instance { info, .. } if info.name == show)
                }) {
                    return Err(format!("no instance `{show}` at `{source}`"));
                }
                let taken = shown.filter_map(|item| match &item.kind {
                    ir::ItemKind::Instance { info, methods } if info.name == name => {
                        Some((info, methods))
                    }
                    _ => None,
                });
                let [(info, methods)] = taken.collect::<Vec<_>>()[..] else {
                    return Err(format!("no one instance `{name}` at `{source}`"));
                };
                let class = ("App.Parent".to_owned(), "Describe".to_owned());
                let defaults = vec![("describe".to_owned(), 0, vec![instance(show)])];
                if info.module != "App.Parent"
                    || info.class != class
                    || info.defaults != defaults
                    || !methods.is_empty()
                {
                    return Err(format!("`{name}` is {info:?}"));
                }
            }
            let described = ir::ExprKind::Method {
                dict: instance("Child$Describe Box"),
                name: "describe".into(),
            };
            calls(answer, &described)
        }
        "generic" => {
            let generic = item("deriving Generic, Show, Describe")
                .into_iter()
                .find_map(|item| match &item.kind {
                    ir::ItemKind::Instance { info, .. } if info.class.1 == "Generic" => Some(info),
                    _ => None,
                })
                .ok_or("no derived `Generic`")?;
            let cases = generic.generic.as_ref().map(|g| (&g.cases, &g.inl.module));
            if generic.module != "App.Parent"
                || !generic.name.starts_with("Child$Generic Pair ")
                || cases != Some((&vec![(pair(), None)], &"Generic".to_owned()))
            {
                return Err(format!("the derived `Generic` is {generic:?}"));
            }
            Ok(())
        }
        "catch" => {
            let foreign = "foreign fail from \"./bindings.mjs\" \"fail\"\n    catch to_broken";
            let ir::ItemKind::Foreign(foreign) = &one(foreign)?.kind else {
                return Err("`fail` is not foreign".into());
            };
            let catch = foreign.catch.as_ref().ok_or("`fail` has no `catch`")?;
            if foreign.name != "Child$fail"
                || catch.kind != global("Child$to_broken")
                || at(catch.span) != Some("to_broken")
            {
                return Err(format!("`fail` is {foreign:?}"));
            }
            body("let to_broken _ = Broken \"mapped\"")?;
            calls(answer, &global("Child$fail"))
        }
        _ => Err(format!("no form is moved by `{visit}`")),
    }
}

/// The parent of [`FORMS`] compiled on its own, by a fresh analysis.
fn analyzed_forms() -> Compiled {
    let host = Memory(BTreeMap::from([(PathBuf::from(PARENT), FORMS.into())]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from(PARENT)],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let [module] = <[Compiled; 1]>::try_from(analysis.compilation.modules)
        .ok()
        .expect("one module");
    assert_eq!(module.path, PathBuf::from(PARENT));
    module
}

/// Each form moves as written into the parent's file, and `relocate_ir`
/// still agrees with the JSON form of the move.
#[test]
fn each_form_of_an_inline_module_moves_into_its_parents_file() {
    assert_eq!(broken(&analyzed_forms()), []);
}

/// Each visit the forms need, left out of the walk with the JSON form not
/// consulted, as if it left out the same: the expectations of its form,
/// and only those, notice.
#[test]
fn leaving_out_a_visit_breaks_the_expectations_of_its_form() {
    for visit in VISITS {
        let found = broken(&omitting(visit, analyzed_forms));
        let forms: Vec<_> = found.iter().map(|(form, _)| *form).collect();
        assert_eq!(forms, [visit], "{found:?}");
    }
}

/// An importer of [`FORMS`] that reaches the child's instances through a
/// value of the parent.
const IMPORTER: &str = "module App.Use exposing (used)

import App.Parent exposing (Describe(..), pair)

used : String
let used = describe pair ++ show pair
";

const USE: &str = "/p/src/App/Use.fl";

/// The modules a build from the importer compiles, by name, in a process
/// whose first `shift` files are others.
fn built(cache: Option<&mut ModuleCache>, shift: usize) -> BTreeMap<String, Compiled> {
    let host = Memory(BTreeMap::from([
        (PathBuf::from(PARENT), FORMS.into()),
        (PathBuf::from(USE), IMPORTER.into()),
    ]));
    let mut sources = SourceMap::default();
    for n in 0..shift {
        sources.add(format!("/elsewhere/{n}.fl"), "");
    }
    let project = library(&mut sources);
    let roots = [PathBuf::from(USE)];
    let output = run(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Mode::Compile,
        cache,
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let modules = output.compilation.modules.into_iter();
    modules
        .map(|module| (module.name.clone(), module))
        .collect()
}

/// The definition of `used` in the importer's IR, at its place in its own
/// file and naming the parent's `pair`.
fn used(modules: &BTreeMap<String, Compiled>) -> &ir::Expr {
    let user = &modules["App.Use"];
    let source = "let used = describe pair ++ show pair";
    let used = user.ir.items.iter().find(|item| {
        item.span.file == user.file && IMPORTER.get(item.span.range()) == Some(source)
    });
    let Some(ir::Item {
        kind: ir::ItemKind::Value { body, .. },
        ..
    }) = used
    else {
        panic!("no `used` at `{source}`: {:?}", user.ir.items);
    };
    assert!(find(body, &|e| e.kind == global("pair")).is_some());
    body
}

/// A run that reuses the parent's output from an earlier run keeps what
/// that run moved.
#[test]
fn a_reused_parent_keeps_what_its_first_run_moved() {
    let mut cache = ModuleCache::default();
    let first = built(Some(&mut cache), 0);
    let again = built(Some(&mut cache), 0);
    let reused = |name: &str| Arc::ptr_eq(&first[name].declarations, &again[name].declarations);
    assert!(reused("App.Parent"), "the second run reuses the parent");
    assert_eq!(broken(&again["App.Parent"]), []);
    used(&again);
}

/// A process that takes the importer from a store, with each file under
/// another id, moves the child as the first process did.
#[cfg(feature = "native-project")]
#[test]
fn a_parent_moves_its_child_again_beside_an_importer_from_a_store() {
    let (saved, taken): (Saved, Arc<AtomicUsize>) = Default::default();
    for (shift, restored) in [(0, 0), (2, 1)] {
        let mut store = ModuleCache::stored(memory_store("one", &saved, &taken));
        let modules = built(Some(&mut store), shift);
        assert_eq!(
            stored_count(&taken),
            restored,
            "the importer alone is stored"
        );
        assert_eq!(broken(&modules["App.Parent"]), []);
        used(&modules);
    }
}

/// Each instance of the child the importer names is declared where it
/// names it: a build without a store and one that restores the importer
/// emit no reference to a module they do not compile.
#[test]
fn an_importer_names_the_child_instances_where_the_build_declares_them() {
    let mut builds = vec![built(None, 0)];
    #[cfg(feature = "native-project")]
    {
        let (saved, taken): (Saved, Arc<AtomicUsize>) = Default::default();
        for shift in [0, 2] {
            let mut store = ModuleCache::stored(memory_store("one", &saved, &taken));
            builds.push(built(Some(&mut store), shift));
        }
        assert_eq!(stored_count(&taken), 1);
    }
    for modules in &builds {
        for method in ["describe", "show"] {
            let named = find(
                used(modules),
                &|e| matches!(&e.kind, ir::ExprKind::Method { name, .. } if name == method),
            );
            let Some(ir::ExprKind::Method {
                dict: types::Dict::Instance { module, name, .. },
                ..
            }) = named.map(|e| &e.kind)
            else {
                panic!("`used` calls no `{method}` of an instance");
            };
            let declared = modules.get(module).is_some_and(|m| {
                m.ir.items.iter().any(|item| {
                    matches!(&item.kind, ir::ItemKind::Instance { info, .. } if info.name == *name)
                })
            });
            assert!(declared, "`{method}` names `{name}` of `{module}`");
        }
    }
}
