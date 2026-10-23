//! The top level of a module: declarations are order-independent, so a value
//! without a signature is inferred when it is first used. Recursion must be
//! explicit: a function that uses itself is `let rec`, functions that
//! use each other share a `rec { … }` group, and values never recurse.

use std::collections::HashMap;
use std::mem;

use super::Checker;
use super::bindings::{let_parts, signature_name};
use super::convert::is_type;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

#[derive(Default)]
pub struct TopLevel<'a> {
    by_name: HashMap<String, usize>,
    items: Vec<Item<'a>>,
    /// The item being checked, whose uses of other items are recorded.
    current: Option<usize>,
    edges: Vec<(usize, usize, Span)>,
}

struct Item<'a> {
    names: Vec<String>,
    decl: &'a Node,
    sig: Option<&'a Node>,
    /// The signature's type and whether it has an explicit full `forall`.
    scheme: Option<(Type, bool)>,
    group: Option<usize>,
    function: bool,
    state: State,
    /// Set while its `rec` group is checked and its recursion is monomorphic.
    rec: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Waiting,
    Checking,
    Done,
}

impl<'a> Checker<'a> {
    pub fn module(&mut self, tree: &'a Node) {
        let decls: Vec<&'a Node> = tree
            .nodes()
            .filter(|n| !matches!(n.kind, K::ModuleHeader | K::ImportDecl))
            .collect();
        let refined = self.declare_types(&decls);
        self.declare_classes(&decls);
        let declared = self.declare_instances(&decls, &refined);
        self.declare_schemas(&decls, &refined);
        for decl in &decls {
            if decl.has_token(K::RefinedKw)
                && let (Some(name), Some(predicate)) = (
                    self.con_token(decl),
                    decl.nodes().find(|n| n.kind == K::Refinement),
                )
            {
                self.predicates.insert(name, predicate);
            }
        }
        self.collect(&decls);
        for i in 0..self.top.items.len() {
            self.check_item(i);
        }
        for i in 0..self.top.items.len() {
            let (decl, scheme) = (self.top.items[i].decl, self.top.items[i].scheme.clone());
            if decl.kind == K::ForeignDecl
                && let Some((scheme, _)) = scheme
            {
                self.foreign_info(decl, &scheme);
                self.settle_operators();
            }
        }
        for (predicate, base) in refined {
            if let Some(expr) = predicate.nodes().next() {
                self.check(expr, &Type::pure_fun(base, Type::prim("Bool")));
            }
            self.settle_operators();
        }
        self.check_tables(&decls);
        self.class_defaults(&decls);
        self.solve_classes(true);
        self.check_instances(&declared);
        self.solve_rows();
        self.recursion();
        self.unknown_types();
    }

