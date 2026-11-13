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
    /// A record, its fields by label; `open` with a row `| r`.
    Record {
        fields: Vec<(String, Written)>,
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
    /// `forall a. …` or `C a => …`.
    Generic,
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
        fields: Vec<(String, u32)>,
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
    Generic,
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
                    .map(|(label, t)| (label.clone(), t.push(out)))
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
            WrittenKind::Generic => FlatKind::Generic,
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
                    .map(|(label, id)| Some((label.clone(), one(*id)?)))
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
            FlatKind::Generic => WrittenKind::Generic,
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
            errors: vec![at(
                15,
                23,
                WrittenKind::Con {
                    name: "NotFound".into(),
                    args: vec![],
                },
            )],
            errors_open: true,
        };
        let arrow = at(
            0,
            30,
            WrittenKind::Arrow {
                from: Box::new(list),
                to: Box::new(at(
                    25,
                    30,
                    WrittenKind::Record {
                        fields: vec![("a".into(), int)],
                        open: false,
                    },
                )),
                row: Some(row),
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
