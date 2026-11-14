#[test]
fn a_tail_call_assigns_only_the_arguments_it_changes() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         find : (a -> Bool) -> List a -> Option a\n\
         let rec find p xs =\n  match xs with\n  | [] -> None\n  | [x, ...rest] -> if p x then Some x else find p (List.drop 1 rest)\n",
    );
    let find = export_of(&js, "find");
    assert!(find.contains("for (;;)"), "{find}");
    assert!(!find.contains("const $t"), "{find}");
    // only the initialization sets `p`
    assert_eq!(find.matches("$l0 =").count(), 1, "{find}");
    assert!(find.contains("$l1 ="), "{find}");
}

#[test]
fn a_constrained_function_loops_on_a_call_passing_its_own_dictionaries() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         elem : Eq a => a -> List a -> Bool\n\
         let rec elem y xs =\n  match xs with\n  | [] -> false\n  | [x, ...rest] -> if x == y then true else elem y rest\n\
         nest : forall a. Show a => Float -> a -> String\n\
         let rec nest n x = if n == 0 then show x else nest (n - 1) [x]\n",
    );
    let elem = export_of(&js, "elem");
    assert!(elem.contains("for (;;)"), "{elem}");
    assert!(!elem.contains("elem($d0)"), "{elem}");
    // polymorphic recursion passes another dictionary: a call, not a jump
    let nest = export_of(&js, "nest");
    assert!(!nest.contains("for (;;)"), "{nest}");
}

#[test]
fn the_right_operand_of_and_and_or_is_a_tail_position() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         all : (a -> Bool) -> List a -> Bool\n\
         let rec all p xs =\n  match xs with\n  | [] -> true\n  | [x, ...rest] -> p x && all p rest\n\
         any : (a -> Bool) -> List a -> Bool\n\
         let rec any p xs =\n  match xs with\n  | [] -> false\n  | [x, ...rest] -> p x || any p rest\n",
    );
    let all = export_of(&js, "all");
    assert!(
        all.contains("if (!p(x)) {\n") && all.contains("return false;"),
        "{all}"
    );
    assert!(!all.contains("all(p)"), "{all}");
    let any = export_of(&js, "any");
    assert!(
        any.contains("if (p(x)) {\n") && any.contains("return true;"),
        "{any}"
    );
    assert!(!any.contains("any(p)"), "{any}");
}

#[test]
fn mutually_tail_recursive_functions_run_as_one_loop() {
    let text = "module App.Parity exposing (..)\n\
         rec {\n\
         \x20 is_even : Float -> Bool\n  let is_even n = if n == 0 then true else is_odd (n - 1)\n\
         \x20 is_odd : Float -> Bool\n  let is_odd n = if n == 0 then false else is_even (n - 1)\n\
         }\n\
         rec {\n\
         \x20 a : Float -> Float -> Float\n  let a acc n = if n == 0 then acc else b (acc + 1) (n - 1)\n\
         \x20 b : Float -> Float -> Float\n  let b acc n = if n == 0 then acc else c acc (n - 1)\n\
         \x20 c : Float -> Float -> Float\n  let c acc n = if n == 0 then acc else a (a 0 acc) (n - 1)\n\
         }\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        let parity = js.split("export const").next().unwrap_or_default();
        assert!(parity.contains("const is_even$rec = ($f, $l0) =>"), "{js}");
        assert!(
            parity.contains("switch ($f)") && parity.contains("$f = 1;"),
            "{js}"
        );
        assert!(!parity.contains("is_odd("), "{js}");
        let group = &js[js.find("const a$rec").expect("a group")..];
        let group = &group[..group.find("\nexport").unwrap_or(group.len())];
        // the inner `a 0 acc` is not in tail position: it calls the member
        let inner = if release { "a$2(0, acc)" } else { "a(0)(acc)" };
        assert!(
            group.contains("$f = 2;") && group.contains(inner),
            "{group}"
        );
        let calls_a = if release {
            "a$rec(0, acc, n)"
        } else {
            "(n) => a$rec(0, acc, n)"
        };
        assert!(js.contains(calls_a), "{js}");
    }
}

