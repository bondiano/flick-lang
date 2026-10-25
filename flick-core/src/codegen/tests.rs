use super::{Labels, Options, js};
use crate::ir::tests::lower_text;
use crate::types::{Constructor, Rep};

fn compile(text: &str) -> String {
    compile_with(text, false, Labels::default()).code
}

fn compile_with(text: &str, release: bool, labels: Labels) -> super::Output {
    let ir = lower_text("App", text).expect("lowers");
    let path = |m: &str| format!("{}.js", m.replace('.', "/"));
    let package = |m: &str| (m == "App.Users").then(|| "@acme/users".to_owned());
    let foreign = |_: &str, spec: &str| spec.to_owned();
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: false,
        interface: &|_| None,
        release,
        workers: &|_, _| None,
        direct: &|_, _, _| false,
        labels,
    };
    js(&ir, &options)
}

#[test]
fn top_level_atoms_views_and_actions_are_labelled_by_their_names() {
    let text = "module App.Counter exposing (..)\n\
         count : Atom.Writable Number\nlet count = Atom.make 0\n\
         reset : Action.Action Ui.ClickSnapshot\n\
         let reset = Action.exhaust (fun _ -> task { Atom.set count 0 })\n\
         counter : Ui.View Unit\n\
         let counter = Ui.view (fun () -> ui { button { disabled = false; on_click = reset } [text (show (Atom.value count))] })\n\
         start : Number\nlet start = 0\n";
    let labels = |release, labels| {
        let code = compile_with(text, release, labels).code;
        assert_eq!(
            code.contains("$rt.label("),
            labels != Labels::None || !release,
            "{code}"
        );
        ["count", "reset", "counter", "start"]
            .into_iter()
            // a long call breaks after `$rt.label(`
            .filter(|name| code.contains(&format!("\"App.Counter.{name}\"")))
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(false, Labels::None), ["count", "reset", "counter"]);
    assert_eq!(labels(true, Labels::All), ["count", "reset", "counter"]);
    assert_eq!(labels(true, Labels::Components), ["reset", "counter"]);
    assert!(labels(true, Labels::None).is_empty());
    let named: Vec<_> = compile_with(text, false, Labels::default())
        .mappings
        .into_iter()
        .filter_map(|(.., name)| name)
        .collect();
    assert_eq!(named, ["count", "reset", "counter", "start"]);
}

#[test]
fn pure_functions_are_plain_js() {
    let js = compile(
        "module App.Text exposing (..)\n\
         identity : a -> a\nlet identity value = value\n\
         greet : String -> String -> String\nlet greet prefix name = prefix ++ \", \" ++ name\n\
         hello : String -> String\nlet hello = greet \"Hello\"\n\
         with_default : a -> Option a -> a\n\
         let with_default fallback value =\n  match value with\n  | None -> fallback\n  | Some x -> x\n",
    );
    assert!(!js.contains("Effect"), "{js}");
    assert!(!js.contains("yield"), "{js}");
    insta::assert_snapshot!(js);
}

#[test]
fn runtime_stages_yield_only_on_runtime_calls() {
    let js = compile(
        "module App.Users exposing (..)\n\
         type alias User = { id : Number; name : String }\n\
         type error NotFound = NotFound Number\n\
         service Users =\n  get : Number ->{none; NotFound} User\n\
         service Log =\n  info : String ->{} Unit\n\
         normalize_name : String -> String\nlet normalize_name name = String.trim name\n\
         load_name : Number ->{Users, Log; NotFound} String\n\
         let load_name id =\n  let user = Users.get id\n  let name = normalize_name user.name\n\n  Log.info name\n\n  name\n",
    );
    insta::assert_snapshot!(js);
    let body = js
        .split("export const load_name")
        .nth(1)
        .expect("load_name");
    assert!(body.contains("Effect.gen(function* ()"), "{body}");
    assert_eq!(body.matches("yield*").count(), 2, "{body}");
}

#[test]
fn constructor_ids_name_package_module_and_type() {
    let js = compile(
        "module App.Users exposing (..)\n\
         type Choice = Some Number | Empty\n",
    );
    // the name, and a hash of package, module, type and name
    let c = |module: &str| Constructor {
        module: module.to_owned(),
        ty: "Choice".to_owned(),
        name: "Some".to_owned(),
        arity: 1,
        rep: Rep::Tagged,
    };
    let tag = super::tag(&c("App.Users"), Some("@acme/users"));
    assert!(
        tag.starts_with("Some#") && tag.len() == "Some#".len() + 12,
        "{tag}"
    );
    assert!(js.contains(&format!("_tag: \"{tag}\"")), "{js}");
    assert_ne!(tag, super::tag(&c("App.Users"), Some("@acme/other")));
    assert_ne!(tag, super::tag(&c("App.Other"), Some("@acme/users")));
    let native =
        compile("module App.Main exposing (..)\nnative : Option Number\nlet native = Some 1\n");
    assert!(
        native.contains("export const native = /*#__PURE__*/ $rt.some(1);"),
        "{native}"
    );
}

#[test]
fn debug_imports_a_module_by_its_own_name_unless_taken() {
    let js = compile(
        "module App.Main exposing (..)\n\n\
         type Shape = Option Number | Dot\n\n\
         line : Unit ->{} Unit\nlet line _ = Platform.stdout_line (show (Option.with_default 0 (Some 1)))\n",
    );
    assert!(js.contains("import * as Platform from"), "{js}");
    assert!(js.contains("Platform.stdout_line("), "{js}");
    // the constructor `Option` keeps the module `Option` apart
    assert!(js.contains("import * as $m_Option from"), "{js}");
    assert!(js.contains("export const line = (_) =>"), "{js}");
}

/// The body of the exported `name` in `js`, up to the next export.
fn export_of<'a>(js: &'a str, name: &str) -> &'a str {
    let start = js
        .find(&format!("export const {name} "))
        .unwrap_or_else(|| panic!("no {name} in {js}"));
    let rest = &js[start + 1..];
    &js[start..start + 1 + rest.find("\nexport ").unwrap_or(rest.len())]
}

#[test]
fn a_tail_call_assigns_only_the_arguments_it_changes() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         find : (a -> Bool) -> List a -> Option a\n\
         let rec find p xs =\n  match xs with\n  | [] -> None\n  | [x, ...rest] -> if p x then Some x else find p (List.drop 1 rest)\n",
    );
    let find = export_of(&js, "find");
    assert!(find.contains("for (;;)"), "{find}");
    assert!(!find.contains("const $t"), "{find}");
    // only the initialization sets `p`
    assert_eq!(find.matches("$l0 =").count(), 1, "{find}");
    assert!(find.contains("$l1 ="), "{find}");
}
