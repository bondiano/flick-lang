//! `scope` expressions and the calls that acquire into their owners.

use std::mem;

use super::{Need, Scope, scope};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::convert::TyVar;
use crate::types::kinds::Kind;
use crate::types::lang::Lang;
use crate::types::ty::{Row, Type};
use crate::types::typing::range;

impl Checker<'_> {
    /// `scope [s] { body }`: the body runs with a fresh region as the owner
    /// of what it acquires, which is released when the body ends. A named
    /// scope binds `s` as the region in types and as its owner in values.
    pub fn scope_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let span = node
            .child_tokens()
            .find(|t| t.kind == K::ScopeKw)
            .map_or_else(|| self.span(node), |t| self.token_span(t));
        let empty = Type::empty_row();
        if self.runtime_allowed(span) {
            self.perform(&empty, &empty, span);
        }
        let name = node.child_tokens().find(|t| t.kind == K::Ident);
        self.store.level += 1;
        let region = self.store.skolem(name.map_or("s", |n| n.text(self.text)));
        let Type::Skolem(id) = region else {
            unreachable!("a skolem")
        };
        self.regions.scopes.push(Scope {
            region: id,
            span,
            node: node.range(),
            outer: self.stage.services.clone(),
            parent: self.regions.nest.last().copied(),
        });
        let index = self.regions.scopes.len() - 1;
        self.regions.open.push(index);
        self.regions.nest.push(index);
        let (locals, tyvars) = (self.locals.len(), self.tyvars.len());
        if let Some(token) = name {
            let name = token.text(self.text);
            self.tyvars.push(TyVar {
                name: name.to_owned(),
                ty: region.clone(),
                kind: Kind::Region,
            });
            let owner = Type::app(Type::lang(Lang::Owner), region.clone());
            self.push_local(name, token.start, owner);
        }
        let needed = self.fresh();
        let mut stage = self.stage.clone();
        stage.services = needed.clone();
        stage.returns = None;
        let outer = mem::replace(&mut self.stage, stage);
        let body = node.nodes().next();
        let ty = match (body, expected) {
            (Some(body), Some(expected)) => {
                self.check(body, expected);
                expected.clone()
            }
            (Some(body), None) => self.infer(body),
            (None, _) => Type::Error,
        };
        let inner = mem::replace(&mut self.stage, outer);
        self.stage.calls = inner.calls;
        if self.stage.pure.is_none() {
            let outer = self.stage.services.clone();
            self.scope_row(&needed, &outer, &region, span);
        }
        self.settle_needs(index);
        self.locals.truncate(locals);
        self.tyvars.truncate(tyvars);
        self.regions.open.pop();
        self.regions.nest.pop();
        self.store.level -= 1;
        if let Err(skolem) = self.store.leave(&ty) {
            let shown = self.show(&ty);
            let span = body.map_or(span, |b| self.span(b));
            let head = format!("the value of this `scope` has type `{shown}`");
            self.region_escape(skolem, &head, span);
        }
        ty
    }

    /// A call needing `Scope r` inside a `scope`: `r` not known yet is the
    /// region of the innermost scope. The services the call needs from its stage are the
    /// result: without `Scope` when a scope of the stage provides it, since
    /// each scope provides `Scope` of its own region only.
    pub fn lexical_owner(&mut self, services: &Type, span: Span) -> Type {
        let Some(&inner) = self.regions.open.last() else {
            return services.clone();
        };
        let mut norm = self.store.normalize(services);
        let Some(needed) = norm.get(scope()).cloned() else {
            return services.clone();
        };
        let own = self.regions.open[self.stage.scopes.min(self.regions.open.len())..].to_vec();
        if own.is_empty() {
            // no scope of this stage provides it: the stage passes `Scope` on to
            // what runs it, which provides the region its row names
            if let [region] = needed.spine().1[..]
                && let Type::Meta(_) = self.store.resolve(region)
            {
                match (self.declared_scope(), &mut self.regions.defaults) {
                    (Some(owner), _) => {
                        let _ = self.unify(region, &owner);
                    }
                    (None, Some(defaults)) => defaults.push((region.clone(), inner)),
                    (None, None) => {
                        let owner = Type::Skolem(self.regions.scopes[inner].region);
                        let _ = self.unify(region, &owner);
                    }
                }
            }
            return services.clone();
        }
        let open = self
            .regions
            .open
            .iter()
            .map(|&i| (self.regions.scopes[i].region, self.regions.scopes[i].node))
            .collect();
        self.typing
            .pending_owners
            .push((range(span), needed.clone(), open));
        let [region] = needed.spine().1[..] else {
            return services.clone();
        };
        if let Type::Meta(_) = self.store.resolve(region) {
            let owner = Type::Skolem(self.regions.scopes[inner].region);
            let _ = self.unify(region, &owner);
        }
        self.regions.needs.push(Need {
            region: region.clone(),
            inner,
            own,
            span,
            site: self.stage.site,
        });
        norm.entries.retain(|(label, _)| label != scope());
        norm.into_type()
    }

    /// Gives the tasks an `Eff` argument left without a region the
    /// innermost scope, once the call has all its arguments.
    pub fn settle_defaults(&mut self, defaults: Vec<(Type, usize)>) {
        for (region, inner) in defaults {
            if let Type::Meta(_) = self.store.resolve(&region) {
                let owner = Type::Skolem(self.regions.scopes[inner].region);
                let _ = self.unify(&region, &owner);
            }
        }
    }

    /// The region of the `Scope` the current stage's row names, once known:
    /// a task checked against `Eff {Scope s | r} e a` acquires into `s`.
    fn declared_scope(&self) -> Option<Type> {
        let declared = self.store.normalize(&self.stage.services);
        let scope = declared.get(scope())?;
        let [region] = scope.spine().1[..] else {
            return None;
        };
        let region = self.store.resolve(region);
        (!matches!(region, Type::Meta(_))).then_some(region)
    }

    /// At the end of scope `index`: the needs it is the innermost scope of
    /// that no scope of their stage provides go to the services around
    /// those scopes.
    fn settle_needs(&mut self, index: usize) {
        let needs: Vec<Need> = self
            .regions
            .needs
            .extract_if(.., |n| n.inner == index)
            .collect();
        for need in needs {
            let Some(&first) = need.own.first() else {
                continue;
            };
            let provided = need.own.iter().any(|&i| {
                self.store.resolve(&need.region) == Type::Skolem(self.regions.scopes[i].region)
            });
            if !provided {
                let row = Type::Row(Row {
                    entries: vec![(
                        scope().to_owned(),
                        Type::app(Type::lang(Lang::Scope), need.region.clone()),
                    )],
                    tail: None,
                });
                let outer = self.regions.scopes[first].outer.clone();
                self.sub_row(&row, &outer, need.site, need.span, false);
            }
        }
    }
}
