use crate::types::ForeignBackendSpec;
use std::collections::BTreeSet;
use std::path::Path;
use std::{env, fs};

use crate::diagnostics::{Diagnostic, Severity, codes};
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
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
        plugin_attributes: &[],
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
    super::fill_exports(&mut lowered, &interface);
    Ok((lowered, interface))
}

#[test]
fn every_passing_fixture_lowers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass");
    let mut paths = Vec::new();
    paths.extend(fixture_files(&root, "fl"));
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    let mut all_failures = Vec::new();
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
                if !problems.is_empty() {
                    all_failures.push(format!("{}: {problems:?}", path.display()));
                }
            }
            // blocks compiled by later phases are reported, never miscompiled
            Err(d) => assert!(
                d.iter().all(|d| d.code == codes::P012),
                "{}: {d:?}",
                path.display()
            ),
        }
    }
    assert!(
        all_failures.is_empty(),
        "{} fixture(s) failed validation:\n{}",
        all_failures.len(),
        all_failures.join("\n")
    );
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
         load : Float ->{Host.Console, Users; none} String\n\
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
fn awaits_lower_to_calls_of_the_selected_evidence() {
    fn visit(expr: &super::Expr, methods: &mut Vec<String>, calls: &mut usize) {
        use super::{ExprKind, Stage};
        match &expr.kind {
            ExprKind::Await(_) => panic!("source await lowered to a dedicated operation: {expr:?}"),
            ExprKind::App {
                stage: Stage::Runtime,
                ..
            } => *calls += 1,
            ExprKind::Global { module, name, .. } => methods.push(format!("{module}.{name}")),
            ExprKind::Method {
                dict: flick_ir::Dict::Given(_),
                name,
                ..
            } => {
                methods.push(name.clone());
            }
            _ => {}
        }
        expr.each_child(&mut |child| visit(child, methods, calls));
    }
    let module = lower_text(
        "App",
        "module App exposing (..)\n\
         mixed : Eff {} none Int -> Lazy {} none Int -> Eff {} none Int\n\
         let mixed work cell = task {\n  let first = await work\n  first + await cell\n}\n\
         forward : Awaitable t r e a => t ->{r; e} a\n\
         let forward value = await value\n",
    )
    .expect("lowers built-in and dictionary calls");
    let mut methods = Vec::new();
    let mut calls = 0;
    for item in &module.items {
        item.each_body(&mut |body| visit(body, &mut methods, &mut calls));
    }
    assert_eq!(calls, 3);
    assert_eq!(
        methods,
        ["Prelude.await_eff", "Prelude.lazy_force", "await"]
    );
}

#[test]
fn a_task_infers_the_exact_rows_of_eff_and_lazy_awaits() {
    let (_, interface) = check_text(
        "App",
        r#"module App exposing (..)

type error DbError
type error CacheError

service Db =
  load : Int ->{; DbError} String

service Cache =
  read : Unit ->{; CacheError} String

load_user : Int -> Eff {Db} {DbError} String
let load_user id = task { Db.load id }

let program = task {
  let config = lazy { "cached" }
  await (load_user 42) ++ await config
}

let both = task {
  let config = lazy { Cache.read () }
  await (load_user 42) ++ await config
}

expected_program : Eff {Db} {DbError} String
let expected_program = program

expected_both : Eff {Db, Cache} {DbError, CacheError} String
let expected_both = both
"#,
    )
    .expect("the mixed tasks check");
    let scheme = |name: &str| {
        let export = interface
            .exports
            .iter()
            .find(|e| e.entity.name == name)
            .expect("the test value is exported");
        export.scheme.as_ref().expect("the value has a type")
    };
    let minimum = |name: &str| {
        let types::Type::Forall {
            vars,
            constraints,
            body,
        } = scheme(name)
        else {
            return types::show_signature(scheme(name));
        };
        assert!(constraints.is_empty(), "{constraints:?}");
        // A task's inferred scheme can widen its service and error rows.
        let slack: Vec<_> = vars
            .iter()
            .map(|var| {
                assert!(matches!(
                    var.kind,
                    types::Kind::ServiceRow | types::Kind::ErrorRow
                ));
                (var.name.clone(), types::Type::empty_row())
            })
            .collect();
        types::show_signature(&body.substitute(&slack)).replace(" | none}", "}")
    };
    assert_eq!(
        minimum("program"),
        types::show_signature(scheme("expected_program"))
    );
    assert_eq!(
        minimum("both"),
        types::show_signature(scheme("expected_both"))
    );
}

/// A `let lazy` is a variable of its function: no cell. The declaration makes
/// the task `x$run = task (attempt e)` and the state `x = None`; an `await`
/// runs the task while the state is `None` and keeps `Some o`. A write to a
/// `let lazy mut` makes the state `Some (Ok v)`.
#[test]
fn a_let_lazy_lowers_to_a_variable_of_its_function() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         type error Boom = Boom\n\
         load : Int ->{; Boom} Int\nlet load n = if n < 0 then Fail.fail Boom else n\n\
         run : Bool ->{; Boom} Int\n\
         let run flag =\n  let lazy x =\n    let t = load 1\n    t + 1\n  \
         let lazy mut y = load 2\n  y = 3\n  \
         if flag then await x + await y else await x\n",
    )
    .expect("lowers");
    insta::assert_snapshot!(super::print(&ir));
}

