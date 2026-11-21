use std::fmt::Write;
use std::iter;

use crate::ir::{Expr, ItemKind, Module, memory, tests::lower_text};

use super::lower;

fn answer_body(module: &mut Module) -> &mut Expr {
    module
        .items
        .iter_mut()
        .find_map(|item| match &mut item.kind {
            ItemKind::Value { name, body, .. } if name == "answer" => Some(body),
            _ => None,
        })
        .expect("answer")
}

#[test]
fn actual_checked_source_reaches_the_ownership_path() {
    let module = lower_text("App", "module App exposing (answer)\n\nanswer : Unit -> String\nlet answer () =\n  let prefix = \"hello\"\n  let append suffix = prefix ++ suffix\n  append \" world\"\n").expect("checked compiler IR");
    let program =
        lower([&module], &memory::Name::new("App", "answer")).expect("source ownership lowering");
    assert_eq!(program.globals.len(), 1);
    assert_eq!(memory::validate_program(&program), Ok(()));
    assert!(program.functions.values().any(|function| function.body.instructions.iter().any(|instruction| matches!(instruction, memory::Instruction::Let { value: memory::Operation::Closure { captures, .. }, .. } if !captures.is_empty()))));
}

#[test]
fn source_default_methods_use_owned_getters_not_ordinary_record_fields() {
    let text = r#"module App exposing (answer)
class Label a =
  label : a -> String
  framed : a -> String
  let framed value = "<" ++ label value ++ ">"
instance Label String =
  let label value = value
answer : Unit -> String
let answer () = framed "kept"
"#;
    let module = lower_text("App", text).expect("default method source checks");
    let program = lower([&module], &memory::Name::new("App", "answer"))
        .expect("default instance ownership lowering");
    assert!(program.functions.values().any(|function| {
        function.body.instructions.iter().any(|instruction| {
            matches!(instruction, memory::Instruction::Let {
                value: memory::Operation::Dictionary { entries }, ..
            } if entries.iter().any(|(name, entry)| name == "framed"
                && matches!(entry, memory::DictionaryEntry::Getter(_))))
        })
    }));
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn general_handlers_lower_owned_arms_and_runtime_saturation() {
    let text = r"module App exposing (answer)
effect Pick =
  choose : Int -> Int ->{} Int

answer : Int ->{} Int
let answer captured =
  try
    let partial = Pick.choose captured
    partial 2
  with
  | Pick.choose x y -> x + y + captured
  | return value -> value * 2
";
    let module = lower_text("App", text).expect("checked staged handler");
    let program =
        lower([&module], &memory::Name::new("App", "answer")).expect("counted handler lowering");
    assert_eq!(memory::validate_program(&program), Ok(()));
    let operations: Vec<_> = program
        .functions
        .values()
        .flat_map(|function| {
            function
                .body
                .instructions
                .iter()
                .filter_map(|instruction| match instruction {
                    memory::Instruction::Let { value, .. } => Some(value),
                    _ => None,
                })
        })
        .collect();
    assert!(operations.iter().any(|operation| matches!(operation,
        memory::Operation::Handle { effects, ret: Some(_), errors: None, .. }
            if effects.len() == 1 && effects[0].operations.len() == 1
                && effects[0].operations[0].arity == 2)));
    assert!(operations.iter().any(|operation| matches!(operation,
        memory::Operation::Perform { effect, op, dicts, args }
            if effect == "App.Pick" && op == "choose" && dicts.is_empty() && args.len() == 2)));
}

#[test]
fn sequential_ifs_share_their_continuation_in_linear_code_size() {
    let count = 24;
    let mut text = String::from(
        "module App exposing (answer)\n\nanswer : Bool -> Int\nlet answer choose =\n  let value0 = 0\n",
    );
    for index in 1..=count {
        writeln!(
            &mut text,
            "  let value{index} = if choose then value{} + 1 else value{} + 2",
            index - 1,
            index - 1
        )
        .expect("append source");
    }
    writeln!(&mut text, "  value{count}").expect("append result");
    let module = lower_text("App", &text).expect("checked source");
    let program = lower([&module], &memory::Name::new("App", "answer")).expect("if lowering");
    assert_eq!(
        program.functions.len(),
        count + 2,
        "one helper per if, plus entry and unary source code"
    );
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn known_foreign_unit_adapter_is_supported_but_nullable_is_not_silently_ignored() {
    use super::LowerError;
    use crate::ir::ItemKind;
    let mut module = lower_text("App", "module App exposing (print)\n\nprint : String ->{} Unit\nforeign print from \"flick:rt\" \"stdoutLine\"\n").expect("foreign source");
    let entry = memory::Name::new("App", "print");
    let program = lower([&module], &entry).expect("ordinary Unit result adapter");
    assert_eq!(memory::validate_program(&program), Ok(()));
    let foreign = module
        .items
        .iter_mut()
        .find_map(|item| match &mut item.kind {
            ItemKind::Foreign(value) => Some(value),
            _ => None,
        })
        .expect("foreign");
    foreign.adapter.result.nullable = true;
    assert!(
        matches!(lower([&module], &entry), Err(LowerError::Unsupported { form, .. }) if form == "foreign ownership adapter")
    );
}

#[test]
fn tasks_have_distinct_delayed_code_and_await_consumes_the_task() {
    let text = "module App exposing (answer)\n\nanswer : Unit ->{} String\nlet answer () =\n  let job = task { \"later\" }\n  await job\n";
    let module = lower_text("App", text).expect("task source checks");
    let program =
        lower([&module], &memory::Name::new("App", "answer")).expect("real task lowering");
    let operations: Vec<_> = program
        .functions
        .values()
        .flat_map(|f| &f.body.instructions)
        .filter_map(|instruction| match instruction {
            memory::Instruction::Let { value, .. } => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, memory::Operation::Task { .. }))
            .count(),
        1
    );
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, memory::Operation::Await(_)))
            .count(),
        1
    );
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn nonlocal_question_return_wraps_function_in_boundary_and_lowers_unwrap() {
    let text = "module App exposing (choose)\n\nchoose : Option Int -> Option Int\nlet choose value = Some (value?)\n";
    let module = lower_text("App", text).expect("Option return source checks");
    let program = lower([&module], &memory::Name::new("App", "choose"))
        .expect("Option? lowers with FunctionBoundary");
    assert_eq!(memory::validate_program(&program), Ok(()));
    let boundaries = program
        .functions
        .values()
        .flat_map(|f| f.body.instructions.iter())
        .filter(|i| {
            matches!(
                i,
                memory::Instruction::Let {
                    value: memory::Operation::FunctionBoundary { .. },
                    ..
                }
            )
        })
        .count();
    let unwraps = program
        .functions
        .values()
        .flat_map(|f| f.body.instructions.iter())
        .filter(|i| {
            matches!(
                i,
                memory::Instruction::Let {
                    value: memory::Operation::UnwrapOption(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(boundaries, 1);
    assert_eq!(unwraps, 1);
}

#[test]
fn control_handler_lowers_with_control_operation_class() {
    use crate::build::builtin_modules;
    use crate::ir::{ExprKind, Lit};

    let text = r"module App.HandlerCapture exposing (answer)

type error Denied = Denied String

answer : String ->{} String
let answer captured =
  try from_result (Err (Denied captured)) with
  | Denied reason when reason == captured -> reason
  | Denied _ -> captured
";
    let mut module = lower_text("App.HandlerCapture", text).expect("accepted handler source");
    let body = answer_body(&mut module);
    let ExprKind::Lambda { body, .. } = &mut body.kind else {
        panic!("lambda")
    };
    let ExprKind::Handle(handler) = &mut body.kind else {
        panic!("handler")
    };
    handler.effects.push(flick_ir::Handled {
        effect: "App.HandlerCapture.Control".into(),
        operations: vec![flick_ir::OpArm {
            name: "control".into(),
            class: flick_ir::OpClass::Control,
            dicts: 0,
            arity: 1,
            covers: true,
            function: Expr::new(ExprKind::Lit(Lit::Unit), body.span),
        }],
    });
    let program = lower(
        iter::once(&module).chain(builtin_modules()),
        &memory::Name::new("App.HandlerCapture", "answer"),
    )
    .expect("control handler lowers cleanly");
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn internal_ref_adapters_keep_their_declared_callback_protocol() {
    use super::LowerError;
    use crate::build::builtin_modules;
    use crate::ir::ItemKind;
    let mut module = builtin_modules()
        .iter()
        .find(|module| module.name == "Ref")
        .expect("Ref builtin")
        .clone();
    for name in ["make", "get", "set", "modify", "same"] {
        let program = lower([&module], &memory::Name::new("Ref", name))
            .expect("known internal ownership contract");
        assert_eq!(memory::validate_program(&program), Ok(()));
    }
    let modify = module
        .items
        .iter_mut()
        .find_map(|item| match &mut item.kind {
            ItemKind::Foreign(value) if value.name == "modify" => Some(value),
            _ => None,
        })
        .expect("modify");
    modify.adapter.callbacks[1] = Some(2);
    assert!(
        matches!(lower([&module], &memory::Name::new("Ref", "modify")), Err(LowerError::Unsupported { form, .. }) if form == "foreign ownership adapter")
    );
}

#[test]
fn internal_scope_and_arena_adapters_preserve_owner_protocol() {
    use crate::build::builtin_modules;
    let modules = builtin_modules();
    for (module, name) in [
        ("Scope", "run"),
        ("Scope", "current"),
        ("Arena", "make"),
        ("Arena", "alloc"),
        ("Arena", "get"),
    ] {
        let program = lower(modules.iter(), &memory::Name::new(module, name))
            .unwrap_or_else(|error| panic!("{module}.{name}: {error}"));
        assert_eq!(memory::validate_program(&program), Ok(()));
    }
}

include!("tests/matches_and_evidence.rs");
#[test]
fn try_only_accepts_body_and_arm_captures() {
    let text = r"module App exposing (answer)

type error Denied = Denied String

answer : String ->{} String
let answer captured =
  try captured with
  | Denied reason when reason == captured -> reason
  | Denied _ -> captured
";
    let module = lower_text("App", text).expect("checked try source");
    let program =
        lower([&module], &memory::Name::new("App", "answer")).expect("try-only ownership lowering");
    assert_eq!(memory::validate_program(&program), Ok(()));
}

fn recovery_markers(block: &memory::Block, hits: &mut usize, misses: &mut usize) {
    for instruction in &block.instructions {
        if let memory::Instruction::Let {
            value: memory::Operation::RecoveryResult { selected },
            ..
        } = instruction
        {
            if selected.is_some() {
                *hits += 1;
            } else {
                *misses += 1;
            }
        }
    }
    if let memory::Exit::Branch { then_, else_, .. } = &block.exit {
        recovery_markers(then_, hits, misses);
        recovery_markers(else_, hits, misses);
    }
}

#[test]
fn try_result_question_and_guarded_or_share_the_failure_dispatch() {
    use crate::build::builtin_modules;
    let text = r"module App.TryDispatch exposing (answer)
type error Denied = Denied String
answer : String ->{} String
let answer captured =
  try
    failure : Result (Errors {Denied}) String
    let failure = Err (Denied captured)
    let value = failure?
    value
  with
  | Denied reason | Denied reason when reason == captured -> reason
  | Denied _ -> captured
";
    let module = lower_text("App.TryDispatch", text).expect("checked Result ? and try");
    let program = lower(
        iter::once(&module).chain(builtin_modules()),
        &memory::Name::new("App.TryDispatch", "answer"),
    )
    .expect("typed failure and guarded Or lowering");
    assert_eq!(memory::validate_program(&program), Ok(()));
    let kinds: Vec<_> = program
        .functions
        .values()
        .flat_map(|function| &function.body.instructions)
        .filter_map(|instruction| match instruction {
            memory::Instruction::Let {
                value: memory::Operation::Intrinsic { kind, .. },
                ..
            } => Some(*kind),
            _ => None,
        })
        .collect();
    for kind in [memory::Intrinsic::Try, memory::Intrinsic::FromResult] {
        assert_eq!(
            kinds.iter().filter(|actual| **actual == kind).count(),
            1,
            "{kind:?}"
        );
    }
    assert!(!kinds.contains(&memory::Intrinsic::MatchFail));
    assert!(
        !kinds.contains(&memory::Intrinsic::Raise),
        "selection Miss must preserve whole C, not re-raise a payload"
    );
    let (mut hits, mut misses) = (0, 0);
    for function in program.functions.values() {
        recovery_markers(&function.body, &mut hits, &mut misses);
    }
    assert_eq!(
        (hits, misses),
        (2, 1),
        "two deferred arms and one explicit Miss"
    );
    assert_eq!(
        program.functions.len(),
        13,
        "entry, source lambda, body, handler, two selectors plus two deferred bodies, terminal, from_result initializer and adapter, and two unary constructor adapters"
    );
}
