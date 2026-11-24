/// `FuncRef t`.
fn func_ref(t: Type) -> Type {
    Type::app(ty("FuncRef"), t)
}

fn int_to_int() -> Type {
    Type::fun(ty("Int"), Stage::Pure, ty("Int"))
}

#[test]
fn func_ref_in_takes_a_lambda_that_names_no_local() {
    let double = Term::lam("n", var("n"));
    assert!(check(&double, &mono(func_ref(int_to_int()))).is_ok());
    // what the λ binds itself is no capture
    let nested = Term::lam("n", let_("m", var("n"), var("m")));
    assert!(check(&nested, &mono(func_ref(int_to_int()))).is_ok());
}

#[test]
fn func_ref_in_refuses_a_lambda_that_captures_a_local() {
    let captures = let_(
        "by",
        int(1),
        Term::lam("n", var("by")),
    );
    rejected_by(
        check(&captures, &mono(func_ref(int_to_int()))),
        "FuncRef-In",
    );
}

#[test]
fn a_func_ref_value_passes_on_as_it_is() {
    // `λr. r` at `FuncRef τ → FuncRef τ`: `r` is a value of `FuncRef τ` already
    let pass = Term::lam("r", var("r"));
    let t = Type::fun(func_ref(int_to_int()), Stage::Pure, func_ref(int_to_int()));
    assert!(check(&pass, &mono(t)).is_ok());
}

#[test]
fn a_match_passes_func_ref_on_to_its_arms() {
    let pick = Term::lam(
        "flag",
        match_(
            var("flag"),
            vec![
                arm(Pat::Lit(Lit::Bool(true)), Term::lam("n", var("n"))),
                arm(Pat::Wild, Term::lam("n", int(0))),
            ],
        ),
    );
    let t = Type::fun(ty("Bool"), Stage::Pure, func_ref(int_to_int()));
    assert!(check(&pick, &mono(t)).is_ok());
}

#[test]
fn a_top_level_func_ref_passes_on_as_it_is() {
    // `λr. (λs. s) r` at `FuncRef τ → FuncRef τ`: the inner λ is applied to a `FuncRef`
    let pass = Term::lam("r", Term::app(Term::lam("s", var("s")), var("r")));
    let t = Type::fun(func_ref(int_to_int()), Stage::Pure, func_ref(int_to_int()));
    assert!(check(&pass, &mono(t)).is_ok());
}
