#[test]
fn lexical_owner_is_bound_when_named_callback_is_passed() {
    let ir = lower_retained("", "Layer.run layer acquire");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(with-owner-evidence ") && !main.contains("(Layer.run layer) Test.acquire"),
        "passing a callback must transport its Scope candidate at invocation:\n{ir}"
    );
}

#[test]
fn lexical_owner_is_bound_for_partially_applied_generic_callback() {
    let ir = lower_retained(
        "forward : (Unit ->{r} a) -> Unit ->{r} a\nlet forward f () = f ()",
        "Layer.run layer (forward acquire)",
    );
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(with-owner-evidence ")
            && !main.contains("(Layer.run layer) (Test.forward Test.acquire)"),
        "a generic callback invocation must transport its solved Scope owner:\n{ir}"
    );
}

#[test]
fn generic_provider_forwarding_binds_callback_owner_before_provider_opens() {
    let ir = lower_retained(
        "under : Layer ri e ro -> (Unit ->{ro + r; e2} a) ->{ri + r; e + e2} a\nlet under layer body = Layer.run layer body",
        "under layer acquire",
    );
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(with-owner-evidence ") && !main.contains("(Test.under layer) Test.acquire"),
        "a generic forwarding combinator must preserve its supplied Scope owner:\n{ir}"
    );
}

#[test]
fn binding_callback_owner_does_not_repeat_callback_construction() {
    let ir = lower_retained(
        "factory : Unit ->{} (Unit ->{Scope s} Arena.Value s String)\nlet factory () = acquire",
        "Layer.run layer (factory ())",
    );
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert_eq!(main.matches("Test.factory").count(), 1, "{ir}");
    assert!(
        main.contains("(with-owner-evidence ")
            && !main.contains("(Layer.run layer) (Test.factory ())!"),
        "construct the callback once, before opening the provider:\n{ir}"
    );
}

#[test]
fn scope_free_callbacks_need_no_owner_binding() {
    let ir = lower_body(
        "scope unrelated {\n    let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n    Layer.run layer (fun () -> ())\n  }",
    );
    assert!(
        !ir.contains("(with-owner-evidence ") && !ir.contains("read-owner-evidence"),
        "{ir}"
    );
}

#[test]
fn higher_rank_scope_callback_does_not_bind_unrelated_source_owner() {
    let ir = lower_body(
        "scope unrelated {\n    Scope.run (fun _ ->\n      let _arena = Arena.make ()\n      ()\n    )\n  }",
    );
    assert!(!ir.contains("with-owner-evidence unrelated"), "{ir}");
}

const RUN_RECORD: &str = "run_record : forall a r e fields. { acquire : Unit ->{Read | r; e} a | fields } ->{r; e} a\nlet run_record callbacks =\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  Layer.run layer callbacks.acquire";

#[test]
fn record_literal_binds_latent_callback_owner() {
    let ir = lower_retained(RUN_RECORD, "run_record { acquire = acquire; extra = 7 }");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(main.contains("(with-owner-evidence "), "{ir}");
    assert!(
        main.contains("extra = 7"),
        "preserve unrelated record fields:\n{ir}"
    );
}

#[test]
fn module_level_record_binds_latent_callback_owner_at_use() {
    let declarations = format!(
        "{RUN_RECORD}\ncallbacks : {{ acquire : Unit ->{{Read, Scope s}} Arena.Value s String; extra : Int }}\nlet callbacks = {{ acquire = acquire; extra = 7 }}"
    );
    let ir = lower_retained(&declarations, "run_record callbacks");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(with-owner-evidence ") && main.contains("Test.callbacks"),
        "a named record needs execution-time evidence transport:\n{ir}"
    );
    assert!(
        !main.contains("with acquire"),
        "opaque values are not reconstructed: {ir}"
    );
}

const TASK_VALUES: &str = "acquire_task : Eff {Scope s} none (Arena.Value s String)\nlet acquire_task = task { acquire () }\nrun_task : Eff {Read | r} e a ->{r; e} a\nlet run_task work =\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  Layer.run layer (fun () -> await work)";

#[test]
fn named_task_binds_owner_without_eager_await() {
    let ir = lower_retained(TASK_VALUES, "run_task acquire_task");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(main.contains("(with-owner-evidence "), "{ir}");
    assert_eq!(main.matches("Test.acquire_task").count(), 1, "{ir}");
    assert!(
        !main.contains("(task "),
        "transport must not reconstruct stored tasks: {ir}"
    );
}

#[test]
fn callback_result_task_retains_owner_after_provider_closes() {
    let declarations = format!(
        "{TASK_VALUES}\nfactory : Unit ->{{}} Eff {{Scope s}} none (Arena.Value s String)\nlet factory () = acquire_task"
    );
    let ir = lower_retained(&declarations, "await (Layer.run layer factory)");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(main.contains("(with-owner-evidence "), "{ir}");
}

#[test]
fn task_result_callback_binds_owner_before_provider() {
    let declarations = "callback_task : Eff {} none (Unit ->{Scope s} Arena.Value s String)\nlet callback_task = task { acquire }";
    let ir = lower_retained(declarations, "Layer.run layer (await callback_task)");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(main.contains("(with-owner-evidence "), "{ir}");
}

#[test]
fn pipe_and_reverse_pipe_bind_scoped_callbacks() {
    for call in ["acquire |> Layer.run layer", "Layer.run layer <| acquire"] {
        let ir = lower_retained("", call);
        let main = ir
            .lines()
            .find(|line| line.starts_with("let main ="))
            .expect("lowered test definition");
        assert!(main.contains("(with-owner-evidence "), "{ir}");
    }
}