/// A `let lazy` lowers its initializer once, however many `await`s run it
/// and however deep it is in the initializer of another.
#[test]
fn a_let_lazy_initializer_is_lowered_once() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         mark : Int ->{} Int\nlet mark n = n\n\
         run : Unit ->{} Int\n\
         let run () =\n  let lazy top =\n    let lazy v2 =\n      let lazy v1 = mark 7\n      \
         await v1 + await v1\n    await v2 + await v2\n  await top\n",
    )
    .expect("lowers");
    let printed = super::print(&ir);
    assert_eq!(printed.matches("App.mark 7").count(), 1, "{printed}");
}

/// Only the declaration of a `let lazy` makes a task: an `await` of it is a
/// `match` on its state, so no force allocates one, in a loop or not.
#[test]
fn an_await_of_a_let_lazy_makes_no_task() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         mark : Int ->{} Int\nlet mark n = n\n\
         run : Bool ->{} Int\n\
         let run flag =\n  let lazy x = mark 7\n  let mut sum = 0\n  \
         for i <- [1, 2, 3] do sum = sum + i * await x\n  if flag then await x else sum\n",
    )
    .expect("lowers");
    let printed = super::print(&ir);
    assert_eq!(printed.matches("(task ").count(), 1, "{printed}");
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
            let ty = e.ty.as_ref().map_or("none".to_owned(), ToString::to_string);
            if !kind.is_empty() {
                seen.push(format!("{name} {kind} {ty}"));
            }
            e.each_child(&mut |c| stack.push(c));
        }
    }
    for want in [
        "area arith Prelude.Float",
        "area local Prelude.Float",
        "ident local a",
        "label app Prelude.String",
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

/// The method `name` of the instance `instance`, as the IR prints it.
fn derived_method(ir: &super::Module, instance: &str, name: &str) -> String {
    let printed = super::print(ir);
    let head = format!("instance {instance} ");
    let mut lines = printed
        .lines()
        .skip_while(|l| !(l.starts_with(&head) && l.contains("derived")));
    let line = lines.find(|l| l.trim_start().starts_with(name));
    line.expect("the method").trim().to_owned()
}

#[test]
fn a_derived_instance_has_its_methods_written_out() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         type Shape = Dot | Box Int String\n  deriving Eq, Ord, Hash, Show\n\
         type Meters = Meters Float\n  deriving Eq\n",
    )
    .expect("lowers");
    // the same constructor and every field equal, by the instance of each field
    assert_eq!(
        derived_method(&ir, "Eq Shape", "eq"),
        "eq = (fun a b -> (match a | Dot -> (match b | Dot -> true | _ -> false) \
         | (Box x1_0 x1_1) -> (match b | (Box y1_0 y1_1) -> \
         (((<Eq Int>.eq x1_0) y1_0) && ((<Eq String>.eq x1_1) y1_1)) | _ -> false)))"
    );
    // by constructor in the order declared, then to the first field that is not `EQ`
    let compare = derived_method(&ir, "Ord Shape", "compare");
    assert!(
        compare.contains("let j = (match b | Dot -> 0 | (Box _ _) -> 1)"),
        "{compare}"
    );
    assert!(
        compare.contains("| _ -> ((<Ord Int>.compare 1) j)"),
        "{compare}"
    );
    assert!(
        compare.contains(
            "(match ((<Ord Int>.compare x1_0) y1_0) | EQ -> \
             (match ((<Ord String>.compare x1_1) y1_1) | EQ -> EQ | o1_1 -> o1_1) | o1_0 -> o1_0)"
        ),
        "{compare}"
    );
    // the index of the constructor, then `h * 31 + hash field`
    assert_eq!(
        derived_method(&ir, "Hash Shape", "hash"),
        "hash = (fun a -> (match a | Dot -> 0 | (Box x1_0 x1_1) -> \
         ((((1 * 31) + (<Hash Int>.hash x1_0)) * 31) + (<Hash String>.hash x1_1))))"
    );
    // the name, then each field as an argument is shown
    let show = derived_method(&ir, "Show Shape", "show");
    assert!(show.contains("| Dot -> \"Dot\""), "{show}");
    assert!(
        show.contains("(\"Box\" ++ \" \") ++ (Instances.show_arg (<Show Int>.show x1_0))"),
        "{show}"
    );
    // one constructor: nothing to tell apart
    assert_eq!(
        derived_method(&ir, "Eq Meters", "eq"),
        "eq = (fun a b -> (match a | (Meters x0_0) -> (match b | (Meters y0_0) -> \
         ((<Eq Float>.eq x0_0) y0_0))))"
    );
}

/// The operands of `==` in a local function keep one type, which its uses
/// settle, so it compares `String` and `Int` by identity as at the top level.
#[test]
fn equality_in_a_local_function_compares_by_identity() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         compared : Int -> Int ->{} Bool\n\
         let compared x y =\n  let same a b = a == b\n  let other a b = a != b\n  \
         same \"a\" \"b\" && other x y\n",
    )
    .expect("lowers");
    let printed = super::print(&ir);
    assert!(printed.contains(" === "), "{printed}");
    assert!(printed.contains(" !== "), "{printed}");
    assert!(!printed.contains(" == "), "{printed}");
    assert!(!printed.contains(" != "), "{printed}");
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "layout_tests.rs"]
mod layout_tests;
#[path = "owner_tests.rs"]
mod owner_tests;
#[path = "typed_ir_more_tests.rs"]
mod typed_ir_more_tests;
#[path = "typed_ir_tests.rs"]
mod typed_ir_tests;
