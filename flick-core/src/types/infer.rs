//! Expressions, checked bidirectionally: [`Checker::check`] pushes an expected
//! type into lambdas, branches and literals, so a higher-rank parameter type
//! from a signature reaches the callback that must satisfy it; [`Checker::infer`] synthesizes a type otherwise.

use std::mem;

use super::Checker;
use super::convert::is_type;
use super::effects::{Site, Stage};
use super::regions::Held;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::source;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    pub fn check(&mut self, node: &Node, expected: &Type) {
        let target = self.whnf(expected);
        if let Type::Forall { .. } = target {
            self.store.level += 1;
            let mark = self.classes.givens.len();
            let (body, _, givens) = self.skolemize(&target);
            self.record_givens(self.span(node), &givens);
            self.classes.givens.extend(givens);
            self.check(node, &body);
            self.classes.givens.truncate(mark);
            self.store.level -= 1;
            return;
        }
        let children: Vec<&Node> = node.nodes().collect();
        match (node.kind, &target) {
            (K::ParenExpr, _) => self.check(children[0], expected),
            (K::Block, _) => {
                self.block(node, Some(expected));
            }
            (K::FunExpr, _) => {
                self.lambda(node, Some(expected));
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
            (K::CodecExpr, _) => {
                let actual = self.codec_expr(node, Some(expected));
                self.expect(&actual, expected, self.span(node));
            }
            (K::SqlExpr, _) => {
                self.sql_expr(node, Some(expected));
            }
            (K::TupleExpr, Type::Tuple(items)) if items.len() == children.len() => {
                for (child, item) in children.iter().zip(items) {
                    self.check(child, item);
                }
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
            (K::ListExpr, _) if self.list_item(&target).is_some() => {
                let item = self.list_item(&target).expect("checked");
                for child in children {
                    self.check(child, &item);
                }
            }
            (K::RecordExpr, Type::Record(row)) => {
                let row = self.store.zonk_row(row);
                let actual = self.record(node, Some(&row));
                self.expect(&actual, expected, self.span(node));
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
            }
        }
        self.note_head(node, expected);
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
            K::Literal => {
                let token = node.child_tokens().next().map(|t| t.kind);
                literal_type(token)
            }
            K::NameExpr => self.name(node),
            K::ParenExpr => self.infer(children[0]),
            K::UnitExpr => Type::prim("Unit"),
            K::TupleExpr => Type::Tuple(children.iter().map(|c| self.infer(c)).collect()),
            K::ListExpr if super::comprehension(node).is_some() => {
                let strict = super::comprehension(node).expect("checked");
                self.for_expr(strict, true, None)
            }
            K::ForExpr => self.for_expr(node, false, None),
            K::ListExpr => {
                let item = self.fresh();
                for child in children {
                    self.check(child, &item);
                }
                Type::app(Type::prim("List"), item)
            }
            K::AnnotExpr => {
                let ty = children
                    .iter()
                    .skip(1)
                    .find(|c| is_type(c.kind))
                    .map_or(Type::Error, |t| self.annotation(t));
                self.check(children[0], &ty);
                ty
            }
            K::FieldExpr => self.field(node, children[0]),
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
            K::SchemaExpr => self.schema_expr(node),
            K::CodecExpr => self.codec_expr(node, None),
            K::UiExpr => {
                self.ui += 1;
                let ty = node
                    .nodes()
                    .find(|n| n.kind == K::Block)
                    .map_or(Type::Error, |b| self.block(b, None));
                self.ui -= 1;
                ty
            }
            K::SqlExpr => self.sql_expr(node, None),
            K::CliExpr => self.cli_expr(node),
            _ => Type::Error,
        }
    }

    pub fn list_item(&self, t: &Type) -> Option<Type> {
        match self.whnf(t) {
            Type::App { head, arg } if self.whnf(&head).is_prim("List") => Some(*arg),
            _ => None,
        }
    }

    /// `(r, e, a)` of `Eff r e a`.
    pub fn eff_parts(&self, t: &Type) -> Option<(Type, Type, Type)> {
        let t = self.whnf(t);
        let (head, args) = t.spine();
        match args[..] {
            [r, e, a] if head.is_prim("Eff") => Some((r.clone(), e.clone(), a.clone())),
            _ => None,
        }
    }

    fn if_expr(&mut self, node: &Node, expected: &Type) {
        let children: Vec<&Node> = node.nodes().collect();
        if let Some(condition) = children.first() {
            self.check(condition, &Type::prim("Bool"));
        }
        let Some(&otherwise) = children.get(2) else {
            let (unit, span) = (Type::prim("Unit"), self.span(node));
            let keyword = node.child_tokens().find(|t| t.kind == K::IfKw);
            let keyword = keyword.map_or(span, |t| self.token_span(t));
            let why = |_: &str| "an `if` without `else` is `Unit`".to_string();
            let since = self.diagnostics.len();
            // one mismatch: of the whole `if` when its place wants a value
            let fits = self.expect(&unit, expected, span);
            self.explain_since(since, expected, keyword, why);
            if let Some(then) = children.get(1) {
                if fits {
                    let since = self.diagnostics.len();
                    self.check(then, &unit);
                    self.explain_since(since, &unit, keyword, why);
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
                self.explain_since(since, expected, then, |t| {
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
        let saved = mem::take(&mut self.runtime);
        let mark = self.locals.len();
        let givens = self.classes.givens.len();
        let uses = self.uses_mark();
        let mut rest = expected.cloned();
        let mut stages = Vec::new();
        // per stage: the locals below it, which it may close over, and its
        // expected type unless that is unknown
        let mut closures: Vec<(usize, Option<Type>)> = Vec::new();
        // the first stage whose expected type was unknown, and that type
        let mut unknown: Option<(usize, Type)> = None;
        // the arrow the expected type gives the last stage
        let mut last = None;
        for param in params {
            let target = rest.as_ref().map(|t| self.whnf(t));
            let known = rest
                .clone()
                .filter(|_| !matches!(target, Some(Type::Meta(_))));
            closures.push((self.locals.len(), known));
            let target = match target {
                Some(t @ Type::Forall { .. }) => Some(self.assume(&t)),
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
        let name = (node.kind == K::LetDecl)
            .then(|| super::bindings::let_parts(node, self.text).name)
            .flatten();
        let inferred = last.is_none();
        let stage = match last {
            Some(None) => Stage::pure(match &name {
                Some(name) => format!("`{name}` is pure: its signature has `->`, not `->{{…}}`"),
                None if matches!(node.kind, K::DoExpr | K::BindStmt) => {
                    "the rest of a `do` block is the pure function of `flat_map`: bind the result of \
                     a runtime call with `let` before the `do`"
                        .to_owned()
                }
                None => {
                    "this function must be pure: the expected type has `->`, not `->{…}`".to_owned()
                }
            }),
            Some(Some(effects)) => {
                let site = match (name, expected) {
                    (Some(name), _) => Site::Signature(name),
                    (None, Some(expected)) => Site::Expected(expected.clone()),
                    (None, None) => Site::Inferred,
                };
                let sync = effects.sync;
                let mut stage = self.bounded_stage(effects.services, effects.errors, site);
                stage.sync = sync && stage.pure.is_none();
                stage
            }
            None => self.open_stage(),
        };
        let outer = mem::replace(&mut self.stage, stage);
        let result = body(self, rest);
        let inner = mem::replace(&mut self.stage, outer);
        if inferred && let Some(stage) = stages.last_mut() {
            stage.1 = Self::stage_effects(&inner);
        }
        let closures: Vec<_> = closures
            .into_iter()
            .map(|(locals, expected)| (self.held_since(uses, locals), expected))
            .collect();
        self.locals.truncate(mark);
        self.classes.givens.truncate(givens);
        self.runtime = saved;
        let span = self.span(node);
        self.curry(stages, closures, unknown.as_ref(), result, span)
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
                let meta = meta.clone();
                let _ = self.unify(&meta, &ty);
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
                Some(first) if open => self.explain_since(since, &result, first, |t| {
                    format!("the first arm has type `{t}`")
                }),
                Some(_) => {}
                None => first = body,
            }
        }
        if self.diagnostics.len() == errors {
            self.exhaustive(node, &arms);
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
        let mark = self.locals.len();
        let mut pat = super::patterns::Pat::Wild;
        let mut guarded = false;
        let mut span = self.span(arm);
        for child in arm.nodes() {
            match child.kind {
                K::Guard => {
                    guarded = true;
                    if let Some(condition) = child.nodes().next() {
                        self.check(condition, &Type::prim("Bool"));
                    }
                }
                K::Block => self.check(child, result),
                kind if super::patterns::is_pattern(kind) => {
                    span = self.span(child);
                    pat = self.bind_pattern(child, scrutinee);
                }
                _ => {}
            }
        }
        self.locals.truncate(mark);
        (pat, guarded, span)
    }
}

pub fn literal_type(token: Option<K>) -> Type {
    match token {
        Some(K::Number) => Type::prim("Number"),
        Some(K::BigInt) => Type::prim("BigInt"),
        Some(K::String) => Type::prim("String"),
        Some(K::TrueKw | K::FalseKw) => Type::prim("Bool"),
        _ => Type::Error,
    }
}

pub fn eff(services: Type, errors: Type, result: Type) -> Type {
    Type::apps(Type::prim("Eff"), [services, errors, result])
}
