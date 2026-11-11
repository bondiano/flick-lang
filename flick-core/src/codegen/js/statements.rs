/// Statements as the lines of a block, indented one level.
fn indent(stmts: &[String]) -> String {
    let mut out = String::new();
    for stmt in stmts {
        for line in stmt.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// A top-level `value` that is a call or an immediately run arrow, marked
/// `/*#__PURE__*/`: top-level initialization is pure (spec 11), so a bundler
/// may drop it when nothing uses it.
fn pure(value: &str) -> String {
    // a call starts with the marker of its span
    let code = expr::unmarked(value);
    let rest =
        code.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
    let call = rest.len() < code.len() && rest.starts_with('(');
    if call || code.starts_with("((") {
        format!("/*#__PURE__*/ {value}")
    } else {
        value.to_owned()
    }
}

/// Includes optional release workers and direct instance methods, whose
/// physical owner is the same as the binding that defines them.
pub(crate) fn runtime_exports(item: &Item) -> Vec<String> {
    let mut names = items::defined(item);
    if let Some((name, arity)) = release::exported_worker(item) {
        names.push(format!("{}${arity}", self::names::ident(name)));
    }
    names.extend(
        release::direct_methods(item)
            .into_iter()
            .map(|(instance, method)| format!("{}${method}", self::names::instance(instance))),
    );
    names
}
