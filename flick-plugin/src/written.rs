//! The types a declaration or a signature writes, as the compiler parsed
//! them, so a derive reads what a field holds without parsing tokens again.

use serde::{Deserialize, Serialize};

use crate::Span;

/// A type as written, at its span of the text of its declaration or
/// signature. Parentheses are gone; the span of a parenthesized type keeps
/// them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Written {
    pub kind: WrittenKind,
    pub span: Span,
}

/// One field of a written record type.
///
/// Fields in [`WrittenKind::Record`] are stored in canonical UTF-16 label order
/// for deterministic equality and caching; `source_index` records the 0-based
/// declaration order in source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordField {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub ty: Written,
    pub docs: String,
    pub source_index: u32,
}

/// A class constraint `C a b` in a `forall`, `=>` context, or GADT constructor.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConstraintMeta {
    pub class: String,
    pub args: Vec<Written>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WrittenKind {
    /// A named type, maybe qualified (`Shapes.Shape`), and its arguments.
    Con {
        name: String,
        args: Vec<Written>,
    },
    /// A type variable and its arguments: `a`, `f String`.
    Var {
        name: String,
        args: Vec<Written>,
    },
    /// A record, its fields in canonical UTF-16 label order; `open` with a row `| r`.
    Record {
        fields: Vec<RecordField>,
        open: bool,
    },
    Tuple(Vec<Written>),
    Unit,
    /// `from -> to`, or `from ->{row} to`.
    Arrow {
        from: Box<Written>,
        to: Box<Written>,
        row: Option<EffectRow>,
    },
    /// A type with a capture qualifier, `t^c`: what it holds is a scope's.
    Captured(Box<Written>),
    /// `forall a. C a => T` or `C a => T`.
    Forall {
        vars: Vec<String>,
        constraints: Vec<ConstraintMeta>,
        body: Box<Written>,
    },
    /// A type-level string literal, as in `Record.Label "name"`.
    StringLit(String),
    /// The empty row/set keyword `none`.
    None,
    /// A type no derive reads: a row, a union, a hole.
    Other,
}

/// The row of an arrow: its services, then after `;` its errors, each part
/// `open` with a tail `| r`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EffectRow {
    pub services: Vec<Written>,
    pub services_open: bool,
    pub errors: Vec<Written>,
    pub errors_open: bool,
}

