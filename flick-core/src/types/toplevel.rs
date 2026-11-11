//! The top level of a module: declarations are order-independent, so a value
//! without a signature is inferred when it is first used. Recursion must be
//! explicit: a function that uses itself is `let rec`, functions that
//! use each other share a `rec { … }` group, and values never recurse.

use std::collections::HashMap;
use std::mem;

use super::Checker;
use super::bindings::{let_parts, pattern_names, signature_name};
use super::convert::is_type;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

mod cycles;

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

impl TopLevel<'_> {
    pub(super) fn has_signature(&self, name: &str) -> bool {
        self.by_name
            .get(name)
            .is_some_and(|&index| self.items[index].sig.is_some())
    }
    /// The name of the top-level definition whose declaration holds `offset`:
    /// the user's code of a block is in the declaration the block gave, which
    /// stands after the module's own text, from `generated` on.
    pub fn definition_at(&self, offset: u32, generated: u32) -> Option<&str> {
        let holds = |i: &&Item| {
            let (start, end) = i.decl.range();
            start <= offset && offset < end
        };
        let spliced = |i: &&Item| covers(i.decl, offset, generated);
        let item = (self.items.iter().find(holds)).or_else(|| self.items.iter().find(spliced))?;
        item.names.first().map(String::as_str)
    }

    /// Whether the module declares the value `name` at its top level.
    pub fn declares(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }
}

/// Whether a node of the user's code under `node`, before `generated`,
/// holds `offset`; a generated node may range from the user's code to its own.
fn covers(node: &Node, offset: u32, generated: u32) -> bool {
    node.nodes().any(|n| {
        let (start, end) = n.range();
        (start <= offset && offset < end && end <= generated) || covers(n, offset, generated)
    })
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
        if let Some(header) = tree.nodes().find(|n| n.kind == K::ModuleHeader) {
            self.imported_conflicts(self.span(header));
        }
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
                self.check(expr, &Type::pure_fun(base, Type::lang(Lang::Bool)));
            }
            self.settle_operators();
        }
        self.record_atom_bindings(&decls);
        self.class_defaults(&decls);
        self.solve_classes(true);
        self.check_instances(&declared);
        self.solve_rows();
        self.makers(&decls);
        self.servers(&decls);
        self.tagged_references(&decls);
        self.labelled(&decls);
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
                    let blocked = foreign_words(decl)
                        .0
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
                    // a block below that gave no code is reported already
                    let block = decls.get(i + 1).is_some_and(|d| d.kind == K::PluginDecl);
                    if next.is_none() && !block {
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

    /// The signature of the top-level declaration around `span`.
    pub(super) fn signature_around(&self, span: Span) -> Option<&'a Node> {
        self.top.items.iter().find_map(|item| {
            let (start, end) = item.decl.range();
            (start <= span.start && span.end <= end)
                .then_some(item.sig)
                .flatten()
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
                (None, Some(pattern)) => pattern_names(pattern, self.text, &mut names),
                _ => {}
            }
            (names, parts.is_function())
        };
        let scheme = sig
            .and_then(|s| s.nodes().find(|n| is_type(n.kind)))
            .map(|t| {
                let (scheme, explicit) = self.signature(t);
                self.redundant_owner_constraints(decl, t, &scheme);
                (self.with_owner_constraints(decl, scheme), explicit)
            });
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
        // what the branches of a `match` around the name that asked for the
        // item learned is nothing of the item's either
        let outer = mem::take(&mut self.cx);
        let saved = (
            mem::replace(&mut self.store.level, 0),
            mem::take(&mut self.store.equalities),
            self.top.current,
            mem::take(&mut self.classes.wanted),
            mem::take(&mut self.classes.givens),
            mem::take(&mut self.classes.steps),
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
        let recs: Vec<usize> = members
            .iter()
            .filter_map(|&j| self.top.items[j].rec.take())
            .collect();
        self.finish_recs(&recs);
        for &j in &members {
            self.top.items[j].state = State::Done;
        }
        self.cx = outer;
        (
            self.store.level,
            self.store.equalities,
            self.top.current,
            self.classes.wanted,
            self.classes.givens,
            self.classes.steps,
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
            let mark = self.cx.locals.len();
            self.pattern_let(decl, &parts, None);
            let bound: Vec<_> = self.cx.locals.drain(mark..).collect();
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
            self.polymorphic_signature(&name, &scheme, runtime, decl, &parts);
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

    /// Where the top-level declaration being checked ends.
    pub fn current_end(&self) -> Option<u32> {
        self.top.current.map(|i| self.top.items[i].decl.range().1)
    }

    /// The type the signature of this module's top-level value `name` gives.
    pub(super) fn signed_type(&self, name: &str) -> Option<Type> {
        let i = *self.top.by_name.get(name)?;
        self.top.items[i].scheme.as_ref().map(|(s, _)| s.clone())
    }

    /// The span of the signature of this module's top-level value `name`.
    pub fn signature_span(&self, name: &str) -> Option<Span> {
        let i = *self.top.by_name.get(name)?;
        self.top.items[i].sig.map(|sig| self.span(sig))
    }

    /// The type the signature of the top-level value `name` writes, without
    /// the parentheses a plugin's code puts around the user's.
    pub fn signature_type_span(&self, name: &str) -> Option<Span> {
        let i = *self.top.by_name.get(name)?;
        let mut ty = self.top.items[i].sig?.nodes().find(|n| is_type(n.kind))?;
        while ty.kind == K::ParenType {
            ty = ty.nodes().next()?;
        }
        Some(self.span(ty))
    }

    /// The type of a top-level value of this module at a use.
    pub fn top_value(&mut self, name: &str, span: Span) -> Type {
        let entity = self.env.entity(Namespace::Value, name);
        let Some(&i) = self.top.by_name.get(name) else {
            let scheme = self.env.value(&entity).cloned();
            return scheme.map_or(Type::Error, |s| {
                self.instantiate(&self.live_scheme(&s), span)
            });
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
    foreign_words(decl).1.map(|t| t.text(text))
}
