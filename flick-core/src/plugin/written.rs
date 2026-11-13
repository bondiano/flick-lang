//! A type of the syntax tree as a plugin reads it: the parse the compiler
//! already made, so a derive does not read the type's tokens again.

use flick_plugin::{EffectRow, Hole, HoleKind, Span, Written, WrittenKind};

use super::source::BlockSource;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The type `node` writes, its spans counting from `origin` of `text`.
pub fn written(node: &Node, text: &str, origin: u32) -> Written {
    let (start, end) = node.range();
    let span = Span {
        start: start - origin,
        end: end - origin,
    };
    let children: Vec<&Node> = node.nodes().collect();
    let each = |nodes: &[&Node]| -> Vec<Written> {
        nodes.iter().map(|n| written(n, text, origin)).collect()
    };
    let kind = match node.kind {
        K::ParenType => match children[..] {
            [inner] => written(inner, text, origin).kind,
            _ => WrittenKind::Other,
        },
        K::NameType => named(node, text, Vec::new()),
        K::AppType => match children.split_first() {
            Some((head, args)) => match written(head, text, origin).kind {
                WrittenKind::Con {
                    name,
                    args: mut before,
                } => {
                    before.extend(each(args));
                    WrittenKind::Con { name, args: before }
                }
                WrittenKind::Var {
                    name,
                    args: mut before,
                } => {
                    before.extend(each(args));
                    WrittenKind::Var { name, args: before }
                }
                _ => WrittenKind::Other,
            },
            _ => WrittenKind::Other,
        },
        K::TupleType => WrittenKind::Tuple(each(&children)),
        K::UnitType => WrittenKind::Unit,
        K::RecordType => {
            let mut fields: Vec<(String, Written)> = (children.iter())
                .filter(|n| n.kind == K::RecordTypeField)
                .filter_map(|f| {
                    let label = f.child_tokens().find(|t| t.kind == K::Ident)?.text(text);
                    let ty = f.nodes().next()?;
                    Some((label.to_owned(), written(ty, text, origin)))
                })
                .collect();
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            WrittenKind::Record {
                fields,
                open: children.iter().any(|n| n.kind == K::RowTail),
            }
        }
        K::ArrowType => {
            let row = children.iter().find(|n| n.kind == K::EffectAnnot);
            let sides: Vec<&Node> = (children.iter().copied())
                .filter(|n| n.kind != K::EffectAnnot)
                .collect();
            match sides[..] {
                [from, to] => WrittenKind::Arrow {
                    from: Box::new(written(from, text, origin)),
                    to: Box::new(written(to, text, origin)),
                    row: row.map(|r| effect_row(r, text, origin)),
                },
                _ => WrittenKind::Other,
            }
        }
        K::CaptureType => match children.first() {
            Some(of) => WrittenKind::Captured(Box::new(written(of, text, origin))),
            None => WrittenKind::Other,
        },
        K::ForallType | K::ConstrainedType => WrittenKind::Generic,
        _ => WrittenKind::Other,
    };
    Written { kind, span }
}

/// The type the user wrote at `span` of the block of `source`, as a derive
/// reads it; what does not parse as exactly that type is no type it reads.
pub fn user_type(source: &BlockSource, span: Span) -> Written {
    let hole = Hole {
        kind: HoleKind::Ty,
        span,
    };
    source.hole_tree(hole).map_or(
        Written {
            kind: WrittenKind::Other,
            span,
        },
        |tree| written(&tree, source.text(), source.start()),
    )
}

/// A `NameType`: a constructor, maybe qualified, or a variable.
fn named(node: &Node, text: &str, args: Vec<Written>) -> WrittenKind {
    let tokens: Vec<_> = node.child_tokens().collect();
    match &tokens[..] {
        [one] if one.kind == K::Ident => WrittenKind::Var {
            name: one.text(text).to_owned(),
            args,
        },
        _ if !tokens.is_empty() && tokens.iter().all(|t| matches!(t.kind, K::Con | K::Dot)) => {
            WrittenKind::Con {
                name: tokens.iter().map(|t| t.text(text)).collect(),
                args,
            }
        }
        _ => WrittenKind::Other,
    }
}

