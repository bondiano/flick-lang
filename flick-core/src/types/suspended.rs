//! Suspended parameters (`docs/formal/lazy.md` §7): a signature marks a
//! parameter of type `Eff` with `~`, and a call whose argument there is a
//! runtime call passes it unstarted, as `task { … }` would. The marker is
//! not part of the type: it stays beside the signature, in the module's
//! interface, and only a call of the value by its name reads it.

use super::Checker;
use super::env::{Form, Found};
use super::ty::Type;
use super::typing::Ref;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The parameters each top-level signature of `root` marks `~`, by position.
pub fn marks(root: &Node, text: &str) -> Vec<(String, Vec<usize>)> {
    let mut out = Vec::new();
    for signature in root.nodes().filter(|n| n.kind == K::Signature) {
        let Some(name) = signature.child_tokens().find(|t| t.kind == K::Ident) else {
            continue;
        };
        let Some(mut ty) = signature.nodes().last() else {
            continue;
        };
        // the arrows past a `forall` and a context
        while matches!(ty.kind, K::ForallType | K::ConstrainedType) {
            let Some(body) = ty.nodes().last() else { break };
            ty = body;
        }
        let mut marked = Vec::new();
        let mut index = 0;
        while ty.kind == K::ArrowType {
            let mut parts = ty.nodes().filter(|n| n.kind != K::EffectAnnot);
            let (Some(param), Some(result)) = (parts.next(), parts.last()) else {
                break;
            };
            if param.kind == K::SuspendedType {
                marked.push(index);
            }
            index += 1;
            ty = result;
        }
        if !marked.is_empty() {
            out.push((name.text(text).to_owned(), marked));
        }
    }
    out
}

impl Checker<'_> {
    /// The parameters of the value `head` names that its signature marks.
    pub(super) fn suspended_params(&self, head: &Node) -> Vec<usize> {
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()) else {
            return Vec::new();
        };
        let marks = if *module == self.env.module {
            Some(&self.typing.suspended)
        } else {
            self.env.interface(module).map(|i| &i.suspended)
        };
        (marks.into_iter().flatten())
            .find(|(n, _)| n == name)
            .map_or_else(Vec::new, |(_, marked)| marked.clone())
    }

    /// `arg`, a runtime call for a parameter marked `~` of type `param`,
    /// checked as the body of the task it is passed as.
    pub(super) fn suspended_arg(&mut self, arg: &Node, param: &Type) -> Type {
        self.typing.suspends.insert(arg.range());
        let task = self.deferred(arg, arg, Some(param));
        self.expect(&task, param, self.span(arg));
        task
    }

    /// Whether `arg` runs a runtime call where it is written: an application
    /// whose arrows, as many as it has arguments, include a runtime one, or
    /// an `await`. A value of type `Eff`, or a call that builds one, is not.
    pub(super) fn runs_now(&self, arg: &Node) -> bool {
        let arg = super::sugar_lints::unparen(arg);
        match arg.kind {
            K::AppExpr => {
                let parts: Vec<&Node> = arg.nodes().collect();
                let Some((head, args)) = parts.split_first() else {
                    return false;
                };
                if args.iter().any(|a| a.kind == K::HoleExpr) {
                    return false;
                }
                self.is_operation(head)
                    || (self.peek_type(head)).is_some_and(|ty| crosses_runtime(&ty, args.len()))
            }
            K::PrefixExpr => arg.has_token(K::AwaitKw),
            _ => false,
        }
    }

    /// Whether `head` names an operation of a service, `S.op`: a runtime call.
    fn is_operation(&self, head: &Node) -> bool {
        let text = self.text_of(head);
        let Some((service, _)) = text.rsplit_once('.') else {
            return false;
        };
        let Found::Entity(entity) = self.env.find_type(service) else {
            return false;
        };
        (self.env.type_def(&entity.module, &entity.name))
            .is_some_and(|d| matches!(d.form, Form::Service { .. }))
    }

    /// The type a name stands for, looked up without recording anything: a
    /// local's, or a value's scheme. None for anything but a plain name.
    fn peek_type(&self, head: &Node) -> Option<Type> {
        if head.kind != K::NameExpr {
            return None;
        }
        let parts: Vec<&str> = (head.child_tokens())
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(self.text))
            .collect();
        let (&name, qualifier) = parts.split_last()?;
        if !qualifier.is_empty() {
            let (_, interface) = self.env.qualified_module(&qualifier.join("."))?;
            let export = (interface?.exports.iter())
                .find(|e| e.entity.namespace == Namespace::Value && e.entity.name == name)?;
            return export.scheme.clone();
        }
        if let Some(local) = self.cx.locals.iter().rev().find(|l| l.name == name) {
            return Some(local.ty.clone());
        }
        match self.env.find(Namespace::Value, name) {
            Found::Entity(entity) => self.env.value(&entity).cloned(),
            _ => None,
        }
    }
}

/// Whether applying `ty` to `args` arguments goes through a runtime arrow.
fn crosses_runtime(ty: &Type, args: usize) -> bool {
    let mut ty = ty;
    for _ in 0..args {
        loop {
            match ty {
                Type::Forall { body, .. } => ty = body,
                Type::Bounded { ty: inner, .. } | Type::Captured { ty: inner, .. } => ty = inner,
                _ => break,
            }
        }
        let Type::Fun {
            effects, result, ..
        } = ty
        else {
            return false;
        };
        if effects.is_some() {
            return true;
        }
        ty = result;
    }
    false
}
