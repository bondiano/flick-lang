//! Checks of the UI library the types alone cannot state.

use std::collections::HashMap;

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Range, let_parts};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};
use flick_ir::Label;

impl Checker<'_> {
    /// A foreign component takes props only inside `ui`, through `Ui.component`.
    pub fn ui_callable(&mut self, ty: &Type, call: Range, span: Span) -> Type {
        let target = self.whnf(ty);
        if self.ui > 0
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
    /// The top-level atoms, views and actions, which the generated code
    /// labels `Module.name` for debugging tools.
    pub fn labelled(&mut self, decls: &[&Node]) {
        for decl in members(decls) {
            if decl.kind != K::LetDecl {
                continue;
            }
            let parts = let_parts(decl, self.text);
            let Some(name) = parts.name.clone().filter(|_| !parts.is_function()) else {
                continue;
            };
            let entity = self.env.entity(Namespace::Value, &name);
            let Some(ty) = self.env.value(&entity).cloned() else {
                continue;
            };
            let label = match self.bare(&ty).spine().0 {
                // a family is not an atom: its atoms are made later, by key
                Type::Con { module, name }
                    if !lang::is(Lang::AtomFamily, module, name) && is_atom(module, name) =>
                {
                    "atom"
                }
                Type::Con { module, name }
                    if [Lang::UiView, Lang::UiHandler, Lang::Action]
                        .into_iter()
                        .any(|l| lang::is(l, module, name)) =>
                {
                    "component"
                }
                _ => continue,
            };
            let kind = label.to_owned();
            self.typing.labelled.insert(name, Label { kind });
        }
    }

    /// Keeps the unknowns inside the types of atoms in `ty` from being
    /// generalized: one atom holds one value.
    pub fn lower_atoms(&mut self, ty: &Type) {
        let mut held = Vec::new();
        self.zonk(ty).visit(&mut |t| {
            if let (Type::Con { module, name }, args) = t.spine()
                && is_atom(module, name)
            {
                held.extend(args.into_iter().cloned());
            }
        });
        for t in &held {
            self.store.lower(t);
        }
    }

    /// A value of the type `scheme`, not a function, holding an atom whose
    /// type has a variable the scheme quantifies: one atom would hold a value
    /// of every type the variable stands for, each reader taking it for its own.
    pub fn polymorphic_atom(&mut self, name: Option<&str>, scheme: &Type, span: Span) {
        let ty = match scheme {
            Type::Forall { body, .. } => body,
            ty => ty,
        };
        if matches!(self.whnf(ty), Type::Fun { .. }) {
            return;
        }
        // a variable of a scheme is one it quantifies, written or not
        let quantified = |t: &Type| t.contains(&mut |v| matches!(v, Type::Var(_)));
        let polymorphic = ty.contains(&mut |t| match t.spine() {
            (Type::Con { module, name }, args) => {
                is_atom(module, name) && args.into_iter().any(quantified)
            }
            _ => false,
        });
        if polymorphic {
            let shown = self.show(ty);
            let (atom, it) = name.map_or_else(
                || ("this atom".to_owned(), "it".to_owned()),
                |n| (format!("the atom `{n}`"), format!("`{n}`")),
            );
            self.error(
                codes::U005,
                span,
                format!("{atom} has the type `{shown}` for every type its variables stand for, yet holds one value: give {it} the type of its value, without variables"),
            );
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

/// The types of atoms; a signature keeps the aliases.
const ATOMS: [Lang; 6] = [
    Lang::ReactivityAtom,
    Lang::ReactivityWritable,
    Lang::Atom,
    Lang::AtomWritable,
    Lang::AtomBrowser,
    Lang::AtomFamily,
];

/// Whether `module.name` is a type of atoms.
fn is_atom(module: &str, name: &str) -> bool {
    ATOMS.into_iter().any(|l| lang::is(l, module, name))
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
