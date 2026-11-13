use std::collections::BTreeSet;
use std::path::Path;
use std::{env, fs};

use crate::diagnostics::Diagnostic;
use crate::diagnostics::Severity;
use crate::diagnostics::codes;
use crate::iface::Interface;
use crate::plugin::derive::builtin_expansion;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::types::Watched;
use crate::{syntax, types};
use flick_test_support::files as fixture_files;

/// Checks one self-contained module and lowers it.
pub(crate) fn lower_text(name: &str, text: &str) -> Result<super::Module, Vec<Diagnostic>> {
    check_text(name, text).map(|(ir, _)| ir)
}

/// [`lower_text`] with the module's interface.
pub(crate) fn check_text(
    name: &str,
    text: &str,
) -> Result<(super::Module, Interface), Vec<Diagnostic>> {
    check_text_with(name, text, types::builtins())
}

/// [`check_text`] against the built-in modules `builtins`.
pub(crate) fn check_text_with(
    name: &str,
    text: &str,
    builtins: &[Interface],
) -> Result<(super::Module, Interface), Vec<Diagnostic>> {
    let mut sources = SourceMap::default();
    let file = sources.add(format!("{name}.fl"), text);
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module_name = syntax.header.as_ref().map_or(name, |h| h.name.as_str());
    let (expansion, derived) = builtin_expansion(text, file, &parse.tree, module_name, &syntax);
    assert!(derived.is_empty(), "{derived:?}");
    let module = Module {
        name: module_name,
        file,
        text: expansion.as_ref().map_or(text, |e| &e.text),
        tree: expansion.as_ref().map_or(&parse.tree, |e| &e.tree),
        syntax: &syntax,
        generated: expansion.as_ref().map_or(resolve::NO_GENERATED, |e| e.len),
    };
    let builtin = |name: &str| builtins.iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &builtin);
    let local = |n: &str| n == module_name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: Watched::none(),
        stages_by_class: false,
    };
    let (more, typing) = types::check_typed(&module, &builtin, builtins, options, &mut interface);
    diagnostics.extend(parse.diagnostics);
    diagnostics.extend(more);
    // lowering needs a module without errors; a lint does not stop it
    diagnostics.retain(|d| d.severity == Severity::Error);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let mut lowered = super::lower(&module, &typing)?;
    lowered.exports = super::exports(module_name, &interface);
    Ok((lowered, interface))
}

#[test]
fn every_passing_fixture_lowers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass");
    let mut paths = Vec::new();
    paths.extend(fixture_files(&root, "fl"));
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    for path in paths {
        let text = fs::read_to_string(&path).expect("utf-8");
        match lower_text("Fixture", &text) {
            // what a plugin's pass over the IR gives is held to the same check
            Ok(module) => {
                let mut files = BTreeSet::new();
                module.clone().each_span_mut(&mut |s| {
                    files.insert(s.file);
                });
                let program = flick_ir::Program {
                    ir_version: flick_ir::VERSION.into(),
                    compiler: String::new(),
                    files: Vec::new(),
                    modules: vec![module],
                    lang: super::lang_items(),
                };
                let problems = flick_ir::validate(&program, &builtin, &files);
                assert!(problems.is_empty(), "{}: {problems:?}", path.display());
            }
            // blocks compiled by later phases are reported, never miscompiled
            Err(d) => assert!(
                d.iter().all(|d| d.code == codes::P012),
                "{}: {d:?}",
                path.display()
            ),
        }
    }
}

#[test]
fn ir_makes_stages_dictionaries_and_lifts_explicit() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         type error NotFound = NotFound Float\n\
         service Users =\n  get : Float ->{; NotFound} String\n\
         type Shape = Circle Float | Square Float\n  deriving Eq\n\
         render : Show a => a -> String\nlet render value = show value\n\
         load : Float ->{Users; none} String\n\
         let load id =\n  let name = try Users.get id with\n    | NotFound _ -> \"?\"\n  Platform.stdout_line (render 1 ++ name)\n  name\n\
         twice : (Float ->{} Float) -> Float ->{} Float\nlet twice f x = f (f x)\n\
         inc : Float -> Float\nlet inc x = x + 1\n\
         run : Unit ->{} Float\nlet run () = twice inc 1\n\
         pick : Option Float -> Float\nlet pick o =\n  match o with\n  | Some (1 | 2) -> 0\n  | Some n when n > 3 -> n\n  | _ -> -1\n",
    )
    .expect("lowers");
    insta::assert_snapshot!(super::print(&ir));
}

