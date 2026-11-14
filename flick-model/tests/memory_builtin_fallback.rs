//! Checked project roots override individual builtin entities, not whole modules.
use flick_core::iface::{Namespace, SupportType};
use flick_model::run;
use std::sync::Arc;

fn stdout(source: &str, expected: &str) {
    let modules = run::compile_source(source).expect("source checks with builtin entity fallback");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, 100_000, release)
            .unwrap_or_else(|error| panic!("release={release}: {error}"));
        assert_eq!(ran.stdout, expected, "release={release}");
        assert_eq!(ran.memory.live_objects, 0);
        assert_eq!(ran.memory.allocations, ran.memory.frees);
    }
}

#[test]
fn disjoint_app_entities_do_not_compete_with_builtin_app() {
    stdout(
        r#"module App exposing (main)
main : Unit ->{} Unit
let main () = Platform.stdout_line "kept"
"#,
        "kept\n",
    );
}

#[test]
fn explicit_app_value_overrides_the_matching_builtin_entity() {
    stdout(
        r"module App exposing (main)
fail : Int -> Int
let fail value = value + 1
main : Unit ->{} Unit
let main () = Platform.stdout_line (show (fail 41))
",
        "42\n",
    );
}

#[test]
fn same_module_builtin_scalar_fallback_keeps_its_checked_declaration() {
    stdout(
        r"module BigInt exposing (main)
main : Unit ->{} Unit
let main () = Platform.stdout_line (show (BigInt.to_number 7n))
",
        "7\n",
    );
}

#[test]
fn explicit_scalar_scheme_and_ir_override_together_without_losing_other_builtins() {
    stdout(
        r#"module BigInt exposing (main)
to_string : Int -> BigInt
foreign pure to_string from global "BigInt"
main : Unit ->{} Unit
let main () =
  Platform.stdout_line (show (to_string 42) ++ "|" ++ show (BigInt.to_number 9n))
"#,
        "42n|9\n",
    );
}

#[test]
fn independent_explicit_roots_are_not_silently_coalesced() {
    let mut modules =
        run::compile_source("module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n")
            .expect("first explicit root checks");
    let main = run::main_of(&modules).expect("main export");
    modules.extend(
        run::compile_source("module App exposing (other)\nother : Int\nlet other = 9\n")
            .expect("second independent root checks"),
    );
    for release in [false, true] {
        let Err(error) = run::run_main_counted(&modules, &main, 100_000, release) else {
            panic!("independent explicit App roots must be rejected");
        };
        assert!(
            error.contains("duplicate declaration context App"),
            "{error}"
        );
    }
}

#[test]
fn malformed_explicit_support_order_is_not_repaired_by_builtin_composition() {
    let mut modules = run::compile_source(
        "module App exposing (main)\ntype alias A = Int\ntype alias B = String\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("checked support definitions");
    let main = run::main_of(&modules).expect("main export");
    let declarations = Arc::make_mut(&mut modules[0].declarations);
    for name in ["B", "A"] {
        let index = declarations
            .exports
            .iter()
            .position(|export| {
                export.entity.name == name && export.entity.namespace == Namespace::Type
            })
            .expect("alias fact");
        let export = declarations.exports.remove(index);
        declarations.support_types.push(SupportType {
            entity: export.entity,
            definition: export.definition.expect("checked alias representation"),
            owner_path: vec![],
        });
    }
    for release in [false, true] {
        let Err(error) = run::run_main_counted(&modules, &main, 100_000, release) else {
            panic!("malformed original support order must remain rejected");
        };
        assert!(
            error.contains("support entities must be unique and sorted"),
            "{error}"
        );
    }
}
