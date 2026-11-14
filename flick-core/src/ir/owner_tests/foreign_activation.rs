#[test]
fn original_scoped_foreign_alias_keeps_raw_activation_inside_callable() {
    let text = "module Test exposing (alias)\nacquire : Unit ->{Scope s} Arena.Arena s\nforeign effect acquire from \"flick:rt\" \"arenaMake\"\n  retention call\nalias : Unit ->{Scope s} Arena.Arena s\nlet alias = acquire\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let raw = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(f) => Some(f),
            _ => None,
        })
        .expect("raw host item");
    assert_ne!(raw.name, "acquire");
    assert_eq!(raw.source_name.as_deref(), Some("acquire"));
    assert_eq!(raw.binding.name, "arenaMake");
    assert_eq!(raw.arity, 1);
    let ir = print(&module);
    let wrapper = ir
        .lines()
        .find(|line| line.starts_with("let acquire ="))
        .expect("original source identity wrapper");
    assert!(
        wrapper.contains("read-owner-evidence") && wrapper.contains("(within "),
        "{ir}"
    );
    assert!(
        ir.lines()
            .any(|line| line.starts_with("let alias =") && line.contains("Test.acquire")),
        "alias shares the callable wrapper: {ir}"
    );
}

#[test]
fn outer_abstract_row_uses_saved_entry_inside_fresh_scope_callback() {
    let text = "module Test exposing (forward)\nforward : (Unit ->{r} Unit) ->{r} Unit\nlet forward body = Scope.run (fun _ -> body ())\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    transport_invariants(&module);
    let ir = print(&module);
    let definition = ir
        .lines()
        .find(|line| line.starts_with("let forward ="))
        .expect("lowered definition");
    let entry = definition.split_once("let ").expect("definition prefix").1;
    let capture = entry
        .find("read-owner-evidence")
        .expect("save source entry candidate");
    assert!(
        capture < entry.find("Scope.run").expect("fresh callback boundary"),
        "source r must be captured before the fresh scope changes C: {ir}"
    );
    assert!(
        definition.contains("with-owner-evidence $entry (body"),
        "outer row invocation must forward the saved entry, not a fresh dynamic read: {ir}"
    );
}

#[test]
fn original_foreign_returned_task_is_wrapped_cold_not_run_at_factory_entry() {
    let text = "module Test exposing (make)\nmake : Unit ->{} Eff {Scope s} none (Arena.Arena s)\nforeign effect make from \"flick:rt\" \"foreignAcquireTask\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    let wrapper = ir
        .lines()
        .find(|line| line.starts_with("let make ="))
        .expect("factory shim");
    let task = wrapper
        .find("(task ")
        .expect("returned executable stays cold");
    let read = wrapper
        .find("read-owner-evidence")
        .expect("future execution reads its evidence");
    assert!(
        task < read,
        "task evidence must be read when awaited, not when factory executes: {ir}"
    );
    assert_eq!(
        wrapper.matches("Test.$raw_make").count(),
        1,
        "factory evaluates once: {ir}"
    );
}

#[test]
fn original_scoped_foreign_type_alias_keeps_raw_activation() {
    for signature in ["Scoped s", "(Scoped s)^{}"] {
        let text = format!(
            "module Test exposing (acquire)\ntype alias ScopeRow s = {{Scope s}}\ntype alias Scoped s = Unit ->{{...ScopeRow s}} Arena.Arena s\nacquire : {signature}\nforeign effect acquire from \"flick:rt\" \"arenaMake\"\n  retention call\n"
        );
        let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{d:?}"));
        let ir = print(&module);
        let raw = module
            .items
            .iter()
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(f) => Some(f),
                _ => None,
            })
            .expect("raw foreign");
        assert_eq!(
            raw.source_name.as_deref(),
            Some("acquire"),
            "original alias Scope must install a raw shim: {ir}"
        );
        let wrapper = ir
            .lines()
            .find(|line| line.starts_with("let acquire ="))
            .expect("aliased callable shim");
        assert!(
            wrapper.contains("read-owner-evidence") && wrapper.contains("(within "),
            "{ir}"
        );
    }
}

#[test]
fn original_foreign_returned_executable_aliases_keep_future_scope_policy() {
    for (alias, signature, mode, arity, expected) in [
        (
            "type alias Scoped s = Unit ->{Scope s} Arena.Arena s",
            "Int -> Scoped s",
            "pure",
            "  arity 1\n",
            "(fun $arg",
        ),
        (
            "type alias Scoped s = Eff {Scope s} none (Arena.Arena s)",
            "Unit ->{} Scoped s",
            "effect",
            "",
            "(task ",
        ),
    ] {
        let text = format!(
            "module Test exposing (make)\n{alias}\nmake : {signature}\nforeign {mode} make from \"flick:rt\" \"aliasedExecutable\"\n{arity}"
        );
        let module = lower_text("Test", &text).unwrap_or_else(|d| panic!("{text}\n{d:?}"));
        let ir = print(&module);
        let wrapper = ir
            .lines()
            .find(|line| line.starts_with("let make ="))
            .expect("returned alias shim");
        assert!(
            wrapper.contains(expected)
                && wrapper.contains("read-owner-evidence")
                && wrapper.contains("(within "),
            "{ir}"
        );
        assert_eq!(
            wrapper.matches("Test.$raw_make").count(),
            1,
            "construction once: {ir}"
        );
    }
}

