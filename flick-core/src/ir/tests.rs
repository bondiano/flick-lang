use std::path::{Path, PathBuf};
use std::{env, fs};

use crate::diagnostics::{Diagnostic, codes};
use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::{syntax, types};

/// Checks one self-contained module and lowers it.
pub(crate) fn lower_text(name: &str, text: &str) -> Result<super::Module, Vec<Diagnostic>> {
    check_text(name, text).map(|(ir, _)| ir)
}

/// [`lower_text`] with the module's interface.
pub(crate) fn check_text(
    name: &str,
    text: &str,
) -> Result<(super::Module, Interface), Vec<Diagnostic>> {
    let mut sources = SourceMap::default();
    let file = sources.add(format!("{name}.fl"), text);
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module_name = syntax.header.as_ref().map_or(name, |h| h.name.as_str());
    let module = Module {
        name: module_name,
        file,
        text,
        tree: &parse.tree,
        syntax: &syntax,
    };
    let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &builtin);
    let local = |n: &str| n == module_name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        sql: Default::default(),
    };
    let (more, typing) = types::check_typed(
        &module,
        &builtin,
        types::builtins(),
        options,
        &mut interface,
    );
    diagnostics.extend(parse.diagnostics);
    diagnostics.extend(more);
    diagnostics.retain(|d| !d.unnecessary());
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok((super::lower(&module, &typing)?, interface))
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("readable") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            fixtures(&path, out);
        } else if path.extension().is_some_and(|e| e == "fl") {
            out.push(path);
        }
    }
}

#[test]
fn every_passing_fixture_lowers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass");
    let mut paths = Vec::new();
    fixtures(&root, &mut paths);
    for path in paths {
        let text = fs::read_to_string(&path).expect("utf-8");
        if let Err(d) = lower_text("Fixture", &text) {
            // blocks compiled by later phases are reported, never miscompiled
            assert!(
                d.iter().all(|d| d.code == codes::P012),
                "{}: {d:?}",
                path.display()
            );
        }
    }
}

#[test]
fn ir_makes_stages_dictionaries_and_lifts_explicit() {
    let ir = lower_text(
        "App",
        "module App exposing (..)\n\
         type error NotFound = NotFound Number\n\
         service Users =\n  get : Number ->{; NotFound} String\n\
         type Shape = Circle Number | Square Number\n  deriving Eq\n\
         render : Show a => a -> String\nlet render value = show value\n\
         load : Number ->{Users; none} String\n\
         let load id =\n  let name = try Users.get id with\n    | NotFound _ -> \"?\"\n  Platform.stdout_line (render 1 ++ name)\n  name\n\
         twice : (Number ->{} Number) -> Number ->{} Number\nlet twice f x = f (f x)\n\
         inc : Number -> Number\nlet inc x = x + 1\n\
         run : Unit ->{} Number\nlet run () = twice inc 1\n\
         pick : Option Number -> Number\nlet pick o =\n  match o with\n  | Some (1 | 2) -> 0\n  | Some n when n > 3 -> n\n  | _ -> -1\n",
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
    assert_eq!(body("save_button"), body("save_button_ui"));
}
