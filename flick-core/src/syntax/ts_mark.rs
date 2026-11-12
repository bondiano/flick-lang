//! `@ts`: the TypeScript form of a type, said where the type is declared.
//!
//! ```text
//! @ts "ReadonlyArray<{0}>"                        the form: `{i}` is the type's i-th argument,
//!                                                 `{i:row}` that argument as a row
//! @ts "$Option.Option<{0}>" rt "$Option<{0}>"     and the form on Flick's own runtime
//! @ts none                                        the type has no form
//! @ts import "dayjs" "Dayjs"                      the export a `$Dayjs` of the form stands for, from
//!                                                 a module named as `foreign … from` names one
//! @ts import runtime "Option" rt "OptionValue"    or from the runtime, in the built-in modules
//! ```
//!
//! A form is written into generated code as it stands, and one comes from a
//! package or from the interface of a library: it is the text of a type and
//! nothing else, or it is refused, both where it is written and where an
//! interface is read.

use super::cst::Node;
use super::kind::SyntaxKind as K;

/// The tag of an interface for the types with a form: each with the
/// [`parts`] of the form.
pub const TAG: &str = "ts";

/// What one `@ts` attribute says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// The form with the Effect adapter, and on Flick's own runtime.
    Form { effect: String, rt: String },
    /// The type has no TypeScript form.
    None,
    /// An export a form names as `$` and the export's name with the adapter.
    Import(Import),
}

/// An export a form names: `$` and its name with the adapter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Import {
    pub from: Origin,
    pub effect: String,
    pub rt: String,
}

/// The module an export comes from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// The runtime and its module of the UI; the build knows their paths.
    Runtime,
    Ui,
    /// A module by its specifier: a package, or a file beside the module
    /// that declares the type.
    Module(String),
}

/// A part of a form: its own text, an export it names, or the place of an
/// argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Name(String),
    Place { index: usize, row: bool },
}

/// The form of a type, as its marks give it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub arity: usize,
    /// With the Effect adapter and on Flick's own runtime; `None` for a
    /// type that has no form.
    pub form: Option<(Vec<Piece>, Vec<Piece>)>,
    pub imports: Vec<Import>,
}

/// What the attribute `node` says, if it is a `@ts`; why it says nothing
/// that reads.
pub fn read(node: &Node, text: &str) -> Option<Result<Mark, String>> {
    let mut words = (node.child_tokens())
        .filter(|t| matches!(t.kind, K::Ident | K::String))
        .map(|t| (t.kind, t.text(text)));
    if words.next() != Some((K::Ident, TAG)) {
        return None;
    }
    let rest: Vec<(K, &str)> = words.collect();
    Some(mark(&rest))
}

const USAGE: &str = "`@ts` takes a form, `\"…\"` and maybe `rt \"…\"`, or `none`, or `import \"module\" \"Export\"`";

fn mark(words: &[(K, &str)]) -> Result<Mark, String> {
    match words {
        [(K::Ident, "none")] => Ok(Mark::None),
        [(K::Ident, "import"), (kind, from), rest @ ..] => {
            let from = match (*kind, *from) {
                (K::Ident, "runtime") => Origin::Runtime,
                (K::Ident, "ui") => Origin::Ui,
                (K::String, spec) => Origin::Module(value(spec).ok_or(USAGE)?),
                (_, other) => {
                    return Err(format!(
                        "`{other}` is no module: `runtime`, `ui` or a specifier, as `\"dayjs\"`"
                    ));
                }
            };
            let (effect, rt) = pair(rest).ok_or(USAGE)?;
            import(from, effect, rt).map(Mark::Import)
        }
        rest => {
            let (effect, rt) = pair(rest).ok_or(USAGE)?;
            pieces(&effect)?;
            pieces(&rt)?;
            Ok(Mark::Form { effect, rt })
        }
    }
}

/// An import whose module and exports are names and nothing else.
fn import(from: Origin, effect: String, rt: String) -> Result<Import, String> {
    if let Origin::Module(spec) = &from {
        let plain = |c: char| c.is_ascii_alphanumeric() || "@/._:-".contains(c);
        if spec.is_empty() || !spec.chars().all(plain) {
            return Err(format!("`{spec}` is no specifier of a module"));
        }
    }
    for export in [&effect, &rt] {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        if export.is_empty() || !export.chars().all(word) {
            return Err(format!("`{export}` is no name of an export"));
        }
    }
    Ok(Import { from, effect, rt })
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

/// The parts of a form; one that is not the text of a type is refused.
pub fn pieces(form: &str) -> Result<Vec<Piece>, String> {
    let mut out = Vec::new();
    let mut rest = form;
    while let Some(open) = rest.find('{') {
        let close = rest[open..]
            .find('}')
            .ok_or("a `{` of the form is not closed")?
            + open;
        names(&rest[..open], &mut out)?;
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
    names(rest, &mut out)?;
    plain(form)?;
    Ok(out)
}

/// `text` as the pieces of a form: each `$Name` in it is an export.
fn names(text: &str, out: &mut Vec<Piece>) -> Result<(), String> {
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        let after = &rest[at + 1..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if end == 0 {
            return Err("a `$` of the form names no export".into());
        }
        if at > 0 {
            out.push(Piece::Text(rest[..at].to_owned()));
        }
        out.push(Piece::Name(after[..end].to_owned()));
        rest = &after[end..];
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest.to_owned()));
    }
    Ok(())
}

