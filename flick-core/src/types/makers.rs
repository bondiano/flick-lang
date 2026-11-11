//! What makes an atom, an action or a view: the makers of the library, the
//! values its modules mark `@maker`, and every function that calls one
//! outside the task of a handler, in this module or in one it imports,
//! whose interface lists them. The plugin that watches the tag is told where
//! a function names one, and refuses it in a render (`FLICK-U006`).

use std::collections::HashMap;
use std::mem;

use super::ty::{Norm, Type};
use super::typing::Ref;
use super::ui::members;
use super::{Checker, Range, let_parts};
use crate::iface::{self, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};
use flick_plugin::{CallSite, Enclosing, Tagged};

impl Checker<'_> {
    /// The top-level functions of this module that make an atom, an action
    /// or a view, each with the maker of the library it ends at.
    pub fn makers(&mut self, decls: &[&Node]) {
        for name in marked(decls, self.text) {
            let maker = format!("{}.{name}", self.env.module);
            self.typing.makers.insert(name, maker);
        }
        let mut functions: Vec<(String, &Node)> = Vec::new();
        for decl in members(decls).filter(|d| d.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let syntactic = parts.is_function();
            let (Some(name), Some(body)) = (parts.name, parts.body) else {
                continue;
            };
            // what a component's setup makes, it makes once per instance
            let setup = self
                .env
                .value(&self.env.entity(Namespace::Value, &name))
                .is_some_and(needs_setup);
            if setup {
                continue;
            }
            if syntactic {
                functions.push((name, body));
            } else if let Some(alias) = alias(body)
                && let Some(Ref::Value { module, name: of }) = self.typing.names.get(&alias.range())
                && let Some(maker) = self.maker(module, of)
            {
                // `let new_state = Atom.make`; an application, such as a
                // serializable family, gives what its maker made
                self.typing.makers.insert(name, maker);
            }
        }
        // a function calling one found in this pass is found in the next
        loop {
            let mut changed = false;
            for (name, body) in &functions {
                if self.typing.makers.contains_key(name) {
                    continue;
                }
                let mut found = Vec::new();
                self.made(body, &mut found);
                if let Some((.., maker)) = found.into_iter().next() {
                    self.typing.makers.insert(name.clone(), maker);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// The maker of the library that the value `module.name` is or calls.
    fn maker(&self, module: &str, name: &str) -> Option<String> {
        if module == self.env.module {
            return self.typing.makers.get(name).cloned();
        }
        let interface = self.env.interface(module)?;
        interface
            .tagged(iface::MAKER, name)
            .map(|_| format!("{module}.{name}"))
    }

    /// The makers named under `node`, outside the tasks of handlers: where,
    /// the name as written, and the maker of the library it ends at.
    pub(super) fn made(&self, node: &Node, out: &mut Vec<(Range, String, String)>) {
        if node.kind == K::TaskExpr {
            return;
        }
        if let Some(Ref::Value { module, name }) = self.typing.names.get(&node.range())
            && let Some(maker) = self.maker(module, name)
        {
            let written = if *module == self.env.module {
                name.clone()
            } else {
                format!("{module}.{name}")
            };
            out.push((node.range(), written, maker));
        }
        for child in node.nodes() {
            self.made(child, out);
        }
    }
}

impl Checker<'_> {
    /// The places that name a maker inside a function, for the plugin that
    /// watches the tag: each with the functions around it up to the nearest
    /// `task`, whose rows say what kind of function it is. A view renders
    /// again at any time, and its plugin knows a render by its row.
    pub fn tagged_references(&mut self, decls: &[&Node]) {
        if !self.options.watched.tag(iface::MAKER) {
            return;
        }
        let heads: HashMap<Range, Type> = (self.typing.pending_types.iter())
            .map(|(at, ty)| (*at, ty.clone()))
            .collect();
        let mut found = Vec::new();
        for decl in members(decls).filter(|d| d.kind == K::LetDecl) {
            let parts = let_parts(decl, self.text);
            let function = Some((parts.name, parts.body)).filter(|_| !parts.params.is_empty());
            let function = function.and_then(|(name, body)| Some((name?, body?)));
            let Some((name, body)) = function else {
                self.tagged_under(decl, &heads, &mut Vec::new(), &mut found);
                continue;
            };
            // the body of a top-level function is inside each of its arrows
            let mut around = Vec::new();
            let entity = self.env.entity(Namespace::Value, &name);
            let mut ty = self.env.value(&entity).cloned();
            while let Some(Type::Fun { result, .. }) = ty.as_ref().map(|t| self.bare(t)) {
                around.extend(ty.as_ref().and_then(|t| self.enclosing(t)));
                ty = Some(*result);
            }
            self.tagged_under(body, &heads, &mut around, &mut found);
        }
        self.typing.call_sites.extend(found);
    }

    fn tagged_under(
        &self,
        node: &Node,
        heads: &HashMap<Range, Type>,
        around: &mut Vec<Enclosing>,
        out: &mut Vec<CallSite>,
    ) {
        if node.kind == K::TaskExpr {
            // what a task does runs later, outside the functions around it
            let fresh = &mut Vec::new();
            return (node.nodes()).for_each(|child| self.tagged_under(child, heads, fresh, out));
        }
        let entered = (node.kind == K::FunExpr)
            .then(|| node.child_tokens().find(|t| t.kind == K::FunKw))
            .flatten()
            .and_then(|keyword| heads.get(&(keyword.start, keyword.end)))
            .and_then(|ty| self.enclosing(ty));
        let depth = around.len();
        around.extend(entered);
        if let Some(Ref::Value { module, name }) = self.typing.names.get(&node.range())
            && let Some(reaches) = self.maker(module, name)
            && !around.is_empty()
        {
            let (start, end) = node.range();
            let named = if *module == self.env.module {
                name.clone()
            } else {
                format!("{module}.{name}")
            };
            out.push(CallSite {
                module: module.clone(),
                name: name.clone(),
                span: flick_plugin::Span { start, end },
                arguments: Vec::new(),
                refinement: None,
                binding: None,
                generated: start >= self.generated,
                tagged: Some(Tagged {
                    tag: iface::MAKER.to_owned(),
                    named,
                    reaches,
                    inside: around.iter().rev().cloned().collect(),
                }),
            });
        }
        for child in node.nodes() {
            self.tagged_under(child, heads, around, out);
        }
        around.truncate(depth);
    }

    /// The function `ty` is, as a place inside it is told of it: the
    /// services of its arrow by name, and whether its row is open.
    fn enclosing(&self, ty: &Type) -> Option<Enclosing> {
        let Type::Fun { effects, .. } = self.bare(ty) else {
            return None;
        };
        let services = effects.map(|e| self.store.normalize(&e.services));
        let names = |row: &Norm| {
            (row.entries.iter())
                .filter_map(|(l, _)| l.name().map(str::to_owned))
                .collect()
        };
        Some(Enclosing {
            services: services.as_ref().map(names).unwrap_or_default(),
            open: services.is_some_and(|row| !row.tails.is_empty()),
        })
    }
}

/// The one name a body is, inside parentheses.
fn alias(body: &Node) -> Option<&Node> {
    let mut statements = body.nodes();
    let (Some(mut node), None) = (statements.next(), statements.next()) else {
        return None;
    };
    while node.kind == K::ParenExpr {
        node = node.nodes().next()?;
    }
    matches!(node.kind, K::NameExpr | K::Path).then_some(node)
}

/// Whether a stage of `ty` needs the service `Setup` of a component.
fn needs_setup(ty: &Type) -> bool {
    let Some(setup) = lang::label(Lang::Setup) else {
        return false;
    };
    let mut found = false;
    ty.visit(&mut |t| {
        if let Type::Fun {
            effects: Some(effects),
            ..
        } = t
        {
            effects.services.visit(&mut |s| {
                found |= matches!(s, Type::Row(row) if row.entries.iter().any(|(l, _)| l == setup));
            });
        }
    });
    found
}

/// The values `decls` mark `@maker`: the makers of the library.
fn marked(decls: &[&Node], text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = false;
    for decl in decls {
        if decl.kind == K::Attribute {
            let mut words = (decl.child_tokens()).filter(|t| t.kind == K::Ident);
            pending |= words.next().is_some_and(|w| w.text(text) == "maker");
            continue;
        }
        if mem::take(&mut pending)
            && let Some(name) = decl.child_tokens().find(|t| t.kind == K::Ident)
        {
            out.push(name.text(text).to_owned());
        }
    }
    out
}
