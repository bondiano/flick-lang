//! Names, application and operators. Operators are fixed: arithmetic on the
//! numeric types, `++` on `String` or `List`, comparisons on the numeric
//! types and `String`; operands never mix types or coerce.

use std::mem;

use super::ty::Type;
use super::typing::{Ref, range};
use super::{Checker, Domain, Local, Operator};
use crate::consteval;
use crate::diagnostics::suggest::closest;
use crate::diagnostics::{Diagnostic, Edit, Severity, codes};
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// Binds `name`, written at `at`, to `ty`.
    pub fn push_local(&mut self, name: &str, at: u32, ty: Type) {
        self.cx.locals.push(Local {
            name: name.to_owned(),
            at: Some(at),
            generated: at >= self.generated,
            ty,
            rec: None,
            unsigned: false,
            binding: super::mutation::Binding::Other,
        });
    }

    /// The type of a value or constructor reference, instantiated.
    pub fn name(&mut self, node: &Node) -> Type {
        let ty = self.name_type(node);
        let declared = self.gadt.declared.take();
        let ty = self.value_bound(ty);
        let ty = if matches!(self.typing.names.get(&node.range()), Some(Ref::Local)) {
            ty
        } else {
            Type::Bounded {
                ty: Box::new(ty),
                captures: Box::new(Type::empty_row()),
            }
        };
        match declared {
            Some(declared) => self.note_refined(range(self.span(node)), &ty, &declared),
            None => self.note_type(self.span(node), &ty),
        }
        ty
    }

    fn name_type(&mut self, node: &Node) -> Type {
        let tokens: Vec<_> = node.child_tokens().filter(|t| t.kind != K::Dot).collect();
        let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
        let Some((&name, qualifier)) = parts.split_last() else {
            return Type::Error;
        };
        let span = self.span(node);
        // a companion `T$schema` of generated code is a value, its token one
        let namespace = if tokens.last().is_some_and(|t| t.kind == K::Con) {
            Namespace::Constructor
        } else {
            Namespace::Value
        };
        if !qualifier.is_empty() {
            return self.qualified(&qualifier.join("."), namespace, name, span);
        }
        if namespace == Namespace::Value
            && let Some(i) = (self.cx.locals.iter())
                .rposition(|l| l.name == name && l.generated == (span.start >= self.generated))
        {
            self.record_name(span, Ref::Local);
            if let Some(at) = self.cx.locals[i].at
                && span.start < self.generated
            {
                let end = at + u32::try_from(name.len()).expect("source fits in u32");
                self.typing
                    .binders
                    .insert((span.start, span.end), (at, end));
            }
            self.gadt.declared = self.declared_local(i);
            return self.use_local(i, span);
        }
        if self.ui > 0
            && let Some((_, Some(ui))) = self.env.qualified_module("Ui")
            && ui.find(namespace, name).is_some()
        {
            return self.qualified("Ui", namespace, name, span);
        }
        match self.env.find(namespace, name) {
            super::env::Found::Entity(entity) => {
                let ty = if entity.module == self.env.module && namespace == Namespace::Value {
                    self.top_value(name, span)
                } else {
                    let scheme = self.env.value(&entity).cloned();
                    scheme.map_or(Type::Error, |s| {
                        self.instantiate(&self.live_scheme(&s), span)
                    })
                };
                self.record_entity(&entity, &ty, span);
                ty
            }
            super::env::Found::Unknown => Type::Error,
            super::env::Found::Missing if namespace == Namespace::Constructor => {
                if let super::env::Found::Entity(e) = self.env.find(Namespace::Type, name) {
                    self.record_name(span, Ref::ServiceTag(format!("{}.{}", e.module, e.name)));
                }
                self.service_tag(name).unwrap_or_else(|| {
                    self.error_guess(
                        codes::T001,
                        span,
                        format!("cannot find constructor `{name}`"),
                        |this| closest(name, this.env.names(namespace)).map(str::to_owned),
                    );
                    Type::Error
                })
            }
            super::env::Found::Missing => {
                if let Some(value) = name.strip_suffix("$server") {
                    self.not_a_server_fn(value, span);
                    return Type::Error;
                }
                let what = match namespace {
                    Namespace::Constructor => "constructor",
                    _ => "value",
                };
                self.error_guess(
                    codes::T001,
                    span,
                    format!("cannot find {what} `{name}`"),
                    |this| {
                        let locals = this.cx.locals.iter().map(|l| l.name.as_str());
                        closest(name, this.env.names(namespace).chain(locals)).map(str::to_owned)
                    },
                );
                Type::Error
            }
        }
    }

    pub fn use_local(&mut self, index: usize, span: Span) -> Type {
        self.cx.regions.uses.push(index);
        self.mutable_use(index, span);
        let Local { ty, rec, .. } = self.cx.locals[index].clone();
        let ty = self.instantiate(&ty, span);
        if let Some(rec) = rec {
            self.recs[rec].uses.push((ty.clone(), span));
        }
        ty
    }

    /// `FLICK-T003` on a callee of type `ty` given `args`, which are still
    /// checked.
    fn not_a_function(&mut self, ty: &Type, span: Span, args: &[&Node]) -> Type {
        let shown = self.show(ty);
        self.error(
            codes::T003,
            span,
            format!(
                "this is not a function: it has type `{shown}`, so it cannot take another argument"
            ),
        );
        for arg in args {
            self.infer(arg);
        }
        Type::Error
    }

    /// `f a b` is `(f a) b`: each argument is one stage, and a runtime arrow
    /// at a stage makes the application a runtime call.
    pub fn apply(&mut self, head: &Node, args: &[&Node]) -> Type {
        let mut defaults = Vec::new();
        let provided = self.cx.regions.provided.len();
        let ty = self.apply_args(head, args, &mut defaults);
        self.settle_defaults(defaults);
        self.settle_provided(provided);
        ty
    }

    fn apply_args(
        &mut self,
        head: &Node,
        args: &[&Node],
        defaults: &mut Vec<(Type, usize)>,
    ) -> Type {
        let since = self.diagnostics.len();
        // a local `forall v. τ^v -> σ` keeps its argument out of its result
        let parametric = self.local_parametric(head);
        let mut ty = self.infer(head);
        let mut unknown = Vec::new();
        let mut params = Vec::new();
        // the places a plugin watches: read before any argument, so an
        // application of a name none watches pays only the lookup
        let watched = self.watched_name(head);
        let mut given = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let span = Span::new(self.file, head.range().0, arg.range().1);
            match self.ui_callable(&ty, (span.start, span.end), self.span(head)) {
                Type::Fun {
                    param,
                    effects,
                    result,
                } => {
                    let calls = self.cx.stage.calls;
                    let since = self.diagnostics.len();
                    // a task acquiring into a region a later argument names
                    // waits for it (`docs/formal/regions.md#scope-region-of-a-task-argument-is-resolved-after-the-call`)
                    let eff = self.eff_parts(&param).is_some();
                    let outer = eff.then(|| self.cx.regions.defaults.replace(Vec::new()));
                    let argument = self.check_value(arg, &param);
                    if let Some(outer) = outer {
                        let mine = mem::replace(&mut self.cx.regions.defaults, outer);
                        defaults.extend(mine.unwrap_or_default());
                    }
                    self.explain_argument(since, head, i, &param);
                    if watched.is_some() {
                        let folded = consteval::eval(arg, self.text);
                        given.push((self.span(arg), argument.clone(), folded));
                    }
                    if i == 0 {
                        self.ui_render_handlers(head, arg);
                    }
                    if eff {
                        if self.cx.stage.calls > calls {
                            self.early_argument(head, arg);
                        }
                        // the rows the `Eff` needs are lower bounds of the parameter's: grown
                        // now, before the result's rows unify as if they could be smaller
                        self.grow();
                    }
                    if let Some(e) = effects {
                        self.call_effects(&self.live_call(&ty, &e), span);
                    }
                    params.push(*param);
                    let result = self.instantiate(&result, span);
                    ty = self.application_result(result, &ty, &argument, parametric && i == 0);
                }
                Type::Error => {
                    let inferred = self.infer(arg);
                    unknown.push(inferred);
                }
                meta @ Type::Meta(_) => {
                    let (param, result) = (self.fresh(), self.fresh());
                    // a number literal's variable stands for a number only
                    if self
                        .unify(&meta, &Type::pure_fun(param.clone(), result.clone()))
                        .is_err()
                    {
                        return self.not_a_function(&meta, span, &args[i..]);
                    }
                    self.check(arg, &param);
                    ty = result;
                }
                other => return self.not_a_function(&other, span, &args[i..]),
            }
        }
        if unknown.len() == args.len() && head.kind == K::NameExpr {
            self.stub_fix(since, head, args, &unknown);
        }
        if let Some((module, name)) = watched {
            let span = Span::new(self.file, head.range().0, head.range().1);
            self.watched_call(&module, &name, span, given);
        }
        if let [first, second] = &params[..]
            && self.is_builtin(head, Lang::LayerMerge)
        {
            let span = Span::new(self.file, head.range().0, args[1].range().1);
            self.merge_conflicts(first, second, span);
        }
        ty
    }

    /// A runtime call in an argument runs before the function it is passed
    /// to; a function that takes an `Eff` wants the computation unstarted.
    fn early_argument(&mut self, head: &Node, arg: &Node) {
        let span = self.span(arg);
        let name = self.text_of(head);
        self.diagnostics.push(
            Diagnostic::new(
                codes::E003,
                Severity::Warning,
                format!(
                    "this argument runs a runtime call before `{name}` is applied, although `{name}` takes an `Eff` to run itself"
                ),
            )
            .with_span(span).with_fix(|| ("pass the computation unstarted with `task { … }`", vec![
                Edit {
                    span: Span::new(span.file, span.start, span.start),
                    replacement: "task { ".to_owned(),
                },
                Edit {
                    span: Span::new(span.file, span.end, span.end),
                    replacement: " }".to_owned(),
                },
            ])),
        );
    }

    /// The value `head` refers to when a plugin of the project watches it.
    fn watched_name(&self, head: &Node) -> Option<(String, String)> {
        if self.options.watched.is_empty() {
            return None;
        }
        let Some(Ref::Value { module, name }) = self.typing.names.get(&head.range()) else {
            return None;
        };
        (self.options.watched.holds(module, name)).then(|| (module.clone(), name.clone()))
    }

    /// `head` names the value `name` of the built-in module `module`.
    fn is_builtin(&self, head: &Node, item: Lang) -> bool {
        let Some(item) = lang::table().find(item) else {
            return false;
        };
        let parts: Vec<&str> = head
            .child_tokens()
            .filter(|t| t.kind != K::Dot)
            .map(|t| t.text(self.text))
            .collect();
        head.kind == K::NameExpr
            && parts.len() == 2
            && parts[1] == item.name
            && self
                .env
                .qualified_module(parts[0])
                .is_some_and(|(m, _)| m == item.module)
    }

    pub fn binary(&mut self, node: &Node, children: &[&Node]) -> Type {
        let Some(op) = node.child_tokens().find(|t| t.kind.is_binary_op()) else {
            return Type::Error;
        };
        let (Some(lhs), Some(rhs)) = (children.first(), children.get(1)) else {
            return Type::Error;
        };
        let span = self.span(node);
        let bool_ = Type::lang(Lang::Bool);
        match op.kind {
            K::PipeGt => {
                // at the `|>`, the type after the stage: hover and inlay hints show it
                let ty = self.pipe(lhs, rhs);
                self.note_type(self.token_span(op), &ty);
                ty
            }
            K::OrOr | K::AndAnd => {
                self.check(lhs, &bool_);
                self.check(rhs, &bool_);
                bool_
            }
            K::EqEq | K::NotEq => {
                let ty = self.infer(lhs);
                self.check(rhs, &ty);
                let givens = self.equalities_in_scope();
                self.equalities.push((ty, span, givens));
                bool_
            }
            kind => {
                let ty = self.infer(lhs);
                self.check(rhs, &ty);
                let (name, domain) = match kind {
                    K::Lt => ("<", Domain::Ordered),
                    K::Le => ("<=", Domain::Ordered),
                    K::Gt => (">", Domain::Ordered),
                    K::Ge => (">=", Domain::Ordered),
                    K::PlusPlus => ("++", Domain::Concat),
                    K::Plus => ("+", Domain::Numeric),
                    K::Minus => ("-", Domain::Numeric),
                    K::Star => ("*", Domain::Numeric),
                    K::Slash => ("/", Domain::Numeric),
                    K::Percent => ("%", Domain::Numeric),
                    _ => ("**", Domain::Numeric),
                };
                self.cx.operators.push(Operator {
                    ty: ty.clone(),
                    span,
                    op: name,
                    domain,
                    givens: self.classes.givens.clone(),
                });
                if domain == Domain::Ordered { bool_ } else { ty }
            }
        }
    }

    /// `x |> f` applies `f` to `x`, evaluating `x` first.
    fn pipe(&mut self, lhs: &Node, rhs: &Node) -> Type {
        let arg = self.infer(lhs);
        let function = self.infer(rhs);
        let call = (lhs.range().0, rhs.range().1);
        match self.ui_callable(&function, call, self.span(rhs)) {
            Type::Fun {
                param,
                effects,
                result,
            } => {
                self.expect(&arg, &param, self.span(lhs));
                if let Some(e) = effects {
                    let (start, _) = lhs.range();
                    let (_, end) = rhs.range();
                    let e = self.live_call(&function, &e);
                    self.call_effects(&e, Span::new(self.file, start, end));
                }
                let span = Span::new(self.file, lhs.range().0, rhs.range().1);
                let result = self.instantiate(&result, span);
                self.application_result(result, &function, &arg, false)
            }
            Type::Error => Type::Error,
            meta @ Type::Meta(_) => {
                let result = self.fresh();
                let _ = self.unify(&meta, &Type::pure_fun(arg, result.clone()));
                result
            }
            other => {
                let shown = self.show(&other);
                self.error(
                    codes::T003,
                    self.span(rhs),
                    format!("the right side of `|>` must be a function, found `{shown}`"),
                );
                Type::Error
            }
        }
    }

    pub fn prefix(&mut self, node: &Node, children: &[&Node]) -> Type {
        let Some(op) = node.child_tokens().next() else {
            return Type::Error;
        };
        if op.kind == K::EchoKw {
            return self.echo(node, op, children.first().copied());
        }
        let Some(operand) = children.first() else {
            return Type::Error;
        };
        match op.kind {
            K::NotKw => {
                let bool_ = Type::lang(Lang::Bool);
                self.check(operand, &bool_);
                bool_
            }
            K::IgnoreKw => {
                let ty = self.infer(operand);
                self.note_type(self.token_span(op), &ty);
                Type::lang(Lang::Unit)
            }
            K::AwaitKw => {
                let ty = self.infer(operand);
                self.note_type(self.token_span(op), &ty);
                let (services, errors, result) =
                    (self.fresh(), self.store.fresh_error_row(), self.fresh());
                let shape = super::infer::eff(services.clone(), errors.clone(), result.clone());
                if self.unify(&self.whnf(&ty), &shape).is_err() {
                    let shown = self.show(&ty);
                    self.error(
                        codes::T003,
                        self.span(operand),
                        format!("`await` runs an `Eff` value, but this has type `{shown}`"),
                    );
                    return Type::Error;
                }
                let span = self.span(node);
                if self.runtime_allowed(span) {
                    let services = self.live_await(&ty, services);
                    self.perform(&services, &errors, span);
                }
                result
            }
            _ => {
                let ty = self.infer(operand);
                // `-2147483648` is a literal of `Int`, though `2147483648` is not
                let at = self.span(operand);
                if operand.kind == K::Literal
                    && let Some(literal) = self.literals.last_mut()
                    && literal.span == at
                {
                    literal.value = literal.value.map(|v| -v);
                }
                self.cx.operators.push(Operator {
                    ty: ty.clone(),
                    span: self.span(node),
                    op: "-",
                    domain: Domain::Numeric,
                    givens: self.classes.givens.clone(),
                });
                ty
            }
        }
    }
}
