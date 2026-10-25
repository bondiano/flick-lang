//! Checks of the UI library the types alone cannot state.

use std::collections::{HashMap, HashSet};

use super::typing::Ref;
use super::{Checker, Range, let_parts};
use crate::consteval::{self, Const};
use crate::diagnostics::codes;
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
    /// declared at the top level, or `Action.with` data, is made once.
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
        let members = decls.iter().flat_map(|d| match d.kind {
            K::RecGroup => d
                .nodes()
                .filter(|n| n.kind == K::Members)
                .flat_map(Node::nodes)
                .collect(),
            _ => vec![*d],
        });
        for decl in members {
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

/// The applications of an event policy or action constructor to a `fun`
/// under `node`: their spans and the constructor's name.
fn handlers_of_lambdas(node: &Node, names: &HashMap<Range, Ref>, out: &mut Vec<(Range, String)>) {
    if node.kind == K::AppExpr {
        let parts: Vec<&Node> = node.nodes().collect();
        let constructor = parts
            .first()
            .and_then(|head| match names.get(&head.range()) {
                Some(Ref::Value { module, name })
                    if matches!(module.as_str(), "Ui.Event" | "Action")
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
