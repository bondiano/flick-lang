//! Expressions, blocks and match arms.

use std::sync::Arc;

use super::Lower;
use super::atoms::completed;
use super::numeric::{apply, arith_of};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use crate::numeric::Arith;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::{self, Lang};
use crate::types::{Dict, Identity, Range, Ref, is_pattern, let_parts};

impl Lower<'_> {
    /// An expression with the lift and the dictionaries its position gives it.
    pub(super) fn expr(&mut self, node: &Node) -> Expr {
        let key = node.range();
        let lift = self
            .consumed
            .insert((key, false))
            .then(|| self.typing.lifts.get(&key).cloned())
            .flatten();
        let dicts = self
            .consumed
            .insert((key, true))
            .then(|| self.typing.givens.get(&key).cloned())
            .flatten();
        let span = self.span(node);
        let mut expr = self.expr_kind(node);
        if let Some(ty) = self.typing.values.get(&key) {
            expr.ty = Some(Arc::new(ty.clone()));
        }
        if let Some(lift) = lift {
            expr = Expr::new(ExprKind::Lift(Box::new(expr), lift), span);
        }
        if let Some(dicts) = dicts {
            expr = Expr::new(ExprKind::Abstract(dicts, Box::new(expr)), span);
        }
        expr
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

    fn expr_kind(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let children: Vec<&Node> = node.nodes().collect();
        let kind = match node.kind {
            K::Literal if self.typing.generic_numeric.contains(&node.range()) => {
                return self.generic_literal(node, span);
            }
            K::Literal => ExprKind::Lit(self.literal(node)),
            K::UnitExpr => ExprKind::Lit(Lit::Unit),
            // the checker resolved `server name` to the implementation
            K::NameExpr | K::ServerExpr => return self.name(node),
            K::ParenExpr | K::AnnotExpr => return self.expr(children[0]),
            K::TupleExpr => ExprKind::Tuple(children.iter().map(|c| self.expr(c)).collect()),
            K::ListExpr => return self.list(node, &children),
            K::ForExpr => return self.for_expr(node, false),
            K::RangeExpr => return self.range(node, false),
            K::QuestionExpr => return self.question(node, children[0]),
            K::FieldExpr => return self.field(node, children[0]),
            K::RecordExpr => ExprKind::Record(self.fields(node)),
            K::RecordUpdate => {
                let record = self.boxed(children[0]);
                ExprKind::Update(record, self.fields(node))
            }
            K::AppExpr => return self.apply(children[0], &children[1..]),
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
            K::ComponentExpr => return self.component(node, &children),
            K::SqlExpr => return self.sql_block(node),
            // `Ui` names and implicit component applications were resolved by the checker.
            K::UiExpr => {
                return match children.iter().find(|n| n.kind == K::Block) {
                    Some(body) => self.block(body),
                    None => Expr::new(ExprKind::Lit(Lit::Unit), span),
                };
            }
            K::IfExpr => self.if_expr(&children, span),
            K::MatchExpr => {
                let scrutinee = self.boxed(children[0]);
                ExprKind::Match(scrutinee, self.arms(node))
            }
            K::TryExpr => {
                let body = self.boxed(children[0]);
                ExprKind::Try(body, self.arms(node))
            }
            K::TaskExpr => ExprKind::Task(self.boxed(children[0])),
            K::DoExpr => return self.do_expr(node),
            K::ScopeExpr => {
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
                let body = self.boxed(children[0]);
                self.scope.truncate(mark);
                ExprKind::Scope { owner, outer, body }
            }
            K::ProvideExpr => ExprKind::Provide(self.boxed(children[0]), self.boxed(children[1])),
            K::SchemaExpr | K::CodecExpr => self.schema_or_codec(node),
            _ => {
                self.unsupported(node);
                ExprKind::Lit(Lit::Unit)
            }
        };
        Expr::new(kind, span)
    }

    /// `schema T` names the witness; a `codec` block lists its fields.
    fn schema_or_codec(&mut self, node: &Node) -> ExprKind {
        match node.kind {
            K::SchemaExpr => match self.typing.names.get(&node.range()) {
                Some(Ref::Value { module, name }) => ExprKind::Global {
                    module: module.clone(),
                    name: name.clone(),
                },
                _ => ExprKind::Lit(Lit::Unit),
            },
            K::CodecExpr => {
                let name = self
                    .typing
                    .codecs
                    .get(&node.range())
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let fields = node
                    .nodes()
                    .filter(|n| n.kind == K::RecordExpr)
                    .flat_map(Node::nodes)
                    .filter(|n| n.kind == K::RecordField)
                    .filter_map(|f| {
                        let label = self.token_text(f, K::Ident)?;
                        Some((label, self.expr(f.nodes().next()?)))
                    })
                    .collect();
                ExprKind::CodecBlock(name, fields)
            }
            _ => ExprKind::Lit(Lit::Unit),
        }
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
        let mut dicts = self.typing.dicts.get(&key).cloned().unwrap_or_default();
        let kind = match r {
            Ref::Local => ExprKind::Local(
                self.local(&last, key.0)
                    .unwrap_or_else(|| panic!("`{last}` is a local in scope"))
                    .to_owned(),
            ),
            Ref::Value { module, name } if lang::is(Lang::RecordLabel, module, name) => {
                ExprKind::Label(self.typing.labels.get(&key).cloned().unwrap_or_default())
            }
            Ref::Value { module, name } if lang::is(Lang::WireCodec, module, name) => {
                self.wire_codec(key, span)
            }
            Ref::Value { module, name } if module == "Atom" && completed(name) => {
                self.atom_call(name, node, span)
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
            } => ExprKind::Operation {
                service: service.clone(),
                name: name.clone(),
                stages: stages.clone(),
            },
            Ref::ServiceTag(key) => ExprKind::ServiceTag(key.clone()),
        };
        let expr = Expr::new(kind, span);
        with_dicts(expr, dicts)
    }

    /// `f a b` as `(f a) b`, each stage with its mode and dictionaries.
    fn apply(&mut self, head: &Node, args: &[&Node]) -> Expr {
        let start = head.range().0;
        let mut func = self.expr(head);
        for arg in args {
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
            let dicts = self.typing.dicts.get(&key).cloned().unwrap_or_default();
            func = with_dicts(self.within(key, Expr::new(app, span)), dicts);
        }
        func
    }

    fn ui_callable(&self, func: Expr, key: Range) -> Expr {
        if !self.typing.ui_components.contains(&key) {
            return func;
        }
        let span = func.span;
        let component = ExprKind::Global {
            module: "Ui".to_owned(),
            name: "component".to_owned(),
        };
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

    fn binary(&mut self, node: &Node, children: &[&Node]) -> Expr {
        let span = self.span(node);
        let key = node.range();
        let op = node
            .child_tokens()
            .find(|t| t.kind.is_binary_op())
            .map(|t| t.kind);
        let lhs = self.boxed(children[0]);
        let rhs = self.expr(children[1]);
        let rhs = Box::new(if op == Some(K::PipeGt) {
            self.ui_callable(rhs, key)
        } else {
            rhs
        });
        if let (Some(kind @ (K::EqEq | K::NotEq)), Some(&identity)) =
            (op, self.typing.identity_eq.get(&key))
        {
            let op = if kind == K::EqEq {
                BinOp::Same
            } else {
                BinOp::NotSame
            };
            let tag = |e: Box<Expr>| {
                let span = e.span;
                Box::new(Expr::new(ExprKind::Field(e, "_tag".to_owned()), span))
            };
            let (lhs, rhs) = match identity {
                Identity::Value => (lhs, rhs),
                Identity::Tag => (tag(lhs), tag(rhs)),
            };
            return Expr::new(ExprKind::Binary(op, lhs, rhs), span);
        }
        if let Some(generic) = op.and_then(|op| self.generic_binary(key, op, &lhs, &rhs, span)) {
            return generic;
        }
        let op = match op {
            Some(K::PipeGt) => {
                let app = ExprKind::App {
                    func: rhs,
                    arg: lhs,
                    stage: self.call_stage(key),
                    arg_first: true,
                };
                let dicts = self.typing.dicts.get(&key).cloned().unwrap_or_default();
                return with_dicts(self.within(key, Expr::new(app, span)), dicts);
            }
            Some(K::OrOr) => BinOp::Or,
            Some(K::AndAnd) => BinOp::And,
            Some(K::EqEq) => BinOp::Eq,
            Some(K::NotEq) => BinOp::NotEq,
            Some(K::Lt) => BinOp::Lt,
            Some(K::Le) => BinOp::Le,
            Some(K::Gt) => BinOp::Gt,
            Some(K::Ge) => BinOp::Ge,
            Some(K::PlusPlus) if self.typing.list_concat.contains(&key) => BinOp::ListConcat,
            Some(K::PlusPlus) => BinOp::Concat,
            k => BinOp::Arith(k.and_then(arith_of).unwrap_or(Arith::Pow), self.num(key)),
        };
        Expr::new(ExprKind::Binary(op, lhs, rhs), span)
    }

    /// `label = value` fields of a record literal or update, in order.
    fn fields(&mut self, node: &Node) -> Vec<(String, Expr)> {
        node.nodes()
            .filter(|n| n.kind == K::RecordField)
            .filter_map(|field| {
                let label = self.token_text(field, K::Ident)?;
                let value = field.nodes().next()?;
                Some((label, self.expr(value)))
            })
            .collect()
    }

    pub(super) fn block(&mut self, node: &Node) -> Expr {
        let span = self.span(node);
        let mark = self.scope.len();
        let statements: Vec<&Node> = node.nodes().collect();
        let block = self.statements(&statements, span);
        self.scope.truncate(mark);
        block
    }

    /// The statements of a block; a `let assert` takes the ones after it.
    pub(super) fn statements(&mut self, statements: &[&Node], span: Span) -> Expr {
        let mut stmts = Vec::new();
        let mut result = None;
        for (i, statement) in statements.iter().enumerate() {
            let last = i + 1 == statements.len();
            match statement.kind {
                K::Signature => {}
                K::LetDecl if statement.has_token(K::AssertKw) => {
                    let rest = &statements[i + 1..];
                    result = Some(self.let_assert(statement, span, |l| l.statements(rest, span)));
                    break;
                }
                K::LetDecl => stmts.push(self.local_let(statement)),
                _ if last => result = Some(self.expr(statement)),
                _ => stmts.push(Stmt::Expr(self.expr(statement))),
            }
        }
        let result = result.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span));
        if stmts.is_empty() {
            return result;
        }
        Expr::new(ExprKind::Block(stmts, Box::new(result)), span)
    }

    pub(super) fn local_let(&mut self, decl: &Node) -> Stmt {
        let parts = let_parts(decl, self.text);
        let Some(name) = parts.name.clone() else {
            let value = self.definition(decl);
            let pat = parts.pattern.map_or(Pat::Wild, |p| self.pattern(p));
            return Stmt::Let(pat, value);
        };
        let dicts = self.givens(decl);
        let at =
            (decl.nodes().find(|n| n.kind == K::VarPat)).map_or(decl.range().0, |n| n.range().0);
        if parts.rec {
            let unique = self.bind(&name, at);
            let value = self.definition(decl);
            return Stmt::Fun {
                name: unique,
                rec: true,
                dicts,
                value,
            };
        }
        let value = self.definition(decl);
        let unique = self.bind(&name, at);
        if parts.params.is_empty() && dicts.is_empty() {
            Stmt::Let(Pat::Bind(unique), value)
        } else {
            Stmt::Fun {
                name: unique,
                rec: false,
                dicts,
                value,
            }
        }
    }

    fn arms(&mut self, node: &Node) -> Vec<Arm> {
        node.nodes()
            .filter(|n| n.kind == K::MatchArm)
            .map(|arm| {
                let mark = self.scope.len();
                let mut pat = Pat::Wild;
                let mut guard = None;
                let mut body = None;
                for child in arm.nodes() {
                    match child.kind {
                        K::Guard => guard = child.nodes().next().map(|g| self.expr(g)),
                        K::Block => body = Some(self.expr(child)),
                        kind if is_pattern(kind) => pat = self.pattern(child),
                        _ => {}
                    }
                }
                self.scope.truncate(mark);
                let span = self.span(arm);
                Arm {
                    pat,
                    guard,
                    body: body.unwrap_or_else(|| Expr::new(ExprKind::Lit(Lit::Unit), span)),
                }
            })
            .collect()
    }
}

pub(super) fn with_dicts(expr: Expr, dicts: Vec<Dict>) -> Expr {
    if dicts.is_empty() {
        return expr;
    }
    let span = expr.span;
    Expr::new(ExprKind::Dicts(Box::new(expr), dicts), span)
}