/// A node of a written type without its children, which are indices of
/// earlier nodes: how a type crosses an interface without recursion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatWritten {
    pub kind: FlatKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatRecordField {
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub ty: u32,
    pub docs: String,
    pub source_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatConstraint {
    pub class: String,
    pub args: Vec<u32>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlatKind {
    Con {
        name: String,
        args: Vec<u32>,
    },
    Var {
        name: String,
        args: Vec<u32>,
    },
    Record {
        fields: Vec<FlatRecordField>,
        open: bool,
    },
    Tuple(Vec<u32>),
    Unit,
    Arrow {
        from: u32,
        to: u32,
        row: Option<FlatRow>,
    },
    Captured(u32),
    Forall {
        vars: Vec<String>,
        constraints: Vec<FlatConstraint>,
        body: u32,
    },
    StringLit(String),
    None,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatRow {
    pub services: Vec<u32>,
    pub services_open: bool,
    pub errors: Vec<u32>,
    pub errors_open: bool,
}

impl Written {
    /// The nodes of the type, each after the nodes it refers to; the last
    /// is the type itself.
    pub fn flatten(&self) -> Vec<FlatWritten> {
        let mut out = Vec::new();
        self.push(&mut out);
        out
    }

    fn push(&self, out: &mut Vec<FlatWritten>) -> u32 {
        let all = |ts: &[Written], out: &mut Vec<FlatWritten>| -> Vec<u32> {
            ts.iter().map(|t| t.push(out)).collect()
        };
        let kind = match &self.kind {
            WrittenKind::Con { name, args } => FlatKind::Con {
                name: name.clone(),
                args: all(args, out),
            },
            WrittenKind::Var { name, args } => FlatKind::Var {
                name: name.clone(),
                args: all(args, out),
            },
            WrittenKind::Record { fields, open } => FlatKind::Record {
                fields: (fields.iter())
                    .map(|f| FlatRecordField {
                        name: f.name.clone(),
                        name_span: f.name_span,
                        span: f.span,
                        ty: f.ty.push(out),
                        docs: f.docs.clone(),
                        source_index: f.source_index,
                    })
                    .collect(),
                open: *open,
            },
            WrittenKind::Tuple(items) => FlatKind::Tuple(all(items, out)),
            WrittenKind::Unit => FlatKind::Unit,
            WrittenKind::Arrow { from, to, row } => {
                let from = from.push(out);
                let to = to.push(out);
                let row = row.as_ref().map(|r| FlatRow {
                    services: all(&r.services, out),
                    services_open: r.services_open,
                    errors: all(&r.errors, out),
                    errors_open: r.errors_open,
                });
                FlatKind::Arrow { from, to, row }
            }
            WrittenKind::Captured(of) => FlatKind::Captured(of.push(out)),
            WrittenKind::Forall {
                vars,
                constraints,
                body,
            } => {
                let constraints = (constraints.iter())
                    .map(|c| FlatConstraint {
                        class: c.class.clone(),
                        args: all(&c.args, out),
                        span: c.span,
                    })
                    .collect();
                let body = body.push(out);
                FlatKind::Forall {
                    vars: vars.clone(),
                    constraints,
                    body,
                }
            }
            WrittenKind::StringLit(s) => FlatKind::StringLit(s.clone()),
            WrittenKind::None => FlatKind::None,
            WrittenKind::Other => FlatKind::Other,
        };
        out.push(FlatWritten {
            kind,
            span: self.span,
        });
        u32::try_from(out.len() - 1).expect("a type of fewer than 2^32 nodes")
    }

    /// The type whose nodes `flatten` gave, or `None` when an index does
    /// not name an earlier node.
    pub fn unflatten(nodes: &[FlatWritten]) -> Option<Written> {
        let last = u32::try_from(nodes.len().checked_sub(1)?).ok()?;
        Self::at(nodes, last, nodes.len())
    }

    fn at(nodes: &[FlatWritten], id: u32, before: usize) -> Option<Written> {
        let index = usize::try_from(id).ok().filter(|&i| i < before)?;
        let node = &nodes[index];
        let one = |id: u32| Self::at(nodes, id, index);
        let all = |ids: &[u32]| ids.iter().map(|&id| one(id)).collect::<Option<Vec<_>>>();
        let kind = match &node.kind {
            FlatKind::Con { name, args } => WrittenKind::Con {
                name: name.clone(),
                args: all(args)?,
            },
            FlatKind::Var { name, args } => WrittenKind::Var {
                name: name.clone(),
                args: all(args)?,
            },
            FlatKind::Record { fields, open } => WrittenKind::Record {
                fields: (fields.iter())
                    .map(|f| {
                        Some(RecordField {
                            name: f.name.clone(),
                            name_span: f.name_span,
                            span: f.span,
                            ty: one(f.ty)?,
                            docs: f.docs.clone(),
                            source_index: f.source_index,
                        })
                    })
                    .collect::<Option<_>>()?,
                open: *open,
            },
            FlatKind::Tuple(items) => WrittenKind::Tuple(all(items)?),
            FlatKind::Unit => WrittenKind::Unit,
            FlatKind::Arrow { from, to, row } => WrittenKind::Arrow {
                from: Box::new(one(*from)?),
                to: Box::new(one(*to)?),
                row: match row {
                    Some(r) => Some(EffectRow {
                        services: all(&r.services)?,
                        services_open: r.services_open,
                        errors: all(&r.errors)?,
                        errors_open: r.errors_open,
                    }),
                    None => None,
                },
            },
            FlatKind::Captured(of) => WrittenKind::Captured(Box::new(one(*of)?)),
            FlatKind::Forall {
                vars,
                constraints,
                body,
            } => WrittenKind::Forall {
                vars: vars.clone(),
                constraints: (constraints.iter())
                    .map(|c| {
                        Some(ConstraintMeta {
                            class: c.class.clone(),
                            args: all(&c.args)?,
                            span: c.span,
                        })
                    })
                    .collect::<Option<_>>()?,
                body: Box::new(one(*body)?),
            },
            FlatKind::StringLit(s) => WrittenKind::StringLit(s.clone()),
            FlatKind::None => WrittenKind::None,
            FlatKind::Other => WrittenKind::Other,
        };
        Some(Written {
            kind,
            span: node.span,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: u32, end: u32, kind: WrittenKind) -> Written {
        Written {
            kind,
            span: Span { start, end },
        }
    }

    #[test]
    fn a_type_crosses_as_nodes_and_comes_back_whole() {
        let int = at(
            0,
            3,
            WrittenKind::Con {
                name: "Int".into(),
                args: vec![],
            },
        );
        let list = at(
            0,
            8,
            WrittenKind::Con {
                name: "List".into(),
                args: vec![int.clone()],
            },
        );
        let row = EffectRow {
            services: vec![at(
                12,
                13,
                WrittenKind::Var {
                    name: "r".into(),
                    args: vec![],
                },
            )],
            services_open: false,
            errors: vec![
                at(
                    15,
                    23,
                    WrittenKind::Con {
                        name: "NotFound".into(),
                        args: vec![],
                    },
                ),
                at(24, 28, WrittenKind::None),
            ],
            errors_open: true,
        };
        let label = at(25, 31, WrittenKind::StringLit("name".into()));
        let arrow = at(
            0,
            40,
            WrittenKind::Forall {
                vars: vec!["a".into()],
                constraints: vec![ConstraintMeta {
                    class: "Show".into(),
                    args: vec![at(
                        10,
                        11,
                        WrittenKind::Var {
                            name: "a".into(),
                            args: vec![],
                        },
                    )],
                    span: Span { start: 5, end: 11 },
                }],
                body: Box::new(at(
                    15,
                    40,
                    WrittenKind::Arrow {
                        from: Box::new(list),
                        to: Box::new(at(
                            25,
                            40,
                            WrittenKind::Record {
                                fields: vec![
                                    RecordField {
                                        name: "a".into(),
                                        name_span: Span { start: 27, end: 28 },
                                        span: Span { start: 27, end: 34 },
                                        ty: int,
                                        docs: "field doc".into(),
                                        source_index: 1,
                                    },
                                    RecordField {
                                        name: "b".into(),
                                        name_span: Span { start: 35, end: 36 },
                                        span: Span { start: 35, end: 39 },
                                        ty: label,
                                        docs: String::new(),
                                        source_index: 0,
                                    },
                                ],
                                open: false,
                            },
                        )),
                        row: Some(row),
                    },
                )),
            },
        );
        let nodes = arrow.flatten();
        assert_eq!(Written::unflatten(&nodes), Some(arrow));
        // an index may only name an earlier node
        let mut broken = nodes;
        broken[0].kind = FlatKind::Tuple(vec![3]);
        assert_eq!(Written::unflatten(&broken), None);
    }
}
