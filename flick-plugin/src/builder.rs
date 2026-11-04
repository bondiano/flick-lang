//! Building an [`Arena`]: each node is pushed after the nodes it refers to,
//! with the origin last set by [`Builder::at`].

use crate::{Arena, Decl, Expr, Kind, Node, NodeId, Pat, Span, Ty};

pub struct Builder {
    nodes: Vec<Node>,
    origin: Span,
}

impl Builder {
    /// An empty arena whose nodes stand for `origin` until [`Builder::at`].
    pub fn new(origin: Span) -> Self {
        Builder {
            nodes: Vec::new(),
            origin,
        }
    }

    /// The builder itself: what `flick!` builds in, whether it is given
    /// the builder or a `&mut` of it.
    #[doc(hidden)]
    pub fn as_builder(&mut self) -> &mut Self {
        self
    }

    /// The origin of the nodes pushed from now on.
    pub fn at(&mut self, origin: Span) -> &mut Self {
        self.origin = origin;
        self
    }

    pub fn expr(&mut self, expr: Expr) -> NodeId {
        self.push(Kind::Expr(expr))
    }

    pub fn ty(&mut self, ty: Ty) -> NodeId {
        self.push(Kind::Ty(ty))
    }

    pub fn decl(&mut self, decl: Decl) -> NodeId {
        self.push(Kind::Decl(decl))
    }

    /// The declarations `decls`, in order: the code of a block that gives
    /// several.
    pub fn group(&mut self, decls: Vec<NodeId>) -> NodeId {
        self.decl(Decl::Group(decls))
    }

    /// `decl` with the documentation `text`, which may be several lines;
    /// an empty text is no documentation.
    pub fn doc(&mut self, text: &str, decl: NodeId) -> NodeId {
        if text.is_empty() {
            return decl;
        }
        self.decl(Decl::Doc {
            text: text.to_owned(),
            decl,
        })
    }

    pub fn pat(&mut self, pat: Pat) -> NodeId {
        self.push(Kind::Pat(pat))
    }

    pub fn finish(self, root: NodeId) -> Arena {
        Arena {
            nodes: self.nodes,
            root,
        }
    }

    /// Panics when `kind` refers to a node not pushed yet: ids come from
    /// this builder, so that is a bug of the plugin.
    fn push(&mut self, kind: Kind) -> NodeId {
        let id = NodeId::try_from(self.nodes.len()).expect("fewer than 2^32 nodes");
        assert!(
            kind.children().iter().all(|&child| child < id),
            "node {id} refers to a node pushed after it"
        );
        self.nodes.push(Node {
            kind,
            origin: self.origin,
        });
        id
    }
}

impl Kind {
    /// The nodes this one refers to.
    pub fn children(&self) -> Vec<NodeId> {
        let fields = |fields: &[crate::Field]| fields.iter().map(|f| f.value).collect();
        match self {
            Kind::Expr(expr) => match expr {
                Expr::Name(_)
                | Expr::Number(_)
                | Expr::Text(_)
                | Expr::Boolean(_)
                | Expr::Unit
                | Expr::Hole(_) => Vec::new(),
                Expr::Apply {
                    function,
                    arguments,
                } => [*function]
                    .into_iter()
                    .chain(arguments.iter().copied())
                    .collect(),
                Expr::Lambda { params, body } => params.iter().copied().chain([*body]).collect(),
                Expr::Record(entries) => fields(entries),
                Expr::Get { record, .. } => vec![*record],
                Expr::Items(items) | Expr::Tuple(items) => items.clone(),
                Expr::Annot { expr, ty } => vec![*expr, *ty],
            },
            Kind::Ty(ty) => match ty {
                Ty::Name(_) | Ty::Text(_) | Ty::Wildcard | Ty::Hole(_) => Vec::new(),
                Ty::Apply {
                    function,
                    arguments,
                } => [*function]
                    .into_iter()
                    .chain(arguments.iter().copied())
                    .collect(),
                Ty::Record(entries) => fields(entries),
                Ty::Function { input, output } => vec![*input, *output],
            },
            Kind::Pat(pat) => match pat {
                Pat::Var(_) | Pat::Wildcard | Pat::Hole(_) => Vec::new(),
                Pat::Con { arguments, .. } => arguments.clone(),
                Pat::Record(entries) => fields(entries),
            },
            Kind::Decl(decl) => match decl {
                Decl::Let {
                    params, ty, body, ..
                } => params.iter().copied().chain(*ty).chain([*body]).collect(),
                Decl::Signature { ty, .. } | Decl::Alias { ty, .. } => vec![*ty],
                Decl::Adt { cases, .. } => cases
                    .iter()
                    .flat_map(|c| c.fields.iter().copied())
                    .collect(),
                Decl::Instance {
                    context,
                    head,
                    members,
                } => (context.iter().copied())
                    .chain([*head])
                    .chain(members.iter().copied())
                    .collect(),
                Decl::Group(decls) => decls.clone(),
                Decl::Hole(_) => Vec::new(),
                Decl::Doc { decl, .. } => vec![*decl],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Builder;
    use crate::{Expr, Field, Kind, Span};

    fn span(start: u32, end: u32) -> Span {
        Span { start, end }
    }

    #[test]
    fn nodes_come_after_their_children_with_the_origin_last_set() {
        let mut b = Builder::new(span(0, 10));
        let x = b.at(span(2, 3)).expr(Expr::Hole(span(2, 3)));
        let name = b.at(span(0, 1)).expr(Expr::Name("Pair.make".into()));
        let call = b.at(span(0, 10)).expr(Expr::Apply {
            function: name,
            arguments: vec![x],
        });
        let record = b.expr(Expr::Record(vec![Field {
            label: "pair".into(),
            value: call,
        }]));
        let arena = b.finish(record);
        assert_eq!(arena.root, 3);
        let origins: Vec<Span> = arena.nodes.iter().map(|n| n.origin).collect();
        assert_eq!(origins, [span(2, 3), span(0, 1), span(0, 10), span(0, 10)]);
        assert_eq!(arena.nodes[2].kind.children(), [1, 0]);
        assert_eq!(
            arena.nodes[3].kind,
            Kind::Expr(Expr::Record(vec![Field {
                label: "pair".into(),
                value: 2
            }]))
        );
    }

    #[test]
    #[should_panic(expected = "refers to a node pushed after it")]
    fn a_node_cannot_refer_ahead() {
        let mut b = Builder::new(span(0, 1));
        b.expr(Expr::Items(vec![0]));
    }
}
