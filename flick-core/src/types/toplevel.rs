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

mod cycles;
mod lookup;

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
    /// Whether `let rec` on this non-function already reported `FLICK-T012`.
    rec_value_reported: bool,
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
        self.root = Some(tree);
        self.typing.suspended = super::suspended::marks(tree, self.text);
        let decls: Vec<&'a Node> = tree
            .nodes()
            .filter(|n| !matches!(n.kind, K::ModuleHeader | K::ImportDecl))
            .collect();
        self.declare_types(&decls);
        self.declare_cells(&decls);
        self.declare_labels(&decls);
        self.check_backend_attributes(tree, &decls);
        self.declare_classes(&decls);
        if let Some(header) = tree.nodes().find(|n| n.kind == K::ModuleHeader) {
            self.imported_conflicts(self.span(header));
        }
        let declared = self.declare_instances(&decls);
        self.declare_schemas(&decls);
        self.declare_checks(&decls);
        self.declare_invariants(&decls);
        self.collect(&decls);
        for i in 0..self.top.items.len() {
            self.check_item(i);
        }
        for i in 0..self.top.items.len() {
            let (decl, scheme) = (self.top.items[i].decl, self.top.items[i].scheme.clone());
            if decl.kind == K::ForeignDecl {
                if let Some((scheme, _)) = scheme {
                    self.foreign_info(decl, &scheme);
                    self.settle_operators();
                } else {
                    self.check_foreign_meta_words(decl);
                }
            } else if decl.kind == K::LetDecl && decl.nodes().any(|n| n.kind == K::NativeOverride) {
                self.check_native_overrides(decl, scheme.as_ref().map(|(s, _)| s));
            }
        }
        for decl in decls.iter().filter(|d| d.kind == K::TypeDecl) {
            self.check_type_native(decl);
        }
        self.check_represented_uses(&decls);
        self.record_atom_bindings(&decls);
        self.class_defaults(&decls);
        self.solve_rows();
        self.solve_classes(true);
        self.check_instances(&declared);
        self.check_proofs(&decls);
        self.check_laws(&decls);
        self.check_contracts(&decls);
        self.solve_rows();
        self.makers(&decls);
        self.servers(&decls);
        self.tagged_references(&decls);
        self.labelled(&decls);
        self.check_host_exports(&decls);
        self.recursion();
        self.unknown_types();
        self.func_ref_dictionaries();
    }

    /// Pairs signatures with the definitions right below them and makes
    /// every top-level value known by name.
    fn collect(&mut self, decls: &[&'a Node]) {
        let mut group = 0;
        for (i, decl) in decls.iter().enumerate() {
            match decl.kind {
                K::LetDecl => {
                    // a misplaced signature is reported once and still used
                    let sig = prev_non_attr(decls, i)
                        .filter(|s| s.kind == K::Signature && self.pairs(s, decl))
                        .or_else(|| self.misplaced(decls, decl));
                    let own = decl.has_token(K::RecKw).then(|| {
                        group += 1;
                        group
                    });
                    if sig.is_none() && decl.nodes().any(|n| n.kind == K::NativeOverride) {
                        self.missing_native_signature(decl);
                    }
                    self.add_item(decl, sig, own);
                }
                K::ForeignDecl => {
                    let sig = prev_non_attr(decls, i)
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
                    let following = next_non_attr(decls, i);
                    let next = following.filter(|d| self.pairs(decl, d));
                    // a block below that gave no code is reported already
                    let block = following.is_some_and(|d| d.kind == K::PluginDecl);
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
                                let sig = prev_non_attr(&members, j)
                                    .filter(|s| s.kind == K::Signature && self.pairs(s, member));
                                if sig.is_none()
                                    && member.nodes().any(|n| n.kind == K::NativeOverride)
                                {
                                    self.missing_native_signature(member);
                                }
                                self.add_item(member, sig, Some(group));
                            }
                            K::Signature
                                if !next_non_attr(&members, j)
                                    .is_some_and(|d| self.pairs(member, d)) =>
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

    fn missing_native_signature(&mut self, decl: &Node) {
        let name = let_parts(decl, self.text).name.unwrap_or_default();
        let end = decl
            .nodes()
            .find(|n| n.kind == K::VarPat)
            .map_or_else(|| decl.range().1, |v| v.range().1);
        self.error(
            codes::T004,
            Span::new(self.file, decl.range().0, end),
            format!("function `{name}` with `@native` overrides needs a signature right above it"),
        );
    }

    /// A signature of `decl` that is not right above it.
    fn misplaced(&self, decls: &[&'a Node], decl: &Node) -> Option<&'a Node> {
        decls.iter().enumerate().find_map(|(i, s)| {
            let paired = next_non_attr(decls, i).is_some_and(|d| self.pairs(s, d));
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

    /// The top-level declaration containing `span`: a paired signature and
    /// definition share one owner, and any other top-level declaration (such
    /// as a `class`, `instance`, or `type` declaration) owns only itself.
    pub(super) fn owner_of(&self, span: Span) -> Option<(u32, u32)> {
        let within = |node: &Node| {
            let (start, end) = node.range();
            (start <= span.start && span.end <= end)
                || (span.start < self.generated && covers(node, span.start, self.generated))
        };
        if let Some(item) = self
            .top
            .items
            .iter()
            .find(|item| item.sig.is_some_and(within) || within(item.decl))
        {
            return Some(item.decl.range());
        }
        self.root?
            .nodes()
            .find(|node| within(node))
            .map(Node::range)
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
        let (names, function, rec_value_reported) = if decl.kind == K::ForeignDecl {
            (
                foreign_name(decl, self.text)
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
                true,
                false,
            )
        } else {
            let parts = let_parts(decl, self.text);
            let mut names = Vec::new();
            match (&parts.name, parts.pattern) {
                (Some(name), _) => names.push(name.clone()),
                (None, Some(pattern)) => pattern_names(pattern, self.text, &mut names),
                _ => {}
            }
            let function = parts.is_function();
            let rec_value_reported = parts.rec && !function && parts.name.is_some();
            (names, function, rec_value_reported)
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
            rec_value_reported,
            state: State::Waiting,
            rec: None,
        });
    }

    /// Checks an item, with its whole `rec` group, in a fresh context.
    pub(super) fn check_item(&mut self, i: usize) {
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
            mem::take(&mut self.store.row_equalities),
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
            self.store.row_equalities,
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
}

pub(super) fn foreign_name<'t>(decl: &Node, text: &'t str) -> Option<&'t str> {
    foreign_words(decl).1.map(|t| t.text(text))
}

fn prev_non_attr<'a>(decls: &[&'a Node], i: usize) -> Option<&'a Node> {
    decls[..i]
        .iter()
        .rev()
        .copied()
        .find(|n| n.kind != K::Attribute)
}

pub(super) fn next_non_attr<'a>(decls: &[&'a Node], i: usize) -> Option<&'a Node> {
    decls[i + 1..]
        .iter()
        .copied()
        .find(|n| n.kind != K::Attribute)
}