/// Whether `form` can only be read as a type where it is written: names,
/// brackets that close in order, a `,` inside brackets alone, and no `=`
/// but that of `=>`.
// shortcut: a form has no string literal and no object type; widen the characters, with the same care, when a package needs one
fn plain(form: &str) -> Result<(), String> {
    let mut open = Vec::new();
    let mut chars = form.chars();
    while let Some(c) = chars.next() {
        match c {
            // a place, read already
            '{' => while chars.next().is_some_and(|c| c != '}') {},
            '<' | '(' | '[' => open.push(c),
            '>' | ')' | ']' => {
                let opened = match c {
                    '>' => '<',
                    ')' => '(',
                    _ => '[',
                };
                if open.pop() != Some(opened) {
                    return Err(format!("a `{c}` of the form closes nothing"));
                }
            }
            '=' if chars.next() == Some('>') => {}
            ',' if !open.is_empty() => {}
            c if c.is_ascii_alphanumeric() || " _$.|&?:".contains(c) => {}
            other => return Err(format!("`{other}` is no part of the form of a type")),
        }
    }
    match open.pop() {
        Some(c) => Err(format!("a `{c}` of the form is not closed")),
        None => Ok(()),
    }
}

/// What an interface keeps of the form `marks` give a type of `arity`
/// parameters; why they give none that can be written.
pub fn parts(marks: Vec<Mark>, arity: usize) -> Result<Vec<String>, String> {
    let mut out = vec![arity.to_string()];
    let mut forms = 0;
    let mut none = false;
    let mut imports = Vec::new();
    for mark in marks {
        match mark {
            Mark::None => none = true,
            Mark::Form { effect, rt } => {
                forms += 1;
                out.extend([effect, rt]);
            }
            Mark::Import(import) => {
                let from = match import.from {
                    Origin::Runtime => RUNTIME.to_owned(),
                    Origin::Ui => UI.to_owned(),
                    Origin::Module(spec) => spec,
                };
                imports.extend([from, import.effect, import.rt]);
            }
        }
    }
    if forms != usize::from(!none) {
        return Err("a type has one form, or `@ts none`".into());
    }
    out.extend(imports);
    form(&out)?;
    Ok(out)
}

/// The runtime and its UI among the [`parts`]: no specifier has a `<`.
const RUNTIME: &str = "<runtime>";
const UI: &str = "<ui>";

