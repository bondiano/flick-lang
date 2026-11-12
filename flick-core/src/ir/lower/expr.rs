//! Expressions, blocks and match arms.

use super::numeric::{apply, arith_of};
use super::{Lower, type_name};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Handler, Lit, Param, Pat, Stage, Stmt};
use crate::numeric::Arith;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::handle::{arm_pattern, failed_pattern, operation_path};
use crate::types::lang::{self, Lang};
use crate::types::{
    Dict, Identity, Range, Ref, applied_order, children, composed_key, composition, is_pattern,
    let_parts, param_key, result_key, right_application,
};

impl Lower<'_> {
    /// An expression with the lift and the dictionaries its position gives it.
    pub(super) fn expr(&mut self, node: &Node) -> Expr {
        let key = node.range();
        let first = self.consumed.insert((key, false));
        let lift = first
            .then(|| self.typing.lifts.get(&key).cloned())
            .flatten();
        let coercion = first
            .then(|| self.typing.coercions.get(&key).cloned())
            .flatten();
        let dicts = self
            .consumed
            .insert((key, true))
            .then(|| self.typing.givens.get(&key).cloned())
            .flatten();
        let span = self.span(node);
        let mut expr = self.expr_kind(node);
        if let Some(ty) = self.typing.values.get(&key) {
            expr.ty = type_name(ty);
        }
        if let Some(lift) = lift {
            expr = Expr::new(ExprKind::Lift(Box::new(expr), lift), span);
        }
        if let Some(coercion) = coercion {
            expr = Self::coercion_expr(expr, coercion.dict, coercion.method, span);
        }
        if let Some(dicts) = dicts {
            expr = super::abstraction::abstract_value(dicts, expr);
            expr.span = span;
        }
        expr
    }

    /// Applies a conversion recorded at a range that lowers to generated IR.
    pub(super) fn apply_coercion(&mut self, key: Range, expr: Expr) -> Expr {
        if !self.consumed.insert((key, false)) {
            return expr;
        }
        let Some(coercion) = self.typing.coercions.get(&key).cloned() else {
            return expr;
        };
        let span = expr.span;
        Self::coercion_expr(expr, coercion.dict, coercion.method, span)
    }

    fn coercion_expr(expr: Expr, dict: Dict, name: String, span: Span) -> Expr {
        let method = Expr::new(ExprKind::Method { dict, name }, span);
        apply(method, [Box::new(expr)], span)
    }

    fn boxed(&mut self, node: &Node) -> Box<Expr> {
        Box::new(self.expr(node))
    }

    /// `if`, whose missing `else` is `()`.
    fn if_expr(&mut self, children: &[&Node], span: Span) -> ExprKind {
        let (condition, then) = (self.boxed(children[0]), self.boxed(children[1]));
        let otherwise = children.get(2).map_or_else(
            || Box::new(Expr::new(ExprKind::Lit(Lit::Unit), span)),
            |e| self.boxed(e),
        );
        ExprKind::If(condition, then, otherwise)
    }

    fn pack_module_expr(&mut self, node: &Node, span: Span) -> Expr {
        let fields = self
            .typing
            .packs
            .get(&node.range())
            .expect("the checker validated `pack`")
            .iter()
            .map(|(field, module, name)| {
                (
                    field.clone(),
                    Expr::new(
                        ExprKind::Global {
                            module: module.clone(),
                            name: name.clone(),
                        },
                        span,
                    ),
                )
            })
            .collect();
        Expr::new(ExprKind::Record(fields), span)
    }

    fn unpack_module_expr(&mut self, node: &Node, children: &[&Node], span: Span) -> Expr {
        let Some(source) = children.first() else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let Some(body) = children.iter().find(|n| n.kind == K::Block) else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let Some(alias) = node.child_tokens().find(|t| t.kind == K::Con) else {
            self.unsupported(node);
            return Expr::new(ExprKind::Lit(Lit::Unit), span);
        };
        let source = self.expr(source);
        let mark = self.scope.len();
        let mut statements = Vec::new();
        let opened = self.typing.existential_opens.contains(&node.range());
        let pair = if opened {
            let pair = self.bind("$existential", node.range().0);
            statements.push(Stmt::Let(Pat::Bind(pair.clone()), source.clone()));
            Some(pair)
        } else {
            None
        };
        let unique = self.bind(alias.text(self.text), alias.start);
        if let Some(pair) = &pair {
            let pair_value = Expr::new(ExprKind::Local(pair.clone()), span);
            let module = Expr::new(
                ExprKind::Field(Box::new(pair_value), "$module".to_owned()),
                span,
            );
            statements.push(Stmt::Let(Pat::Bind(unique.clone()), module));
            let mut tokens = node.child_tokens();
            let payload = tokens
                .find(|t| t.kind == K::Comma)
                .and_then(|_| tokens.find(|t| t.kind == K::Ident));
            if let Some(payload) = payload {
                let value = self.bind(payload.text(self.text), payload.start);
                let pair_value = Expr::new(ExprKind::Local(pair.clone()), span);
                let field = Expr::new(
                    ExprKind::Field(Box::new(pair_value), "$value".to_owned()),
                    span,
                );
                statements.push(Stmt::Let(Pat::Bind(value), field));
            }
        } else {
            statements.push(Stmt::Let(Pat::Bind(unique.clone()), source));
        }
        let body = self.expr(body);
        self.scope.truncate(mark);
        let result = if self.typing.existential_unpacks.contains(&node.range()) {
            Expr::new(
                ExprKind::Record(vec![
                    (
                        "$module".to_owned(),
                        Expr::new(ExprKind::Local(unique.clone()), span),
                    ),
                    ("$value".to_owned(), body),
                ]),
                span,
            )
        } else {
            body
        };
        Expr::new(ExprKind::Block(statements, Box::new(result)), span)
    }

    fn app_expr(&mut self, children: &[&Node], span: Span) -> Expr {
        if let [callee, argument] = children
            && let Some(slots) = self.typing.dictionary_constructors.get(&callee.range())
        {
            let mut fields = self.fields(argument);
            for (written, runtime) in slots {
                if let Some((label, _)) = fields.iter_mut().find(|(label, _)| label == written) {
                    runtime.clone_into(label);
                }
            }
            Expr::new(ExprKind::Dictionary(fields), span)
        } else {
            self.apply(children[0], &children[1..])
        }
    }

    fn scope_expr(&mut self, node: &Node, body: &Node, span: Span) -> Expr {
        let mark = self.scope.len();
        let key = node.range();
        let uses = |outside| {
            self.typing
                .owners
                .values()
                .any(|o| o.scope == key && o.outside == outside)
        };
        let (inner, outside) = (uses(false), uses(true));
        let owner = match node.child_tokens().find(|t| t.kind == K::Ident) {
            Some(name) => Some(self.bind(name.text(self.text), name.start)),
            None => inner.then(|| self.hidden("$owner")),
        };
        let outer = outside.then(|| self.hidden("$outer"));
        self.owners.insert(key, (owner.clone(), outer.clone()));
        let body = self.expr(body);
        self.scope.truncate(mark);
        Self::scope_run(owner, outer, body, span)
    }

    fn expr_kind(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let children: Vec<&Node> = node.nodes().collect();
        let kind = match node.kind {
            K::Literal if self.typing.generic_numeric.contains(&node.range()) => {
                return self.generic_literal(node, span);
            }
            K::Literal => ExprKind::Lit(self.literal(node)),
            K::InterpExpr => return self.interpolation(node),
            K::UnitExpr => ExprKind::Lit(Lit::Unit),
            // the checker resolved `server name` to the implementation
            K::NameExpr => return self.name(node),
            K::PackExpr => return self.pack_module_expr(node, span),
            K::UnpackExpr => return self.unpack_module_expr(node, children.as_slice(), span),
            K::ParenExpr | K::AnnotExpr => return self.expr(children[0]),
            K::TupleExpr => ExprKind::Tuple(children.iter().map(|c| self.expr(c)).collect()),
            K::ListExpr => return self.list(node, &children),
            K::LoopExpr | K::WhileExpr => return self.control_loop(node),
            K::BreakExpr | K::ContinueExpr => return self.loop_exit(node),
            K::ForExpr => return self.for_expr(node, false),
            K::RangeExpr => return self.range(node, false),
            K::QuestionExpr => return self.question(node, children[0]),
            K::FieldExpr => return self.field(node, children[0]),
            K::RecordExpr => ExprKind::Record(self.fields(node)),
            K::RecordUpdate => {
                let record = self.boxed(children[0]);
                ExprKind::Update(record, self.fields(node))
            }
            K::AppExpr => return self.app_expr(children.as_slice(), span),
            K::BinExpr => return self.binary(node, &children),
            K::TodoExpr => return self.todo(node),
            K::PrefixExpr if node.has_token(K::EchoKw) => {
                return self.echo(node, children.first().copied());
            }
            K::PrefixExpr if self.negative_int(node, &children) => {
                ExprKind::Lit(self.signed_literal(children[0], true))
            }
            K::PrefixExpr => {
                let operand = self.boxed(children[0]);
                match node.child_tokens().next().map(|t| t.kind) {
                    Some(K::NotKw) => ExprKind::Not(operand),
                    Some(K::IgnoreKw) => ExprKind::Ignore(operand),
                    Some(K::AwaitKw) => {
                        let span = self.span(node);
                        let wait = Expr::new(ExprKind::Await(operand), span);
                        return self.within(node.range(), wait);
                    }
                    _ if self.typing.generic_numeric.contains(&node.range()) => {
                        let negate = self.prelude_method(node.range(), Lang::Negate);
                        return apply(negate, [operand], span);
                    }
                    _ => ExprKind::Neg(self.num(node.range()), operand),
                }
            }
            K::FunExpr => return self.lambda(node, &children),
            K::Block => return self.block(node),
            // the names of the open module, and inside `Ui` the implicit component
            // applications, were resolved by the checker
            K::OpenExpr => {
                // the body is last, after the type of a child when one is named
                return match children.last() {
                    Some(body) => self.expr(body),
                    None => Expr::new(ExprKind::Lit(Lit::Unit), span),
                };
            }
            K::IfExpr => self.if_expr(&children, span),
            K::MatchExpr => {
                let scrutinee = self.boxed(children[0]);
                ExprKind::Match(scrutinee, self.arms(node))
            }
            K::TryExpr | K::ProvideExpr if self.handles_effects(node) => self.handler(node),
            K::TryExpr => {
                let body = *self.boxed(children[0]);
                ExprKind::Handle(Box::new(Handler {
                    effects: Vec::new(),
                    body,
                    ret: None,
                    errors: self.arms(node),
                }))
            }
            K::TaskExpr => ExprKind::Task(self.boxed(children[0])),
            K::DoExpr => return self.do_expr(node),
            K::ScopeExpr => return self.scope_expr(node, children[0], span),
            K::ProvideExpr => self.layer_run(children[0], children[1], span),
            _ => {
                self.unsupported(node);
                ExprKind::Lit(Lit::Unit)
            }
        };
        Expr::new(kind, span)
    }

    /// A field; a polymorphic one with constraints takes their dictionaries.
    fn field(&mut self, node: &Node, record: &Node) -> Expr {
        let label = self.token_text(node, K::Ident).unwrap_or_default();
        let field = Expr::new(ExprKind::Field(self.boxed(record), label), self.span(node));
        let dicts = self
            .typing
            .dicts
            .get(&node.range())
            .cloned()
            .unwrap_or_default();
        with_dicts(field, dicts)
    }

    fn name(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let key = node.range();
        let last = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Ident | K::Con))
            .last()
            .map(|t| t.text(self.text).to_owned())
            .unwrap_or_default();
        let r = self
            .typing
            .names
            .get(&key)
            .unwrap_or_else(|| panic!("the checker resolved `{last}`"));
        let mut dicts = self.dictionaries(key);
        let kind = match r {
            Ref::Local => {
                let unique = (self.local(&last, key.0))
                    .unwrap_or_else(|| panic!("`{last}` is a local in scope"))
                    .to_owned();
                if self.vars.contains(&unique) {
                    ExprKind::Var(unique)
                } else {
                    ExprKind::Local(unique)
                }
            }
            Ref::PackedField { binder, field } => {
                let local = self
                    .local(binder, key.0)
                    .unwrap_or_else(|| panic!("`{binder}` is an opened module in scope"))
                    .to_owned();
                ExprKind::Field(
                    Box::new(Expr::new(ExprKind::Local(local), span)),
                    field.clone(),
                )
            }
            Ref::Value { module, name } if lang::is(Lang::RecordLabel, module, name) => {
                ExprKind::Label(self.typing.labels.get(&key).cloned().unwrap_or_default())
            }
            Ref::Value { module, name } if lang::is(Lang::WireCodec, module, name) => {
                self.wire_codec(key, span)
            }
            Ref::Value { module, name } => ExprKind::Global {
                module: module.clone(),
                name: name.clone(),
            },
            Ref::Method { name, .. } => {
                let dict = dicts.remove(0);
                ExprKind::Method {
                    dict,
                    name: name.clone(),
                }
            }
            Ref::Constructor(c) => ExprKind::Con(c.clone()),
            Ref::Operation {
                service,
                name,
                stages,
            } => ExprKind::Perform {
                effect: service.clone(),
                op: name.clone(),
                stages: stages.clone(),
            },
            Ref::ServiceTag(key) => ExprKind::ServiceTag(key.clone()),
        };
        let expr = Expr::new(kind, span);
        with_dicts(expr, dicts)
    }

    /// `f a b` as `(f a) b`, each stage with its mode and dictionaries.
    fn apply(&mut self, head: &Node, written: &[&Node]) -> Expr {
        let start = head.range().0;
        let mut func = self.expr(head);
        // each line under the call is an argument too
        let args = children::arguments(written);
        let end = args.last().map_or(head.range().1, |last| last.range().1);
        // the arguments that are the elements of the list the function takes last
        let gathered = self.typing.gathered.get(&(start, end)).copied();
        let alone = gathered.map_or(args.len(), |at| at.min(args.len()));
        for arg in &args[..alone] {
            let key = (start, arg.range().1);
            let span = Span::new(self.file, key.0, key.1);
            func = self.ui_callable(func, key);
            let arg = self.expr(arg);
            let app = ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: self.call_stage(key),
                arg_first: false,
            };
            let dicts = self.dictionaries(key);
            func = with_dicts(self.within(key, Expr::new(app, span)), dicts);
        }
        if gathered.is_some() {
            // as the checker placed it: at the call up to its last child, or
            // at its end when it has none
            let key = if alone < args.len() {
                (start, end)
            } else {
                (end, end)
            };
            let span = Span::new(self.file, key.0, key.1);
            let items = args[alone..].iter().map(|child| self.expr(child)).collect();
            let app = ExprKind::App {
                func: Box::new(func),
                arg: Box::new(Expr::new(ExprKind::List(items), span)),
                stage: self.call_stage(key),
                arg_first: false,
            };
            let dicts = self.dictionaries(key);
            func = with_dicts(self.within(key, Expr::new(app, span)), dicts);
        }
        func
    }

    fn ui_callable(&self, func: Expr, key: Range) -> Expr {
        let Some(Ref::Value { module, name }) = self.typing.through.get(&key).cloned() else {
            return func;
        };
        let span = func.span;
        let component = ExprKind::Global { module, name };
        Expr::new(
            ExprKind::App {
                func: Box::new(Expr::new(component, span)),
                arg: Box::new(func),
                stage: Stage::Pure,
                arg_first: false,
            },
            span,
        )
    }

    /// `call`, acquiring into the owner the checker chose for it.
    /// The stage of the call the application or pipe at `key` makes.
    pub(super) fn call_stage(&self, key: Range) -> Stage {
        if self.typing.runtime.contains(&key) {
            Stage::Runtime
        } else if self.typing.sync.contains(&key) {
            Stage::Sync
        } else {
            Stage::Pure
        }
    }

    fn within(&self, key: Range, call: Expr) -> Expr {
        let Some(owner) = self.typing.owners.get(&key) else {
            return call;
        };
        let names = &self.owners[&owner.scope];
        let name = if owner.outside { &names.1 } else { &names.0 };
        let name = name
            .clone()
            .expect("the scope binds the owner a call needs");
        let span = call.span;
        Expr::new(ExprKind::Within(name, Box::new(call)), span)
    }
}

include!("expr_tail.rs");

pub(super) fn with_dicts(expr: Expr, dicts: Vec<Dict>) -> Expr {
    if dicts.is_empty() {
        return expr;
    }
    let span = expr.span;
    Expr::new(ExprKind::Dicts(Box::new(expr), dicts), span)
}
