#[test]
fn real_escaped_closures_preserve_shared_captures_in_both_profiles() {
    let text = r#"module App exposing (answer)

make : String -> String -> String
let make prefix = fun suffix -> prefix ++ suffix

answer : Unit -> String
let answer () =
  let append = make "hello"
  let discarded = append " discarded"
  append " world"
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("counted source execution");
        assert_eq!(heap.string(&ran.value), Ok("hello world"));
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("release result");
        assert_eq!(
            heap.finish().expect("no live source objects").live_objects,
            0
        );
    }
}

#[test]
fn real_curried_local_recursion_keeps_the_outer_self_and_capture() {
    let text = r#"module App exposing (answer)

answer : Unit -> String
let answer () =
  let prefix = "done: "
  loop : Int -> String -> String
  let rec loop n value =
    if n == 0 then prefix ++ value else loop (n - 1) value
  loop 2000 "kept"
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 1_000_000)
            .expect("curried recursion");
        assert_eq!(heap.string(&ran.value), Ok("done: kept"));
        heap.drop_value(ran.value).expect("release result");
        assert_eq!(heap.finish().expect("balanced recursion").live_objects, 0);
    }
}

#[test]
fn selected_corpus_cases_run_on_the_counted_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in [
        "bootstrap/hello",
        "bootstrap/inline_declarations",
        "bootstrap/inline_module",
        "bootstrap/inline_nested",
        "bootstrap/inline_pack",
        "bootstrap/inline_pack_nominals",
        "bootstrap/packed_module",
        "codegen/call_site",
        "codegen/classes",
        "codegen/closures",
        "codegen/composition",
        "codegen/defaults",
        "codegen/default_signatures",
        "codegen/derive_default",
        "codegen/counted_failures",
        "codegen/stage_order",
        "codegen/eval_order",
        "codegen/errors",
        "codegen/exports",
        "codegen/effect_sets",
        "codegen/enums",
        "codegen/fail",
        "codegen/handlers",
        "codegen/partial_handlers",
        "codegen/prototype_operation",
        "codegen/service_dictionary",
        "codegen/service_place",
        "codegen/services",
        "codegen/task_twice",
        "codegen/given_shadow",
        "codegen/generic",
        "codegen/gadt",
        "codegen/gadt_derive",
        "codegen/gadt_dicts",
        "codegen/higher_rank",
        "codegen/instance_names",
        "codegen/interpolation",
        "codegen/lift",
        "codegen/patterns",
        "codegen/repeated_superclass",
        "codegen/record_constructor",
        "codegen/counted_patterns",
        "runtime/arena_scope",
        "runtime/arena_layer_scope",
        "runtime/arena_owner_list",
        "runtime/counted_layer_merge",
        "effect/row_without",
        "stdlib/effect/mutable_ref",
        "stdlib/effect/optic_parts",
    ] {
        let modules = run::compile(&root.join(format!("{case}.fl"))).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        let expected = fs::read_to_string(root.join(format!("{case}.stdout")))
            .expect("independent corpus stdout");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|error| panic!("{case} release={release}: {error}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0, "{case}");
            assert_eq!(ran.memory.allocations, ran.memory.frees, "{case}");
        }
    }
}

#[test]
fn dictionary_only_captures_survive_escaped_lambdas() {
    let text = r"module App exposing (answer)

render : Show a => a -> Unit -> String
let render value = fun () -> show value

answer : Unit -> String
let answer () =
  let saved = render 123
  saved ()
";
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("dictionary captured through method metadata");
        assert_eq!(heap.string(&ran.value), Ok("123"));
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn short_circuit_and_pipe_argument_order_follow_source_execution() {
    let text = r#"module App exposing (answer)

function : Unit ->{} (Int -> Int)
let function () =
  Platform.stdout_line "function"
  fun value -> value + 1

argument : Unit ->{} Int
let argument () =
  Platform.stdout_line "argument"
  1

unexpected : Unit ->{} Bool
let unexpected () =
  Platform.stdout_line "unexpected"
  true

answer : Unit ->{} Unit
let answer () =
  let skipped_and = false && unexpected ()
  let skipped_or = true || unexpected ()
  let ordinary = function () (argument ())
  let piped = argument () |> function ()
  Platform.stdout_line (show (ordinary + piped))
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("source order");
        assert_eq!(ran.stdout, "function\nargument\nargument\nfunction\n4\n");
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn nullary_constructor_identity_and_record_tag_fields_keep_distinct_semantics() {
    let text = r#"module App exposing (answer)

type Color = Red | Blue deriving Eq

answer : Unit -> Bool
let answer () =
  let record = { _tag = "user field" }
  Red == Red && Red != Blue && record._tag == "user field"
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("nominal tag lookup is not record-only");
        assert_eq!(ran.value, Value::Bool(true));
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn tasks_are_lazy_replayable_owned_values_not_callable_closures() {
    let text = r#"module App exposing (answer)

make : String -> Eff {} none String
let make prefix = task {
  Platform.stdout_line prefix
  prefix ++ "!"
}

answer : Unit ->{} String
let answer () =
  let job = make "body"
  Platform.stdout_line "built"
  let first = await job
  let second = await job
  first ++ second
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("counted delayed task");
        assert_eq!(ran.stdout, "built\nbody\nbody\n");
        assert_eq!(heap.string(&ran.value), Ok("body!body!"));
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn root_cell_arena_breaks_closure_cycles_without_a_collector() {
    let text = r#"module App exposing (main)

nop : Unit ->{} Unit
let nop () = ()

main : Unit ->{} Unit
let main () =
  let left = Ref.make nop
  let right = Ref.make nop
  Ref.set left (fun () ->
    let held = Ref.get right
    ()
  )
  Ref.set right (fun () ->
    let held = Ref.get left
    ()
  )
  Platform.stdout_line "cyclic cells closed"
"#;
    let modules = run::compile_source(text).expect("well-typed closure cycle");
    let main = run::main_of(&modules).expect("main");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, 100_000, release)
            .expect("root cell arena closes");
        assert_eq!(ran.stdout, "cyclic cells closed\n");
        assert_eq!(ran.memory.live_objects, 0);
    }
}

#[test]
fn task_mains_keep_one_execution_arena_across_function_and_await_stages() {
    let sources = [
        r#"module App exposing (main)
main : Eff {} none Unit
let main = task { Platform.stdout_line "task main" }
"#,
        r#"module App exposing (main)
main : Unit ->{} Eff {} none Unit
let main () =
  let cell = Ref.make "task main"
  task { Platform.stdout_line (Ref.get cell) }
"#,
    ];
    for text in sources {
        let modules = run::compile_source(text).expect("task main checks");
        let main = run::main_of(&modules).expect("main");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 100_000, release)
                .expect("one arena through main stages");
            assert_eq!(ran.stdout, "task main\n");
            assert_eq!(ran.memory.live_objects, 0);
        }
    }
}

