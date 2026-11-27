//! The rule of a `type` with `@native when <cond>`: where the condition holds,
//! the type is the backend's own and its values have no constructors, so
//! only the Flick body of a definition whose `@native` clauses hold there too
//! may build or match them (`FLICK-F017`).

use super::Checker;
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::{Node, Token};
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::when;
use crate::types::env::Found;
use crate::types::ty::Type;
use crate::types::{implicit_package_of, is_type};

impl<'a> Checker<'a> {
    /// Reports each constructor of a native type that `decls` name where the
    /// type may be the backend's own.
    pub(in crate::types) fn check_represented_uses(&mut self, decls: &[&'a Node]) {
        for &decl in decls {
            match decl.kind {
                K::TypeDecl => self.check_represented_deriving(decl),
                K::RecGroup => {
                    for member in decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                    {
                        self.check_represented_decl(member);
                    }
                }
                _ => self.check_represented_decl(decl),
            }
        }
    }

    /// A derived instance builds and matches the type's constructors.
    fn check_represented_deriving(&mut self, decl: &Node) {
        let (Some(condition), Some(deriving)) = (
            super::type_native_condition(decl, self.text),
            decl.nodes().find(|n| n.kind == K::DerivingClause),
        ) else {
            return;
        };
        let name = decl
            .child_tokens()
            .find(|t| t.kind == K::Con)
            .map_or("", |t| t.text(self.text));
        self.diagnostics.push(Diagnostic::error(
            codes::F017,
            format!(
                "`{name}` cannot derive an instance: a derived instance matches its constructors, \
                 which are not its values where `{condition}` holds; write the instance over \
                 functions whose `@native` clauses cover it"
            ),
            self.span(deriving),
        ));
    }

    /// Whether the definition around `span` covers `ty`, a native type: its
    /// `@native` clauses hold wherever the type is the backend's own. Such a
    /// body stands for the binding, and G4 holds it, not the checker, to
    /// what the binding promises of the regions its values reach
    /// (`docs/formal/regions.md`, native types).
    pub(in crate::types) fn covers_native(&self, ty: &Type, span: Span) -> bool {
        // only a definition with `@native` clauses covers anything
        let Some(cover) = self.decl_at(span).and_then(|decl| cover(decl, self.text)) else {
            return false;
        };
        let Type::Con { module, name } = self.whnf_captured(ty).0.spine().0.clone() else {
            return false;
        };
        let condition = (self.env.type_def(&module, &name)).and_then(|d| d.native.as_deref());
        let env = self.options.native_env();
        condition.is_some_and(|c| when::parse(c).is_ok_and(|native| env.implies(&native, &cover)))
    }

    /// The constructors of native types `decl` names, each checked against the
    /// `@native` clauses of `decl`.
    fn check_represented_decl(&mut self, decl: &Node) {
        let cover = cover(decl, self.text);
        let mut uses = Vec::new();
        constructors(decl, self.text, &mut uses);
        for (qualifier, token) in uses {
            self.check_represented_use(qualifier.as_deref(), token, cover.as_ref());
        }
    }

    fn check_represented_use(
        &mut self,
        qualifier: Option<&str>,
        token: Token,
        cover: Option<&when::Expr>,
    ) {
        let name = token.text(self.text);
        let entity = match qualifier {
            None => match self.env.find(Namespace::Constructor, name) {
                Found::Entity(entity) => entity,
                _ => return,
            },
            Some(q) => match self.env.qualified_module(q) {
                Some((module, _)) => Entity {
                    module: module.to_owned(),
                    namespace: Namespace::Constructor,
                    name: name.to_owned(),
                },
                None => return,
            },
        };
        let Some((module, ty)) = self.env.parent(&entity) else {
            return;
        };
        let Some(condition) = self
            .env
            .type_def(&module, &ty)
            .and_then(|d| d.native.clone())
        else {
            return;
        };
        let Ok(native) = when::parse(&condition) else {
            return;
        };
        let env = self.options.native_env();
        let covered = cover.is_some_and(|cover| env.implies(&native, cover));
        let message = if !self.same_package(&module) {
            format!(
                "`{name}` is a constructor of `{ty}`, a native type of another package: its \
                 representation is that package's, which only its own bodies may build or match"
            )
        } else if covered {
            return;
        } else {
            format!(
                "`{name}` is a constructor of `{ty}`, which is the backend's own where `{condition}` \
                 holds: only the Flick body of a definition whose `@native` clauses hold there \
                 too may build or match it"
            )
        };
        self.diagnostics.push(Diagnostic::error(
            codes::F017,
            message,
            self.token_span(token),
        ));
    }

    /// Whether `module` is of the package of the module being checked: the
    /// same implicit package, or both of the package being compiled.
    fn same_package(&self, module: &str) -> bool {
        let here = self.env.module.as_str();
        match (implicit_package_of(module), implicit_package_of(here)) {
            (Some(theirs), Some(ours)) => theirs == ours,
            (None, None) => (self.options.local)(module) && (self.options.local)(here),
            _ => false,
        }
    }
}

/// Where the `@native` clauses of `decl` hold: the disjunction of their
/// conditions, none without clauses.
fn cover(decl: &Node, text: &str) -> Option<when::Expr> {
    (decl.nodes())
        .filter(|n| n.kind == K::NativeOverride)
        .filter_map(|ov| ov.nodes().find(|n| n.kind == K::WhenCond))
        .filter_map(|cond| {
            let (start, end) = cond.range();
            when::parse(text[start as usize..end as usize].trim()).ok()
        })
        .reduce(|a, b| when::Expr::Or(Box::new(a), Box::new(b)))
}

/// Each constructor token under `node` outside its types and `@native`
/// clauses, with the qualifier written before it, in the order of the text.
/// It walks with a stack of its own: an expression nests as deep as a chain
/// of operators is long.
fn constructors(node: &Node, text: &str, out: &mut Vec<(Option<String>, Token)>) {
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if is_type(node.kind) || node.kind == K::NativeOverride || node.kind == K::Signature {
            continue;
        }
        let tokens: Vec<Token> = node.child_tokens().collect();
        for (i, token) in tokens.iter().enumerate() {
            if token.kind != K::Con {
                continue;
            }
            // `M.N.Con`: the qualifier is the module, not a constructor
            let next_is_dot = tokens.get(i + 1).is_some_and(|t| t.kind == K::Dot);
            if next_is_dot {
                continue;
            }
            let mut qualifier = Vec::new();
            let mut j = i;
            while j >= 2 && tokens[j - 1].kind == K::Dot && tokens[j - 2].kind == K::Con {
                qualifier.push(j - 2);
                j -= 2;
            }
            qualifier.reverse();
            let qualifier = (!qualifier.is_empty()).then(|| {
                let names: Vec<&str> = qualifier.iter().map(|&k| tokens[k].text(text)).collect();
                names.join(".")
            });
            out.push((qualifier, *token));
        }
        let children: Vec<&Node> = node.nodes().collect();
        pending.extend(children.into_iter().rev());
    }
}
