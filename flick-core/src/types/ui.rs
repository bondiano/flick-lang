//! Checks of the UI library the types alone cannot state.

use std::collections::HashMap;

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Range, let_parts};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::ir::Label;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

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

    /// `Ui.view render`: a handler made from a `fun` inside the render is
    /// a new closure on every render, so it is worth a warning; an action
    /// declared at the top level, with `Action.with` data, is made once.
    pub fn ui_render_handlers(&mut self, head: &Node, render: &Node) {
        let is_view = matches!(
            self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if lang::is(Lang::View, module, name)
        );
        // a `component` is a `Ui.view` its plugin writes: its handlers stay as
        // they are, as before the plugin wrote it
        if !is_view || head.range().0 >= self.generated {
            return;
        }
        let mut found = Vec::new();
        handlers_of_lambdas(render, &self.typing.names, &mut found);
        for (span, what) in found {
            let span = Span::new(self.file, span.0, span.1);
            self.warning(
                codes::U002,
                span,
                format!("`{what}` of a `fun` makes a handler on every render: declare an `Action` at the top level, or pass data with `Action.with`"),
            );
        }
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
    /// Whether `ty` is a render: a function whose services are `Atom.Read`
    /// alone, as a view's render and a derived atom's computation are.
    pub(crate) fn reads_only(&mut self, ty: &Type) -> bool {
        let Type::Fun {
            effects: Some(effects),
            ..
        } = self.bare(ty)
        else {
            return false;
        };
        let services = self.store.normalize(&effects.services);
        services.tails.is_empty()
            && services
                .entries
                .iter()
                .filter_map(|(l, _)| l.name())
                .eq(lang::label(Lang::AtomRead))
    }

    /// Whether `ty` is the setup of a component: a function whose services
    /// are `Setup` alone.
    pub(crate) fn setup_only(&mut self, ty: &Type) -> bool {
        let Type::Fun {
            effects: Some(effects),
            ..
        } = self.bare(ty)
        else {
            return false;
        };
        let services = self.store.normalize(&effects.services);
        services.tails.is_empty()
            && services
                .entries
                .iter()
                .filter_map(|(l, _)| l.name())
                .eq(["Prelude.Setup"])
    }

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

    /// Atoms, actions and views made while rendering: a view renders again
    /// at any time, and each render would make another one, losing its
    /// state, its subscribers or its policy. A render is a `fun` or a
    /// top-level function whose services are `Atom.Read` alone; the task of
    /// a handler in it runs on an event, not while rendering.
    pub fn made_in_render(&mut self, decls: &[&Node]) {
        let heads: HashMap<Range, Type> = self
            .typing
            .pending_types
            .iter()
            .map(|(at, ty)| (*at, ty.clone()))
            .collect();
        let mut renders = Vec::new();
        for decl in members(decls) {
            if decl.kind != K::LetDecl {
                continue;
            }
            let parts = let_parts(decl, self.text);
            if let (Some(name), Some(body)) = (&parts.name, parts.body)
                && !parts.params.is_empty()
            {
                let entity = self.env.entity(Namespace::Value, name);
                let mut ty = self.env.value(&entity).cloned();
                while let Some(t) = ty.take() {
                    if self.reads_only(&t) {
                        renders.push(body);
                    } else if let Type::Fun { result, .. } = self.bare(&t) {
                        ty = Some(*result);
                    }
                }
            }
            funs(
                decl,
                &mut |fun| {
                    let keyword = fun.child_tokens().find(|t| t.kind == K::FunKw);
                    keyword.and_then(|t| heads.get(&(t.start, t.end)).cloned())
                },
                &mut |fun, ty| {
                    if self.reads_only(&ty) {
                        renders.push(fun);
                    }
                },
            );
        }
        let mut found = Vec::new();
        for render in renders {
            self.made(render, &mut found);
        }
        found.sort_unstable();
        found.dedup();
        for (range, what, maker) in found {
            let what = if what == maker {
                format!("`{what}`")
            } else {
                format!("`{what}`, which calls `{maker}`,")
            };
            self.error(
                codes::U006,
                Span::new(self.file, range.0, range.1),
                format!("{what} while rendering makes another one on every render: make it once, with a top-level `let`, or one per key with `Atom.family`"),
            );
        }
    }
}

/// Every `fun` under `node` with the type `head` finds for it.
fn funs<'n>(
    node: &'n Node,
    head: &mut impl FnMut(&Node) -> Option<Type>,
    found: &mut impl FnMut(&'n Node, Type),
) {
    if node.kind == K::FunExpr
        && let Some(ty) = head(node)
    {
        found(node, ty);
    }
    for child in node.nodes() {
        funs(child, head, found);
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

/// The event policies of `Ui.Event`, which make an action.
const EVENTS: [Lang; 4] = [
    Lang::EventExhaust,
    Lang::EventLatest,
    Lang::EventQueue,
    Lang::EventConcurrent,
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

/// The applications of an event policy to a `fun` under `node`: their
/// spans and the policy's name. An action made there is `FLICK-U006`.
fn handlers_of_lambdas(node: &Node, names: &HashMap<Range, Ref>, out: &mut Vec<(Range, String)>) {
    if node.kind == K::AppExpr {
        let parts: Vec<&Node> = node.nodes().collect();
        let constructor = parts
            .first()
            .and_then(|head| match names.get(&head.range()) {
                Some(Ref::Value { module, name })
                    if EVENTS.into_iter().any(|l| lang::is(l, module, name)) =>
                {
                    Some(format!("{module}.{name}"))
                }
                _ => None,
            });
        let unwrap = |mut n: &Node| {
            while n.kind == K::ParenExpr {
                match n.nodes().next() {
                    Some(inner) => n = inner,
                    None => break,
                }
            }
            n.kind
        };
        if let (Some(what), Some(last)) = (constructor, parts.last())
            && parts.len() > 1
            && unwrap(last) == K::FunExpr
        {
            out.push((node.range(), what));
        }
    }
    for child in node.nodes() {
        handlers_of_lambdas(child, names, out);
    }
}
