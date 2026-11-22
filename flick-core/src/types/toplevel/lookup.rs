//! Queries over top-level items: signature spans, value lookup, and
//! unresolved-type checks.

use super::State;
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::convert::is_type;
use crate::types::kinds::Kind;
use crate::types::ty::Type;

impl Checker<'_> {
    /// Where the top-level declaration being checked ends.
    pub fn current_end(&self) -> Option<u32> {
        self.top.current.map(|i| self.top.items[i].decl.range().1)
    }

    /// The type the signature of this module's top-level value `name` gives.
    pub(in crate::types) fn signed_type(&self, name: &str) -> Option<Type> {
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

    /// The type of a top-level value of this module at a use, along with its
    /// instantiated type arguments.
    pub(in crate::types) fn top_value_args(
        &mut self,
        name: &str,
        span: Span,
    ) -> (Type, Vec<(Kind, Type)>) {
        let entity = self.env.entity(Namespace::Value, name);
        let Some(&i) = self.top.by_name.get(name) else {
            let scheme = self.env.value(&entity).cloned();
            return scheme.map_or((Type::Error, Vec::new()), |s| {
                let live = self.live_scheme(&s);
                self.instantiate_args(&live, span)
            });
        };
        if let Some(current) = self.top.current {
            self.top.edges.push((current, i, span));
        }
        if let Some(rec) = self.top.items[i].rec {
            let scheme = self.env.value(&entity).cloned().unwrap_or(Type::Error);
            // in its own body a value goes through the regions of its signature
            let own = self.top.current == Some(i);
            let scheme = if own {
                self.own_regions(&scheme, &self.recs[rec].skolems)
            } else {
                scheme
            };
            let (ty, args) = self.instantiate_args(&scheme, span);
            self.recs[rec].uses.push((ty.clone(), span, own));
            return (ty, args);
        }
        if self.env.value(&entity).is_none() && self.top.items[i].state == State::Waiting {
            self.check_item(i);
        }
        let scheme = self.env.value(&entity).cloned();
        scheme.map_or((Type::Error, Vec::new()), |s| {
            self.instantiate_args(&s, span)
        })
    }

    /// A top-level value whose type is still not fully known after the
    /// whole module is checked, e.g. a runtime result nothing pins down.
    pub(super) fn unknown_types(&mut self) {
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