/// `{services; errors}`: the rows on either side of the `;`, each open with
/// a tail.
fn effect_row(annot: &Node, text: &str, origin: u32) -> EffectRow {
    let semicolon = annot
        .child_tokens()
        .find(|t| t.kind == K::Semi)
        .map(|t| t.start);
    let mut row = EffectRow::default();
    for part in annot.nodes().filter(|n| n.kind == K::Row) {
        let items: Vec<Written> = (part.nodes())
            .filter(|n| n.kind != K::RowTail)
            .map(|n| written(n, text, origin))
            .collect();
        let open = part.nodes().any(|n| n.kind == K::RowTail);
        if semicolon.is_some_and(|at| part.range().0 > at) {
            row.errors = items;
            row.errors_open = open;
        } else {
            row.services = items;
            row.services_open = open;
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;
    use crate::syntax::parse;

    /// The type of the signature `f : <ty>`.
    fn of(ty: &str) -> (Written, String) {
        let text = format!("module A exposing (..)\nf : {ty}\nlet f = f\n");
        let tree = parse(&text, FileId::default()).tree;
        let sig = tree
            .nodes()
            .find(|n| n.kind == K::Signature)
            .expect("a signature");
        let node = sig.nodes().next().expect("its type");
        (written(node, &text, node.range().0), text)
    }

    #[test]
    fn a_signature_reads_as_the_types_it_writes() {
        let (ty, _) = of(
            "Shapes.Shape (List a) -> { b : Int; a : f String | r } ->{Server | r; NotFound, E.Bad} (Int, ())^c",
        );
        let WrittenKind::Arrow {
            from,
            to,
            row: None,
        } = ty.kind
        else {
            panic!("an arrow without a row: {ty:?}")
        };
        let WrittenKind::Con { name, args } = from.kind else {
            panic!("a named type: {from:?}")
        };
        assert_eq!(name, "Shapes.Shape");
        assert!(
            matches!(&args[..], [Written { kind: WrittenKind::Con { name, .. }, span }] if name == "List" && span.start == 13)
        );
        let WrittenKind::Arrow {
            from: record,
            to: result,
            row: Some(row),
        } = to.kind
        else {
            panic!("an arrow with a row: {to:?}")
        };
        let WrittenKind::Record { fields, open: true } = record.kind else {
            panic!("an open record: {record:?}")
        };
        let labels: Vec<&str> = fields.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["a", "b"]);
        assert!(row.services_open && !row.errors_open);
        assert_eq!(row.errors.len(), 2);
        assert!(matches!(&row.errors[1].kind, WrittenKind::Con { name, .. } if name == "E.Bad"));
        assert!(matches!(result.kind, WrittenKind::Captured(_)));
    }

    #[test]
    fn a_generic_signature_is_no_type_a_derive_reads() {
        assert_eq!(
            of("forall a. Show a => a -> String").0.kind,
            WrittenKind::Generic
        );
        assert_eq!(of("Show a => a -> String").0.kind, WrittenKind::Generic);
    }
    #[test]
    fn grouped_heads_keep_all_type_arguments() {
        for (source, expected, names) in [
            ("(List) Int", "List", vec!["Int"]),
            ("(f) Int", "f", vec!["Int"]),
            ("(Result String) Int", "Result", vec!["String", "Int"]),
        ] {
            let ty = of(source).0;
            let (name, args) = match ty.kind {
                WrittenKind::Con { name, args } | WrittenKind::Var { name, args } => (name, args),
                other => panic!("{source}: {other:?}"),
            };
            assert_eq!(name, expected);
            let actual: Vec<&str> = args
                .iter()
                .map(|a| match &a.kind {
                    WrittenKind::Con { name, args } if args.is_empty() => name.as_str(),
                    other => panic!("a named argument: {other:?}"),
                })
                .collect();
            assert_eq!(actual, names);
        }
    }

    #[test]
    fn derive_parameter_kinds_are_independent_of_whitespace() {
        use crate::plugin::derive::target;
        use std::sync::Arc;
        for kind in ["Type -> Type", "Type->Type", "Type  ->Type"] {
            let text: Arc<str> = format!("type Box (f : {kind}) a = Box (f a)\n").into();
            let file = FileId::default();
            let tree = parse(&text, file);
            assert!(tree.diagnostics.is_empty(), "{:?}", tree.diagnostics);
            let node = tree
                .tree
                .nodes()
                .find(|n| n.kind == K::TypeDecl)
                .expect("type declaration");
            let block =
                BlockSource::declaration(Arc::clone(&text), file, node, Arc::default(), true)
                    .block();
            let target = target(node, &text, block, None).expect("derive target");
            assert_eq!(target.params, ["f", "a"]);
            assert_eq!(target.kinds, ["Type -> Type", ""]);
        }
    }
}