    /// Pairs signatures with the definitions right below them and makes
    /// every top-level value known by name.
    fn collect(&mut self, decls: &[&'a Node]) {
        let mut group = 0;
        for (i, decl) in decls.iter().enumerate() {
            match decl.kind {
                K::LetDecl => {
                    // a misplaced signature is reported once and still used
                    let sig = i
                        .checked_sub(1)
                        .map(|j| decls[j])
                        .filter(|s| s.kind == K::Signature && self.pairs(s, decl))
                        .or_else(|| self.misplaced(decls, decl));
                    let own = decl.has_token(K::RecKw).then(|| {
                        group += 1;
                        group
                    });
                    self.add_item(decl, sig, own);
                }
                K::ForeignDecl => {
                    let sig = i
                        .checked_sub(1)
                        .map(|j| decls[j])
                        .filter(|s| s.kind == K::Signature && self.pairs(s, decl))
                        .or_else(|| self.misplaced(decls, decl));
                    let blocked = decl
                        .child_tokens()
                        .find(|t| t.kind == K::Ident)
                        .is_some_and(|t| t.text(self.text) == "blocked");
                    if sig.is_none() && !blocked {
                        let name = foreign_name(decl, self.text).unwrap_or_default();
                        self.error(
                            codes::T004,
                            self.span(decl),
                            format!("foreign `{name}` needs a signature right above it"),
                        );
                    }
                    self.add_item(decl, sig, None);
                }
                K::Signature => {
                    let next = decls.get(i + 1).filter(|d| self.pairs(decl, d));
                    if next.is_none() {
                        self.orphan_signature(decl, decls);
                    }
                }
                K::RecGroup => {
                    group += 1;
                    let members: Vec<&'a Node> = decl
                        .nodes()
                        .filter(|n| n.kind == K::Members)
                        .flat_map(Node::nodes)
                        .collect();
                    for (j, member) in members.iter().enumerate() {
                        match member.kind {
                            K::LetDecl => {
                                let sig = j
                                    .checked_sub(1)
                                    .map(|k| members[k])
                                    .filter(|s| s.kind == K::Signature && self.pairs(s, member));
                                self.add_item(member, sig, Some(group));
                            }
                            K::Signature
                                if !members.get(j + 1).is_some_and(|d| self.pairs(member, d)) =>
                            {
                                self.orphan_signature(member, &members);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// A signature of `decl` that is not right above it.
    fn misplaced(&self, decls: &[&'a Node], decl: &Node) -> Option<&'a Node> {
        decls.iter().enumerate().find_map(|(i, s)| {
            let paired = decls.get(i + 1).is_some_and(|d| self.pairs(s, d));
            (s.kind == K::Signature && self.pairs(s, decl) && !paired).then_some(*s)
        })
    }

    pub(super) fn pairs(&self, sig: &Node, decl: &Node) -> bool {
        let name = signature_name(sig, self.text);
        name.is_some()
            && match decl.kind {
                K::LetDecl => let_parts(decl, self.text).name.as_deref() == name,
                K::ForeignDecl => foreign_name(decl, self.text) == name,
                _ => false,
            }
    }

    /// A signature not followed by its definition; its type still describes
    /// the name for the rest of the module.
    fn orphan_signature(&mut self, sig: &Node, decls: &[&Node]) {
        let name = signature_name(sig, self.text)
            .unwrap_or_default()
            .to_owned();
        let elsewhere = decls.iter().any(|d| {
            (d.kind == K::LetDecl && let_parts(d, self.text).name.as_deref() == Some(&name))
                || (d.kind == K::ForeignDecl && foreign_name(d, self.text) == Some(&name))
        });
        self.lone_signature(sig, elsewhere);
        if !elsewhere && let Some(ty) = sig.nodes().find(|n| is_type(n.kind)) {
            self.env.declare(Namespace::Value, &name);
            let (scheme, _) = self.signature(ty);
            let entity = self.env.entity(Namespace::Value, &name);
            self.env.set_value(entity, scheme);
        }
    }

    fn add_item(&mut self, decl: &'a Node, sig: Option<&'a Node>, group: Option<usize>) {
        let (names, function) = if decl.kind == K::ForeignDecl {
            (
                foreign_name(decl, self.text)
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
                true,
            )
        } else {
            let parts = let_parts(decl, self.text);
            let mut names = Vec::new();
            match (&parts.name, parts.pattern) {
                (Some(name), _) => names.push(name.clone()),
                (None, Some(pattern)) => binders(pattern, self.text, &mut names),
                _ => {}
            }
            (names, parts.is_function())
        };
        let scheme = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .map(|t| self.signature(t));
        let index = self.top.items.len();
        for name in &names {
            self.env.declare(Namespace::Value, name);
            self.top.by_name.insert(name.clone(), index);
            if let Some((scheme, _)) = &scheme {
                let entity = self.env.entity(Namespace::Value, name);
                self.env.set_value(entity, scheme.clone());
            }
        }
        self.top.items.push(Item {
            names,
            decl,
            sig,
            scheme,
            group,
            function,
            state: State::Waiting,
            rec: None,
        });
    }

    /// Checks an item, with its whole `rec` group, in a fresh context.
    fn check_item(&mut self, i: usize) {
        if self.top.items[i].state != State::Waiting {
            return;
        }
        let members: Vec<usize> = match self.top.items[i].group {
            Some(g) => (0..self.top.items.len())
                .filter(|&j| self.top.items[j].group == Some(g))
                .collect(),
            None => vec![i],
        };
        let saved = (
            mem::take(&mut self.locals),
            mem::take(&mut self.tyvars),
            mem::take(&mut self.runtime),
            mem::take(&mut self.stage),
            mem::take(&mut self.subrows),
            mem::take(&mut self.operators),
            mem::replace(&mut self.store.level, 0),
            self.top.current,
            mem::take(&mut self.classes.wanted),
            mem::take(&mut self.classes.givens),
            mem::take(&mut self.classes.steps),
            mem::take(&mut self.regions),
        );
        for &j in &members {
            self.top.items[j].state = State::Checking;
            let monomorphic = matches!(self.top.items[j].scheme, Some((_, false)));
            if self.top.items[j].group.is_some() && monomorphic {
                let name = self.top.items[j].names.first().cloned().unwrap_or_default();
                self.top.items[j].rec = Some(self.new_rec(&name));
            }
        }
        for &j in &members {
            self.top.current = Some(j);
            self.check_definition(j);
            self.settle_operators();
        }
        self.solve_rows();
        self.solve_classes(true);
        for &j in &members {
            if let Some(rec) = self.top.items[j].rec.take() {
                self.finish_rec(rec);
            }
            self.top.items[j].state = State::Done;
        }
        (
            self.locals,
            self.tyvars,
            self.runtime,
            self.stage,
            self.subrows,
            self.operators,
            self.store.level,
            self.top.current,
            self.classes.wanted,
            self.classes.givens,
            self.classes.steps,
            self.regions,
        ) = saved;
    }

    fn check_definition(&mut self, i: usize) {
        let item = &self.top.items[i];
        let (decl, scheme, rec) = (item.decl, item.scheme.clone(), item.rec);
        if decl.kind != K::LetDecl {
            return;
        }
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            let mark = self.locals.len();
            self.pattern_let(decl, &parts);
            let bound: Vec<_> = self.locals.drain(mark..).collect();
            for local in bound {
                let entity = self.env.entity(Namespace::Value, &local.name);
                let ty = self.zonk(&local.ty);
                self.env.set_value(entity, ty);
            }
            return;
        };
        let entity = self.env.entity(Namespace::Value, &name);
        if let Some((scheme, _)) = scheme {
            let runtime = self.check_signed(decl, &parts, &scheme, rec, None);
            self.polymorphic_runtime(&name, &scheme, runtime, decl);
            if let Some(binder) = parts.pattern {
                self.note_type(self.span(binder), &scheme);
            }
        } else {
            let ty = self.unsigned(decl, &parts, &name);
            if let Some(binder) = parts.pattern {
                self.note_type(self.span(binder), &ty);
            }
            self.env.set_value(entity, ty);
        }
    }

    /// The type of a top-level value of this module at a use.
    pub fn top_value(&mut self, name: &str, span: Span) -> Type {
        let entity = self.env.entity(Namespace::Value, name);
        let Some(&i) = self.top.by_name.get(name) else {
            let scheme = self.env.value(&entity).cloned();
            return scheme.map_or(Type::Error, |s| self.instantiate(&s, span));
        };
        if let Some(current) = self.top.current {
            self.top.edges.push((current, i, span));
        }
        if let Some(rec) = self.top.items[i].rec {
            let scheme = self.env.value(&entity).cloned().unwrap_or(Type::Error);
            let ty = self.instantiate(&scheme, span);
            self.recs[rec].uses.push((ty.clone(), span));
            return ty;
        }
        if self.env.value(&entity).is_none() && self.top.items[i].state == State::Waiting {
            self.check_item(i);
        }
        let scheme = self.env.value(&entity).cloned();
        scheme.map_or(Type::Error, |s| self.instantiate(&s, span))
    }

    /// Reports recursion that is not declared with `rec`.
    fn recursion(&mut self) {
        let n = self.top.items.len();
        let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(from, to, _) in &self.top.edges {
            if !next[from].contains(&to) {
                next[from].push(to);
            }
        }
        let mut reported = vec![false; n];
        for start in 0..n {
            if reported[start] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(start, &next) else {
                continue;
            };
            let items = &self.top.items;
            let group = items[start].group;
            if group.is_some() && cycle.iter().all(|&i| items[i].group == group) {
                continue;
            }
            for &i in &cycle {
                reported[i] = true;
            }
            let after = cycle.get(1).copied().unwrap_or(start);
            let span = self
                .top
                .edges
                .iter()
                .find(|(f, t, _)| *f == start && *t == after)
                .map(|e| e.2)
                .expect("an edge of the cycle");
            let names: Vec<String> = cycle
                .iter()
                .map(|&i| format!("`{}`", items[i].names.join("`, `")))
                .collect();
            let values = cycle.iter().any(|&i| !items[i].function);
            let (code, message) = match (values, &names[..]) {
                (true, _) => (
                    codes::T012,
                    format!(
                        "{} would be evaluated while being defined; only functions can be recursive",
                        names.join(" and ")
                    ),
                ),
                (false, [one]) => (
                    codes::T017,
                    format!("{one} uses itself: write `let rec` for a recursive function"),
                ),
                (false, _) => (
                    codes::T017,
                    format!(
                        "{} use each other: define mutually recursive functions in one `rec {{ … }}` group",
                        names.join(", ")
                    ),
                ),
            };
            self.error(code, span, message);
        }
    }

    /// A top-level value whose type is still not fully known after the
    /// whole module is checked, e.g. a runtime result nothing pins down.
    fn unknown_types(&mut self) {
        for i in 0..self.top.items.len() {
            if self.top.items[i].sig.is_some() {
                continue;
            }
            for name in self.top.items[i].names.clone() {
                let entity = self.env.entity(Namespace::Value, &name);
                let Some(ty) = self.env.value(&entity).map(|t| self.zonk(t)) else {
                    continue;
                };
                if ty.contains(&mut |t| matches!(t, Type::Meta(_))) {
                    let shown = self.show(&ty);
                    self.error(
                        codes::T013,
                        self.span(self.top.items[i].decl),
                        format!(
                            "the type of `{name}` is not fully known (`{shown}`) and cannot be generalized; give it a signature"
                        ),
                    );
                }
            }
        }
    }
}

fn foreign_name<'t>(decl: &Node, text: &'t str) -> Option<&'t str> {
    decl.child_tokens()
        .filter(|t| t.kind == K::Ident)
        .nth(1)
        .map(|t| t.text(text))
}

fn binders(pattern: &Node, text: &str, out: &mut Vec<String>) {
    match pattern.kind {
        K::VarPat => out.extend(
            pattern
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned()),
        ),
        K::RecordPatField if !pattern.has_token(K::Eq) => out.extend(
            pattern
                .child_tokens()
                .find(|t| t.kind == K::Ident)
                .map(|t| t.text(text).to_owned()),
        ),
        _ => {
            for child in pattern.nodes() {
                binders(child, text, out);
            }
        }
    }
}