#[test]
fn scope_free_foreign_alias_does_not_activate_a_widened_expected_scope() {
    let text = "module Test exposing (widened)\ntype alias Idle = Unit ->{} Unit\nidle : (Idle)^{}\nforeign effect idle from \"flick:rt\" \"idle\"\nwidened : Unit ->{Scope s} Unit\nlet widened = idle\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    assert!(
        !ir.contains("(within "),
        "widening cannot make original scope-free work acquire into C: {ir}"
    );
    let raw = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(f) => Some(f),
            _ => None,
        })
        .expect("scope-free foreign");
    assert_eq!(
        raw.source_name, None,
        "no scoped raw shim for actual empty row: {ir}"
    );
}

#[test]
fn qualified_original_scoped_foreign_keeps_dictionary_arguments() {
    let text = "module Test exposing (acquire)\nclass Gate a =\n  gate : a -> Bool\ntype alias Scoped s a = Unit ->{Scope s} a\nacquire : Gate a => (Scoped s a)^{}\nforeign effect acquire from \"flick:rt\" \"genericAcquire\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    let raw = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Foreign(f) => Some(f),
            _ => None,
        })
        .expect("raw foreign");
    assert_eq!(raw.source_name.as_deref(), Some("acquire"), "{ir}");
    assert_eq!(raw.dicts, 1, "raw dictionary ABI preserved: {ir}");
    let dicts = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, .. } if name == "acquire" => Some(dicts),
            _ => None,
        })
        .expect("callable shim");
    assert_eq!(
        dicts.len(),
        1,
        "shim retains original dictionary arity: {ir}"
    );
    assert!(ir.contains("(within "), "{ir}");
}

#[test]
fn original_foreign_returned_forall_preserves_dictionary_stage_before_scoped_arrow() {
    let text = "module Test exposing (make)\nclass Gate a =\n  gate : a -> Bool\nmake : Unit ->{} (forall a. Gate a => Unit ->{Scope s} a)\nforeign effect make from \"flick:rt\" \"returnedQualified\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    let ir = print(&module);
    let wrapper = ir
        .lines()
        .find(|line| line.starts_with("let make ="))
        .expect("factory wrapper");
    assert!(
        wrapper.contains("(\\["),
        "returned forall dictionary stage must not be consumed as source Unit: {ir}"
    );
    assert!(
        wrapper.contains("(within "),
        "actual returned Scope arrow still activates: {ir}"
    );
    assert_eq!(
        wrapper.matches("Test.$raw_make").count(),
        1,
        "factory runs once: {ir}"
    );
}

#[test]
fn original_foreign_recursive_task_alias_uses_cold_recursive_adapter() {
    let text = "module Test exposing (repeat)\nopaque type alias Repeat s = Eff {Scope s} none (Repeat s)\nrepeat : Repeat s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    assert_recursive_owner_adapter(&module);
    let ir = print(&module);
    let wrapper = ir
        .lines()
        .find(|line| line.starts_with("let repeat ="))
        .expect("recursive task shim");
    assert!(
        wrapper.contains("let rec $owner"),
        "regular alias cycle must be a real recursive adapter, not a dropped result: {ir}"
    );
    assert!(
        wrapper.contains("(task ")
            && wrapper.contains("read-owner-evidence")
            && wrapper.contains("(within "),
        "future task executions consume their own candidate: {ir}"
    );
    assert!(
        wrapper.matches("$owner").count() >= 3,
        "returned tasks must reenter the adapter graph: {ir}"
    );
    assert_eq!(
        wrapper.matches("Test.$raw_repeat").count(),
        1,
        "raw constant constructed once: {ir}"
    );
}

#[test]
fn original_foreign_returned_unconstrained_forall_never_calls_empty_dictionary_stage() {
    fn no_empty_dict_call(expr: &Expr) {
        if let ExprKind::Dicts(_, dicts) = &expr.kind {
            assert!(
                !dicts.is_empty(),
                "zero dictionaries must not eagerly invoke the returned ordinary arrow: {expr:?}"
            );
        }
        expr.each_child(&mut no_empty_dict_call);
    }
    let text = "module Test exposing (make)\nmake : Unit ->{} (forall a. Unit ->{Scope s} a)\nforeign effect make from \"flick:rt\" \"returnedUnconstrained\"\n";
    let module = lower_text("Test", text).unwrap_or_else(|d| panic!("{d:?}"));
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            no_empty_dict_call(body);
        }
    }
}
