//! Expressions and statements. A Go expression is returned; statements it
//! needs go into the current statement list before it.

use flick_pretty::{Document, docvec};

use super::arith::Native;
use super::go_type::{GoType, Typed, Want};
use super::layout::{Code, call, if_else, list, statement};
use super::{Gen, names};
use flick_ir::{Arm, BinOp, EnumLit, Expr, ExprKind, Lift, Lit, Stage};

/// A literal as Go: numbers typed as the runtime holds them.
pub(super) fn literal(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => format!("int32({n})"),
        Lit::Int8(n) => format!("int8({n})"),
        Lit::Int16(n) => format!("int16({n})"),
        Lit::Int64(n) => format!("int64({n})"),
        Lit::UInt8(n) => format!("uint8({n})"),
        Lit::UInt16(n) => format!("uint16({n})"),
        Lit::UInt32(n) => format!("uint32({n})"),
        Lit::UInt64(n) => format!("uint64({n})"),
        Lit::Float(x) if x.is_nan() => "flickrt.NaN".to_owned(),
        Lit::Float(x) if x.is_infinite() && *x > 0.0 => "flickrt.Inf".to_owned(),
        Lit::Float(x) if x.is_infinite() => "flickrt.NegInf".to_owned(),
        Lit::Float(x) if *x == 0.0 && x.is_sign_negative() => "flickrt.NegZero".to_owned(),
        // Rust's shortest digits read back to the same float in Go
        Lit::Float(x) => format!("float64({x:e})"),
        Lit::Float32(x) if x.is_nan() => "flickrt.NaN32".to_owned(),
        Lit::Float32(x) if x.is_infinite() && *x > 0.0 => "flickrt.Inf32".to_owned(),
        Lit::Float32(x) if x.is_infinite() => "flickrt.NegInf32".to_owned(),
        Lit::Float32(x) if *x == 0.0 && x.is_sign_negative() => "flickrt.NegZero32".to_owned(),
        Lit::Float32(x) => format!("float32({x:e})"),
        Lit::BigInt(n) => format!("flickrt.BigLit({})", names::string(n)),
        Lit::String(s) => names::string(s),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "flickrt.Unit".to_owned(),
    }
}

/// The literal an enum's constructor is, as Go holds it.
pub(super) fn enum_literal(literal: &EnumLit) -> String {
    match literal {
        EnumLit::Int(n) => self::literal(&Lit::Int(*n)),
        EnumLit::String(s) => self::literal(&Lit::String(s.clone())),
    }
}

/// A Go function that adapts a function of one stage where one of another
/// is expected, or the result of a task.
pub(super) fn lift(how: &Lift) -> String {
    match how {
        Lift::Stage { from, to, result } => format!(
            "flickrt.LiftStage({}, {}, {})",
            *from != Stage::Pure,
            *to != Stage::Pure,
            result.as_deref().map_or_else(|| "nil".to_owned(), lift)
        ),
        Lift::Eff(inner) => format!("flickrt.LiftEff({})", lift(inner)),
    }
}