#[test]
fn a_loop_walks_a_list_by_offset_unless_its_rest_escapes() {
    let text = "module App.Walk exposing (..)\n\
         pairs : Float -> List Float -> Float\n\
         let rec pairs acc xs =\n  match xs with\n  | [a, b, ...rest] -> pairs (acc + a * b) rest\n  | [a] -> acc + a\n  | _ -> acc\n\
         restart : Float -> List Float -> Float\n\
         let rec restart n xs =\n  match xs with\n  | [] -> if n == 0 then 0 else restart (n - 1) [1, 2]\n  | [_, ...rest] -> restart n rest\n\
         sum : List Float -> Float\n\
         let rec sum xs =\n  match xs with\n  | [] -> 0\n  | [x, ...rest] -> x + sum rest\n\
         total : Float -> List Float -> Float\n\
         let rec total acc xs =\n  match xs with\n  | [] -> acc\n  | [x, ...rest] -> if x > 9 then acc + sum rest else total (acc + x) rest\n\
         rec {\n\
         \x20 even_len : List a -> Bool\n  let even_len xs =\n    match xs with\n    | [] -> true\n    | [_, ...rest] -> odd_len rest\n\
         \x20 odd_len : List a -> Bool\n  let odd_len xs =\n    match xs with\n    | [] -> false\n    | [_, ...rest] -> even_len rest\n\
         }\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        let pairs = export_of(&js, if release { "pairs$2" } else { "pairs" });
        assert!(!pairs.contains("slice("), "{pairs}");
        assert!(
            pairs.contains("xs[$o1 + 1]") && pairs.contains("$o1 = $o1 + 2;"),
            "{pairs}"
        );
        // another list on the walked position starts at its beginning
        let restart = export_of(&js, if release { "restart$2" } else { "restart" });
        assert!(
            restart.contains("$l1 = [1, 2];\n") && restart.contains("$o1 = 0;"),
            "{restart}"
        );
        assert!(!restart.contains("slice("), "{restart}");
        let group = &js[js.find("const even_len$rec").expect("a group")..];
        assert!(
            group.contains("let $o0 = 0;") && !group.contains("slice("),
            "{group}"
        );
        // `rest` is a value of its own outside a loop, or when it escapes one
        let total = export_of(&js, if release { "total$2" } else { "total" });
        assert!(total.contains("slice(1)"), "{total}");
        assert!(export_of(&js, "sum").contains("slice(1)"), "{js}");
    }
}

#[test]
fn an_atom_write_names_its_place() {
    let text = "module App.Counter exposing (..)\n\
         count : Atom.Writable Float\nlet count = Atom.make 0\n\
         reset : Action.Action Ui.ClickSnapshot\n\
         let reset = Action.exhaust (fun _ -> task {\n  Atom.set count 0\n  Atom.update count (fun n -> n + 1)\n})\n";
    let code = compile(text);
    // each write's `CallSite` is the record of its own place
    assert!(code.contains("place: \"App.Counter:6\""), "{code}");
    assert!(code.contains("place: \"App.Counter:7\""), "{code}");
}

#[test]
fn effect_bridge_does_not_treat_a_user_api_type_as_http_handlers() {
    let code = on_rt_with_bridge(
        "module App exposing (use)\n\ntype Api a = Api a\n\n\
         use : Api a -> a -> a\n\
foreign pure use from \"@flick-lang/effect-ts\" \"use\"\n  arity 2\n",
        EffectBridge::Available,
    );
    assert!(!code.contains("handlersToEffect"), "{code}");
}

#[test]
fn effect_bridge_adapts_the_handler_record_of_an_rpc_group() {
    let code = on_rt_with_bridge(
        "module App exposing (use)\n\nuse : Rpc.Group r hs -> hs -> Unit\n\
foreign pure use from \"@flick-lang/effect-ts\" \"use\"\n  arity 2\n",
        EffectBridge::Available,
    );
    assert!(code.contains("handlersToEffect"), "{code}");
}

#[test]
fn generic_owner_transport_does_not_restore_default_owner() {
    let js = compile(
        "module App exposing (forward)\nforward : (Unit ->{r} a) ->{r} a\nlet forward body = body ()\n",
    );
    assert!(js.contains(".readOwnerEvidence()"), "{js}");
    assert!(js.contains(".withOwnerEvidence("), "{js}");
    assert!(
        !js.contains(".within("),
        "compiled forwarding must leave provider/cancellation ownership intact: {js}"
    );
    assert!(
        !js.contains("Scope.js"),
        "evidence reads are compiler-runtime operations: {js}"
    );
}
