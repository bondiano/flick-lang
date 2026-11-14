#[test]
fn original_foreign_nonregular_task_alias_uses_finite_ownership_policy() {
    let text = "module Test exposing (repeat)\nopaque type alias Repeat a s = Eff {Scope s} none (Repeat (List a) s)\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    assert!(
        ir.contains("let rec $owner"),
        "growing irrelevant List type arguments cannot erase recursive execution policy: {ir}"
    );
    assert!(
        ir.contains("(task ") && ir.contains("read-owner-evidence") && ir.contains("(within "),
        "every future raw task consumes its own candidate: {ir}"
    );
}

#[test]
fn original_foreign_error_projection_retains_executable_argument_policy() {
    let text = "module Test exposing (make)\ntype alias Err a = Errors {a | none}\nmake : Unit -> Err (Unit -> Eff {Scope s} none Unit)\nforeign pure make from \"flick:rt\" \"projectedExecutable\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    let wrapper = ir
        .lines()
        .find(|line| line.starts_with("let make ="))
        .expect("projected executable shim");
    assert!(
        wrapper.contains("(task ")
            && wrapper.contains("read-owner-evidence")
            && wrapper.contains("(within "),
        "Errors projection exposes the original executable, not opaque data: {ir}"
    );
    assert_eq!(
        wrapper.matches("Test.$raw_make").count(),
        1,
        "factory evaluated once: {ir}"
    );
}

#[test]
fn original_foreign_row_growth_uses_lazy_scope_policy() {
    let text = "module Test exposing (repeat)\nopaque type alias Repeat r s = Eff r none (Repeat {Scope s | r} s)\nrepeat : Repeat {} s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    assert!(
        ir.contains("(task ") && ir.contains("read-owner-evidence") && ir.contains("(within "),
        "future row gains Scope without eagerly specializing recursive arguments: {ir}"
    );
}

#[test]
fn original_foreign_projected_recursive_alias_remains_lazy() {
    let text = "module Test exposing (repeat)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    assert!(
        ir.contains("(task ") && ir.contains("read-owner-evidence") && ir.contains("(within "),
        "singleton projection must not discard the recursive executable descriptor: {ir}"
    );
}

#[test]
fn original_foreign_nested_identity_aliases_do_not_erase_a_productive_head() {
    let text = "module Test exposing (repeat)\nopaque type alias Id a = a\nopaque type alias Repeat a s = Eff {Scope s} none (Id (Id (Repeat (List a) s)))\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    assert!(
        ir.contains("(task ") && ir.contains("read-owner-evidence"),
        "a repeated alias name with a different lazy argument still exposes its task: {ir}"
    );
}

#[test]
fn original_foreign_unproductive_alias_keeps_the_existing_opaque_head_behavior() {
    let text = "module Test exposing (idle)\nopaque type alias Idle a = Idle (List a)\nidle : Idle (Eff {Scope s} none Unit)\nforeign pure idle from \"flick:rt\" \"idle\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    assert!(
        module.items.iter().any(|item| matches!(&item.kind, ItemKind::Foreign(foreign) if foreign.name == "idle" && foreign.source_name.is_none())),
        "the existing unproductive opaque head needs no callable shim or source rejection: {ir}"
    );
}

#[test]
fn original_foreign_returned_sync_arrow_preserves_its_stage_before_scoped_task_data() {
    fn has_sync(expr: &Expr) -> bool {
        let mut found = matches!(&expr.kind, ExprKind::Lambda { params, .. } if params.iter().any(|param| param.stage == Stage::Sync));
        expr.each_child(&mut |child| found |= has_sync(child));
        found
    }
    let text = "module Test exposing (repeat)\nopaque type alias Repeat s = Eff {Scope s} none (Unit ->{Atom.Read} Repeat s)\nrepeat : Repeat s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert!(
        module
            .items
            .iter()
            .any(|item| matches!(&item.kind, ItemKind::Value { body, .. } if has_sync(body))),
        "returned Atom.Read arrow must remain Sync, so its Task result stays data"
    );
}

#[test]
fn original_foreign_descriptor_sentinels_use_runtime_discrimination() {
    fn visit(expr: &Expr) {
        if let ExprKind::Match(_, arms) = &expr.kind {
            assert!(
                !arms
                    .iter()
                    .any(|arm| matches!(arm.pat, Pat::Lit(Lit::Unit))),
                "Unit patterns are source-irrefutable, not a runtime descriptor sentinel test"
            );
        }
        expr.each_child(&mut visit);
    }
    let text = "module Test exposing (repeat)\nopaque type alias Repeat r s = Eff r none (Repeat {Scope s | r} s)\nrepeat : Repeat {} s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            visit(body);
        }
    }
}

#[test]
fn original_foreign_generated_integer_dispatches_are_exhaustive() {
    fn visit(expr: &Expr) {
        if let ExprKind::Match(_, arms) = &expr.kind
            && arms
                .iter()
                .any(|arm| matches!(arm.pat, Pat::Lit(Lit::Int(_))))
        {
            assert!(
                arms.iter().any(|arm| matches!(arm.pat, Pat::Wild)),
                "generated integer IDs need an explicit impossible default, not weakened exhaustivity"
            );
        }
        expr.each_child(&mut visit);
    }
    let text = "module Test exposing (repeat)\nopaque type alias Repeat s = Eff {Scope s} none (Repeat s)\nrepeat : Repeat s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            visit(body);
        }
    }
}

