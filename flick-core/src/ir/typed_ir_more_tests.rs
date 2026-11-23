//! Additional typed IR regression tests: polymorphic `do`/`applicative_do`/`for`
//! prelude methods, higher-rank `Forall` / existential `Exists` / open `Record`
//! types, generalized local functions (#862), and bounded `opt/eval` folding (#859).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use flick_ir::{Expr, ExprKind, IrType, ItemKind, Program, Stmt};
use flick_test_support::files as fixture_files;

use super::lower_text;
use crate::diagnostics::codes;
use crate::ir::lang_items;
use crate::opt::optimize;
use crate::types;

fn validate_module(module: &flick_ir::Module) -> Vec<String> {
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    let mut files = BTreeSet::new();
    module.clone().each_span_mut(&mut |s| {
        files.insert(s.file);
    });
    let program = Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![module.clone()],
        lang: lang_items(),
    };
    flick_ir::validate(&program, &builtin, &files)
}

fn collect_exprs<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    out.push(expr);
    expr.each_child(&mut |child| collect_exprs(child, out));
}

fn find_body<'a>(module: &'a flick_ir::Module, wanted: &str) -> &'a Expr {
    module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, body, .. } if name == wanted => Some(body),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing value `{wanted}`"))
}

#[test]
fn do_applicative_do_and_for_methods_carry_type_args() {
    let src = "\
module App exposing (..)

monadic : Option Int -> Option String
let monadic mx =
  do {
    x <- mx
    if x > 0
    Some (show x)
  }

applicative : Option Int -> Option Int -> Option Int
let applicative ma mb =
  do {
    a <- ma and b <- mb
    pure (a + b)
  }
";
    let module = lower_text("App", src).expect("lowers");
    let problems = validate_module(&module);
    assert!(problems.is_empty(), "validation failed: {problems:?}");

    let method_args = |fn_name: &str, method_name: &str| -> Vec<Vec<String>> {
        let mut exprs = Vec::new();
        collect_exprs(find_body(&module, fn_name), &mut exprs);
        exprs
            .into_iter()
            .filter_map(|e| match &e.kind {
                ExprKind::Method {
                    name, type_args, ..
                } if name == method_name => Some(
                    type_args
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .collect()
    };

    let flat_maps = method_args("monadic", "flat_map");
    assert!(
        !flat_maps.is_empty() && flat_maps.iter().all(|args| !args.is_empty()),
        "do-notation flat_map must carry type_args, got: {flat_maps:?}"
    );
    let nothings = method_args("monadic", "nothing");
    assert!(
        !nothings.is_empty() && nothings.iter().all(|args| !args.is_empty()),
        "do-notation guard nothing must carry type_args, got: {nothings:?}"
    );
    let map2s = method_args("applicative", "map2");
    assert!(
        !map2s.is_empty() && map2s.iter().all(|args| !args.is_empty()),
        "applicative do map2 must carry type_args, got: {map2s:?}"
    );
}

#[test]
fn lowered_ir_preserves_forall_exists_and_open_record_types() {
    let src = "\
module App exposing (..)

type module Counter =
  zero : Int

type alias Packed = exists K : Counter. Unit

apply_poly : (forall a. a -> a) -> (Int, String)
let apply_poly f = (f 1, f \"ok\")

keep_packed : Packed -> Packed
let keep_packed p = p

get_x : { x : Int | r } -> Int
let get_x row = row.x
";
    let module = lower_text("App", src).expect("lowers");
    let problems = validate_module(&module);
    assert!(problems.is_empty(), "validation failed: {problems:?}");

    let item_ty = |wanted: &str| {
        module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Value { name, ty, .. } if name == wanted => ty.clone(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing typed value `{wanted}`"))
    };

    let apply_ty = item_ty("apply_poly");
    assert!(
        matches!(
            &*apply_ty,
            IrType::Fun { param, .. } if matches!(&**param, IrType::Forall { vars, .. } if vars == &["a"])
        ),
        "expected IrType::Forall parameter in apply_poly, got: {apply_ty}"
    );

    let packed_ty = item_ty("keep_packed");
    assert!(
        matches!(
            &*packed_ty,
            IrType::Fun { param, result, .. }
                if matches!(&**param, IrType::Exists { .. })
                    && matches!(&**result, IrType::Exists { .. })
        ),
        "expected IrType::Exists in keep_packed, got: {packed_ty}"
    );

    let get_x_ty = item_ty("get_x");
    assert!(
        matches!(
            &*get_x_ty,
            IrType::Fun { param, .. }
                if matches!(&**param, IrType::Record { fields, tail: Some(_) } if fields.len() == 1 && fields[0].0 == "x")
        ),
        "expected open IrType::Record with tail in get_x, got: {get_x_ty}"
    );
}

#[test]
fn local_generalization_and_eval_folding_preserve_valid_types() {
    let src = "\
module App exposing (..)

local_poly : Unit -> (Int, String, Int, String)
let local_poly () =
  let id x = x
  let f = (fun x -> x)
  (id 1, id \"a\", f 2, f \"b\")

wrap : a -> Option a
let wrap x = Some x

fact : Int -> Int
let rec fact n =
  if n <= 1 then 1 else n * fact (n - 1)

folded : Unit -> (Option Int, Int)
let folded () = (wrap 42, fact 5)
";
    let mut module = lower_text("App", src).expect("lowers");
    assert!(validate_module(&module).is_empty());

    // Verify local `id` got generalized with a type parameter.
    let mut exprs = Vec::new();
    collect_exprs(find_body(&module, "local_poly"), &mut exprs);
    let has_poly_id = exprs.iter().any(|e| match &e.kind {
        ExprKind::Block(stmts, _) => stmts.iter().any(|s| {
            matches!(s, Stmt::Fun { name, type_params, .. } if name == "id" && !type_params.is_empty())
        }),
        _ => false,
    });
    assert!(
        has_poly_id,
        "local `id` should carry generalized type_params"
    );

    optimize(
        &mut module,
        &|n| n == "local_poly" || n == "folded",
        &|_, _| None,
        &|_, _| None,
    );
    let problems = validate_module(&module);
    assert!(
        problems.is_empty(),
        "post-opt validation failed: {problems:?}"
    );
}

#[test]
fn every_passing_fixture_optimizes_preserving_valid_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass");
    let paths = fixture_files(&root, "fl");
    let mut failures = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).expect("utf-8");
        match lower_text("Fixture", &text) {
            Ok(mut module) => {
                optimize(&mut module, &|_| true, &|_, _| None, &|_, _| None);
                let problems = validate_module(&module);
                if !problems.is_empty() {
                    failures.push(format!("{}: {problems:?}", path.display()));
                }
            }
            Err(d) => assert!(
                d.iter().all(|d| d.code == codes::P012),
                "{}: {d:?}",
                path.display()
            ),
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) failed post-optimization IR validation:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
