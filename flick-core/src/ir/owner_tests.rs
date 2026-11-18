//! Acquisition must use the region in its row, including across provider callbacks.
use std::fmt::Write;

use super::lower_text;
use crate::ir::{BinOp, Expr, ExprKind, ItemKind, Lit, Module, Pat, Stage, Stmt, print};
use crate::numeric::Arith;

fn transport_invariants(module: &Module) {
    fn visit(expr: &Expr) {
        assert!(
            !matches!(expr.kind, ExprKind::Within(_, _)),
            "compiled code must not change the default owner: {expr:?}"
        );
        if let ExprKind::WithOwnerEvidence(_, body) = &expr.kind {
            let atomic = |e: &Expr| matches!(e.kind, ExprKind::Local(_) | ExprKind::Lit(_));
            match &body.kind {
                ExprKind::App {
                    func, arg, stage, ..
                } => assert!(*stage != Stage::Pure && atomic(func) && atomic(arg)),
                ExprKind::Await(value) => assert!(atomic(value)),
                other => panic!("noncanonical evidence body: {other:?}"),
            }
        }
        expr.each_child(&mut visit);
    }
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            visit(body);
        }
    }
    assert!(
        !print(module).contains("Scope.current"),
        "execution evidence must not import Scope"
    );
}

fn assert_recursive_owner_adapter(module: &Module) {
    fn count(expr: &Expr, name: &str) -> usize {
        let mut found = usize::from(matches!(&expr.kind, ExprKind::Local(local) if local == name));
        expr.each_child(&mut |child| found += count(child, name));
        found
    }
    fn visit(expr: &Expr, found: &mut bool) {
        if let ExprKind::Block(statements, _) = &expr.kind {
            for statement in statements {
                if let Stmt::Fun {
                    name,
                    rec: true,
                    value,
                    ..
                } = statement
                {
                    assert!(
                        count(value, name) > 0,
                        "recursive adapter must reenter its own dispatcher"
                    );
                    *found = true;
                }
            }
        }
        expr.each_child(&mut |child| visit(child, found));
    }
    let mut found = false;
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            visit(body, &mut found);
        }
    }
    assert!(
        found,
        "recursive foreign policy requires a recursive ordinary-IR adapter"
    );
}

fn lower_body(body: &str) -> String {
    let text = format!(
        "module Test exposing (main)\nservice Read =\n  read : Unit ->{{}} String\nmain : Unit ->{{}} Unit\nlet main () = {body}\n"
    );
    let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{text}\n{d:?}"));
    transport_invariants(&module);
    print(&module)
}

fn check_owner(body: &str) -> String {
    let ir = lower_body(body);
    assert!(
        ir.contains("(with-owner-evidence ") && ir.contains("Arena.make"),
        "acquisition must explicitly receive its lexical owner:\n{ir}"
    );
    ir
}

fn lexical_case(scope: &str, body: &str) {
    let (declaration, body) = if body.contains('\n') {
        body.rsplit_once('\n').expect("a local helper and its call")
    } else {
        ("", body)
    };
    check_owner(&format!(
        "{scope} {{\n    let layer = Layer.succeed Read {{ read = fun () -> \"unused\" }}\n    {declaration}\n    let _arena = {body}\n    ()\n  }}"
    ));
}

macro_rules! lexical_case {
    ($name:ident, $scope:expr, $body:expr) => {
        #[test]
        fn $name() {
            lexical_case($scope, $body);
        }
    };
}

lexical_case!(
    anonymous_literal_callback,
    "scope",
    "Layer.run layer (fun () -> Arena.make ())"
);
lexical_case!(
    named_literal_callback,
    "scope outer",
    "Layer.run layer (fun () -> Arena.make ())"
);
lexical_case!(
    anonymous_provide,
    "scope",
    "provide layer { Arena.make () }"
);
lexical_case!(
    named_provide,
    "scope outer",
    "provide layer { Arena.make () }"
);
lexical_case!(
    anonymous_local_callback,
    "scope",
    "let acquire () = Arena.make ()\n    Layer.run layer acquire"
);
lexical_case!(
    named_local_callback,
    "scope outer",
    "let acquire () = Arena.make ()\n    Layer.run layer acquire"
);
lexical_case!(
    anonymous_growing_row,
    "scope",
    "forward : (Unit ->{r} a) ->{r} a\n    let forward f = f ()\n    Layer.run layer (fun () -> forward (fun () -> Arena.make ()))"
);
lexical_case!(
    named_growing_row,
    "scope outer",
    "forward : (Unit ->{r} a) ->{r} a\n    let forward f = f ()\n    Layer.run layer (fun () -> forward (fun () -> Arena.make ()))"
);

#[test]
fn higher_rank_owner_survives_nested_provider() {
    let ir = lower_body(
        "scope unrelated {\n    Scope.run (fun _ ->\n      let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n      let _arena = Layer.run layer (fun () -> Arena.make ())\n      ()\n    )\n  }",
    );
    assert!(
        ir.contains("(with-owner-evidence ") && ir.contains("read-owner-evidence"),
        "the supplied owner must be bound before the nested provider opens:\n{ir}"
    );
    assert!(
        !ir.contains("with-owner-evidence unrelated"),
        "must not select unrelated outer region:\n{ir}"
    );
}

fn lower_retained(declarations: &str, call: &str) -> String {
    let text = format!(
        "module Test exposing (main)\n\
         service Read =\n  read : Unit ->{{}} String\n\
         acquire : Unit ->{{Scope s}} Arena.Value s String\n\
         let acquire () = Arena.alloc (Arena.make ()) \"owned\"\n\
         {declarations}\n\
         main : Unit ->{{Host.Console}} Unit\n\
         let main () = scope {{\n\
           let layer = Layer.succeed Read {{ read = fun () -> \"unused\" }}\n\
           let retained = {call}\n\
           Platform.stdout_line (Arena.get retained)\n\
         }}\n"
    );
    let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{text}\n{d:?}"));
    transport_invariants(&module);
    print(&module)
}

include!("owner_tests/callback_transport.rs");
include!("owner_tests/foreign_activation.rs");
include!("owner_tests/recursive_foreign.rs");