#[test]
#[ignore = "prints the IR of a file: DUMP_FILE=path"]
fn dump() {
    let path = env::var("DUMP_FILE").expect("DUMP_FILE");
    let text = fs::read_to_string(path).expect("utf-8");
    println!(
        "{}",
        super::print(&lower_text("Dump", &text).expect("lowers"))
    );
}

#[test]
fn both_forms_of_save_button_lower_to_the_same_ir() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/ui/save_button.fl");
    let text = fs::read_to_string(path).expect("utf-8");
    let ir = lower_text("App", &text).expect("lowers");
    let body = |wanted: &str| {
        ir.items
            .iter()
            .find_map(|item| match &item.kind {
                super::ItemKind::Value { name, body, .. } if name == wanted => {
                    Some(super::print::expr(body))
                }
                _ => None,
            })
            .expect("defined")
    };
    assert!(body("save_button").contains("Ui.button"));
    assert_eq!(body("save_button"), body("save_button_calls"));
}

#[test]
fn ir_does_not_depend_on_a_backend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ir");
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).expect("ir dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).expect("utf-8");
                // split so that this file does not match itself
                let codegen = concat!("codegen", "::");
                assert!(!text.contains(codegen), "{} uses codegen", path.display());
            }
        }
    }
}

#[test]
fn ir_carries_the_checker_types() {
    use super::{ExprKind, ItemKind};
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         area : Float -> Float\nlet area r = r * r\n\
         ident : a -> a\nlet ident x = x\n\
         label : String\nlet label = ident \"x\"\n",
    )
    .expect("lowers");
    // the name of each expression's type constructor, by item and expression kind
    let mut seen = Vec::new();
    for item in &ir.items {
        let ItemKind::Value { name, body, .. } = &item.kind else {
            continue;
        };
        let mut stack = vec![body];
        while let Some(e) = stack.pop() {
            let kind = match &e.kind {
                ExprKind::Binary(..) => "arith",
                ExprKind::Local(_) => "local",
                ExprKind::App { .. } => "app",
                _ => "",
            };
            let ty = e.ty.as_ref().map_or("none".to_owned(), |t| t.name.clone());
            if !kind.is_empty() {
                seen.push(format!("{name} {kind} {ty}"));
            }
            e.each_child(&mut |c| stack.push(c));
        }
    }
    for want in [
        "area arith Float",
        "area local Float",
        "ident local none",
        "label app String",
    ] {
        assert!(seen.iter().any(|s| s == want), "no `{want}` in {seen:?}");
    }
}

#[test]
fn compositions_lower_to_one_closure_and_left_pipes_to_calls() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         say_n : Int ->{} Int\nforeign say_n from \"./io.js\" \"sayN\"\n\
         inc : Int -> Int\nlet inc n = n + 1\n\
         left : List Int -> List Int\nlet left xs = List.map inc <| xs\n\
         atoms : Int -> Int\nlet atoms = inc >> inc\n\
         backward : Int -> String\nlet backward = show << inc\n\
         partial : List Int -> List Int\nlet partial = List.map inc >> List.filter (fun n -> n > 2)\n\
         chain : Int -> String\nlet chain = inc >> (inc >> inc) >> show\n\
         effects : Int ->{} Int\nlet effects = inc >> say_n\n\
         later : List Int ->{} Int\nlet later = List.map_effect say_n >> List.length\n",
    )
    .expect("lowers");
    let printed = super::print(&ir);
    for line in [
        // `f a <| x` is `f a x`
        "let left = (fun xs -> ((List.map App.inc) xs))",
        // names are used in place, other operands evaluated once before,
        // a method too: its dictionary may be built where it is read
        "let atoms = (fun $x -> (App.inc (App.inc $x)))",
        "let backward = { let $f = <Show Int>.show; (fun $x -> ($f (App.inc $x))) }",
        "let partial = { let $f = (List.map App.inc); let $f$1 = (List.Core.filter (fun n -> (n > 2))); (fun $x -> ($f$1 ($f $x))) }",
        // a chain is one closure, through parentheses
        "let chain = { let $f = <Show Int>.show; (fun $x -> ($f (App.inc (App.inc (App.inc $x))))) }",
        // the closure's stage and its calls' are those the checker gave them
        "let effects = (fun $x! -> (App.say_n (App.inc $x))!)",
        "let later = { let $f = (List.map_effect App.say_n); (fun $x! -> (List.Core.length ($f $x)!)) }",
    ] {
        assert!(printed.contains(line), "{line}\nnot in\n{printed}");
    }
}
