//! Expressions, checked bidirectionally: [`Checker::check`] pushes an expected
//! type into lambdas, branches and literals, so a higher-rank parameter type
//! from a signature reaches the callback that must satisfy it (§5
//! "Полиморфные callbacks"); [`Checker::infer`] synthesizes a type otherwise.

use std::mem;

use super::Checker;
use super::convert::is_type;
use super::effects::{Site, Stage};
use super::ty::{Row, Type};
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    pub fn check(&mut self, node: &Node, expected: &Type) {
        let target = self.whnf(expected);
        if let Type::Forall { .. } = target {
            self.store.level += 1;
            let (body, _) = self.skolemize(&target);
            self.check(node, &body);
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
            (K::TupleExpr, Type::Tuple(items)) if items.len() == children.len() => {
                for (child, item) in children.iter().zip(items) {
                    self.check(child, item);
                }
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
            (K::TaskExpr, _) if self.eff_parts(&target).is_some() => {
                self.task(children[0], Some(&target));
            }
            _ => {
                let actual = self.infer(node);
                self.expect(&actual, expected, self.span(node));
            }
        }
    }

    pub fn infer(&mut self, node: &Node) -> Type {
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
            K::TaskExpr => self.task(children[0], None),
            K::ScopeExpr => {
                let mark = self.locals.len();
                if let Some(name) = node.child_tokens().find(|t| t.kind == K::Ident) {
                    // shortcut: the scope handle is untyped until regions exist
                    let ty = self.fresh();
                    self.push_local(name.text(self.text), ty);
                }
                let ty = children.first().map_or(Type::Error, |b| self.infer(b));
                self.locals.truncate(mark);
                ty
            }
            K::ProvideExpr => self.provide(node, None),
            // shortcut: SQL, UI and Schema blocks are typed by their elaboration
            K::SqlExpr | K::UiExpr | K::CodecExpr | K::SchemaExpr => self.fresh(),
            _ => Type::Error,
        }
    }

    /// The type of a field of a record whose fields are already known,
    /// found without copying the record type: reading every field of a wide
    /// record stays linear.
    fn known_field(&self, record: &Type, label: &str) -> Option<Type> {
        let record = self.store.resolve(record);
        let (head, args) = record.spine();
        let (row, subst) = match head {
            Type::Record(row) => (row, Vec::new()),
            Type::Con { module, name } => match &self.env.type_def(module, name)?.form {
                super::env::Form::Alias {
                    params,
                    body: Type::Record(row),
                    ..
                } if params.len() == args.len() => {
                    let subst: Vec<(String, Type)> = params
                        .iter()
                        .cloned()
                        .zip(args.iter().map(|a| (*a).clone()))
                        .collect();
                    (row, subst)
                }
                _ => return None,
            },
            _ => return None,
        };
        row.get(label).map(|t| t.substitute(&subst))
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
        for branch in children.iter().skip(1) {
            self.check(branch, expected);
        }
    }

    fn field(&mut self, node: &Node, target: &Node) -> Type {
        let Some(label) = node.child_tokens().find(|t| t.kind == K::Ident) else {
            return Type::Error;
        };
        let label = label.text(self.text).to_owned();
        let record = self.infer(target);
        if let Some(field) = self.known_field(&record, &label) {
            return field;
        }
        let field = self.fresh();
        let rest = self.fresh();
        let shape = Type::Record(Row {
            entries: vec![(label.clone(), field.clone())],
            tail: Some(Box::new(rest)),
        });
        if self.unify(&record, &shape).is_err() {
            let shown = self.show(&record);
            self.error(
                codes::T003,
                self.span(node),
                format!("`{shown}` has no field `{label}`"),
            );
            return Type::Error;
        }
        field
    }

    /// A record literal; `expected` gives the types of fields it mentions.
    fn record(&mut self, node: &Node, expected: Option<&Row>) -> Type {
        let mut entries: Vec<(String, Type)> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let name = label.text(self.text).to_owned();
            let Some(value) = field.nodes().next() else {
                continue;
            };
            let ty = match expected.and_then(|row| row.get(&name)) {
                Some(ty) => {
                    let ty = ty.clone();
                    self.check(value, &ty);
                    ty
                }
                None => self.infer(value),
            };
            if entries.iter().any(|(l, _)| *l == name) {
                self.duplicate_label(&name, self.token_span(label));
            } else {
                entries.push((name, ty));
            }
        }
        Type::Record(Row::closed(entries))
    }

    /// `{ r with f = v }` replaces existing fields with values of the same type.
    fn record_update(&mut self, node: &Node, children: &[&Node]) -> Type {
        let record = self.infer(children[0]);
        let mut seen: Vec<String> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordField) {
            let Some(label) = field.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let name = label.text(self.text).to_owned();
            if seen.contains(&name) {
                self.duplicate_label(&name, self.token_span(label));
                continue;
            }
            let ty = self.fresh();
            let rest = self.fresh();
            let shape = Type::Record(Row {
                entries: vec![(name.clone(), ty.clone())],
                tail: Some(Box::new(rest)),
            });
            if self.unify(&record, &shape).is_err() {
                let shown = self.show(&record);
                self.error(
                    codes::T003,
                    self.token_span(label),
                    format!(
                        "`{shown}` has no field `{name}`; `with` only replaces existing fields"
                    ),
                );
            } else if let Some(value) = field.nodes().next() {
                self.check(value, &ty);
            }
            seen.push(name);
        }
        record
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
    /// that arrow is runtime exactly when the body makes a runtime call (§3).
    pub fn function(
        &mut self,
        params: &[&Node],
        body: Option<&Node>,
        expected: Option<&Type>,
        node: &Node,
    ) -> Type {
        let saved = mem::take(&mut self.runtime);
        let mark = self.locals.len();
        let mut rest = expected.cloned();
        let mut stages = Vec::new();
        // the first stage whose expected type was unknown, and that type
        let mut unknown: Option<(usize, Type)> = None;
        // the arrow the expected type gives the last stage
        let mut last = None;
        for param in params {
            let target = rest.as_ref().map(|t| self.whnf(t));
            let target = match target {
                Some(t @ Type::Forall { .. }) => Some(self.skolemize(&t).0),
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
            let pat = self.bind_pattern(param, &ty);
            self.irrefutable(&pat, param);
            stages.push((ty, effects));
        }
        let name = (node.kind == K::LetDecl)
            .then(|| super::bindings::let_parts(node, self.text).name)
            .flatten();
        let inferred = last.is_none();
        let stage = match last {
            Some(None) => Stage::pure(match &name {
                Some(name) => format!("`{name}` is pure: its signature has `->`, not `->{{…}}`"),
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
                self.bounded_stage(effects.services, effects.errors, site)
            }
            None => self.open_stage(),
        };
        let outer = mem::replace(&mut self.stage, stage);
        let result = match (rest, body) {
            (Some(rest), Some(body)) => {
                self.check(body, &rest);
                rest
            }
            (None, Some(body)) => self.infer(body),
            (_, None) => Type::Error,
        };
        let inner = mem::replace(&mut self.stage, outer);
        if inferred && let Some(stage) = stages.last_mut() {
            stage.1 = Self::stage_effects(&inner);
        }
        self.locals.truncate(mark);
        self.runtime = saved;
        let mut ty = result;
        for (i, (param, effects)) in stages.into_iter().enumerate().rev() {
            ty = Type::Fun {
                param: Box::new(param),
                effects,
                result: Box::new(ty),
            };
            if let Some((j, meta)) = &unknown
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
        let mut arms = Vec::new();
        for arm in nodes.filter(|n| n.kind == K::MatchArm) {
            arms.push(self.arm(arm, &scrutinee, &result));
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
    ) -> (super::patterns::Pat, bool, crate::source::Span) {
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
