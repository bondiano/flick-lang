//! Builds of several modules through `generate`: the chunks' load order and
//! prologue, their source maps, and refusals at the user's source.

use flick_backend::{
    Backend, BuildRequest, Diagnostic, InputError, Inputs, ModuleInput, ModuleRole, Source,
};
use flick_ir::{Arith, BinOp, ExprKind, FileId, Initialization, Item, Lit, Prefix, Program, Span};

use super::{lambda, local, param, value_at};
use crate::Lua;
use crate::tests::{build_request, module};

/// `src/App.fl`, whose `main` calls `List.<name> ()`.
const APP: &str = "module App\nlet main () = List.id ()\n";

fn at(file: u32, start: u32, end: u32) -> Span {
    Span::new(FileId::new(file), start, end)
}

fn item(item: Item, file: u32, start: u32, end: u32) -> Item {
    Item {
        span: at(file, start, end),
        ..item
    }
}

/// `App.main` calling `List.<name>` at `25..35`, and `List` of `list`.
fn program(name: &str, list: Vec<Item>, app: Vec<Item>) -> Program {
    let call = flick_ir::Expr::new(
        ExprKind::App {
            func: Box::new(flick_ir::Expr::new(
                ExprKind::Global {
                    module: "List".into(),
                    name: name.into(),
                    type_args: vec![],
                },
                at(0, 25, 32),
            )),
            arg: Box::new(flick_ir::Expr::new(ExprKind::Lit(Lit::Unit), at(0, 33, 35))),
            stage: flick_ir::Stage::Pure,
            arg_first: false,
        },
        at(0, 25, 35),
    );
    let main = item(value_at("main", lambda(&["_"], call), 0, 0), 0, 11, 35);
    let mut app_items = vec![main];
    app_items.extend(app);
    Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: "test".into(),
        files: vec!["src/App.fl".into(), "List.fl".into()],
        modules: vec![module("List", list), module("App", app_items)],
        lang: vec![],
    }
}

fn request(platform: &str, program: &Program) -> BuildRequest {
    let input = |name: &str, role| ModuleInput {
        name: name.into(),
        role,
        package: None,
        output: format!("{name}.js"),
        interface: None,
    };
    let mut request = build_request(
        platform,
        &["lua", "lua51"],
        program,
        vec![
            input("App", ModuleRole::Own),
            input("List", ModuleRole::Builtin),
        ],
    );
    request.sources = vec![
        Source {
            name: "src/App.fl".into(),
            input: "source/0".into(),
            line_starts: vec![0, 11, 36],
        },
        Source {
            name: "List.fl".into(),
            input: "source/1".into(),
            line_starts: vec![0],
        },
    ];
    request
}

/// The source texts of the build.
struct Texts;
impl Inputs for Texts {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        match id {
            "source/0" => Ok(APP.as_bytes().to_vec()),
            "source/1" => Ok(b"let id x = x\n".to_vec()),
            _ => Err(InputError::Undeclared),
        }
    }
}

fn identity() -> Item {
    item(value_at("id", lambda(&["x"], local("x")), 0, 0), 1, 0, 12)
}

#[test]
fn a_module_loads_after_the_modules_it_uses() {
    let program = program("id", vec![identity()], vec![]);
    let built = Lua
        .generate(&request("wow-retail", &program), &mut Texts)
        .expect("generated");
    let paths: Vec<&str> = built.artifacts.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "_flick/List.lua",
            "_flick/List.lua.map",
            "App.lua",
            "App.lua.map"
        ]
    );
    let text = |i: usize| String::from_utf8(built.artifacts[i].bytes.clone()).expect("UTF-8");
    assert_eq!(
        text(0),
        "local _, ns = ...\nlocal M = {}\nns.List = M\nM.id = function(x)\n  return x\nend\n"
    );
    assert_eq!(
        text(2),
        "local _, ns = ...\nlocal M = {}\nns.App = M\nM.main = function(_)\n  return \
         ns.List.id(true)\nend\n"
    );
    // the prologue and the definition map to `main`, at line 1 column 0;
    // the return to `List.id ()`, 14 columns on
    let map = text(3);
    assert!(
        map.contains(r#""mappings":"AACA;AAAA;AAAA;AAAA;AAAc""#),
        "{map}"
    );
    assert!(map.contains(r#""sources":["src/App.fl"]"#), "{map}");
    assert!(map.contains(r#""file":"App.lua""#), "{map}");
}

fn refused(program: &Program) -> Vec<Diagnostic> {
    Lua.generate(&request("wow-retail", program), &mut Texts)
        .expect_err("refused")
}

#[test]
fn a_refusal_in_a_built_in_module_is_reported_where_it_was_reached() {
    let sum = flick_ir::Expr::new(
        ExprKind::Binary(
            BinOp::Arith(Arith::Add, flick_ir::Num::Int),
            Box::new(local("x")),
            Box::new(local("x")),
        ),
        at(1, 13, 18),
    );
    let double = item(value_at("double", lambda(&["x"], sum), 0, 0), 1, 0, 20);
    let found = refused(&program("double", vec![double], vec![]));
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].code, "P012");
    assert!(found[0].message.contains("`+` of `Int`"), "{found:#?}");
    let span = found[0].span.as_ref().expect("a span");
    assert_eq!((span.file, span.start, span.end), (0, 25, 32));
    assert_eq!(found[0].notes, ["reached through `List.double`"]);
}

#[test]
fn a_vm_limit_and_lexical_prefixes_are_refused_at_the_source() {
    let lets = (0..200)
        .map(|i| {
            flick_ir::Stmt::let_bind(
                flick_ir::Pat::Bind(format!("x{i}")),
                flick_ir::Expr::new(ExprKind::Lit(Lit::Int(0)), at(0, 100 + i, 101 + i)),
            )
        })
        .collect();
    let body = flick_ir::Expr::new(ExprKind::Block(lets, Box::new(local("x0"))), at(0, 0, 0));
    let wide = flick_ir::Expr::new(
        ExprKind::Lambda {
            params: vec![param("_")],
            body: Box::new(body),
        },
        at(0, 0, 0),
    );
    let wide = item(value_at("wide", wide, 0, 0), 0, 40, 60);
    let mut program = program("id", vec![identity()], vec![wide]);
    // `wide` is reached as an export of a library
    program.modules[1].exports.values.push(flick_ir::Exported {
        name: "wide".into(),
        sync: None,
        projection: None,
    });
    let mut request = request("wow-retail", &program);
    request.entries.clear();
    let found = Lua
        .generate(&request, &mut Texts)
        .expect_err("over the limit");
    let span = found[0].span.as_ref().expect("a span");
    assert_eq!(
        (span.file, span.start, span.end),
        (0, 299, 300),
        "{found:#?}"
    );
    assert!(found[0].message.contains("201 locals"), "{found:#?}");
    program.modules[1].initialization = Initialization {
        prefixes: vec![Prefix {
            before: 0,
            bindings: vec!["main".into()],
        }],
    };
    let found = refused(&program);
    assert!(
        found
            .iter()
            .any(|d| d.message.contains("the lexical module prefixes of `App`")),
        "{found:#?}"
    );
}
