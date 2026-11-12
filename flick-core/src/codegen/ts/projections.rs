//! The TypeScript forms the built-in modules give their types with `@ts`,
//! read once: the backend names no type of a library.

use std::mem;
use std::sync::OnceLock;

use crate::source::SourceMap;
use crate::syntax;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::ts_mark::{self, Mark, Piece, Slot};
use crate::types::builtin_sources;

/// An export of the runtime a form names: `$` and its name with the adapter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Import {
    pub slot: Slot,
    pub effect: String,
    pub rt: String,
}

/// A type with a TypeScript form of its own, or with none.
#[derive(Debug)]
pub struct Projected {
    pub module: String,
    pub name: String,
    pub arity: usize,
    /// The form with the Effect adapter and on Flick's own runtime; `None`
    /// for a type that has no form.
    pub form: Option<(Vec<Piece>, Vec<Piece>)>,
    pub imports: Vec<Import>,
}

/// The types `sources`, modules as `(name, text)`, give a form.
pub fn read(sources: &[(&str, &str)]) -> Result<Vec<Projected>, String> {
    let mut out = Vec::new();
    for &(module, text) in sources {
        // most modules mark none: only one that may is parsed
        if !text.contains("@ts") {
            continue;
        }
        let mut map = SourceMap::default();
        let file = map.add(format!("{module}.fl"), text);
        let tree = syntax::parse(text, file).tree;
        let mut pending: Vec<Mark> = Vec::new();
        for child in tree.nodes() {
            if child.kind == K::Attribute {
                if let Some(mark) = ts_mark::read(child, text) {
                    pending.push(mark.map_err(|why| format!("{module}: {why}"))?);
                }
                continue;
            }
            let marks = mem::take(&mut pending);
            if marks.is_empty() {
                continue;
            }
            let name = (child.child_tokens().find(|t| t.kind == K::Con))
                .filter(|_| matches!(child.kind, K::TypeDecl | K::ServiceDecl))
                .ok_or_else(|| format!("{module}: `@ts` stands before no type or service"))?
                .text(text);
            let arity = child.nodes().filter(|n| n.kind == K::TypeParam).count();
            out.push(projected(module, name, arity, marks)?);
        }
    }
    Ok(out)
}

fn projected(
    module: &str,
    name: &str,
    arity: usize,
    marks: Vec<Mark>,
) -> Result<Projected, String> {
    let at = |why: String| format!("{module}.{name}: {why}");
    let mut form = None;
    let mut none = false;
    let mut imports = Vec::new();
    for mark in marks {
        match mark {
            Mark::None => none = true,
            Mark::Form { effect, rt } => {
                let pieces = (
                    ts_mark::pieces(&effect).map_err(&at)?,
                    ts_mark::pieces(&rt).map_err(&at)?,
                );
                form = Some(pieces);
            }
            Mark::Import { slot, effect, rt } => imports.push(Import { slot, effect, rt }),
        }
    }
    if none == form.is_some() {
        return Err(at("a type has one form, or `@ts none`".into()));
    }
    if let Some((effect, rt)) = &form {
        for piece in effect.iter().chain(rt) {
            if let Piece::Place { index, .. } = piece
                && *index >= arity
            {
                return Err(at(format!("`{{{index}}}` is past its {arity} parameters")));
            }
        }
        // each `$Name` of a form is an export the type imports
        for name in ts_mark::names(effect).into_iter().chain(ts_mark::names(rt)) {
            if !imports.iter().any(|i| i.effect == name) {
                return Err(at(format!(
                    "`${name}` is no export it imports: add `@ts import`"
                )));
            }
        }
    }
    Ok(Projected {
        module: module.to_owned(),
        name: name.to_owned(),
        arity,
        form,
        imports,
    })
}

/// The forms of the built-in modules.
fn table() -> &'static [Projected] {
    static TABLE: OnceLock<Vec<Projected>> = OnceLock::new();
    TABLE.get_or_init(|| read(builtin_sources()).unwrap_or_else(|why| panic!("{why}")))
}

/// The form the built-in modules give `module.name`.
pub fn find(module: &str, name: &str) -> Option<&'static Projected> {
    (table().iter()).find(|p| p.module == module && p.name == name)
}

#[cfg(test)]
mod tests {
    use super::{Import, find, read};
    use crate::syntax::ts_mark::{Piece, Slot};

    const HEAD: &str = "module Shapes exposing (..)\n\n";

    fn why(body: &str) -> String {
        read(&[("Shapes", &format!("{HEAD}{body}"))]).expect_err("a form that does not read")
    }

    #[test]
    fn a_module_gives_its_types_their_forms() {
        let text = format!(
            "{HEAD}@ts \"$Box.Box<{{1}}, {{0:row}}>\" rt \"$Box<{{1}}>\"\n@ts import runtime \"Box\" rt \"BoxValue\"\ntype Box e a = Box a\n\n@ts none\nservice Hidden =\n  peek : Unit ->{{}} Float\n\ntype Plain = Plain\n"
        );
        let found = read(&[("Shapes", &text), ("Other", "module Other exposing (..)\n")])
            .expect("forms that read");
        let [boxed, hidden] = &found[..] else {
            panic!("two types have a form: {found:?}")
        };
        assert_eq!((boxed.name.as_str(), boxed.arity), ("Box", 2));
        let (effect, rt) = boxed.form.as_ref().expect("a form");
        assert_eq!(effect.len(), 5);
        assert_eq!(rt[0], Piece::Text("$Box<".into()));
        assert_eq!(
            boxed.imports,
            [Import {
                slot: Slot::Runtime,
                effect: "Box".into(),
                rt: "BoxValue".into()
            }]
        );
        assert_eq!(
            (hidden.name.as_str(), hidden.form.is_none()),
            ("Hidden", true)
        );
    }

    #[test]
    fn a_form_that_cannot_be_written_is_refused_where_it_is_read() {
        assert_eq!(
            why("@ts \"Array<{1}>\"\ntype Rows a = Rows a\n"),
            "Shapes.Rows: `{1}` is past its 1 parameters"
        );
        assert_eq!(
            why("@ts \"$Box<{0}>\"\ntype Box a = Box a\n"),
            "Shapes.Box: `$Box` is no export it imports: add `@ts import`"
        );
        assert_eq!(
            why("@ts \"number\"\n@ts none\ntype Both = Both\n"),
            "Shapes.Both: a type has one form, or `@ts none`"
        );
        assert_eq!(
            why("@ts import ui \"Ui\"\ntype Lone = Lone\n"),
            "Shapes.Lone: a type has one form, or `@ts none`"
        );
        assert_eq!(
            why("@ts \"number\"\nlet one = 1\n"),
            "Shapes: `@ts` stands before no type or service"
        );
    }

    /// The built-in modules say every form the backend writes for them.
    #[test]
    fn the_built_in_modules_give_their_types_their_forms() {
        let list = find("Prelude", "List").expect("the form of `List`");
        assert_eq!(list.arity, 1);
        assert!(find("Prelude", "ServiceTag").is_some_and(|p| p.form.is_none()));
        assert!(find("Prelude", "Ordering").is_none());
    }
}
