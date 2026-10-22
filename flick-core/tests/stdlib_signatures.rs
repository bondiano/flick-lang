//! The standard library declares every signature the specification gives
//! for it, word for word: each `Module.name :` of a ```` ```text ```` block
//! whose module is a built-in one, and the Prelude functions the spec names
//! bare.

use std::fs;
use std::path::Path;

use flick_core::types::builtin_sources;

const SPEC: &str = "Flick спецификация ML языка с HKT и Effect.md";

/// Prelude values the spec gives without a module.
const PRELUDE: &[&str] = &["bind", "bind_eff", "from_result"];

/// The signature texts of ```` ```text ```` blocks: a line `name :` and the
/// indented lines after it, as `(name, signature)` with whitespace collapsed.
fn spec_signatures(markdown: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut in_text = false;
    let mut current: Option<(String, String)> = None;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            out.extend(current.take());
            in_text = line.trim() == "```text";
            continue;
        }
        if !in_text {
            continue;
        }
        let continued = line.starts_with(' ') && !line.trim().is_empty();
        match &mut current {
            Some((_, text)) if continued => {
                text.push(' ');
                text.push_str(line.trim());
            }
            _ => {
                out.extend(current.take());
                if let Some((name, rest)) = line.split_once(" :")
                    && !name.contains(' ')
                    && (rest.is_empty() || rest.starts_with(' '))
                {
                    current = Some((name.to_owned(), rest.trim().to_owned()));
                }
            }
        }
    }
    out.extend(current);
    out.into_iter()
        .map(|(n, s)| (n, s.trim().to_owned()))
        .collect()
}

/// The signature of `name` in a module source, whitespace collapsed.
fn declared(source: &str, name: &str) -> Option<String> {
    let mut lines = source
        .lines()
        .skip_while(|l| !l.starts_with(&format!("{name} :")));
    let first = lines.next()?;
    let mut text = first[name.len() + 2..].trim().to_owned();
    for line in lines.take_while(|l| l.starts_with(' ')) {
        text.push(' ');
        text.push_str(line.trim());
    }
    Some(text.trim().to_owned())
}

#[test]
fn stdlib_declares_the_spec_signatures_word_for_word() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let spec = fs::read_to_string(root.join(SPEC)).expect("spec is readable");
    let mut checked = Vec::new();
    let mut mismatches = Vec::new();
    for (name, signature) in spec_signatures(&spec) {
        let (module, value) = match name.rsplit_once('.') {
            Some((m, v)) => (m, v),
            None if PRELUDE.contains(&name.as_str()) => ("Prelude", name.as_str()),
            None => continue,
        };
        let Some((_, source)) = builtin_sources().iter().find(|(m, _)| *m == module) else {
            continue;
        };
        match declared(source, value) {
            Some(text) if text == signature => checked.push(name.clone()),
            Some(text) => {
                mismatches.push(format!("{name}\n  spec:   {signature}\n  stdlib: {text}"));
            }
            None => mismatches.push(format!("{name}: not declared in {module}")),
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    for expected in [
        "List.map",
        "List.map_effect",
        "Ref.make",
        "Ref.get",
        "Ref.set",
        "Ref.modify",
        "Failure.primary",
        "Failure.raw_cause",
        "Failure.inspect",
        "Failure.rethrow",
        "RawCause.inspect",
        "Redacted.make",
        "Redacted.reveal",
        "from_result",
        "bind",
        "bind_eff",
    ] {
        assert!(
            checked.iter().any(|c| c == expected),
            "{expected} was not checked: {checked:?}"
        );
    }
}
