#[test]
fn match_and_update_reach_owned_source_lowering() {
    let text = r"module App.MatchUpdate exposing (answer)
answer : Bool -> Int
let answer choose =
  let base = { left = 1; right = 2 }
  let changed = { base with right = 3; left = 4 }
  match choose with
  | true -> changed.left
  | false -> base.right
";
    let module = lower_text("App.MatchUpdate", text).expect("checked match/update source");
    let program = lower([&module], &memory::Name::new("App.MatchUpdate", "answer"))
        .expect("match and immutable update lowering");
    assert_eq!(memory::validate_program(&program), Ok(()));
    let fields = program
        .functions
        .values()
        .flat_map(|function| &function.body.instructions)
        .find_map(|instruction| match instruction {
            memory::Instruction::Let {
                value: memory::Operation::Update { fields, .. },
                ..
            } => Some(fields),
            _ => None,
        })
        .expect("immutable update operation");
    assert_eq!(
        fields
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["right", "left"],
        "written replacement order must not become label order"
    );
}

#[test]
fn match_constructor_dictionary_is_bound_in_the_guard() {
    let text = r#"module App.MatchDictionary exposing (answer)
class Gate a =
  allow : a -> Bool

instance Gate Int =
  let allow value = value > 0

type Packed =
  | Gate a => Packed a : Packed

answer : Unit -> String
let answer () =
  let packed = Packed 42
  match packed with
  | Packed value when allow value -> "allowed"
  | _ -> "denied"
"#;
    let module = lower_text("App.MatchDictionary", text).expect("checked GADT dictionary source");
    let program = lower(
        [&module],
        &memory::Name::new("App.MatchDictionary", "answer"),
    )
    .expect("stored dictionary in constructor match guard");
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn sequential_matches_share_continuations_in_linear_code_size() {
    let count = 24;
    let mut text = String::from(
        "module App.Matches exposing (answer)\nanswer : Bool -> Int\nlet answer choose =\n  let value0 = 0\n",
    );
    for index in 1..=count {
        writeln!(
            &mut text,
            "  let value{index} = match choose with\n    | true -> value{} + 1\n    | false -> value{} + 2",
            index - 1,
            index - 1
        )
        .expect("append match");
    }
    writeln!(&mut text, "  value{count}").expect("append result");
    let module = lower_text("App.Matches", &text).expect("checked sequential matches");
    let program =
        lower([&module], &memory::Name::new("App.Matches", "answer")).expect("match lowering");
    assert_eq!(program.functions.len(), 3 * count + 2);
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn zero_field_constructor_keeps_its_dictionary_payload() {
    let text = r"module App.EmptyConstructor exposing (answer)
class Gate a =
  allow : a -> Bool
instance Gate Int =
  let allow value = value > 0
type Token =
  | Gate Int => Token : Token
answer : Unit -> Bool
let answer () =
  match Token with
  | Token -> true
";
    let module = lower_text("App.EmptyConstructor", text).expect("checked constrained constant");
    let program = lower(
        [&module],
        &memory::Name::new("App.EmptyConstructor", "answer"),
    )
    .expect("zero-field constrained constructor lowering");
    let (fields, dicts) = program
        .functions
        .values()
        .flat_map(|function| &function.body.instructions)
        .find_map(|instruction| match instruction {
            memory::Instruction::Let {
                value:
                    memory::Operation::ConstructWithDicts { tag, fields, dicts }
                    | memory::Operation::ReuseWithDicts {
                        tag, fields, dicts, ..
                    },
                ..
            } if tag == "App.EmptyConstructor.Token.Token" => Some((fields, dicts)),
            _ => None,
        })
        .expect("nominal constructor with explicit stored evidence");
    assert!(fields.is_empty(), "no ordinary fields");
    assert_eq!(dicts, &[1], "one owned evidence argument");
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn false_guard_passes_the_original_outer_environment_to_the_suffix() {
    use crate::ir::{ExprKind, Lit, Pat};
    let text = r"module App.ShadowedMatch exposing (answer)
answer : Int -> Int
let answer outer =
  match 1 with
  | value when false -> value
  | _ -> outer
";
    let mut module = lower_text("App.ShadowedMatch", text).expect("checked guarded match");
    let body = answer_body(&mut module);
    let ExprKind::Lambda { params, body } = &mut body.kind else {
        panic!("ordinary source arrow")
    };
    let Pat::Bind(outer) = &params[0].pat else {
        panic!("outer parameter")
    };
    let outer = outer.clone();
    let ExprKind::Match(_, arms) = &mut body.kind else {
        panic!("source match")
    };
    // Checked source has globally unique binder identities. Exercise the
    // public IR's lexical shadowing too, without hiding the suffix's outer.
    arms[0].pat = Pat::Bind(outer.clone());
    arms[0].body.kind = ExprKind::Local(outer.clone());
    let program = lower([&module], &memory::Name::new("App.ShadowedMatch", "answer"))
        .expect("lexically shadowed match lowering");
    let helper = program
        .functions
        .values()
        .find(|function| function.body.instructions.iter().any(|instruction| matches!(
            instruction,
            memory::Instruction::Let { value: memory::Operation::Match { pattern, .. }, .. } if matches!(&**pattern, Pat::Bind(name) if name == &outer)
        )))
        .expect("binding arm helper");
    let memory::Exit::Branch { then_, .. } = &helper.body.exit else {
        panic!("pattern branch")
    };
    let memory::Exit::Branch { else_, .. } = &then_.exit else {
        panic!("guard branch")
    };
    assert!(then_.instructions.iter().any(|instruction| matches!(
        instruction,
        memory::Instruction::Let {
            value: memory::Operation::Literal(Lit::Bool(false)),
            ..
        }
    )));
    let args = else_
        .instructions
        .iter()
        .find_map(|instruction| match instruction {
            memory::Instruction::Let {
                value: memory::Operation::Call { args, .. },
                ..
            } => Some(args),
            _ => None,
        })
        .expect("guard failure calls next arm");
    assert_eq!(
        args,
        &[0, 1],
        "original outer parameter, not projected binding"
    );
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn malformed_public_pattern_metadata_reports_its_source_match_span() {
    use super::LowerError;
    use crate::ir::{ExprKind, Pat};
    let text = r"module App.BadPattern exposing (answer)
answer : Int -> Int
let answer value =
  match value with
  | 0 -> 1
  | _ -> 2
";
    let mut module = lower_text("App.BadPattern", text).expect("checked source baseline");
    let ExprKind::Lambda { body, .. } = &mut answer_body(&mut module).kind else {
        panic!("ordinary arrow")
    };
    let span = body.span;
    let ExprKind::Match(_, arms) = &mut body.kind else {
        panic!("match body")
    };
    arms[0].pat = Pat::Or(vec![Pat::Bind("bound".into()), Pat::Wild]);
    assert!(matches!(
        lower([&module], &memory::Name::new("App.BadPattern", "answer")),
        Err(LowerError::Pattern { error: memory::Error::UnequalOrBindings, span: actual }) if actual == span
    ));
}

#[test]
fn nested_matches_emit_each_arm_body_once() {
    let count = 20;
    let mut expression = String::from("0");
    for _ in 0..count {
        let nested = expression
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        expression = format!("match choose with\n| true ->\n{nested}\n| false -> 1");
    }
    let body = expression
        .lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!(
        "module App.NestedMatches exposing (answer)\nanswer : Bool -> Int\nlet answer choose =\n{body}\n"
    );
    let module = lower_text("App.NestedMatches", &text).expect("checked nested matches");
    let program = lower([&module], &memory::Name::new("App.NestedMatches", "answer"))
        .expect("nested match lowering");
    assert_eq!(program.functions.len(), count * 3 + 2);
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn match_helpers_preserve_task_and_selected_owner_boundaries() {
    use crate::build::builtin_modules;
    let text = r#"module App.MatchOwners exposing (answer)
import Arena
answer : Unit ->{} String
let answer () = scope outer {
  acquire : Unit ->{Scope outer} Arena.Arena outer
  let acquire () = Arena.make ()
  let job = task {
    scope inner {
      match true with
      | true ->
        let arena = acquire ()
        Arena.get (Arena.alloc arena "owned")
      | false -> "not selected"
    }
  }
  await job
}
"#;
    let module = lower_text("App.MatchOwners", text).expect("checked task/selected-owner match");
    let program = lower(
        iter::once(&module).chain(builtin_modules()),
        &memory::Name::new("App.MatchOwners", "answer"),
    )
    .expect("ordinary arm helpers preserve owner captures");
    assert_eq!(memory::validate_program(&program), Ok(()));
}

#[test]
fn constructor_emission_separates_multiple_evidence_arguments_but_keeps_legacy_plain_form() {
    let text = r#"module App.ConstructorParts exposing (answer)
class Gate a =
  allow : a -> Bool
class Textual a =
  text : a -> String
instance Gate Int =
  let allow value = value > 0
instance Textual Int =
  let text _ = "int"
type Packed =
  | (Gate a, Textual a) => Packed String a : Packed
answer : Unit -> Packed
let answer () = Packed "payload" 42
"#;
    let module =
        lower_text("App.ConstructorParts", text).expect("checked multiple evidence constructor");
    let program = lower(
        [&module],
        &memory::Name::new("App.ConstructorParts", "answer"),
    )
    .expect("explicit evidence construction");
    let parts = program
        .functions
        .values()
        .flat_map(|function| &function.body.instructions)
        .find_map(|instruction| match instruction {
            memory::Instruction::Let {
                value:
                    memory::Operation::ConstructWithDicts { tag, fields, dicts }
                    | memory::Operation::ReuseWithDicts {
                        tag, fields, dicts, ..
                    },
                ..
            } if tag == "App.ConstructorParts.Packed.Packed" => Some((fields, dicts)),
            _ => None,
        })
        .expect("separate ordinary and dictionary vectors");
    assert_eq!(
        parts.0,
        &[4, 1],
        "ordinary written fields follow the two captured dictionaries"
    );
    assert_eq!(
        parts.1,
        &[2, 3],
        "both dictionary arguments remain strongly owned in their original order"
    );
    let module = lower_text("App.Plain", "module App.Plain exposing (answer)\ntype Plain = Plain Int\nanswer : Unit -> Plain\nlet answer () = Plain 42\n").expect("plain constructor source");
    let program = lower([&module], &memory::Name::new("App.Plain", "answer"))
        .expect("plain constructor lowering");
    assert!(program.functions.values().flat_map(|function| &function.body.instructions).any(|instruction| matches!(instruction,
        memory::Instruction::Let { value: memory::Operation::Construct { tag, fields }, .. } if tag == "App.Plain.Plain.Plain" && fields == &[1])));
    assert!(
        !program
            .functions
            .values()
            .flat_map(|function| &function.body.instructions)
            .any(|instruction| matches!(
                instruction,
                memory::Instruction::Let {
                    value: memory::Operation::ConstructWithDicts { .. }
                        | memory::Operation::ReuseWithDicts { .. },
                    ..
                }
            ))
    );
}
