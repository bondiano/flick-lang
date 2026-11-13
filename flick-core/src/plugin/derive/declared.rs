//! The types the code of a block declares: what the derive of a class is
//! told of one, its own in `deriving` or a neighbour's, as of a declaration
//! of the module. The block is the declaration, and the fields of the type are the
//! types the user wrote in it.

use flick_plugin::{Arena, Block, CaseTarget, Decl, DeriveTarget, Kind, Span, Ty, Written};

/// A type of a block's code and the classes it names.
pub struct Declared {
    /// What a derive is told of the type, or why it can be told nothing.
    pub target: Result<DeriveTarget, String>,
    pub classes: Vec<String>,
}

/// Each type `arena`, the code of `block`, declares; `read` is the type
/// the user wrote at a span of the block.
pub fn declared(arena: &Arena, block: &Block, read: &dyn Fn(Span) -> Written) -> Vec<Declared> {
    let field = |id: u32| match arena.nodes.get(id as usize).map(|n| &n.kind) {
        Some(Kind::Ty(Ty::Hole(span))) => Some(read(*span)),
        _ => None,
    };
    (arena.nodes.iter())
        .filter_map(|node| match &node.kind {
            Kind::Decl(Decl::Adt {
                name,
                params,
                cases,
                deriving,
                unboxed,
            }) => {
                let fields: Option<Vec<Vec<Written>>> = (cases.iter())
                    .map(|c| c.fields.iter().map(|&f| field(f)).collect())
                    .collect();
                let target = fields.map(|fields| {
                    // a type over another, which only its block's code makes
                    let wraps = match (&cases[..], &fields[..]) {
                        ([case], [of]) if *unboxed && case.name.contains('$') => {
                            of.first().cloned()
                        }
                        _ => None,
                    };
                    let cases = (cases.iter().zip(fields))
                        .map(|(c, fields)| CaseTarget {
                            name: c.name.clone(),
                            fields,
                            literal: None,
                            refined: false,
                            constrained: false,
                        })
                        .collect();
                    DeriveTarget {
                        decl: block.clone(),
                        name: name.clone(),
                        params: params.clone(),
                        kinds: vec![String::new(); params.len()],
                        cases,
                        alias: None,
                        wraps,
                        modifiers: if *unboxed {
                            vec!["unboxed".into(), "type".into()]
                        } else {
                            vec!["type".into()]
                        },
                        deriving: deriving.clone(),
                        local: true,
                    }
                });
                let why = || {
                    format!(
                        "cannot derive for `{name}`: a field of it is no type written in the block"
                    )
                };
                Some(Declared {
                    target: target.ok_or_else(why),
                    classes: deriving.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use flick_plugin::{Builder, Case, Place, WrittenKind, flick};

    use super::*;

    fn wrapped(field: impl Fn(&mut Builder) -> u32) -> Vec<Declared> {
        let whole = Span { start: 0, end: 5 };
        let mut b = Builder::new(whole);
        let base = field(&mut b);
        let root = b.decl(Decl::Adt {
            name: "Meters".into(),
            params: Vec::new(),
            cases: vec![Case {
                name: "Meters$Make".into(),
                fields: vec![base],
            }],
            deriving: vec!["Pairs.Arbitrary".into()],
            unboxed: true,
        });
        let block = Block {
            head: "wrapped".into(),
            text: "Float".into(),
            body: 5,
            place: Place::Declaration,
            tokens: Vec::new(),
            docs: String::new(),
            signature: None,
            module: "App.Main".into(),
        };
        // the user's `Float`, as the host reads a hole of a block
        let read = |span: Span| Written {
            kind: WrittenKind::Con {
                name: "Float".into(),
                args: Vec::new(),
            },
            span,
        };
        declared(&b.finish(root), &block, &read)
    }

    #[test]
    fn a_type_over_the_users_type_tells_a_derive_what_it_wraps() {
        let span = Span { start: 0, end: 5 };
        let found = wrapped(|b| b.ty(Ty::Hole(span)));
        let target = found[0].target.as_ref().expect("a target");
        assert_eq!(target.wraps.as_ref().map(|w| w.span), Some(span));
        assert_eq!(target.cases[0].fields[0].span, span);
        assert!(
            matches!(&target.cases[0].fields[0].kind, WrittenKind::Con { name, .. } if name == "Float")
        );
        assert_eq!(target.modifiers, ["unboxed", "type"]);
        assert_eq!(found[0].classes, ["Pairs.Arbitrary"]);
    }

    #[test]
    fn a_field_the_plugin_made_is_nothing_a_derive_can_read() {
        let found = wrapped(|b| flick!(b, type Float));
        assert_eq!(
            found[0].target.as_ref().err().map(String::as_str),
            Some("cannot derive for `Meters`: a field of it is no type written in the block")
        );
    }
}
