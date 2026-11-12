//! Checks of the UI library the types alone cannot state.

use std::collections::HashMap;

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Range, Watched, let_parts};
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// The innermost module a block around the expression opens that exports
    /// `name`, as the file names the module.
    pub(super) fn opened_with(&self, namespace: Namespace, name: &str) -> Option<String> {
        let exports = |module: &&String| matches!(self.env.qualified_module(module), Some((_, Some(i))) if i.find(namespace, name).is_some());
        let modules = self.opened.iter().rev().map(|o| &o.module);
        modules.into_iter().find(exports).cloned()
    }

    /// The function a block applies values through, as `written` names it,
    /// and its type.
    pub(super) fn through(&mut self, written: &Node) -> Option<(Ref, Type)> {
        self.infer(written);
        let function = self.typing.names.get(&written.range())?.clone();
        let Ref::Value { module, name } = &function else {
            return None;
        };
        let entity = Entity {
            namespace: Namespace::Value,
            module: module.clone(),
            name: name.clone(),
        };
        let scheme = self.env.value(&entity)?.clone();
        Some((function, scheme))
    }

    /// The type a callee of the type `ty` is applied at. Inside a block that
    /// applies values through a function, a value of the type the function
    /// takes first is applied through it: a foreign component takes its
    /// props inside `ui` through `Ui.component`.
    pub fn ui_callable(&mut self, ty: &Type, call: Range, span: Span) -> Type {
        let target = self.whnf(ty);
        if matches!(target, Type::Fun { .. }) {
            return target;
        }
        let head = |t: &Type| match t.spine().0 {
            Type::Con { module, name } => Some((module.clone(), name.clone())),
            _ => None,
        };
        let Some(of) = head(&target) else {
            return target;
        };
        // the innermost block that names one
        let through = self.opened.iter().rev().find_map(|o| o.through.clone());
        if let Some((function, scheme)) = through
            && let Type::Fun {
                param,
                effects: None,
                result,
            } = self.instantiate(&scheme, span)
            && head(&self.whnf(&param)) == Some(of)
        {
            self.expect(ty, &param, span);
            self.typing.through.insert(call, function);
            return self.whnf(&result);
        }
        target
    }
}

/// A module a block opens around the expression being checked.
pub(super) struct Opened {
    pub module: String,
    /// The type of a child, when calls give their last list element by
    /// element.
    pub child: Option<Type>,
    /// The function values are applied through, and its type.
    pub through: Option<(Ref, Type)>,
}

impl Checker<'_> {
    /// Where each watched value is named: the declaration it stands in,
    /// whether inside a function of it, and how many arguments it is given.
    /// A plugin that watches the name reads them: the UI's decides by them
    /// whether the key of a serializable atom is safe.
    pub fn record_atom_bindings(&mut self, decls: &[&Node]) {
        let watched = self.options.watched;
        if watched.is_empty() {
            return;
        }
        for decl in members(decls) {
            let mut keyed = Keyed::default();
            keyed.walk(decl, false, &self.typing.names, watched);
            if keyed.found.is_empty() {
                continue;
            }
            let binding = (decl.kind == K::LetDecl)
                .then(|| let_parts(decl, self.text))
                .filter(|p| !p.is_function())
                .and_then(|p| p.name);
            // a binding whose scheme takes a dictionary, as `CallSite =>`,
            // makes its atoms where it is used, as a function does
            let constrained = binding.as_ref().is_some_and(|b| {
                let entity = self.env.entity(Namespace::Value, b);
                matches!(self.env.value(&entity), Some(Type::Forall { constraints, .. }) if !constraints.is_empty())
            });
            for (range, name, in_function) in keyed.found {
                let Some(Ref::Value { module, .. }) = self.typing.names.get(&range) else {
                    continue;
                };
                self.typing.call_sites.push(flick_plugin::CallSite {
                    module: module.clone(),
                    name,
                    span: flick_plugin::Span {
                        start: range.0,
                        end: range.1,
                    },
                    arguments: Vec::new(),
                    refinement: None,
                    generated: range.0 >= self.generated,
                    tagged: None,
                    lacking: None,
                    binding: Some(flick_plugin::Binding {
                        module: self.env.module.clone(),
                        name: binding.clone(),
                        span: flick_plugin::Span {
                            start: decl.range().0,
                            end: decl.range().1,
                        },
                        constrained,
                        in_function,
                        applied: u32::try_from(keyed.applied.get(&range).copied().unwrap_or(0))
                            .unwrap_or(u32::MAX),
                        gives: None,
                    }),
                });
            }
        }
    }
}

impl Checker<'_> {
    /// `ty` resolved, without its captures and quantifiers.
    pub(super) fn bare(&self, ty: &Type) -> Type {
        let mut ty = self.store.resolve(ty);
        while let Type::Captured { ty: inner, .. }
        | Type::Bounded { ty: inner, .. }
        | Type::Forall { body: inner, .. } = ty
        {
            ty = self.store.resolve(&inner);
        }
        ty
    }
}

/// The declarations, those of each `rec` group one by one.
pub(super) fn members<'n>(decls: &[&'n Node]) -> impl Iterator<Item = &'n Node> {
    decls.iter().flat_map(|d| match d.kind {
        K::RecGroup => d
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .collect(),
        _ => vec![*d],
    })
}

/// The watched values under a declaration: where each is named, and how
/// many arguments each is applied to.
#[derive(Default)]
struct Keyed {
    found: Vec<(Range, String, bool)>,
    /// The heads of applications and how many arguments each is given.
    applied: HashMap<Range, usize>,
}

impl Keyed {
    fn walk(
        &mut self,
        node: &Node,
        in_function: bool,
        names: &HashMap<Range, Ref>,
        watched: &Watched,
    ) {
        let parts: Vec<&Node> = node.nodes().collect();
        match node.kind {
            // a piped application already counted its piped argument
            K::AppExpr => {
                let given = self.applied.entry(parts[0].range()).or_default();
                *given = (*given).max(parts.len() - 1);
            }
            // `state |> Atom.serializable codec`: the piped state is the last argument
            K::BinExpr if node.has_token(K::PipeGt) => {
                if let Some(app) = parts.get(1).filter(|n| n.kind == K::AppExpr)
                    && let Some(head) = app.nodes().next()
                {
                    self.applied.insert(head.range(), app.nodes().count());
                }
            }
            // `Atom.serializable codec <| state` is the application itself
            K::BinExpr if node.has_token(K::LtPipe) => {
                let (head, given) = match parts.first() {
                    Some(app) if app.kind == K::AppExpr => {
                        (app.nodes().next(), app.nodes().count())
                    }
                    other => (other.copied(), 1),
                };
                if let Some(head) = head {
                    let known = self.applied.entry(head.range()).or_default();
                    *known = (*known).max(given);
                }
            }
            _ => {}
        }
        if let Some(Ref::Value { module, name }) = names.get(&node.range())
            && watched.holds(module, name)
            && self.found.last().is_none_or(|(r, ..)| *r != node.range())
        {
            self.found.push((node.range(), name.clone(), in_function));
        }
        let inner = in_function
            || node.kind == K::FunExpr
            || (node.kind == K::LetDecl && parts.iter().any(|n| n.kind == K::Params));
        for child in parts {
            self.walk(child, inner, names, watched);
        }
    }
}
