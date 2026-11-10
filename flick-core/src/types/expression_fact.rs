//! Read-only expression syntax for plugins, with resolved global references.
use super::{Checker, env::Found};
use crate::consteval;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use flick_plugin::{ExpressionFact, ExpressionKind as E};

impl Checker<'_> {
    pub(super) fn expression_fact(&self, node: &Node) -> ExpressionFact {
        let mut nodes = Vec::new();
        let root = self.expression_node(node, &mut nodes, None);
        ExpressionFact {
            nodes: nodes.into(),
            root,
        }
    }

    fn expression_node(&self, node: &Node, out: &mut Vec<E>, parameter: Option<&str>) -> u32 {
        let children: Vec<_> = node.nodes().collect();
        let kind = match (node.kind, children.as_slice()) {
            (K::Refinement | K::ParenExpr | K::Block, [only]) => {
                return self.expression_node(only, out, parameter);
            }
            (K::Literal, _) => match consteval::eval(node, self.text) {
                Some(consteval::Const::BigInt(n)) => E::BigInt(n.to_string()),
                constant => constant
                    .as_ref()
                    .and_then(super::facts::constant_of)
                    .map_or(E::Unknown, E::Constant),
            },
            (K::NameExpr, _) => {
                let written: String = node.text(self.text).split_whitespace().collect();
                if parameter == Some(written.as_str()) {
                    E::Variable(written)
                } else {
                    let entity = match written.rsplit_once('.') {
                        Some((qualifier, name)) => self
                            .env
                            .qualified_module(qualifier)
                            .map(|(m, _)| (m.to_owned(), name.to_owned())),
                        None => match self.env.find(Namespace::Value, &written) {
                            Found::Entity(e) => Some((e.module.clone(), e.name.clone())),
                            _ => None,
                        },
                    };
                    entity.map_or(E::Unknown, |(module, name)| E::Reference { module, name })
                }
            }
            (K::FunExpr, _) => {
                match (
                    children.iter().find(|n| n.kind == K::VarPat),
                    children.iter().find(|n| n.kind == K::Block),
                ) {
                    (Some(param), Some(body)) => {
                        let name = param.text(self.text).trim().to_owned();
                        let body = self.expression_node(body, out, Some(&name));
                        E::Lambda {
                            parameter: name,
                            body,
                        }
                    }
                    _ => E::Unknown,
                }
            }
            (K::PrefixExpr, [value]) => {
                let value = self.expression_node(value, out, parameter);
                let op = node
                    .child_tokens()
                    .next()
                    .map_or("", |t| t.text(self.text))
                    .to_owned();
                E::Unary { op, value }
            }
            (K::BinExpr, [left, right]) => {
                let left = self.expression_node(left, out, parameter);
                let right = self.expression_node(right, out, parameter);
                let op = node
                    .child_tokens()
                    .find(|t| t.kind.is_binary_op())
                    .map_or("", |t| t.text(self.text))
                    .to_owned();
                E::Binary { op, left, right }
            }
            (K::IfExpr, [condition, then, otherwise]) => {
                let condition = self.expression_node(condition, out, parameter);
                let then = self.expression_node(then, out, parameter);
                let otherwise = self.expression_node(otherwise, out, parameter);
                E::If {
                    condition,
                    then,
                    otherwise,
                }
            }
            _ => E::Unknown,
        };
        let id = u32::try_from(out.len()).expect("expression facts fit in u32");
        out.push(kind);
        id
    }
}