impl<'a> Gen<'a> {
    /// Compiles expressions in order for erased `flickrt.Value` positions.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<Code<'a>> {
        let wants = vec![None; exprs.len()];
        self.ordered_into(exprs, &wants)
    }

    /// Compiles expressions in order, each into its requested `GoType` (`None`
    /// means erased `flickrt.Value`). An operand whose code needs statements
    /// spills earlier non-atomic operands into temporaries of their exact Go type.
    pub(super) fn ordered_into(
        &mut self,
        exprs: &[&Expr],
        wants: &[Option<GoType>],
    ) -> Vec<Code<'a>> {
        let a = self.a;
        let mut out: Vec<Typed<'a>> = Vec::new();
        let reads: Vec<bool> = exprs.iter().map(|e| reads_variable(e)).collect();
        for (i, e) in exprs.iter().enumerate() {
            let mark = self.buf.len();
            let want = wants.get(i).and_then(Option::as_ref);
            let mut go = self.eval(e, Want::of(want));
            let later = (i + 1..exprs.len())
                .any(|j| (!plain(e) && reads[j]) || (reads[i] && !plain(exprs[j])));
            if later && !go.code.atomic {
                let t = self.temp();
                if let Some(gt) = &go.ty {
                    self.register_go_type(gt);
                }
                let ty = self.go_type_name(go.ty.as_ref());
                self.emit(docvec![a, "var ", t, " ", ty, " = ", go.code]);
                go.code = self.code(t);
            }
            if self.buf.len() > mark {
                let mut spills = Vec::new();
                for prev in &mut out {
                    if !prev.code.atomic {
                        let t = self.temp();
                        if let Some(gt) = &prev.ty {
                            self.register_go_type(gt);
                        }
                        let ty = self.go_type_name(prev.ty.as_ref());
                        spills.push(statement(
                            a,
                            docvec![a, "var ", t, " ", ty, " = ", prev.code],
                        ));
                        prev.code = self.code(t);
                    }
                }
                self.buf.splice(mark..mark, spills);
            }
            out.push(go);
        }
        out.into_iter().map(|t| t.code).collect()
    }

    /// `go` as a `flickrt.Value` variable unless it is atomic.
    pub(super) fn spill(&mut self, go: Code<'a>) -> Code<'a> {
        if go.atomic {
            return go;
        }
        let t = self.temp();
        self.emit(docvec![self.a, "var ", t, " flickrt.Value = ", go]);
        self.code(t)
    }

    /// Declares the local `name` as an erased `flickrt.Value`.
    pub(super) fn bind(&mut self, name: &str, value: Code<'a>) {
        self.bind_typed(name, value, None);
    }

    /// Compiles `e` to an expression valid as `flickrt.Value` (never an
    /// unconverted typed `Option`).
    pub(super) fn expr(&mut self, e: &Expr) -> Code<'a> {
        self.eval(e, Want::Value).code
    }

    /// Compiles `e` in its natural typed Go representation.
    pub(super) fn expr_typed(&mut self, e: &Expr) -> Typed<'a> {
        self.eval(e, Want::Any)
    }

    /// Compiles `e` into the exact requested Go type (`None` means erased
    /// `flickrt.Value`).
    pub(super) fn expr_into(&mut self, e: &Expr, want: Option<&GoType>) -> Code<'a> {
        self.eval(e, Want::of(want)).code
    }

    /// Compiles `e` as a Go `bool`.
    pub(super) fn expr_bool(&mut self, e: &Expr) -> Code<'a> {
        self.expr_into(e, Some(&GoType::Scalar(Native::Bool)))
    }

    /// Evaluates `e` honoring `want`:
    /// - `Want::Value` guarantees the returned `Typed.ty` is never `Some(GoType::Opt(_))`.
    /// - `Want::Any` preserves the natural Go representation (scalars and typed `Option`).
    /// - `Want::Exact(gt)` guarantees the returned code has Go type `gt`.
    pub(super) fn eval(&mut self, e: &Expr, want: Want<'_>) -> Typed<'a> {
        let a = self.a;
        let raw = match &e.kind {
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                return self.eval(result, want);
            }
            ExprKind::If(cond, then, other) => return self.if_expr(e, cond, then, other, want),
            ExprKind::Match(scrutinee, arms) => return self.match_expr(e, scrutinee, arms, want),
            ExprKind::Con(_) | ExprKind::App { .. } => self.app_typed(e, want),
            ExprKind::Dicts(inner, _) if matches!(inner.kind, ExprKind::Con(_)) => {
                self.app_typed(e, want)
            }
            ExprKind::Lit(lit) => {
                let ty = match lit {
                    Lit::Int(_) => Some(GoType::Scalar(Native::Int)),
                    Lit::Int8(_) => Some(GoType::Scalar(Native::Int8)),
                    Lit::Int16(_) => Some(GoType::Scalar(Native::Int16)),
                    Lit::Int64(_) => Some(GoType::Scalar(Native::Int64)),
                    Lit::UInt8(_) => Some(GoType::Scalar(Native::UInt8)),
                    Lit::UInt16(_) => Some(GoType::Scalar(Native::UInt16)),
                    Lit::UInt32(_) => Some(GoType::Scalar(Native::UInt32)),
                    Lit::UInt64(_) => Some(GoType::Scalar(Native::UInt64)),
                    Lit::Float(x) if Native::is_native_float_lit(*x) => {
                        Some(GoType::Scalar(Native::Float))
                    }
                    Lit::Float32(x) if Native::is_native_float32_lit(*x) => {
                        Some(GoType::Scalar(Native::Float32))
                    }
                    Lit::String(_) => Some(GoType::Scalar(Native::String)),
                    Lit::Bool(_) => Some(GoType::Scalar(Native::Bool)),
                    _ => None,
                };
                Typed {
                    code: self.lit(lit),
                    ty,
                }
            }
            ExprKind::Local(name) => Typed {
                code: self.code(&names::local(name)),
                ty: self.go_types.get(name).cloned(),
            },
            // not atomic: an operand after it that assigns it is ordered after the read
            ExprKind::Var(name) => Typed {
                code: Code::from(a.text(a.alloc_str(&names::local(name)))),
                ty: self.go_types.get(name).cloned(),
            },
            ExprKind::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs),
            ExprKind::Not(inner) => Typed {
                code: docvec![a, "!", self.expr_bool(inner).doc].into(),
                ty: Some(GoType::Scalar(Native::Bool)),
            },
            ExprKind::Unwrap(inner) => self.unwrap(inner),
            ExprKind::Neg(num, inner) => self.neg(*num, inner),
            ExprKind::Record(fields) => self.record_typed(e, fields, want),
            ExprKind::Update(base, fields) => self.update_typed(e, base, fields, want),
            ExprKind::Field(record, label) => self.field_typed(record, label),
            _ => Typed {
                code: self.erased_expr(e),
                ty: None,
            },
        };
        match want {
            Want::Any => raw,
            Want::Value => match raw.ty {
                Some(ref gt) if gt.needs_boxing() => Typed {
                    code: self.to_value(raw),
                    ty: None,
                },
                _ => raw,
            },
            Want::Exact(gt) => {
                self.register_go_type(gt);
                Typed {
                    code: self.coerce(raw, Some(gt)),
                    ty: Some(gt.clone()),
                }
            }
        }
    }

    fn erased_expr(&mut self, e: &Expr) -> Code<'a> {
        let a = self.a;
        match &e.kind {
            ExprKind::ControlLoop { .. } | ExprKind::LoopExit { .. } | ExprKind::Loop { .. } => {
                self.loop_expr(e)
            }
            ExprKind::Global { module, name, .. } => self.global_value(module, name, Vec::new()),
            ExprKind::Method { dict, name, .. } => self.method(dict, name),
            ExprKind::Dicts(inner, dicts) => self.given(inner, dicts),
            ExprKind::Abstract(shown, inner) => self.abstraction(shown, inner),
            ExprKind::Lambda { params, body } => self.lambda(params, body, None),
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                let items = self.ordered(&items).into_iter().map(|item| item.doc);
                list(a, "[]flickrt.Value{", items, "}").into()
            }
            ExprKind::Dictionary(fields) => {
                let all: Vec<&Expr> = fields.iter().map(|(_, value)| value).collect();
                let values = self.ordered(&all);
                call(self.a, "flickrt.DictOf", self.entries(fields, values)).into()
            }
            ExprKind::Ignore(inner) => {
                let v = self.expr_typed(inner).code;
                // a result read by nothing: Go refuses a variable nobody reads
                self.emit(docvec![a, "_ = ", v]);
                self.unit()
            }
            ExprKind::Lift(inner, how) => {
                let v = self.expr(inner);
                call(a, self.once(&lift(how)), [v.doc]).into()
            }
            ExprKind::Each { .. } | ExprKind::Yield(_) => {
                self.discard(e);
                self.unit()
            }
            ExprKind::Generate {
                body,
                strict: false,
                sync: false,
                limit,
            } => self.lazy(body, limit.as_deref()),
            ExprKind::Generate { strict: false, .. } => {
                self.unsupported(e.span, "a lazy `for` that reads services")
            }
            ExprKind::Generate { body, .. } => self.strict(body),
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => self.operation(service, name, stages),
            ExprKind::ServiceTag(key) => self.code(&names::string(key)),
            ExprKind::Label(label) => self.code(&names::string(label)),
            // outside effectful code nothing can fail with a typed error
            ExprKind::Handle(h) if h.is_try() && self.fiber.is_none() => self.expr(&h.body),
            ExprKind::Handle(h) if h.is_try() => self.try_expr(&h.body, &h.errors),
            ExprKind::Await(task) => {
                let t = self.expr(task);
                self.run(t)
            }
            ExprKind::Task(body) => self.task(body),
            ExprKind::ReadOwnerEvidence => self.read_owner_evidence(),
            ExprKind::WithOwnerEvidence(owner, inner) => self.with_owner_evidence(owner, inner),
            ExprKind::Within(owner, inner) => self.within(owner, inner),
            ExprKind::Handle(h) => self.handle(h),
            _ => unreachable!("handled in eval"),
        }
    }

    fn if_expr(
        &mut self,
        e: &Expr,
        cond: &Expr,
        then: &Expr,
        other: &Expr,
        want: Want<'_>,
    ) -> Typed<'a> {
        let a = self.a;
        let test = self.expr_bool(cond).doc;
        let (mut then_stmts, then_val) = self.block(|g| g.eval(then, want));
        let (mut other_stmts, other_val) = self.block(|g| g.eval(other, want));
        let r_ty = match want {
            Want::Exact(gt) => Some(gt.clone()),
            Want::Value => {
                if then_val.ty == other_val.ty
                    && !then_val.ty.as_ref().is_some_and(GoType::needs_boxing)
                {
                    then_val.ty.clone()
                } else {
                    None
                }
            }
            Want::Any => {
                if then_val.ty == other_val.ty {
                    then_val.ty.clone()
                } else if let Some(gt) = self.go_type_of(e.ty.as_deref())
                    && matches!(gt, GoType::Opt(_) | GoType::Record(_) | GoType::Adt(_))
                    && (then_val.ty.as_ref() == Some(&gt) || other_val.ty.as_ref() == Some(&gt))
                {
                    Some(gt)
                } else {
                    None
                }
            }
        };
        if let Some(gt) = &r_ty {
            self.register_go_type(gt);
        }
        let r = self.temp();
        let go_ty = self.go_type_name(r_ty.as_ref());
        self.emit(docvec![a, "var ", r, " ", go_ty]);
        let then_code = self.coerce(then_val, r_ty.as_ref());
        let other_code = self.coerce(other_val, r_ty.as_ref());
        then_stmts.push(statement(a, docvec![a, r, " = ", then_code]));
        other_stmts.push(statement(a, docvec![a, r, " = ", other_code]));
        self.emit(if_else(a, test, then_stmts, other_stmts));
        Typed {
            code: self.code(r),
            ty: r_ty,
        }
    }

    fn match_expr(
        &mut self,
        e: &Expr,
        scrutinee: &Expr,
        arms: &[Arm],
        want: Want<'_>,
    ) -> Typed<'a> {
        let s = self.expr_typed(scrutinee);
        let s = self.spill_typed(s);
        let r_ty = match want {
            Want::Exact(gt) => Some(gt.clone()),
            Want::Value => match self.match_go_type(e, s.ty.as_ref(), arms, &[]) {
                Some(gt @ (GoType::Scalar(_) | GoType::Record(_))) => Some(gt),
                _ => None,
            },
            Want::Any => self.match_go_type(e, s.ty.as_ref(), arms, &[]),
        };
        if let Some(gt) = &r_ty {
            self.register_go_type(gt);
        }
        let r = self.temp();
        let go_ty = self.go_type_name(r_ty.as_ref());
        self.emit(docvec![self.a, "var ", r, " ", go_ty]);
        let target = (r, r_ty.clone());
        self.arms(s.code.doc, s.ty.as_ref(), arms, Some(&target));
        Typed {
            code: self.code(r),
            ty: r_ty,
        }
    }

    /// Emits `e` in tail position: its value is returned.
    pub(super) fn tail_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.tail_expr(result);
            }
            ExprKind::If(cond, then, other) => {
                let test = self.expr_bool(cond).doc;
                let (then, ()) = self.block(|g| g.tail_expr(then));
                let (other, ()) = self.block(|g| g.tail_expr(other));
                self.emit(if_else(self.a, test, then, other));
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr_typed(scrutinee);
                let s = self.spill_typed(s);
                self.arms(s.code.doc, s.ty.as_ref(), arms, None);
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) if self.jumps_in(rhs) => {
                self.tail_logic(*op, lhs, rhs);
            }
            _ if self.jump(e) => {}
            _ => {
                self.line(e.span);
                let ret = self.returns.clone();
                let v = self.expr_into(e, ret.as_ref());
                self.emit(docvec![self.a, "return ", v]);
            }
        }
    }

    /// Each label, then its value, as one item of a list.
    pub(super) fn entries(
        &self,
        fields: &[(String, Expr)],
        values: Vec<Code<'a>>,
    ) -> Vec<Document<'a>> {
        fields
            .iter()
            .zip(values)
            .map(|((label, _), v)| docvec![self.a, self.string(label), ", ", v])
            .collect()
    }
}

/// A literal or a name: evaluating it runs no code.
fn plain(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Var(_) | ExprKind::Con(_)
    )
}

/// Whether `e` reads a `let mut` variable where it is evaluated.
fn reads_variable(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) => true,
        ExprKind::Lambda { .. } | ExprKind::Task(_) => false,
        _ => {
            let mut found = false;
            e.each_child(&mut |c| found |= reads_variable(c));
            found
        }
    }
}
