//! Compiler-only owner plumbing preserves the public foreign scheme.
use std::collections::HashSet;
use std::{fs, path::Path as FilePath};

use flick_core::ir::ItemKind;
use flick_core::markdown::blocks;

use flick_model::{
    check::{TypeError, check_program},
    eval::Outcome,
    run,
    syntax::{Name, Path},
};

fn declared_external(error: &TypeError) -> bool {
    error.rule == "Translation" && error.message.starts_with("no native for")
}

#[test]
fn scoped_foreign_keeps_the_polymorphic_callback_row() {
    let source = r#"module OwnerScheme exposing (add_finalizer)
add_finalizer : (Eff r none Unit)^c ->{Scope s | r} Unit
foreign effect add_finalizer from "./scope.js" "addFinalizer"
  retention owner
"#;
    let modules = run::compile_source(source).expect("accepted scoped foreign");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(_, error)| !declared_external(error))
        .collect();
    assert!(
        errors.is_empty(),
        "owner plumbing changes the scheme: {errors:?}"
    );
}

#[test]
fn scoped_foreign_returned_function_waits_for_its_acquisition_owner() {
    let source = r#"module OwnerReturned exposing (make)
make : Fiber.Provided r s => Unit ->{Scope s | r} (Unit ->{Async} Unit)^{s}
foreign effect make from "./scope.js" "make"
  retention owner
"#;
    let modules = run::compile_source(source).expect("accepted returned function foreign");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(_, error)| !declared_external(error))
        .collect();
    assert!(
        errors.is_empty(),
        "owner information arrives too late: {errors:?}"
    );
}

#[test]
fn rpc_handler_preserves_its_scoped_factory_and_returned_callable_scheme() {
    let program = run::program(&[]);
    let decls = run::decls(&[]);
    let checked = HashSet::from([Name::from("Rpc")]);
    let path = Path::new("Rpc", "handler");
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(p, error)| *p == path && !declared_external(error))
        .collect();
    assert!(
        errors.is_empty(),
        "returned factory scheme changed: {errors:?}"
    );
}

#[test]
fn a_pure_foreign_factory_keeps_its_returned_runtime_arrow() {
    let source = r#"module OwnerFuture exposing (make)
make : Unit -> (Unit ->{Scope s} Unit)
foreign pure make from "./scope.js" "make"
  arity 1
  retention call
"#;
    let modules = run::compile_source(source).expect("accepted future runtime arrow");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(_, error)| !declared_external(error))
        .collect();
    assert!(
        errors.is_empty(),
        "returned runtime stage became pure: {errors:?}"
    );
}

#[test]
fn rpc_scoped_partial_operands_keep_open_region_obligations_monomorphic() {
    let file =
        FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../docs/src/content/docs/reference/rpc.md");
    let page = fs::read_to_string(file).expect("RPC documentation");
    let source = blocks(&page)
        .into_iter()
        .find(|block| block.text.contains("let double"))
        .expect("scoped router example")
        .text;
    let modules = run::compile_source(&source).expect("accepted scoped RPC example");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    let errors: Vec<_> = check_program(&program, &decls, &checked)
        .into_iter()
        .filter(|(path, error)| {
            !declared_external(error)
                && !(error.rule == "Translation"
                    && ((path.name.as_ref() == "double"
                        && error.message == "`Record.label` has no λFlick form")
                        || (path.name.as_ref() == "users"
                            && error.message.starts_with("the row primitive `Lacks "))))
        })
        .collect();
    assert!(
        errors.is_empty(),
        "partial operand lost its region obligation: {errors:?}"
    );
}

#[test]
fn returned_raw_values_do_not_generalize_open_instance_bounds() {
    let program = run::program(&[]);
    let decls = run::decls(&[]);
    for (module, name) in [("RcMap", "get"), ("Resource", "acquire_release")] {
        let path = Path::new(module, name);
        let errors: Vec<_> = check_program(&program, &decls, &HashSet::from([Name::from(module)]))
            .into_iter()
            .filter(|(p, error)| *p == path && !declared_external(error))
            .collect();
        assert!(
            errors.is_empty(),
            "instance bound lost for {path}: {errors:?}"
        );
    }
}

#[test]
fn an_inline_opaque_container_keeps_its_flattened_nominal_identity() {
    let source = r#"module InlineOwnerBox exposing (answer)
module Box exposing (Box, make, get) =
  opaque type Box a = Box a
  make : a -> Box a
  let make value = Box value
  get : Box a -> a
  let get (Box value) = value
answer : Unit -> String
let answer () = Box.get (Box.make "kept")
"#;
    let modules = run::compile_source(source).expect("accepted inline opaque container");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked = HashSet::from([Name::from("InlineOwnerBox")]);
    let errors = check_program(&program, &decls, &checked);
    assert!(
        errors.is_empty(),
        "inline constructor identity changed: {errors:?}"
    );
}

