//! Signature help, go to type definition and inlay hints: what the
//! checker knows about types, shown where code is written.

use super::{Ide, Location, Symbol, tree};
use crate::iface::{Entity, Namespace};
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{Type, right_application, show_signature, show_type};

/// The signature of the function a call at the cursor applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Help {
    pub label: String,
    /// Byte ranges of each parameter's type in `label`.
    pub params: Vec<(u32, u32)>,
    pub active: usize,
}

impl Ide<'_> {
    /// For a cursor among the arguments of a call `f a b`, or `f a <| b`.
    pub fn signature_help(&self, file: FileId, offset: u32) -> Option<Help> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        let call = path.iter().rev().find_map(|n| {
            let parts: Vec<&Node> = match n.kind {
                K::AppExpr => n.nodes().collect(),
                K::BinExpr if n.has_token(K::LtPipe) => {
                    let mut operands = n.nodes();
                    right_application(operands.next()?, operands.next()?)
                }
                _ => return None,
            };
            parts
                .first()
                .is_some_and(|h| h.range().1 < offset)
                .then_some(parts)
        })?;
        let (head, nodes) = call.split_first()?;
        let ty = m.typing.types.get(&head.range())?;
        let mut params = Vec::new();
        let mut stage = strip(ty);
        while let Type::Fun { param, result, .. } = stage {
            params.push(show_type(param));
            stage = strip(result);
        }
        if params.is_empty() {
            return None;
        }
        let label = format!("{} : {}", head.text(text), show_signature(ty));
        let mut from = head.text(text).len() + 3;
        let mut ranges = Vec::new();
        for p in &params {
            let at = from + label[from..].find(p.as_str())?;
            ranges.push((to_u32(at), to_u32(at + p.len())));
            from = at + p.len();
        }
        let before = nodes.iter().filter(|a| a.range().1 < offset).count();
        Some(Help {
            label,
            params: ranges,
            active: before.min(params.len() - 1),
        })
    }

    /// The declaration of the type of the expression at `offset`.
    pub fn type_definition(&self, file: FileId, offset: u32) -> Option<Location> {
        let m = self.module(file)?;
        let (_, ty) = tree::innermost(m.typing.types.iter(), offset)?;
        let Type::Con { module, name } = strip(ty).spine().0 else {
            return None;
        };
        self.definition(&Symbol::Entity(Entity {
            module: module.clone(),
            namespace: Namespace::Type,
            name: name.clone(),
        }))
    }

    /// Types the code does not write: after each `let` name without a
    /// signature or annotation, and after each stage of a pipeline written
    /// over several lines. Offsets with labels, in source order.
    pub fn inlay_hints(&self, file: FileId) -> Vec<(u32, String)> {
        let Some(m) = self.module(file) else {
            return Vec::new();
        };
        let text = self.text(file);
        let mut out = Vec::new();
        walk(&m.tree, &mut |node, parent| match node.kind {
            K::LetDecl => {
                let Some(name) = node
                    .nodes()
                    .find(|n| n.kind == K::VarPat)
                    .and_then(|p| p.child_tokens().find(|t| t.kind == K::Ident))
                else {
                    return;
                };
                let signed = parent.is_some_and(|p| {
                    let siblings: Vec<&Node> = p.nodes().collect();
                    let i = siblings.iter().position(|n| n.range() == node.range());
                    i.and_then(|i| i.checked_sub(1)).is_some_and(|i| {
                        siblings[i].kind == K::Signature
                            && tree::name_of(siblings[i], K::Ident, text) == Some(name.text(text))
                    })
                });
                if let (false, Some(ty)) = (signed, m.typing.types.get(&(name.start, name.end))) {
                    out.push((name.end, format!(": {}", show_signature(ty))));
                }
            }
            K::BinExpr => {
                let Some(pipe) = node
                    .child_tokens()
                    .find(|t| matches!(t.kind, K::PipeGt | K::LtPipe))
                else {
                    return;
                };
                let line_end = |at: u32| {
                    text[at as usize..].starts_with(['\n', '\r']) || at as usize == text.len()
                };
                let (start, end) = node.range();
                let multiline = text[start as usize..end as usize].contains('\n');
                // `x |> f` on its line gives the value so far after it; the
                // function of `f <| x` gives its value on the line it is on
                let at = if pipe.kind == K::PipeGt {
                    Some(end).filter(|&end| multiline && line_end(end))
                } else {
                    let function = node.nodes().next().map_or(start, |f| f.range().1);
                    [function, pipe.end].into_iter().find(|&at| line_end(at))
                };
                if let Some(at) = at
                    && let Some(ty) = m.typing.types.get(&(pipe.start, pipe.end))
                {
                    out.push((at, format!(" : {}", show_signature(ty))));
                }
            }
            _ => {}
        });
        out.sort();
        out
    }
}

fn walk<'a>(node: &'a Node, f: &mut dyn FnMut(&'a Node, Option<&'a Node>)) {
    for child in node.nodes() {
        f(child, Some(node));
        walk(child, f);
    }
}

/// Inspect a type through quantifiers and capture proofs, without changing it.
pub(super) fn strip(t: &Type) -> &Type {
    match t {
        Type::Forall { body, .. }
        | Type::Captured { ty: body, .. }
        | Type::Bounded { ty: body, .. } => strip(body),
        _ => t,
    }
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).expect("source offsets fit in u32")
}
