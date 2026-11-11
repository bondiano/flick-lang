fn finish(
    #[cfg(feature = "native-project")] code: &str,
    #[cfg(not(feature = "native-project"))] code: String,
    #[cfg(feature = "native-project")] spans: &[Span],
    #[cfg(feature = "native-project")] names: &HashMap<usize, String>,
) -> Output {
    #[cfg(feature = "native-project")]
    {
        let mut out = String::with_capacity(code.len());
        let mut mappings = Vec::new();
        let (mut line, mut col) = (0u32, 0u32);
        let mut chars = code.chars();
        while let Some(c) = chars.next() {
            match c {
                MARK_START => {
                    let id: String = chars.by_ref().take_while(|&c| c != MARK_END).collect();
                    if let Ok(id) = id.parse::<usize>() {
                        mappings.push((line, col, spans[id], names.get(&id).cloned()));
                    }
                }
                '\n' => {
                    out.push(c);
                    line += 1;
                    col = 0;
                }
                c => {
                    out.push(c);
                    col += u32::try_from(c.len_utf16()).unwrap_or(1);
                }
            }
        }
        Output {
            code: out,
            mappings,
            imports: Vec::new(),
        }
    }
    #[cfg(not(feature = "native-project"))]
    Output {
        code,
        imports: Vec::new(),
    }
}

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