#[test]
fn an_inline_opaque_region_callback_keeps_its_flattened_identity() {
    let file =
        FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime/arena_owner_list.fl");
    let modules = run::compile(&file).expect("accepted opaque owner callback");
    let program = run::program(&modules);
    let decls = run::decls(&modules);
    let checked: HashSet<Name> = modules
        .iter()
        .map(|module| Name::from(module.name.as_str()))
        .collect();
    let errors = check_program(&program, &decls, &checked);
    assert!(
        errors.is_empty(),
        "opaque callback identity changed: {errors:?}"
    );
}

#[test]
fn inline_foreign_origin_resolves_to_its_own_scheme_not_a_parent_homonym() {
    let source = r#"module InlineForeign exposing (answer, make)
make : String -> String
let make value = value
module Inner exposing (make) =
  make : Unit ->{Scope s} Unit
  foreign effect make from "./scope.js" "make"
    retention owner
answer : Unit ->{Scope s} Unit
let answer () = Inner.make ()
"#;
    let modules = run::compile_source(source).expect("accepted inline scoped foreign");
    let module = modules
        .iter()
        .find(|m| m.name == "InlineForeign")
        .expect("parent");
    let foreign = module
        .ir
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(f) => Some(f),
            _ => None,
        })
        .expect("relocated raw foreign");
    assert_eq!(foreign.source_name.as_deref(), Some("Inner$make"));
    assert_eq!(
        foreign.binding.module_export(),
        Some(("./scope.js", "make"))
    );
    let errors = check_program(
        &run::program(&modules),
        &run::decls(&modules),
        &HashSet::from([Name::from("InlineForeign")]),
    );
    let errors: Vec<_> = errors
        .into_iter()
        .filter(|(_, e)| !declared_external(e))
        .collect();
    assert!(errors.is_empty(), "wrong raw source scheme: {errors:?}");
}

#[test]
fn a_task_argument_is_checked_at_its_rank_two_owner_before_lexical_defaulting() {
    let source = r#"module RankOwnerTask exposing (main)
acquire : Unit ->{Scope s} Unit
foreign effect acquire from "./unknown.mjs" "acquire"
  retention owner
consume : (forall t. Eff {Scope t} none Unit)^c ->{} Unit
foreign consume from "./unknown.mjs" "consume"
  retention call
main : Unit ->{} Unit
let main () = scope s {
  consume (task { acquire () })
}
"#;
    let modules = run::compile_source(source).expect("accepted rank-two task argument");
    let errors = check_program(
        &run::program(&modules),
        &run::decls(&modules),
        &HashSet::from([Name::from("RankOwnerTask")]),
    );
    let errors: Vec<_> = errors
        .into_iter()
        .filter(|(_, e)| !declared_external(e))
        .collect();
    assert!(
        errors.is_empty(),
        "task owner defaulted before checking: {errors:?}"
    );
}

#[test]
fn a_resource_refresh_task_is_polymorphic_in_its_acquisition_owner() {
    let source = r#"module ResourceTask exposing (main)
acquire : String ->{Scope s} String
let acquire name = Resource.acquire_release (task { name }) (fun _ -> ())
main : Unit ->{} Unit
let main () = scope s {
  let res : Resource.Resource s none String =
    Resource.manual (task { acquire "kept" })
  ()
}
"#;
    let modules = run::compile_source(source).expect("accepted resource task");
    let errors = check_program(
        &run::program(&modules),
        &run::decls(&modules),
        &HashSet::from([Name::from("ResourceTask")]),
    );
    let errors: Vec<_> = errors
        .into_iter()
        .filter(|(_, e)| !declared_external(e))
        .collect();
    assert!(errors.is_empty(), "acquisition owner lost: {errors:?}");
}

#[test]
fn scoped_leases_keep_rank_two_lookup_fields_until_their_record_is_checked() {
    let file = FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime/leases.fl");
    let modules = run::compile(&file).expect("accepted lease fixture");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "lease typing regressed: {errors:?}");
    let ran = run::run_main(&program, &run::main_of(&modules).expect("main"), 100_000);
    assert!(
        matches!(ran.outcome, Outcome::Stuck(message) if message == "no native for `from_acquire` (flick:ScopedRef flickFromAcquire)")
    );
    assert!(
        ran.stdout.is_empty(),
        "an unsupported host was not executed"
    );
}

#[test]
fn the_accepted_lease_contract_keeps_its_capture_and_rank_two_facts() {
    let file =
        FilePath::new(env!("CARGO_MANIFEST_DIR")).join("../tests/check/pass/regions/leases.fl");
    let modules = run::compile(&file).expect("accepted lease contract");
    let checked = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect::<HashSet<_>>();
    let errors = check_program(&run::program(&modules), &run::decls(&modules), &checked);
    let errors: Vec<_> = errors
        .into_iter()
        .filter(|(_, error)| !declared_external(error))
        .collect();
    assert!(
        errors.is_empty(),
        "lease contract lost its facts: {errors:?}"
    );
}