#[test]
fn ref_modify_keeps_result_before_new_contents_and_retains_shared_values() {
    let text = r#"module App exposing (answer)
answer : Unit ->{} String
let answer () =
  let cell = Ref.make "old"
  let before = Ref.get cell
  let result = Ref.modify cell (fun old -> (old ++ ": result", "new"))
  let after = Ref.get cell
  before ++ ", " ++ result ++ ", " ++ after
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("Ref.modify (result, next)");
        assert_eq!(heap.string(&ran.value), Ok("old, old: result, new"));
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn cell_contents_keep_a_last_use_constructor_shared_for_fbip_fallback() {
    let text = r#"module App exposing (answer)

replace : String -> (String, String) -> (String, String)
let replace next (left, right) = (next, right)

answer : Unit ->{} String
let answer () =
  let cell = Ref.make ("old", "shared")
  let changed = replace "new" (Ref.get cell)
  let (old_left, _) = Ref.get cell
  let (new_left, _) = changed
  old_left ++ ", " ++ new_left
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("cell-owned alias prevents destructive reuse");
        assert_eq!(heap.string(&ran.value), Ok("old, new"));
        assert_eq!(heap.stats().cells_allocated, 1);
        assert_eq!(heap.stats().cells_freed, 1);
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn ref_same_and_existing_get_and_set_wrapper_preserve_identity_and_snapshot() {
    let text = r#"module App exposing (answer)
answer : Unit ->{} String
let answer () =
  let left = Ref.make "old"
  let right = Ref.make "other"
  let alias = left
  let old = Ref.get_and_set left "new"
  if Ref.same left alias && not (Ref.same left right) then
    old ++ ", " ++ Ref.get left
  else
    "wrong identity"
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("existing Ref wrapper lowering");
        assert_eq!(heap.string(&ran.value), Ok("old, new"));
        assert_eq!(heap.stats().cells_allocated, 2);
        assert_eq!(heap.stats().cells_freed, 2);
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn arena_slot_alias_prevents_destructive_fbip_reuse() {
    let text = r#"module App exposing (answer)
import Arena

replace : String -> (String, String) -> (String, String)
let replace next (left, right) = (next, right)

answer : Unit ->{} String
let answer () = scope {
  let arena = Arena.make ()
  let view = Arena.alloc arena ("old", "shared")
  let changed = replace "new" (Arena.get view)
  let (old_left, _) = Arena.get view
  let (new_left, _) = changed
  old_left ++ ", " ++ new_left
}
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("arena slot retains its own counted alias");
        assert_eq!(heap.string(&ran.value), Ok("old, new"));
        assert_eq!(heap.stats().arenas_allocated, 1);
        assert_eq!(heap.stats().arenas_freed, 1);
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn source_acquisition_uses_the_selected_outer_owner() {
    let text = r#"module App exposing (answer)
import Arena

answer : Unit ->{} String
let answer () = scope outer {
  acquire : Unit ->{Scope outer} Arena.Arena outer
  let acquire () = Arena.make ()
  let view = scope inner {
    let arena = acquire ()
    Arena.alloc arena "outer-owned"
  }
  Arena.get view
}
"#;
    let entry = Name::new("App", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("inner close does not release an outer-owned arena");
        assert_eq!(heap.string(&ran.value), Ok("outer-owned"));
        assert_eq!(heap.stats().arenas_allocated, 1);
        assert_eq!(heap.stats().arenas_freed, 1);
        heap.drop_value(ran.value).expect("result");
        assert!(heap.finish().is_ok());
    }
}
