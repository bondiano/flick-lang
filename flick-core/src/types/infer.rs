//! Expressions, checked bidirectionally: [`Checker::check`] pushes an expected
//! type into lambdas, branches and literals, so a higher-rank parameter type
//! from a signature reaches the callback that must satisfy it; [`Checker::infer`] synthesizes a type otherwise.

use std::mem;

use super::Checker;
use super::convert::is_type;
use super::regions::Held;
use super::ty::Type;
use super::typing::Ref;
use crate::diagnostics::{Severity, codes};
use crate::numeric;
use crate::source;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

impl Checker<'_> {
    pub fn check(&mut self, node: &Node, expected: &Type) {
        self.check_value(node, expected);
    }

    /// Whether `node` names a top-level value, a constructor or an operation,
    /// which close over no local.
    fn names_top_level(&self, node: &Node) -> bool {
        let mut node = node;
        while node.kind == K::ParenExpr {
            let Some(inner) = node.nodes().next() else {
                return false;
            };
            node = inner;
        }
        node.kind == K::NameExpr
            && matches!(
                self.typing.names.get(&node.range()),
                Some(Ref::Value { .. } | Ref::Constructor(_) | Ref::Operation { .. })
            )
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn check_value(&mut self, node: &Node, expected: &Type) -> Type {
        let (target, captures) = self.whnf_captured(expected);
        if let Type::Forall { .. } = target {
            self.store.level += 1;
            let mark = self.classes.givens.len();
            let (body, _, givens) = self.skolemize(&target);
            let body = match captures {
                Some(captures) if matches!(self.bound_head(expected), Type::Bounded { .. }) => {
                    Type::Bounded {
                        ty: Box::new(body),
                        captures: Box::new(captures),
                    }
                }
                Some(captures) => Type::Captured {
                    ty: Box::new(body),
                    captures: Box::new(captures),
                    value: false,
                },
                None => body,
            };
            self.record_givens(self.span(node), &givens);
            self.classes.givens.extend(givens);
            let saved = mem::take(&mut self.cx.runtime);
            let (holds, held) = (self.cx.regions.holds.len(), self.cx.regions.held.len());
            self.check(node, &body);
            if self.names_top_level(node) {
                // a top-level value has no environment: the regions its
                // instantiated type names under the `forall` it does not hold
                self.cx.regions.holds.truncate(holds);
                self.cx.regions.held.truncate(held);
            }
            let runtime = mem::replace(&mut self.cx.runtime, saved);
            self.cx.runtime |= runtime;
            self.classes.givens.truncate(mark);
            self.store.level -= 1;
            let span = self.span(node);
            self.polymorphic_runtime(None, &target, runtime, span);
            if !runtime {
                self.polymorphic_atom(None, &target, span);
            }
            return expected.clone();
        }
        let children: Vec<&Node> = node.nodes().collect();
        let mut actual_type = None;
        match (node.kind, &target) {
            (K::ParenExpr, _) => actual_type = Some(self.check_value(children[0], expected)),
            (K::Block, _) => {
                self.block(node, Some(expected));
            }
            (K::FunExpr, _) => {
                self.lambda(node, Some(expected));
            }
            (K::LoopExpr | K::WhileExpr, _) => {
                self.control_loop(node, Some(expected));
            }
            (K::IfExpr, _) => self.if_expr(node, expected),
            (K::MatchExpr, _) => {
                self.match_expr(node, Some(expected));
            }
            (K::TryExpr, _) => {
                self.try_expr(node, Some(expected));
            }
            (K::ProvideExpr, _) => {
                self.provide(node, Some(expected));
            }
            (K::ScopeExpr, _) => {
                self.scope_expr(node, Some(expected));
            }
            (K::TupleExpr, Type::Tuple(items)) if items.len() == children.len() => {
                let captures = self.whnf_captured(expected).1;
                let mut held = Vec::new();
                for (child, item) in children.iter().zip(items) {
                    let item = captures
                        .as_ref()
                        .map_or_else(|| item.clone(), |c| self.data_field(item, c));
                    let actual = self.check_value(child, &item);
                    if captures.is_some() {
                        held.push((None, actual));
                    }
                }
                if let Some(bound) = captures {
                    self.hold(super::regions::Hold {
                        held,
                        visible: super::regions::Visible::Now(Vec::new()),
                        bound,
                        expected: Some(expected.clone()),
                        written: false,
                        what: "value",
                        span: self.span(node),
                    });
                }
            }
            (K::ListExpr, _) if super::range_list(node).is_some() => {
                let range = super::range_list(node).expect("checked");
                let actual = self.range_expr(range, true);
                self.expect(&actual, expected, self.span(node));
            }
            (K::ListExpr, _) if super::comprehension(node).is_some() => {
                let strict = super::comprehension(node).expect("checked");
                let actual = self.for_expr(strict, true, Some(expected));
                self.expect(&actual, expected, self.span(node));
            }
            (K::ForExpr, _) => {
                let actual = self.for_expr(node, false, Some(expected));
                self.expect(&actual, expected, self.span(node));
            }
            (K::ListExpr, _) if self.list_item(expected).is_some() => {
                let item = self.list_item(expected).expect("checked");
                let mut held = Vec::new();
                for child in children {
                    let actual = self.check_value(child, &item);
                    if captures.is_some() {
                        held.push((None, actual));
                    }
                }
                if let Some(bound) = captures {
                    self.hold(super::regions::Hold {
                        held,
                        visible: super::regions::Visible::Now(Vec::new()),
                        bound,
                        expected: Some(expected.clone()),
                        written: false,
                        what: "value",
                        span: self.span(node),
                    });
                }
            }
            (K::RecordExpr, Type::Record(row)) => {
                let row = self.store.zonk_row(row);
                if let Some(captures) = self.whnf_captured(expected).1 {
                    let row = super::ty::Row {
                        entries: row
                            .entries
                            .into_iter()
                            .map(|(n, t)| (n, self.data_field(&t, &captures)))
                            .collect(),
                        tail: row.tail,
                        ..super::ty::Row::default()
                    };
                    // each field holds within the bound where it is checked; the
                    // literal is compared with those fields, as the value's own
                    // record shows its fields bare
                    let actual = self.record(node, Some(&row));
                    self.expect(&actual, &Type::Record(row), self.span(node));
                } else {
                    let actual = self.record(node, Some(&row));
                    self.expect(&actual, expected, self.span(node));
                }
            }
            (K::DoExpr, _) => {
                self.do_expr(node, Some(expected));
            }
            (K::TaskExpr, _) if self.eff_parts(&target).is_some() => {
                self.task(node, Some(expected));
            }
            _ => {
                let actual = self.infer(node);
                self.expect(&actual, expected, self.span(node));
                actual_type = Some(actual);
            }
        }
        self.note_head(node, expected);
        actual_type.unwrap_or_else(|| expected.clone())
    }

    /// The body of a polymorphic type with rigid variables, its class
    /// constraints assumed until the caller drops them.
    fn assume(&mut self, t: &Type) -> Type {
        let (body, _, givens) = self.skolemize(t);
        self.classes.givens.extend(givens);
        body
    }

    pub fn infer(&mut self, node: &Node) -> Type {
        let ty = self.synthesize(node);
        self.note_head(node, &ty);
        ty
    }

    fn synthesize(&mut self, node: &Node) -> Type {
        let children: Vec<&Node> = node.nodes().collect();
        match node.kind {
            K::Literal => match node.child_tokens().next() {
                Some(t) if t.kind == K::Number => self.number(node, t.text(self.text)),
                token => literal_type(token.map(|t| t.kind)),
            },
            K::NameExpr => self.name(node),
            K::AssignStmt => self.assign(node),
            K::LoopExpr | K::WhileExpr => self.control_loop(node, None),
            K::BreakExpr | K::ContinueExpr => self.loop_exit(node),
            K::InterpExpr => self.interpolation(node),
            K::PackExpr => self.pack_module(node),
            K::UnpackExpr => self.unpack_module(node),
            K::ParenExpr => self.infer(children[0]),
            K::UnitExpr => Type::lang(Lang::Unit),
            K::TupleExpr => {
                let tuple = Type::Tuple(children.iter().map(|c| self.infer(c)).collect());
                let captures = self.captures_of(&tuple);
                self.data_value(&tuple, captures)
            }
            K::RangeExpr => self.range_expr(node, false),
            K::ListExpr if super::range_list(node).is_some() => {
                self.range_expr(super::range_list(node).expect("checked"), true)
            }
            K::ListExpr if super::comprehension(node).is_some() => {
                let strict = super::comprehension(node).expect("checked");
                self.for_expr(strict, true, None)
            }
            K::ForExpr => self.for_expr(node, false, None),
            K::ListExpr => {
                let item = self.fresh();
                let mut captures = super::ty::Norm::default();
                for child in children {
                    let actual = self.check_value(child, &item);
                    let held = self.captures_of(&actual);
                    captures.entries.extend(held.entries);
                    captures.tails.extend(held.tails);
                }
                self.data_value(&Type::app(Type::lang(Lang::List), item), captures)
            }
            K::AnnotExpr => {
                let annotation = children.iter().skip(1).find(|c| is_type(c.kind));
                let ty = annotation.map_or(Type::Error, |t| self.annotation(t));
                self.check(children[0], &ty);
                if let Some(annotation) = annotation {
                    self.wire_codec(children[0], annotation, &ty);
                }
                ty
            }
            K::FieldExpr => self.field(node, children[0]),
            K::QuestionExpr => self.question(node, children[0]),
            K::AppExpr => self.apply(children[0], &children[1..]),
            K::BinExpr => self.binary(node, &children),
            K::PrefixExpr => self.prefix(node, &children),
            K::TodoExpr => self.todo(node),
            K::RecordExpr => self.record(node, None),
            K::RecordUpdate => self.record_update(node, &children),
            K::FunExpr => self.lambda(node, None),
            K::Block => self.block(node, None),
            K::MatchExpr => self.match_expr(node, None),
            K::TryExpr => self.try_expr(node, None),
            K::IfExpr => {
                let ty = self.fresh();
                self.if_expr(node, &ty);
                ty
            }
            K::TaskExpr => self.task(node, None),
            K::DoExpr => self.do_expr(node, None),
            K::ScopeExpr => self.scope_expr(node, None),
            K::ProvideExpr => self.provide(node, None),
            K::UiExpr => {
                self.ui += 1;
                let ty = node
                    .nodes()
                    .find(|n| n.kind == K::Block)
                    .map_or(Type::Error, |b| self.block(b, None));
                self.ui -= 1;
                ty
            }
            _ => Type::Error,
        }
    }

    pub fn list_item(&self, t: &Type) -> Option<Type> {
        let (bare, captures) = self.whnf_captured(t);
        match bare {
            Type::App { head, arg } if self.whnf(&head).is_lang(Lang::List) => Some(
                captures
                    .as_ref()
                    .map_or_else(|| *arg.clone(), |c| self.data_field(&arg, c)),
            ),
            _ => None,
        }
    }

    /// `(r, e, a)` of `Eff r e a`.
    /// What a `Layer` needs, how it fails and what it makes.
    pub fn layer_parts(&self, t: &Type) -> Option<(Type, Type, Type)> {
        let t = self.whnf(t);
        let (head, args) = t.spine();
        match args[..] {
            [n, e, o] if head.is_lang(Lang::Layer) => Some((n.clone(), e.clone(), o.clone())),
            _ => None,
        }
    }

    pub fn eff_parts(&self, t: &Type) -> Option<(Type, Type, Type)> {
        let t = self.whnf(t);
        let (head, args) = t.spine();
        match args[..] {
            [r, e, a] if head.is_lang(Lang::Eff) => Some((r.clone(), e.clone(), a.clone())),
            _ => None,
        }
    }

    fn if_expr(&mut self, node: &Node, expected: &Type) {
        let children: Vec<&Node> = node.nodes().collect();
        if let Some(condition) = children.first() {
            self.check(condition, &Type::lang(Lang::Bool));
        }
        let Some(&otherwise) = children.get(2) else {
            let (unit, span) = (Type::lang(Lang::Unit), self.span(node));
            let keyword = node.child_tokens().find(|t| t.kind == K::IfKw);
            let keyword = keyword.map_or(span, |t| self.token_span(t));
            let why = |_: &str| "an `if` without `else` is `Unit`".to_string();
            let since = self.diagnostics.len();
            // one mismatch: of the whole `if` when its place wants a value
            let fits = self.expect(&unit, expected, span);
            self.explain_since(since, expected, keyword, &why);
            if let Some(then) = children.get(1) {
                if fits {
                    let since = self.diagnostics.len();
                    self.check(then, &unit);
                    self.explain_since(since, &unit, keyword, &why);
                } else {
                    self.infer(then);
                }
            }
            return;
        };
        self.redundant_else(node, &children, otherwise);
        // an inferred `if` takes the type of its `then` branch
        let open = matches!(self.store.resolve(expected), Type::Meta(_));
        if let Some(then) = children.get(1) {
            self.check(then, expected);
        }
        for branch in children.iter().skip(2) {
            let since = self.diagnostics.len();
            self.check(branch, expected);
            if open {
                let then = self.span(children[1]);
                self.explain_since(since, expected, then, &|t| {
                    format!("the `then` branch has type `{t}`")
                });
            }
        }
    }

    /// `fun p … -> body`.
    pub fn lambda(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let params: Vec<&Node> = node
            .nodes()
            .filter(|n| super::patterns::is_pattern(n.kind))
            .collect();
        let body = node.nodes().find(|n| n.kind == K::Block);
        self.function(&params, body, expected, node)
    }

    /// A function from its parameters, as `fun` or a `let` with parameters
    /// writes it. Every parameter is a stage of a curried arrow; the expected
    /// type decides each stage's arrow. The body runs at the last stage, so
    /// its runtime calls must fit the last arrow; without an expected type
    /// that arrow is runtime exactly when the body makes a runtime call.
    pub fn function(
        &mut self,
        params: &[&Node],
        body: Option<&Node>,
        expected: Option<&Type>,
        node: &Node,
    ) -> Type {
        let params: Vec<Option<&Node>> = params.iter().copied().map(Some).collect();
        let body = |c: &mut Self, rest: Option<Type>| match (rest, body) {
            (Some(rest), Some(body)) => {
                c.check(body, &rest);
                rest
            }
            (None, Some(body)) => c.infer(body),
            (_, None) => Type::Error,
        };
        self.function_with(&params, body, expected, node)
    }

    /// [`Self::function`] with a body that checks against the result type
    /// the stages leave, if known; a `None` parameter binds nothing.
    pub fn function_with(
        &mut self,
        params: &[Option<&Node>],
        body: impl FnOnce(&mut Self, Option<Type>) -> Type,
        expected: Option<&Type>,
        node: &Node,
    ) -> Type {
        let saved = mem::take(&mut self.cx.runtime);
        let mark = self.cx.locals.len();
        let givens = self.classes.givens.len();
        let uses = self.uses_mark();
        self.enter_apart(super::mutation::Apart::Function);
        self.begin_reader();
        let mut rest = expected.cloned();
        let mut stages = Vec::new();
        // per stage: the locals below it, which it may close over, and its
        // expected type unless that is unknown
        let mut closures: Vec<(usize, Option<Type>)> = Vec::new();
        // the first stage whose expected type was unknown, and that type
        let mut unknown: Option<(usize, Type)> = None;
        // the arrow the expected type gives the last stage
        let mut last = None;
        let level = self.store.level;
        for param in params {
            let target = rest.as_ref().map(|t| self.whnf(t));
            let known = rest.clone().filter(|t| {
                !matches!(target, Some(Type::Meta(_))) || self.whnf_captured(t).1.is_some()
            });
            closures.push((self.cx.locals.len(), known));
            let target = match target {
                Some(t @ Type::Forall { .. }) => {
                    self.store.level += 1;
                    Some(self.assume(&t))
                }
                other => other,
            };
            last = None;
            let (ty, effects) = match target {
                Some(Type::Fun {
                    param: ty,
                    effects,
                    result,
                }) => {
                    rest = Some(*result);
                    last = Some(effects.clone());
                    (*ty, effects)
                }
                Some(Type::Error) => (Type::Error, None),
                Some(meta @ Type::Meta(_)) => {
                    unknown.get_or_insert((stages.len(), meta));
                    rest = None;
                    (self.fresh(), None)
                }
                Some(other) => {
                    let shown = self.show(&other);
                    self.error(
                        codes::T003,
                        self.span(node),
                        format!("expected `{shown}`, found a function"),
                    );
                    rest = Some(Type::Error);
                    (Type::Error, None)
                }
                None => (self.fresh(), None),
            };
            if let Some(param) = param {
                let pat = self.bind_pattern(param, &ty);
                self.irrefutable(&pat, param, None);
            }
            stages.push((ty, effects));
        }
        let inferred = last.is_none();
        let mut stage = match last {
            Some(effects) => self.body_stage(effects.as_deref(), node, expected),
            None => self.open_stage(),
        };
        self.live_bound(&mut stage, closures.last().and_then(|c| c.1.as_ref()));
        // `o?` in the body returns from this function, so it fixes the result type
        let inferred_result = rest.is_none();
        let returns = rest.clone().unwrap_or_else(|| self.fresh());
        stage.returns = Some(returns.clone());
        let outer = mem::replace(&mut self.cx.stage, stage);
        let result = body(self, rest);
        let inner = mem::replace(&mut self.cx.stage, outer);
        if inferred_result && self.unify(&returns, &result).is_err() {
            let (returns, result) = (self.show(&returns), self.show(&result));
            self.error(
                codes::T003,
                self.span(node),
                format!(
                    "`?` in this function returns `{returns}`, but its body has type `{result}`"
                ),
            );
        }
        if inferred && let Some(stage) = stages.last_mut() {
            stage.1 = Self::stage_effects(&inner);
        }
        self.leave_apart();
        let closures: Vec<_> = closures
            .into_iter()
            .map(|(locals, expected)| (self.held_since(uses, locals), expected))
            .collect();
        self.cx.locals.truncate(mark);
        self.classes.givens.truncate(givens);
        self.cx.runtime = saved;
        let span = self.span(node);
        self.end_reader(&result, span);
        let result = self.curry(stages, closures, unknown.as_ref(), result, span);
        self.store.level = level;
        result
    }

    /// The arrows of a function's stages around its result; each stage is a
    /// closure over the locals it uses.
    fn curry(
        &mut self,
        stages: Vec<(Type, Option<Box<super::ty::Effects>>)>,
        closures: Vec<(Held, Option<Type>)>,
        unknown: Option<&(usize, Type)>,
        result: Type,
        span: source::Span,
    ) -> Type {
        let modes = stages
            .iter()
            .map(|(_, e)| super::ty::stage_of(e.as_deref()))
            .collect();
        self.typing.stages.insert(super::typing::range(span), modes);
        let mut ty = result;
        for (i, ((param, effects), (held, expected))) in
            stages.into_iter().zip(closures).enumerate().rev()
        {
            ty = Type::Fun {
                param: Box::new(param),
                effects,
                result: Box::new(ty),
            };
            ty = self.close_over(ty, held, expected, "function", span);
            if let Some((j, meta)) = unknown
                && *j == i
            {
                // a number literal's variable stands for a number only; any
                // other failure is the closure's captures, which `close_over`
                // reports
                let meta = meta.clone();
                let numeric =
                    matches!(self.store.resolve(&meta), Type::Meta(m) if self.store.is_numeric(m));
                if self.unify(&meta, &ty).is_err() && numeric {
                    let shown = self.show(&meta);
                    self.error(
                        codes::T003,
                        span,
                        format!("expected `{shown}`, found a function"),
                    );
                }
            }
        }
        ty
    }

    fn match_expr(&mut self, node: &Node, expected: Option<&Type>) -> Type {
        let mut nodes = node.nodes();
        let scrutinee = nodes.next().map_or(Type::Error, |s| self.infer(s));
        let result = expected.cloned().unwrap_or_else(|| self.fresh());
        let errors = self.diagnostics.len();
        // an inferred `match` takes the type of its first arm
        let open = expected.is_none();
        let mut first = None;
        let mut arms = Vec::new();
        for arm in nodes.filter(|n| n.kind == K::MatchArm) {
            let since = self.diagnostics.len();
            arms.push(self.arm(arm, &scrutinee, &result));
            let body = arm
                .nodes()
                .find(|n| n.kind == K::Block)
                .map(|b| self.span(b));
            match first {
                Some(first) if open => self.explain_since(since, &result, first, &|t| {
                    format!("the first arm has type `{t}`")
                }),
                Some(_) => {}
                None => first = body,
            }
        }
        // an error in an arm makes its patterns unreliable; a warning does
        // not, and must not let a `match` that misses a value through
        if !self.diagnostics[errors..]
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            self.exhaustive(node, &arms, &scrutinee);
        }
        result
    }

    /// One `| pattern [when guard] -> body`; its pattern and whether it is guarded.
    pub fn arm(
        &mut self,
        arm: &Node,
        scrutinee: &Type,
        result: &Type,
    ) -> (super::patterns::Pat, bool, source::Span) {
        let mark = self.cx.locals.len();
        let mut pat = super::patterns::Pat::Wild;
        let mut guarded = false;
        let mut span = self.span(arm);
        let frame = self.enter_arm();
        for child in arm.nodes() {
            // an arm that matches nothing is not checked: its patterns contradict
            if self.gadt.dead {
                break;
            }
            match child.kind {
                K::Guard => {
                    guarded = true;
                    if let Some(condition) = child.nodes().next() {
                        self.check(condition, &Type::lang(Lang::Bool));
                    }
                }
                K::Block => self.check(child, result),
                kind if super::patterns::is_pattern(kind) => {
                    let child = super::handle::failed_pattern(child, self.text).unwrap_or(child);
                    span = self.span(child);
                    pat = self.bind_arm_pattern(child, scrutinee);
                }
                _ => {}
            }
        }
        if self.leave_arm(frame) {
            self.typing
                .dead_arms
                .insert(super::typing::range(self.span(arm)));
        }
        self.cx.locals.truncate(mark);
        (pat, guarded, span)
    }
}

impl Checker<'_> {
    /// The type of a number literal: of any numeric type, a `Float` one when
    /// it has a fraction.
    pub fn number(&mut self, node: &Node, text: &str) -> Type {
        let span = self.span(node);
        if numeric::is_integer_literal(text) {
            self.number_literal(span, numeric::integer_value(text), false, false)
        } else {
            self.number_literal(span, None, true, false)
        }
    }
}

/// The type of a literal; a number literal's is [`Checker::number`]'s.
pub fn literal_type(token: Option<K>) -> Type {
    match token {
        Some(K::Number) => Type::lang(Lang::Float),
        Some(K::BigInt) => Type::lang(Lang::BigInt),
        Some(K::String) => Type::lang(Lang::String),
        Some(K::TrueKw | K::FalseKw) => Type::lang(Lang::Bool),
        _ => Type::Error,
    }
}

pub fn eff(services: Type, errors: Type, result: Type) -> Type {
    Type::apps(Type::lang(Lang::Eff), [services, errors, result])
}
