#[test]
fn list_show_keeps_custom_callbacks_captures_and_nested_dictionary_dispatch() {
    let text = r#"module App.ListShow exposing (answer)

list_show : (a -> String) -> List a -> String
foreign pure list_show from "flick:rt" "listShow"

type Label = Label String

instance Show Label =
  let show value =
    match value with
    | Label text -> "[" ++ text ++ "]"

answer : String -> String
let answer prefix =
  let render value = prefix ++ show value
  let saved = list_show render
  let values = [1, 2]
  saved values ++ " | " ++ saved []
    ++ " | " ++ show [[1, 2], [], [3]]
    ++ " | " ++ show ["a", "b"]
    ++ " | " ++ show [Label "x", Label "a,b"]
    ++ " | " ++ show [0n, 0xffn]
"#;
    let entry = Name::new("App.ListShow", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let prefix = heap.literal(&Lit::String("p:".into())).expect("prefix");
        let external = heap.dup(&prefix).expect("external alias");
        let ran = execute_program(&mut heap, &program, &entry, vec![prefix], 100_000)
            .expect("pure callbacks run as owned language calls");
        assert_eq!(
            heap.string(&ran.value),
            Ok("[p:1, p:2] | [] | [[1, 2], [], [3]] | [\"a\", \"b\"] | [[x], [a,b]] | [0n, 255n]")
        );
        assert_eq!(heap.string(&external), Ok("p:"));
        heap.drop_value(ran.value).expect("result");
        heap.drop_value(external).expect("external alias");
        let memory = heap
            .finish()
            .expect("callbacks, lists and shown values released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn list_show_skips_empty_callbacks_and_releases_roots_on_callback_exhaustion() {
    let text = r#"module App.ListShowFuel exposing (empty, nonempty)

list_show : (a -> String) -> List a -> String
foreign pure list_show from "flick:rt" "listShow"

empty : String -> String
let empty prefix =
  never : String -> String
  let rec never text = never (text ++ prefix)
  list_show never []

nonempty : String -> String
let nonempty prefix =
  never : String -> String
  let rec never text = never (text ++ prefix)
  list_show never ["first", "second"]
"#;
    for release in [false, true] {
        for (name, exhausted) in [("empty", false), ("nonempty", true)] {
            let entry = Name::new("App.ListShowFuel", name);
            let program = source_program(text, release, &entry);
            let mut heap = Heap::new();
            let prefix = heap.literal(&Lit::String("kept".into())).expect("prefix");
            let external = heap.dup(&prefix).expect("external alias");
            let ran = execute_program(&mut heap, &program, &entry, vec![prefix], 1_000);
            if exhausted {
                assert!(matches!(ran, Err(Error::OutOfFuel)));
            } else {
                let ran = ran.expect("empty list does not invoke callback");
                assert_eq!(heap.string(&ran.value), Ok("[]"));
                heap.drop_value(ran.value).expect("result");
            }
            assert_eq!(heap.string(&external), Ok("kept"));
            heap.drop_value(external).expect("external alias");
            let memory = heap
                .finish()
                .expect("all execution roots released without sweeping");
            assert_eq!(memory.allocations, memory.frees);
        }
    }
}

#[test]
fn raw_list_show_callback_instrumentation_observes_one_call_per_element_in_order() {
    let text = r#"module App.ListShowOrder exposing (answer)

list_show : (a -> String) -> List a -> String
foreign pure list_show from "flick:rt" "listShow"

render : String -> String
let render text = "<" ++ text ++ ">"

answer : Unit -> String
let answer () =
  let values = ["first", "second", "third"]
  list_show render values ++ " | " ++ list_show render [] ++ " | " ++ list_show render values
"#;
    let modules = run::compile_source(text).expect("ordinary pure source checks");
    let mut ir: Vec<_> = modules.iter().map(|module| module.ir.clone()).collect();
    let callback = ir
        .iter_mut()
        .flat_map(|module| &mut module.items)
        .find_map(|item| match &mut item.kind {
            ItemKind::Value { name, body, .. } if name == "render" => Some(body),
            _ => None,
        })
        .expect("callback definition");
    let ExprKind::Lambda { params, body } = &mut callback.kind else {
        panic!("ordinary unary callback")
    };
    let Pat::Bind(argument) = &params[0].pat else {
        panic!("named callback parameter")
    };
    let span = body.span;
    // Deliberately instrument RAW IR after checking to observe sequencing.
    // This does not admit effectful source callbacks or alter the pure FFI ABI.
    let printed = Expr::new(
        ExprKind::App {
            func: Box::new(Expr::new(
                ExprKind::Global {
                    module: "Host".into(),
                    name: "raw_stdout_line".into(),
                    type_args: Vec::new(),
                },
                span,
            )),
            arg: Box::new(Expr::new(ExprKind::Local(argument.clone()), span)),
            stage: Stage::Sync,
            arg_first: false,
        },
        span,
    );
    **body = Expr::new(
        ExprKind::Block(vec![Stmt::Expr(printed)], body.clone()),
        span,
    );
    let entry = Name::new("App.ListShowOrder", "answer");
    let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
        .expect("instrumented public IR lowers through the same listShow template");
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
        .expect("instrumented callback calls");
    assert_eq!(ran.stdout, "first\nsecond\nthird\nfirst\nsecond\nthird\n");
    assert_eq!(
        heap.string(&ran.value),
        Ok("[<first>, <second>, <third>] | [] | [<first>, <second>, <third>]")
    );
    heap.drop_value(ran.value).expect("result");
    let memory = heap
        .finish()
        .expect("instrumentation adds no hidden owners");
    assert_eq!(memory.allocations, memory.frees);
}

#[test]
fn every_list_show_fuel_prefix_releases_callback_and_tail_roots() {
    let text = r#"module App.ListShowCuts exposing (answer)

list_show : (a -> String) -> List a -> String
foreign pure list_show from "flick:rt" "listShow"

answer : String -> String
let answer prefix = list_show (fun value -> prefix ++ show value) [1, 2]
"#;
    let entry = Name::new("App.ListShowCuts", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut completed = false;
        for fuel in 0..512 {
            let mut heap = Heap::new();
            let prefix = heap.literal(&Lit::String("p".into())).expect("prefix");
            let external = heap.dup(&prefix).expect("external alias");
            match execute_program(&mut heap, &program, &entry, vec![prefix], fuel) {
                Ok(ran) => {
                    assert_eq!(heap.string(&ran.value), Ok("[p1, p2]"));
                    heap.drop_value(ran.value).expect("result");
                    completed = true;
                }
                Err(error) => assert!(matches!(error, Error::OutOfFuel), "{error}"),
            }
            assert_eq!(heap.string(&external), Ok("p"));
            heap.drop_value(external).expect("external alias");
            let memory = heap
                .finish()
                .expect("every fuel prefix releases only its own roots");
            assert_eq!(memory.allocations, memory.frees);
            if completed {
                break;
            }
        }
        assert!(
            completed,
            "small list completes within the cut-point test range"
        );
    }
}

#[test]
fn constructor_equality_ignores_only_stored_evidence_not_ordinary_records() {
    let text = r#"module App.EvidenceEquality exposing (answer)

class Gate a =
  allow : a -> Bool

instance Gate Int =
  let allow value = value > 0

type Packed =
  | Gate a => Packed a : Packed

type Flag =
  | Gate Int => Flag : Flag
  | Gate Int => OtherFlag : Flag

type Ordinary = Ordinary { allow : Int }

equal_flag : Flag -> Flag -> Bool
let equal_flag left right = left == right

answer : Unit -> String
let answer () =
  let left = Packed 42
  let same = Packed 42
  let different = Packed 43
  let ordinary_left = Ordinary { allow = 1 }
  let ordinary_right = Ordinary { allow = 2 }
  show (left == same)
    ++ ", " ++ show (left == different)
    ++ ", " ++ show (left != different)
    ++ ", " ++ show (equal_flag Flag Flag)
    ++ ", " ++ show (equal_flag Flag OtherFlag)
    ++ ", " ++ show (ordinary_left == ordinary_right)
"#;
    let entry = Name::new("App.EvidenceEquality", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("dictionary closures are ownership edges, not equality payload");
        assert_eq!(
            heap.string(&ran.value),
            Ok("true, false, true, true, false, false")
        );
        heap.drop_value(ran.value).expect("result");
        let memory = heap
            .finish()
            .expect("evidence remains strongly owned and released");
        assert_eq!(memory.allocations, memory.frees);
    }
}
