#[test]
fn match_commits_the_first_or_alternative_before_running_its_guard() {
    let text = r#"module App.MatchControl exposing (answer)

type Pair = Pair Int Int

input : Unit ->{} Pair
let input () =
  Host.raw_stdout_line "scrutinee"
  Pair 1 2

test : Int ->{} Bool
let test n =
  Host.raw_stdout_line ("guard " ++ show n)
  n > 1

select : Pair ->{} String
let select pair =
  match pair with
  | Pair x _ | Pair _ x when test x -> "yes"
  | _ -> "no"

answer : Unit ->{} String
let answer () = select (input ())
"#;
    let entry = Name::new("App.MatchControl", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("false guard proceeds to the next arm, not the next alternative");
        assert_eq!(ran.stdout, "scrutinee\nguard 1\n");
        assert_eq!(heap.string(&ran.value), Ok("no"));
        heap.drop_value(ran.value).expect("result");
        let memory = heap.finish().expect("balanced match execution");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn nested_list_tail_patterns_produce_owned_bindings() {
    let text = r#"module App.ListPatterns exposing (answer)

pick : List Int -> String
let pick items =
  match items with
  | [0, (1 | 2), ...rest] ->
    match rest with
    | [] -> "short"
    | [last] -> show last
    | _ -> "many"
  | _ -> "other"

answer : Unit -> String
let answer () = pick [0, 2, 9] ++ ", " ++ pick [0, 1] ++ ", " ++ pick [0, 1, 8, 7]
"#;
    let entry = Name::new("App.ListPatterns", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("nested tail matches");
        assert_eq!(heap.string(&ran.value), Ok("9, short, many"));
        heap.drop_value(ran.value).expect("result");
        let memory = heap.finish().expect("tail bindings do not leak");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn immutable_record_update_preserves_aliases_and_written_effect_order() {
    let text = r#"module App.RecordUpdate exposing (answer)

base : Unit ->{} { left : String; right : String }
let base () =
  Host.raw_stdout_line "base"
  { left = "old"; right = "keep" }

noted : String -> String ->{} String
let noted label value =
  Host.raw_stdout_line label
  value

answer : Unit ->{} String
let answer () =
  let original = base ()
  let changed = { original with right = noted "right" "next"; left = noted "left" "new" }
  original.left ++ "/" ++ original.right ++ ", " ++ changed.left ++ "/" ++ changed.right
"#;
    let entry = Name::new("App.RecordUpdate", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("immutable record update");
        assert_eq!(ran.stdout, "base\nright\nleft\n");
        assert_eq!(heap.string(&ran.value), Ok("old/keep, new/next"));
        heap.drop_value(ran.value).expect("result");
        let memory = heap.finish().expect("balanced record update");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn stored_constructor_dictionaries_are_used_in_the_guard_and_body() {
    let text = r#"module App.StoredDictionaries exposing (answer)

class Gate a =
  allow : a -> Bool

class Textual a =
  text : a -> String

instance Gate Int =
  let allow value = value > 0

instance Textual Int =
  let text value = show value

type Packed =
  | (Gate a, Textual a) => Packed String a : Packed

render : Packed -> String
let render packed =
  match packed with
  | Packed label value when allow value -> label ++ ":" ++ text value
  | _ -> "denied"

answer : Unit -> String
let answer () =
  complete : Int -> Packed
  let complete = Packed "ok"
  render (complete 42) ++ ", " ++ render (Packed "bad" 0)
"#;
    let entry = Name::new("App.StoredDictionaries", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("both hidden dictionaries follow the ordinary payload fields");
        assert_eq!(heap.string(&ran.value), Ok("ok:42, denied"));
        heap.drop_value(ran.value).expect("result");
        let memory = heap.finish().expect("bound dictionary aliases released");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn literal_patterns_preserve_numeric_float_and_unbounded_bigint_semantics() {
    let text = r#"module App.LiteralPatterns exposing (answer)

zero : Float -> String
let zero value =
  match value with
  | -0.0 -> "zero"
  | _ -> "nonzero"

big : BigInt -> String
let big value =
  match value with
  | 0xffn -> "byte"
  | 9007199254740993n -> "large"
  | _ -> "other"

answer : Unit -> String
let answer () = zero 0.0 ++ ", " ++ big 255n ++ ", " ++ big 9007199254740993n ++ ", " ++ big 0n
"#;
    let entry = Name::new("App.LiteralPatterns", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("literal patterns use language equality, not IR literal identity");
        assert_eq!(heap.string(&ran.value), Ok("zero, byte, large, other"));
        heap.drop_value(ran.value).expect("result");
        let memory = heap
            .finish()
            .expect("literal matching releases its objects");
        assert_eq!(memory.allocations, memory.frees);
    }
}

#[test]
fn bigint_show_preserves_the_language_suffix_without_narrowing() {
    let text = r#"module App.BigIntShow exposing (answer)

answer : Unit -> String
let answer () =
  show 0n ++ ", " ++ show (-0xffn) ++ ", " ++ show 0b1010n
    ++ ", " ++ show 1_000n
    ++ ", " ++ show 0x1_0000_0000_0000_0000_0000_0000_0000_0000n
"#;
    let entry = Name::new("App.BigIntShow", "answer");
    for release in [false, true] {
        let program = source_program(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("BigInt Show preserves literal syntax, unlike to_string");
        assert_eq!(
            heap.string(&ran.value),
            Ok("0n, -255n, 10n, 1000n, 340282366920938463463374607431768211456n")
        );
        heap.drop_value(ran.value).expect("result");
        let memory = heap
            .finish()
            .expect("BigInt inputs and shown strings released");
        assert_eq!(memory.allocations, memory.frees);
    }
}
