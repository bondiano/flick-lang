//! What makes an atom, an action or a view: the makers of the library, the
//! values its modules mark `@maker`, and every function that calls one
//! outside the task of a handler, in this module or in one it imports,
//! whose interface lists them. The plugin that watches the tag is told where
//! a function names one, and refuses it in a render (`FLICK-U006`).

use std::collections::HashMap;
use std::mem;

use super::Elem;
use super::ty::{Norm, Type};
use super::typing::Ref;
use super::ui::members;
use super::{Checker, Range, let_parts};
use crate::iface::{self, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use flick_plugin::{CallSite, Enclosing, Lacking, Tagged};

impl Checker<'_> {
    /// The services marked `@owner`: this module's, and those of the modules
    /// whose interfaces it knows.
    fn declare_owners(&mut self, decls: &[&Node]) {
        let module = self.env.module.clone();
        let own = marked(decls, self.text, iface::OWNER, K::Con);
        let mut owners: Vec<String> = own.iter().map(|name| format!("{module}.{name}")).collect();
        for interface in self.env.known_interfaces() {
            for (name, _) in interface.tags.get(iface::OWNER).into_iter().flatten() {
                owners.push(format!("{}.{name}", interface.module));
            }
        }
        self.typing.owner_services = own;
        self.owners = owners;
    }

    /// The top-level functions of this module that make an atom, an action
    /// or a view, each with the maker of the library it ends at.
    pub fn makers(&mut self, decls: &[&Node]) {
        self.declare_owners(decls);
        for name in marked(decls, self.text, iface::MAKER, K::Ident) {
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
            // what is made under a service that owns it belongs to the
            // handler of the service, as a component's setup makes once per
            // instance: such a function is no maker to those that name it
            let entity = self.env.entity(Namespace::Value, &name);
            if (self.env.value(&entity)).is_some_and(|ty| needs_owner(ty, &self.owners)) {
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
                lacking: None,
            });
        }
        for child in node.nodes() {
            self.tagged_under(child, heads, around, out);
        }
        around.truncate(depth);
    }

    /// Tells the plugins that watch a service of the row of `bound`, the
    /// function expected at `span`, what the code there asks for beyond it:
    /// `what` as the refusal words it, and the services as shown, their
    /// labels and the errors.
    pub(crate) fn lacking_under(
        &mut self,
        bound: &Type,
        span: Span,
        what: &str,
        (services, labels, errors): (&[String], &[Elem], &[String]),
    ) {
        let Some(allowed) = self.enclosing(bound) else {
            return;
        };
        for service in &allowed.services {
            let watched = (service.rsplit_once('.'))
                .filter(|(module, name)| self.options.watched.holds(module, name));
            let Some((module, name)) = watched else {
                continue;
            };
            self.typing.call_sites.push(CallSite {
                module: module.to_owned(),
                name: name.to_owned(),
                span: flick_plugin::Span {
                    start: span.start,
                    end: span.end,
                },
                arguments: Vec::new(),
                refinement: None,
                binding: None,
                generated: span.start >= self.generated,
                tagged: None,
                lacking: Some(Lacking {
                    allowed: allowed.clone(),
                    what: what.to_owned(),
                    services: services.to_vec(),
                    labels: (labels.iter())
                        .filter_map(|l| l.name().map(str::to_owned))
                        .collect(),
                    errors: errors.to_vec(),
                }),
            });
        }
    }

    /// The function `ty` is, as a place inside it is told of it: the
    /// services of its arrow by name, and whether its row is open.
    pub(super) fn enclosing(&self, ty: &Type) -> Option<Enclosing> {
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

/// Whether a stage of `ty` needs one of the services `owners`.
fn needs_owner(ty: &Type, owners: &[String]) -> bool {
    let mut found = false;
    ty.visit(&mut |t| {
        if let Type::Fun {
            effects: Some(effects),
            ..
        } = t
        {
            effects.services.visit(&mut |s| {
                found |= matches!(s, Type::Row(row) if row.entries.iter().any(|(l, _)| owners.iter().any(|o| l == o.as_str())));
            });
        }
    });
    found
}

/// The names of the declarations an attribute `@tag` stands before: each by
/// its first token of `kind`, an identifier of a value or the name of a type.
pub(super) fn marked(decls: &[&Node], text: &str, tag: &str, kind: K) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending = false;
    for decl in decls {
        if decl.kind == K::Attribute {
            let mut words = (decl.child_tokens()).filter(|t| t.kind == K::Ident);
            pending |= words.next().is_some_and(|w| w.text(text) == tag);
            continue;
        }
        if mem::take(&mut pending)
            && let Some(name) = decl.child_tokens().find(|t| t.kind == kind)
        {
            out.push(name.text(text).to_owned());
        }
    }
    out
}