/// The form an interface keeps as `parts`, read as what a stranger wrote.
pub fn form(parts: &[String]) -> Result<Form, String> {
    let unread = || "the form does not read".to_owned();
    let (arity, rest) = parts.split_first().ok_or_else(unread)?;
    let arity = arity.parse::<usize>().map_err(|_| unread())?;
    if rest.is_empty() {
        return Ok(Form {
            arity,
            form: None,
            imports: Vec::new(),
        });
    }
    let [effect, rt, imported @ ..] = rest else {
        return Err(unread());
    };
    let (effect, rt) = (pieces(effect)?, pieces(rt)?);
    let mut imports = Vec::new();
    for parts in imported.chunks(3) {
        let [from, effect, rt] = parts else {
            return Err(unread());
        };
        let from = match from.as_str() {
            RUNTIME => Origin::Runtime,
            UI => Origin::Ui,
            spec => Origin::Module(spec.to_owned()),
        };
        imports.push(import(from, effect.clone(), rt.clone())?);
    }
    for piece in effect.iter().chain(&rt) {
        match piece {
            Piece::Place { index, .. } if *index >= arity => {
                return Err(format!("`{{{index}}}` is past its {arity} parameters"));
            }
            // each `$Name` of a form is an export the type imports
            Piece::Name(name) if !imports.iter().any(|i| i.effect == *name) => {
                return Err(format!(
                    "`${name}` is no export it imports: add `@ts import`"
                ));
            }
            _ => {}
        }
    }
    Ok(Form {
        arity,
        form: Some((effect, rt)),
        imports,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_is_its_text_its_exports_and_the_places_of_its_arguments() {
        assert_eq!(
            pieces("$Effect.Effect<{2}, {1:row}>"),
            Ok(vec![
                Piece::Name("Effect".into()),
                Piece::Text(".Effect<".into()),
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
        assert!(pieces("(arg: {0}) => Promise<readonly [{1}, $Other]>").is_ok());
        assert!(pieces("Array<{a}>").is_err());
        assert!(pieces("Array<{0").is_err());
        assert!(pieces("Array<0}>").is_err());
        assert!(pieces("{0:set}").is_err());
        assert!(pieces("$ <{0}>").is_err());
    }

    /// A form is written into code as it stands: what could end the type
    /// and begin something else is no part of one.
    #[test]
    fn a_form_is_the_text_of_a_type_and_nothing_else() {
        for form in [
            "number = evil()",
            "number; evil()",
            "number, other",
            "Array<number>>",
            "Array<(number>)",
            "Array<number",
            "number /* */",
            "number\tstring",
            "`x`",
            "'x'",
            "-1",
        ] {
            assert!(pieces(form).is_err(), "{form}");
        }
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
        let ui = Import {
            from: Origin::Ui,
            effect: "Ui".into(),
            rt: "Ui".into(),
        };
        assert_eq!(
            mark(&[word("import"), word("ui"), string("\"Ui\"")]),
            Ok(Mark::Import(ui))
        );
        let day = Import {
            from: Origin::Module("./day.d.ts".into()),
            effect: "Day".into(),
            rt: "RtDay".into(),
        };
        let words = [
            word("import"),
            string("\"./day.d.ts\""),
            string("\"Day\""),
            word("rt"),
            string("\"RtDay\""),
        ];
        assert_eq!(mark(&words), Ok(Mark::Import(day)));
        assert!(mark(&[word("import"), word("elsewhere"), string("\"Ui\"")]).is_err());
        assert!(mark(&[word("import"), word("ui"), string("\"a b\"")]).is_err());
        assert!(mark(&[word("import"), string("\"a b\""), string("\"Ui\"")]).is_err());
        assert!(mark(&[word("import"), string("\"\""), string("\"Ui\"")]).is_err());
        assert!(mark(&[]).is_err());
        assert!(mark(&[string("\"A<{x}>\"")]).is_err());
    }

    fn marks(words: &[&[(K, &'static str)]]) -> Vec<Mark> {
        (words.iter())
            .map(|w| mark(w).expect("a mark that reads"))
            .collect()
    }

    #[test]
    fn an_interface_keeps_a_form_and_reads_it_as_a_strangers() {
        let string = |s: &'static str| (K::String, s);
        let word = |s: &'static str| (K::Ident, s);
        let boxed = [string("\"$Box<{1}, {0:row}>\"")];
        let from = [word("import"), string("\"boxes\""), string("\"Box\"")];
        let kept = parts(marks(&[&boxed, &from]), 2).expect("a form");
        assert_eq!(
            kept,
            [
                "2",
                "$Box<{1}, {0:row}>",
                "$Box<{1}, {0:row}>",
                "boxes",
                "Box",
                "Box"
            ]
        );
        let read = form(&kept).expect("the form");
        assert_eq!((read.arity, read.imports.len()), (2, 1));
        assert_eq!(read.imports[0].from, Origin::Module("boxes".into()));
        assert_eq!(
            parts(marks(&[&[word("none")]]), 0),
            Ok(vec!["0".to_owned()])
        );
        assert_eq!(form(&["0".to_owned()]).map(|f| f.form), Ok(None));

        let why = |marks, arity| parts(marks, arity).expect_err("no form");
        assert_eq!(
            why(marks(&[&boxed, &from]), 1),
            "`{1}` is past its 1 parameters"
        );
        assert_eq!(
            why(marks(&[&boxed]), 2),
            "`$Box` is no export it imports: add `@ts import`"
        );
        let both = marks(&[&[string("\"number\"")], &[word("none")]]);
        assert_eq!(why(both, 0), "a type has one form, or `@ts none`");
        assert_eq!(
            why(marks(&[&from]), 0),
            "a type has one form, or `@ts none`"
        );
        let twice = marks(&[&[string("\"number\"")], &[string("\"string\"")]]);
        assert_eq!(why(twice, 0), "a type has one form, or `@ts none`");

        // an interface from elsewhere may say anything
        let s = |parts: &[&str]| parts.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert!(form(&s(&[])).is_err());
        assert!(form(&s(&["one"])).is_err());
        assert!(form(&s(&["0", "number"])).is_err());
        assert!(form(&s(&["0", "number; evil()", "number"])).is_err());
        assert!(form(&s(&["0", "$X", "$X", "a\"; evil(); \"", "X", "X"])).is_err());
        assert!(form(&s(&["0", "$X", "$X", "mod", "X; evil()", "X"])).is_err());
        assert!(form(&s(&["0", "$X", "$X", "mod", "X"])).is_err());
    }
}
