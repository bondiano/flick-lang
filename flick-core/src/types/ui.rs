//! Checks of the UI library the types alone cannot state.

use std::collections::{HashMap, HashSet};

use super::ty::Type;
use super::typing::Ref;
use super::{Checker, Range, let_parts};
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// `Ui.Event.queue c` and `Ui.Event.concurrent c` of a constant `c`:
    /// the capacity or limit must be a positive integer.
    pub fn ui_capacity(&mut self, head: &Node, arg: &Node) {
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()) else {
            return;
        };
        if module != "Ui.Event" || !matches!(name.as_str(), "queue" | "concurrent") {
            return;
        }
        let what = if name == "queue" { "capacity" } else { "limit" };
        if let Some(Const::Number(n)) = consteval::eval(arg, self.text)
            && !(n >= 1.0 && n.fract() == 0.0 && n.is_finite())
        {
            self.error(
                codes::U001,
                self.span(arg),
                format!("the {what} of `Ui.Event.{name}` must be a positive integer"),
            );
        }
    }

    /// `Ui.view render`: a handler made from a `fun` inside the render is
    /// a new closure on every render, so it is worth a warning; an action
    /// declared at the top level, with `Action.with` data, is made once.
    pub fn ui_render_handlers(&mut self, head: &Node, render: &Node) {
        let is_view = matches!(
            self.typing.names.get(&head.range()),
            Some(Ref::Value { module, name }) if module == "Ui" && name == "view"
        );
        if !is_view {
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
    /// `Atom.serializable` and `Atom.serializable_family` take their key
    /// from the top-level binding they define, so each is applied there
    /// once, outside any function: anywhere else one key would name many
    /// atoms.
    pub fn atom_keys(&mut self, decls: &[&Node]) {
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
            let mut defined = false;
            for (range, name, in_function) in keyed.found {
                let message = match &binding {
                    None => format!(
                        "`Atom.{name}` takes its key from the top-level binding it defines: define the atom with a top-level `let` without parameters"
                    ),
                    Some(_) if in_function => format!(
                        "`Atom.{name}` inside a function would give its key to every atom the function makes: define the atom with a top-level `let`"
                    ),
                    Some(_) if !keyed.applied.contains(&range) => format!(
                        "`Atom.{name}` is applied to its codec and state where it is defined, since the binding's name is its key"
                    ),
                    Some(b) if defined => format!(
                        "a second serializable atom in `{b}` would share its key `{}.{b}`: define each with its own top-level `let`",
                        self.env.module
                    ),
                    Some(_) => {
                        defined = true;
                        continue;
                    }
                };
                self.error(codes::U003, Span::new(self.file, range.0, range.1), message);
            }
        }
    }
}

impl Checker<'_> {
    /// A top-level value, not a function, holding an atom whose type has a
    /// variable it quantifies: one atom would hold a value of every type the
    /// variable stands for, each reader taking it for its own.
    pub fn polymorphic_atoms(&mut self, decls: &[&Node]) {
        for decl in members(decls) {
            if decl.kind != K::LetDecl {
                continue;
            }
            let parts = let_parts(decl, self.text);
            let (Some(name), Some(body)) = (parts.name.clone(), parts.body) else {
                continue;
            };
            if parts.is_function() {
                continue;
            }
            let entity = self.env.entity(Namespace::Value, &name);
            let ty = match self.env.value(&entity).cloned() {
                Some(Type::Forall { body, .. }) => *body,
                Some(ty) => ty,
                None => continue,
            };
            // a variable of a top-level type is one it quantifies, written or not
            let quantified = |t: &Type| t.contains(&mut |v| matches!(v, Type::Var(_)));
            let polymorphic = ty.contains(&mut |t| match t.spine() {
                (Type::Con { module, name }, args) => {
                    ATOMS.contains(&(module.as_str(), name.as_str()))
                        && args.into_iter().any(quantified)
                }
                _ => false,
            });
            if polymorphic {
                let shown = self.show(&ty);
                self.error(
                    codes::U005,
                    self.span(body),
                    format!("the atom `{name}` has the type `{shown}` for every type its variables stand for, yet holds one value: give `{name}` the type of its value, without variables"),
                );
            }
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
                .map(|(l, _)| l.as_str())
                .eq(["Atom.Read"])
    }

    /// `ty` resolved, without its captures and quantifiers.
    fn bare(&self, ty: &Type) -> Type {
        let mut ty = self.store.resolve(ty);
        while let Type::Captured { ty: inner, .. } | Type::Forall { body: inner, .. } = ty {
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
            made(render, &self.typing.names, &mut found);
        }
        found.sort_unstable();
        found.dedup();
        for (range, what) in found {
            self.error(
                codes::U006,
                Span::new(self.file, range.0, range.1),
                format!("`{what}` while rendering makes another one on every render: make it once, with a top-level `let`, or one per key with `Atom.family`"),
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

/// What makes an atom, an action or a view, by module and names.
const MAKERS: [(&str, &[&str]); 3] = [
    (
        "Atom",
        &[
            "make",
            "derived",
            "task",
            "serializable",
            "serializable_family",
            "on_server",
            "client_only",
            "search_param",
            "family",
        ],
    ),
    ("Action", &["exhaust", "latest", "queue", "concurrent"]),
    ("Ui", &["view", "suspended", "named"]),
];

/// The names under `node` that make an atom, an action or a view, outside
/// the tasks of handlers.
fn made(node: &Node, names: &HashMap<Range, Ref>, out: &mut Vec<(Range, String)>) {
    if node.kind == K::TaskExpr {
        return;
    }
    if let Some(Ref::Value { module, name }) = names.get(&node.range())
        && MAKERS
            .iter()
            .any(|(m, ns)| m == module && ns.contains(&name.as_str()))
    {
        out.push((node.range(), format!("{module}.{name}")));
    }
    for child in node.nodes() {
        made(child, names, out);
    }
}

/// The types of atoms, by module and name; a signature keeps the aliases.
const ATOMS: [(&str, &str); 5] = [
    ("Reactivity.Atom", "Atom"),
    ("Reactivity.Atom", "Writable"),
    ("Atom", "Atom"),
    ("Atom", "Writable"),
    ("Atom", "Browser"),
];

/// The declarations, those of each `rec` group one by one.
fn members<'n>(decls: &[&'n Node]) -> impl Iterator<Item = &'n Node> {
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
    applied: HashSet<Range>,
}

impl Keyed {
    fn walk(&mut self, node: &Node, in_function: bool, names: &HashMap<Range, Ref>) {
        let parts: Vec<&Node> = node.nodes().collect();
        match node.kind {
            K::AppExpr if parts.len() == 3 => {
                self.applied.insert(parts[0].range());
            }
            // `state |> Atom.serializable codec`
            K::BinExpr if node.has_token(K::PipeGt) => {
                if let Some(app) = parts.get(1).filter(|n| n.kind == K::AppExpr)
                    && let [head, _] = app.nodes().collect::<Vec<_>>()[..]
                {
                    self.applied.insert(head.range());
                }
            }
            _ => {}
        }
        if let Some(Ref::Value { module, name }) = names.get(&node.range())
            && module == "Atom"
            && matches!(name.as_str(), "serializable" | "serializable_family")
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
                    if module == "Ui.Event"
                        && matches!(
                            name.as_str(),
                            "exhaust" | "latest" | "queue" | "concurrent"
                        ) =>
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