#[test]
fn composition_binds_generated_callback_arguments() {
    let ir = lower_retained(
        "identity : a -> a\nlet identity value = value",
        "(identity >> Layer.run layer) acquire",
    );
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(main.contains("(with-owner-evidence "), "{ir}");
}

#[test]
fn inner_source_scope_overrides_supplied_higher_rank_region() {
    let ir = lower_body(
        "scope unrelated {\n    Scope.run (fun _ -> scope inner {\n      let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n      let _arena = Layer.run layer (fun () -> Arena.make ())\n      ()\n    })\n  }",
    );
    assert!(ir.contains("with-owner-evidence inner"), "{ir}");
    assert!(!ir.contains("with-owner-evidence unrelated"), "{ir}");
}

#[test]
fn earlier_runtime_arrow_captures_owner_at_its_own_entry() {
    let ir = lower_retained(
        "prepare : Unit ->{Scope s} Unit ->{Scope s} Arena.Value s String\nlet prepare () () = acquire ()",
        "Layer.run layer (prepare ())",
    );
    let prepare = ir
        .lines()
        .find(|line| line.starts_with("let prepare ="))
        .expect("lowered test definition");
    let (_, after_capture) = prepare
        .split_once("read-owner-evidence")
        .expect("capture supplied entry owner");
    assert!(
        after_capture.contains("(fun ()!"),
        "capture before entering the second arrow:\n{ir}"
    );
}

#[test]
fn named_nested_tuple_record_retains_extra_components() {
    let declarations = "run_tuple : ({ work : Unit ->{Read | r; e} a }, Int) ->{r; e} a\nlet run_tuple pair =\n  let (callbacks, _extra) = pair\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  Layer.run layer callbacks.work\ncallbacks : ({ work : Unit ->{Read, Scope s} Arena.Value s String }, Int)\nlet callbacks = ({ work = acquire }, 7)";
    let ir = lower_retained(declarations, "run_tuple callbacks");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(with-owner-evidence ") && main.contains("Test.callbacks"),
        "{ir}"
    );
    assert!(
        !main.contains("$field"),
        "transport must not destructure or reconstruct tuple components:\n{ir}"
    );
}

#[test]
fn widened_record_callback_does_not_bind_scope_free_work() {
    let declarations = "idle : Unit ->{} Unit\nlet idle () = ()\nrun_record : forall a r e. { acquire : Unit ->{Read | r; e} a; idle : Unit ->{Read | r; e} Unit } ->{r; e} a\nlet run_record callbacks =\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  Layer.run layer (fun () ->\n    callbacks.idle ()\n    callbacks.acquire ()\n  )";
    for idle in ["idle", "loop { break idle }", "if true then idle else idle"] {
        let call = format!("run_record {{ acquire = acquire; idle = {idle} }}");
        let ir = lower_retained(declarations, &call);
        let main = ir
            .lines()
            .find(|line| line.starts_with("let main ="))
            .expect("lowered test definition");
        assert!(
            !main.contains("= Test.idle;") && !main.contains(".idle;"),
            "only actual Scope requirements may bind an owner:\n{ir}"
        );
    }
}

#[test]
fn literal_task_binds_acquisition_to_source_owner() {
    let ir = lower_retained(TASK_VALUES, "run_task (task { acquire () })");
    let main = ir
        .lines()
        .find(|line| line.starts_with("let main ="))
        .expect("lowered test definition");
    assert!(
        main.contains("(task ") && main.contains("(with-owner-evidence "),
        "{ir}"
    );
}

#[test]
fn widened_branch_and_tuple_keep_scope_free_callbacks_unbound() {
    let declarations = "idle : Unit ->{} Unit\nlet idle () = ()\nrun_pair : (Unit ->{Read | r; e} a, Unit ->{Read | r; e} Unit) ->{r; e} a\nlet run_pair pair =\n  let (acquire, idle) = pair\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  Layer.run layer (fun () ->\n    idle ()\n    acquire ()\n  )";
    for call in [
        "run_pair (acquire, idle)",
        "run_pair (acquire, if true then idle else idle)",
    ] {
        let ir = lower_retained(declarations, call);
        let main = ir
            .lines()
            .find(|line| line.starts_with("let main ="))
            .expect("lowered test definition");
        assert!(
            !main.contains("= Test.idle;"),
            "no adapter around the actual Scope-free callback:\n{ir}"
        );
    }
}

#[test]
fn stored_list_is_constructed_once_and_invocations_transport_evidence() {
    let ir = lower_retained(
        "callbacks : List (Unit ->{Read, Scope s} Arena.Value s String)\nlet callbacks = [acquire]\nunder_list : List (Unit ->{Read | r; e} a) ->{r; e} a\nlet under_list callbacks =\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  match callbacks with\n  | [body, ..._] -> Layer.run layer body\n  | [] -> todo",
        "under_list callbacks",
    );
    let callbacks = ir
        .lines()
        .find(|line| line.starts_with("let callbacks ="))
        .expect("single global construction");
    assert_eq!(callbacks.matches("Test.acquire").count(), 1, "{ir}");
    assert!(
        !callbacks.contains("read-owner-evidence"),
        "pure storage does not capture future execution: {ir}"
    );
    let under = ir
        .lines()
        .find(|line| line.starts_with("let under_list ="))
        .expect("lowered definition");
    assert!(
        under.contains("read-owner-evidence") && under.contains("with-owner-evidence $entry"),
        "generic list consumer must forward its saved runtime entry: {ir}"
    );
}
