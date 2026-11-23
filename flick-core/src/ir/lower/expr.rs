//! Expressions, blocks and match arms.

use super::Lower;
use super::numeric::{apply, arith_of};
use super::types::{app_result_type, apply_lift_type};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Handler, Lit, Param, Pat, Stage, Stmt};
use crate::numeric::Arith;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::handle::{arm_pattern, failed_pattern, operation_path};
use crate::types::lang::{self, Lang};
use crate::types::{
    Constructor, Dict, Identity, Owner, Range, Ref, StageKey, app_hole, applied_order, children,
    composed_key, composition, is_pattern, let_parts, param_key, result_key, right_application,
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
        if let Some(ty) = self.ir_type(key) {
            expr.ty = Some(ty);
        }
        if let Some(lift) = lift {
            let lifted_ty = expr.ty.as_ref().map(|t| apply_lift_type(t, &lift));
            expr = Expr::with_ty(ExprKind::Lift(Box::new(expr), lift), span, lifted_ty);
        }
        if let Some(coercion) = coercion {
            let target = self.ir_coercion_target(key);
            expr = Self::coercion_expr(expr, coercion.dict, coercion.method, span, target);
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
        let target = self.ir_coercion_target(key);
        Self::coercion_expr(expr, coercion.dict, coercion.method, span, target)
    }

    pub(super) fn coercion_expr(
        expr: Expr,
        dict: Dict,
        name: String,
        span: Span,
        target: Option<flick_ir::TypeRef>,
    ) -> Expr {
        let method_ty = expr.ty.clone().zip(target.clone()).map(|(from, to)| {
            flick_ir::TypeRef::new(flick_ir::IrType::Fun {
                param: from,
                stage: Stage::Pure,
                result: to,
            })
        });
        let method = Expr::with_ty(
            ExprKind::Method {
                dict,
                name,
                type_args: Vec::new(),
            },
            span,
            method_ty,
        );
        let mut out = apply(method, [Box::new(expr)], span);
        if out.ty.is_none() {
            out.ty = target;
        }
        out
    }

    fn boxed(&mut self, node: &Node) -> Box<Expr> {
        Box::new(self.expr(node))
    }

    /// `if`, whose missing `else` is `()`.
    fn if_expr(&mut self, children: &[&Node], span: Span) -> ExprKind {
        let (condition, then) = (self.boxed(children[0]), self.boxed(children[1]));
        let otherwise = children.get(2).map_or_else(
            || {
                Box::new(Expr::with_ty(
                    ExprKind::Lit(Lit::Unit),
                    span,
                    super::types::lang_bare(Lang::Unit),
                ))
            },
            |e| self.boxed(e),
        );
        ExprKind::If(condition, then, otherwise)
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
            K::AppExpr => return self.app_expr(node, children.as_slice(), span),
            K::HoleExpr => return self.hole_expr(node, span),
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
                    Some(K::AwaitKw) if self.typing.forced.contains(&node.range()) => {
                        let local = self.typing.read_lazy.contains(&node.range());
                        self.force(*operand, span, local)
                    }
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
            K::TryExpr | K::CatchExpr => {
                let body = *self.boxed(children[0]);
                ExprKind::Handle(Box::new(Handler {
                    effects: Vec::new(),
                    body,
                    ret: None,
                    errors: self.arms(node),
                }))
            }
            K::ResultExpr => self.result_expr(children[0], span),
            K::TaskExpr => {
                let key = StageKey::Task { node: node.range() };
                self.register_owner_entry(key);
                let body = self.expr(children[0]);
                ExprKind::Task(Box::new(self.owner_entry(key, body)))
            }
            K::LazyExpr => self.lazy_expr(node, children[0], span),
            K::DoExpr => return self.do_expr(node),
            K::ScopeExpr => return self.scope_expr(node, children[0], span),
            K::DeferStmt => return self.defer_stmt(node),
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
        let expr_ty = self.ir_type(key);
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
                type_args: self.global_type_args(key, module, name, expr_ty.as_ref()),
            },
            Ref::Method { name, .. } => {
                let dict = dicts.remove(0);
                let type_args = self.ir_type_args(key);
                ExprKind::Method {
                    dict,
                    name: name.clone(),
                    type_args,
                }
            }
            Ref::Constructor(c) => {
                let mut c = c.clone();
                if let Some(fields) = self.ir_con_fields(key) {
                    c.fields = fields;
                }
                let type_args = self.ir_type_args(key);
                if !type_args.is_empty() {
                    c.type_args = type_args;
                }
                ExprKind::Con(c)
            }
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
        let expr = Expr::with_ty(kind, span, expr_ty);
        with_dicts(expr, dicts)
    }

    /// `f a b` as `(f a) b`, each stage with its mode and dictionaries.
    pub(super) fn apply(&mut self, head: &Node, written: &[&Node]) -> Expr {
        self.apply_inner(head, written, true)
    }

    /// [`Self::apply`]; when `use_node_ty` is false (inside an expression-hole
    /// lambda body where the outer application's range holds the lambda type),
    /// intermediate and final result types come from `app_result_type(&func)`.
    pub(super) fn apply_inner(
        &mut self,
        head: &Node,
        written: &[&Node],
        use_node_ty: bool,
    ) -> Expr {
        let start = head.range().0;
        let mut func = self.expr(head);
        // each line under the call is an argument too
        let args = children::arguments(written);
        let end = args.last().map_or(head.range().1, |last| last.range().1);
        // the arguments that are the elements of the list the function takes last
        let gathered = self.typing.gathered.get(&(start, end)).copied();
        let alone = gathered.map_or(args.len(), |at| at.min(args.len()));
        for (i, arg) in args[..alone].iter().enumerate() {
            let key = (start, arg.range().1);
            let span = Span::new(self.file, key.0, key.1);
            func = self.ui_callable(func, key);
            let is_final = use_node_ty && gathered.is_none() && i + 1 == alone;
            let res_ty = if is_final {
                self.ir_type(key).or_else(|| app_result_type(&func))
            } else {
                app_result_type(&func)
            };
            let arg = self.argument(arg);
            let app = ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: self.call_stage(key),
                arg_first: false,
            };
            let dicts = self.dictionaries(key);
            func = with_dicts(self.within(key, Expr::with_ty(app, span, res_ty)), dicts);
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
            let list_ty = match func.ty.as_deref() {
                Some(flick_ir::IrType::Fun { param, .. }) => Some(param.clone()),
                _ => None,
            };
            let res_ty = if use_node_ty {
                self.ir_type((start, end))
                    .or_else(|| app_result_type(&func))
            } else {
                app_result_type(&func)
            };
            let items = args[alone..].iter().map(|child| self.expr(child)).collect();
            let app = ExprKind::App {
                func: Box::new(func),
                arg: Box::new(Expr::with_ty(ExprKind::List(items), span, list_ty)),
                stage: self.call_stage(key),
                arg_first: false,
            };
            let dicts = self.dictionaries(key);
            func = with_dicts(self.within(key, Expr::with_ty(app, span, res_ty)), dicts);
        }
        func
    }
}

include!("block.rs");
include!("expr_tail.rs");

pub(super) fn with_dicts(expr: Expr, dicts: Vec<Dict>) -> Expr {
    if dicts.is_empty() {
        return expr;
    }
    let span = expr.span;
    let ty = expr.ty.clone();
    Expr::with_ty(ExprKind::Dicts(Box::new(expr), dicts), span, ty)
}
