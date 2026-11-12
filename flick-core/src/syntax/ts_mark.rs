//! `@ts`: the TypeScript form of a type, said where the type is declared.
//!
//! ```text
//! @ts "ReadonlyArray<{0}>"                        the form: `{i}` is the type's i-th argument,
//!                                                 `{i:row}` that argument as a row
//! @ts "$Option.Option<{0}>" rt "$Option<{0}>"     and the form on Flick's own runtime
//! @ts none                                        the type has no form
//! @ts import runtime "Option" rt "OptionValue"    the export a `$Option` of the form stands for
//! ```

use super::cst::Node;
use super::kind::SyntaxKind as K;

/// What one `@ts` attribute says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// The form with the Effect adapter, and on Flick's own runtime.
    Form { effect: String, rt: String },
    /// The type has no TypeScript form.
    None,
    /// An export of the runtime a form names as `$` and the export's name
    /// with the adapter.
    Import {
        slot: Slot,
        effect: String,
        rt: String,
    },
}

/// The module of the runtime an export comes from; the build knows its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    Runtime,
    Ui,
}

/// A part of a form: its own text, or the place of an argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Place { index: usize, row: bool },
}

/// What the attribute `node` says, if it is a `@ts`; why it says nothing
/// that reads.
pub fn read(node: &Node, text: &str) -> Option<Result<Mark, String>> {
    let mut words = (node.child_tokens())
        .filter(|t| matches!(t.kind, K::Ident | K::String))
        .map(|t| (t.kind, t.text(text)));
    if words.next() != Some((K::Ident, "ts")) {
        return None;
    }
    let rest: Vec<(K, &str)> = words.collect();
    Some(mark(&rest))
}

fn mark(words: &[(K, &str)]) -> Result<Mark, String> {
    let usage = "`@ts` takes a form, `\"…\"` and maybe `rt \"…\"`, or `none`, or `import runtime \"Export\"`";
    match words {
        [(K::Ident, "none")] => Ok(Mark::None),
        [(K::Ident, "import"), (K::Ident, slot), rest @ ..] => {
            let slot = match *slot {
                "runtime" => Slot::Runtime,
                "ui" => Slot::Ui,
                other => {
                    return Err(format!(
                        "`{other}` is no module of the runtime: `runtime` or `ui`"
                    ));
                }
            };
            let (effect, rt) = pair(rest).ok_or(usage)?;
            for export in [&effect, &rt] {
                let word = !export.is_empty()
                    && export
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_');
                if !word {
                    return Err(format!("`{export}` is no name of an export"));
                }
            }
            Ok(Mark::Import { slot, effect, rt })
        }
        rest => {
            let (effect, rt) = pair(rest).ok_or(usage)?;
            pieces(&effect)?;
            pieces(&rt)?;
            Ok(Mark::Form { effect, rt })
        }
    }
}

/// `"a"` or `"a" rt "b"`: the one for both, or each.
fn pair(words: &[(K, &str)]) -> Option<(String, String)> {
    match words {
        [(K::String, one)] => {
            let one = value(one)?;
            Some((one.clone(), one))
        }
        [(K::String, effect), (K::Ident, "rt"), (K::String, rt)] => {
            Some((value(effect)?, value(rt)?))
        }
        _ => None,
    }
}

/// The text of a string literal that has no escape in it.
fn value(literal: &str) -> Option<String> {
    let inner = literal.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains(['\\', '\n', '"'])).then(|| inner.to_owned())
}

/// The parts of a form; a `{…}` that is no place is refused.
pub fn pieces(form: &str) -> Result<Vec<Piece>, String> {
    let mut out = Vec::new();
    let mut rest = form;
    while let Some(open) = rest.find('{') {
        let close = rest[open..]
            .find('}')
            .ok_or("a `{` of the form is not closed")?
            + open;
        if open > 0 {
            out.push(Piece::Text(rest[..open].to_owned()));
        }
        let place = &rest[open + 1..close];
        let (digits, row) = match place.strip_suffix(":row") {
            Some(digits) => (digits, true),
            None => (place, false),
        };
        let index = (digits.parse::<usize>().ok())
            .filter(|_| digits.chars().all(|c| c.is_ascii_digit()))
            .ok_or_else(|| {
                format!("`{{{place}}}` is no place of an argument: `{{0}}` or `{{0:row}}`")
            })?;
        out.push(Piece::Place { index, row });
        rest = &rest[close + 1..];
    }
    if rest.contains('}') {
        return Err("a `}` of the form has no `{`".into());
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_owned()));
    }
    Ok(out)
}

/// The names a form takes from the runtime: each `$Name` in its text.
pub fn names(pieces: &[Piece]) -> Vec<&str> {
    let mut out = Vec::new();
    for piece in pieces {
        let Piece::Text(text) = piece else { continue };
        let mut rest = text.as_str();
        while let Some(at) = rest.find('$') {
            let after = &rest[at + 1..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            if end > 0 {
                out.push(&after[..end]);
            }
            rest = &after[end..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_is_its_text_and_the_places_of_its_arguments() {
        assert_eq!(
            pieces("$Effect.Effect<{2}, {1:row}>"),
            Ok(vec![
                Piece::Text("$Effect.Effect<".into()),
                Piece::Place {
                    index: 2,
                    row: false
                },
                Piece::Text(", ".into()),
                Piece::Place {
                    index: 1,
                    row: true
                },
                Piece::Text(">".into()),
            ])
        );
        assert_eq!(pieces("number"), Ok(vec![Piece::Text("number".into())]));
        assert!(pieces("Array<{a}>").is_err());
        assert!(pieces("Array<{0").is_err());
        assert!(pieces("Array<0}>").is_err());
        assert!(pieces("{0:set}").is_err());
        let form = pieces("$Schema.Codec<{0}, $Other>").expect("a form");
        assert_eq!(names(&form), ["Schema", "Other"]);
    }

    #[test]
    fn a_mark_is_a_form_none_or_an_import() {
        let string = |s: &'static str| (K::String, s);
        let word = |s: &'static str| (K::Ident, s);
        assert_eq!(mark(&[word("none")]), Ok(Mark::None));
        assert_eq!(
            mark(&[string("\"A<{0}>\""), word("rt"), string("\"B<{0}>\"")]),
            Ok(Mark::Form {
                effect: "A<{0}>".into(),
                rt: "B<{0}>".into()
            })
        );
        assert_eq!(
            mark(&[word("import"), word("ui"), string("\"Ui\"")]),
            Ok(Mark::Import {
                slot: Slot::Ui,
                effect: "Ui".into(),
                rt: "Ui".into()
            })
        );
        assert!(mark(&[word("import"), word("elsewhere"), string("\"Ui\"")]).is_err());
        assert!(mark(&[word("import"), word("ui"), string("\"a b\"")]).is_err());
        assert!(mark(&[]).is_err());
        assert!(mark(&[string("\"A<{x}>\"")]).is_err());
    }
}