#[test]
fn original_foreign_productive_head_has_no_owner_adapter_depth_budget() {
    fn no_budget(expr: &Expr) {
        if let ExprKind::Binary(BinOp::Arith(Arith::Sub, _), _, _) = expr.kind {
            panic!("owner descriptor normalization must not consume a depth budget");
        }
        expr.each_child(&mut no_budget);
    }
    let mut result = "Repeat Int s".to_owned();
    for _ in 0..80 {
        result = format!("Id ({result})");
    }
    let text = format!(
        "module Test exposing (repeat)\nopaque type alias Id a = a\nopaque type alias Repeat a s = Eff {{Scope s}} none ({result})\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n"
    );
    let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            no_budget(body);
        }
    }
}

#[test]
fn original_foreign_zero_dictionary_epsilon_cycle_keeps_a_cold_task() {
    let text = "module Test exposing (repeat)\nopaque type alias Idle a = forall b. Idle (List a)\nopaque type alias Repeat s = Eff {Scope s} none (Idle Int)\nrepeat : Repeat s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    assert!(ir.contains("(task ") && ir.contains("read-owner-evidence"));
    assert_eq!(ir.matches("Test.$raw_repeat").count(), 1);
}

#[test]
fn original_foreign_residual_alias_uses_the_exact_descriptor_head() {
    let mut text = "module Test exposing (repeat)\nopaque type alias Chain0 a = a\n".to_owned();
    for index in 1..=200 {
        writeln!(
            text,
            "opaque type alias Chain{index} a = Chain{} a",
            index - 1
        )
        .expect("write alias source");
    }
    text.push_str("opaque type alias Repeat s = Eff {Scope s} none (Chain200 (Repeat s))\nrepeat : Repeat s\nforeign pure repeat from \"flick:rt\" \"repeat\"\nmain : Unit ->{} Unit\nlet main () = scope s { let first = await repeat; let second = await first; let _third = await second; () }\n");
    let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    assert!(ir.contains("(task ") && ir.contains("read-owner-evidence"));
}

#[test]
fn original_foreign_projected_growing_task_can_be_awaited_at_source() {
    let text = "module Test exposing (main)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))\nmake : Fiber.Owner s -> Repeat Int s\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\nservice Read =\n  read : Unit ->{} String\nmain : Unit ->{} Unit\nlet main () = scope outer {\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  let first = make outer\n  let _second = Layer.run layer (fun () -> await first)\n  ()\n}\n";
    lower_text("Test", text).unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
}

#[test]
fn original_foreign_projected_growing_task_can_cross_an_abstract_argument() {
    let text = "module Test exposing (main)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))\nmake : Fiber.Owner s -> Repeat Int s\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\ndrain : a ->{} Unit\nforeign effect drain from \"./owner.mjs\" \"drain\"\n  retention call\nmain : Unit ->{} Unit\nlet main () = scope outer {\n  drain (make outer)\n}\n";
    lower_text("Test", text).unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
}

#[test]
fn original_foreign_task_can_return_an_opaque_zero_dictionary_forall() {
    let text = "module Test exposing (main)\nopaque type alias Idle a = forall b. Idle (List a)\nopaque type alias Repeat s = Eff {Scope s} none (Idle Int)\nmake : Fiber.Owner s -> Repeat s\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\nservice Read =\n  read : Unit ->{} String\nmain : Unit ->{} Unit\nlet main () = scope outer {\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  let first = make outer\n  let _second = Layer.run layer (fun () -> await first)\n  ()\n}\n";
    lower_text("Test", text).unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
}

#[test]
fn transparent_polymorphic_task_results_still_require_an_explicit_signature() {
    let text = "module Test exposing (main)\ntype alias Poly = forall a. Unit -> a\nmake : Fiber.Owner s -> Eff {Scope s} none Poly\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\nservice Read =\n  read : Unit ->{} String\nmain : Unit ->{} Unit\nlet main () = scope outer {\n  let layer = Layer.succeed Read { read = fun () -> \"unused\" }\n  let first = make outer\n  let _second = Layer.run layer (fun () -> await first)\n  ()\n}\n";
    let diagnostics = lower_text("Test", text).expect_err("a transparent forall is not a monotype");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("polymorphic type")),
        "{diagnostics:?}"
    );
}

#[test]
fn projected_recursive_alias_comparison_keeps_nominal_arguments_invariant() {
    let text = "module Test exposing (main)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))\nmake : Fiber.Owner s -> Repeat Int s\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\nconsume : Repeat String s ->{} Unit\nforeign effect consume from \"./owner.mjs\" \"consume\"\n  retention call\nmain : Unit ->{} Unit\nlet main () = scope s { consume (make s) }\n";
    let diagnostics = lower_text("Test", text).expect_err("recursive aliases keep their arguments");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Int")
                && diagnostic.message.contains("String")),
        "{diagnostics:?}"
    );
}

#[test]
fn deeply_qualified_foreign_tasks_are_observed_without_a_head_budget() {
    let qualifiers = "^{s}".repeat(160);
    let text = format!(
        "module Test exposing (main)\nmake : Fiber.Owner s -> (Eff {{Scope s}} none Unit){qualifiers}\nforeign pure make from \"./owner.mjs\" \"make\"\n  retention call\nmain : Unit ->{{}} Unit\nlet main () = scope s {{ await (make s) }}\n"
    );
    lower_text("Test", &text).unwrap_or_else(|diagnostics| panic!("{diagnostics:?}"));
}

#[test]
fn an_unproductive_opaque_value_is_not_guessed_to_be_a_task() {
    let text = "module Test exposing (main)\nopaque type alias Idle a = Idle (List a)\nidle : Idle Int\nforeign pure idle from \"./owner.mjs\" \"idle\"\nmain : Unit ->{} Unit\nlet main () = scope s { await idle }\n";
    let diagnostics = lower_text("Test", text).expect_err("no executable head exists");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("`await` runs an `Eff` value")),
        "{diagnostics:?}"
    );
}
