use super::{Options, js};
use crate::ir::tests::lower_text;

fn compile(text: &str) -> String {
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
    };
    js(&ir, &options).code
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
    assert!(
        js.contains("_tag: \"@acme/users/App.Users.Choice.Some\""),
        "{js}"
    );
    let native =
        compile("module App.Main exposing (..)\nnative : Option Number\nlet native = Some 1\n");
    assert!(
        native.contains("export const native = $rt.some(1);"),
        "{native}"
    );
}
