//! Checks of the UI library the types alone cannot state.

use std::collections::HashMap;

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Range, let_parts};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// The innermost module a block around the expression opens that exports
    /// `name`, as the file names the module.
    pub(super) fn opened_with(&self, namespace: Namespace, name: &str) -> Option<String> {
        let exports = |module: &&String| matches!(self.env.qualified_module(module), Some((_, Some(i))) if i.find(namespace, name).is_some());
        let modules = self.opened.iter().rev().map(|(module, _)| module);
        modules.into_iter().find(exports).cloned()
    }

    /// A foreign component takes props only inside `ui`, through `Ui.component`.
    pub fn ui_callable(&mut self, ty: &Type, call: Range, span: Span) -> Type {
        let target = self.whnf(ty);
        if self.opened.iter().any(|(module, _)| module == "Ui")
            && matches!(
                &target,
                Type::App { head, .. }
                    if matches!(head.as_ref(), Type::Con { module, name }
                        if lang::is(Lang::UiComponent, module, name))
            )
        {
            self.expect(ty, &target, span);
            self.typing.ui_components.insert(call);
            if let Type::App { arg, .. } = target {
                return Type::Fun {
                    param: arg,
                    effects: None,
                    result: Box::new(Type::lang(Lang::Ui)),
                };
            }
        }
        target
    }
}

impl Checker<'_> {
    /// Read-only binding and application facts for serializable atom
    /// references. The watching UI plugin decides whether their keys are safe.
    pub fn record_atom_bindings(&mut self, decls: &[&Node]) {
        for decl in members(decls) {
            let mut keyed = Keyed::default();
            keyed.walk(decl, false, &self.typing.names);
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

/// The makers of serializable atoms, whose key U003 checks.
const SERIALIZABLE: [Lang; 4] = [
    Lang::Serializable,
    Lang::SerializableFamily,
    Lang::SerializableTask,
    Lang::SerializableResult,
];

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

/// The serializable atoms under a declaration: where each is named, and
/// which are applied to their codec and state.
#[derive(Default)]
struct Keyed {
    found: Vec<(Range, String, bool)>,
    /// The heads of applications and how many arguments each is given.
    applied: HashMap<Range, usize>,
}

impl Keyed {
    fn walk(&mut self, node: &Node, in_function: bool, names: &HashMap<Range, Ref>) {
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
            _ => {}
        }
        if let Some(Ref::Value { module, name }) = names.get(&node.range())
            && SERIALIZABLE.into_iter().any(|l| lang::is(l, module, name))
            && self.found.last().is_none_or(|(r, ..)| *r != node.range())
        {
            self.found.push((node.range(), name.clone(), in_function));
        }
        let inner = in_function
            || node.kind == K::FunExpr
            || (node.kind == K::LetDecl && parts.iter().any(|n| n.kind == K::Params));
        for child in parts {
            self.walk(child, inner, names);
        }
    }
}
