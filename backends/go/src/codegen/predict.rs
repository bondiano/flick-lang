//! Static prediction of typed Go representations (`GoType`) for expressions
//! and pattern bindings, used for `match` result types and typed comparisons.

use flick_ir::{Arm, BinOp, Expr, ExprKind, Lit, Pat, Rep, Stmt};

use super::arith::Native;
use super::go_type::{GoType, OptCon};
use super::{Gen, own};

fn lit_go_type(lit: &Lit) -> Option<GoType> {
    let native = match lit {
        Lit::Int(_) => Native::Int,
        Lit::Int8(_) => Native::Int8,
        Lit::Int16(_) => Native::Int16,
        Lit::Int64(_) => Native::Int64,
        Lit::UInt8(_) => Native::UInt8,
        Lit::UInt16(_) => Native::UInt16,
        Lit::UInt32(_) => Native::UInt32,
        Lit::UInt64(_) => Native::UInt64,
        Lit::Float(x) if Native::is_native_float_lit(*x) => Native::Float,
        Lit::Float32(x) if Native::is_native_float32_lit(*x) => Native::Float32,
        Lit::String(_) => Native::String,
        Lit::Bool(_) => Native::Bool,
        _ => return None,
    };
    Some(GoType::Scalar(native))
}

impl Gen<'_> {
    /// Predicts the natural typed Go representation of `e` (with extra pattern
    /// bindings in scope, where `None` records a shadowed local of unknown type).
    pub(super) fn predict_go_type(
        &self,
        e: &Expr,
        extra: &[(&str, Option<GoType>)],
    ) -> Option<GoType> {
        match &e.kind {
            ExprKind::Lit(lit) => lit_go_type(lit),
            ExprKind::Local(name) | ExprKind::Var(name) => {
                if let Some((_, bound)) = extra.iter().rev().find(|(k, _)| *k == name) {
                    bound.clone()
                } else {
                    self.go_types.get(name).cloned()
                }
            }
            ExprKind::Neg(num, _) | ExprKind::Binary(BinOp::Arith(_, num), _, _) => {
                Native::of_num(*num).map(GoType::Scalar)
            }
            ExprKind::Binary(BinOp::Concat, lhs, rhs) => {
                let s = Some(GoType::Scalar(Native::String));
                (self.predict_go_type(lhs, extra) == s && self.predict_go_type(rhs, extra) == s)
                    .then_some(GoType::Scalar(Native::String))
            }
            ExprKind::Not(_)
            | ExprKind::Binary(
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Same
                | BinOp::NotSame
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or,
                _,
                _,
            ) => Some(GoType::Scalar(Native::Bool)),
            ExprKind::Con(c) if self.opt_con(c) == Some(OptCon::None) => {
                self.opt_repr_of(e.ty.as_deref()).map(GoType::Opt)
            }
            ExprKind::App { func, .. } if matches!(&func.kind, ExprKind::Con(c) if self.opt_con(c) == Some(OptCon::Some)) => {
                self.opt_repr_of(e.ty.as_deref()).map(GoType::Opt)
            }
            ExprKind::App { .. } => self.typed_result(e),
            ExprKind::Record(fields) => self
                .record_shape_of(e.ty.as_deref())
                .filter(|s| s.matches_literal(fields))
                .map(GoType::Record),
            ExprKind::Update(base, fields) => self
                .record_shape_of(e.ty.as_deref())
                .or_else(|| match self.predict_go_type(base, extra) {
                    Some(GoType::Record(s)) => Some(s),
                    _ => None,
                })
                .filter(|s| s.matches_update(fields))
                .map(GoType::Record),
            ExprKind::Field(record, label) => match self.predict_go_type(record, extra) {
                Some(GoType::Record(shape)) => {
                    shape.index_of(label).and_then(|i| shape.field_go_type(i))
                }
                _ => None,
            },
            ExprKind::Unwrap(inner) => match self.predict_go_type(inner, extra) {
                Some(GoType::Opt(repr)) => repr.payload_go_type(),
                _ => None,
            },
            ExprKind::Block(stmts, result) => {
                if stmts.is_empty() {
                    self.predict_go_type(result, extra)
                } else {
                    let mut scoped: Vec<(&str, Option<GoType>)> = extra.to_vec();
                    for stmt in stmts {
                        match stmt {
                            Stmt::Let { pat, .. } => {
                                let mut binders = Vec::new();
                                pat.binders(&mut binders);
                                for b in binders {
                                    scoped.push((b, None));
                                }
                            }
                            Stmt::Var { name, .. } | Stmt::Fun { name, .. } => {
                                scoped.push((name.as_str(), None));
                            }
                            Stmt::Assign(_, _) | Stmt::Expr(_) => {}
                        }
                    }
                    self.predict_go_type(result, &scoped)
                }
            }
            ExprKind::If(_, then, other) => {
                let a = self.predict_go_type(then, extra)?;
                (self.predict_go_type(other, extra).as_ref() == Some(&a)).then_some(a)
            }
            ExprKind::Match(scrutinee, arms) => {
                let s_ty = self.predict_go_type(scrutinee, extra);
                self.match_go_type(e, s_ty.as_ref(), arms, extra)
            }
            _ => None,
        }
    }

    /// Bindings of `pat` guaranteed to have a typed Go representation given the
    /// scrutinee's `GoType`.
    pub(super) fn pat_go_types<'p>(
        &self,
        pat: &'p Pat,
        subj: Option<&GoType>,
    ) -> Vec<(&'p str, GoType)> {
        fn collect<'p>(
            g: &Gen<'_>,
            pat: &'p Pat,
            subj: Option<&GoType>,
            out: &mut Vec<(&'p str, GoType)>,
        ) {
            match pat {
                Pat::Wild | Pat::Lit(_) => {}
                Pat::Bind(name) => {
                    if let Some(gt) = subj {
                        out.push((name.as_str(), gt.clone()));
                    }
                }
                Pat::Tuple(items) => {
                    for item in items {
                        collect(g, item, None, out);
                    }
                }
                Pat::List(items, rest) => {
                    for item in items {
                        collect(g, item, None, out);
                    }
                    if let Some(rest) = rest {
                        collect(g, rest, None, out);
                    }
                }
                Pat::Record(fields) => {
                    for (label, p) in fields {
                        let slot = match subj {
                            Some(GoType::Record(shape)) => {
                                shape.index_of(label).and_then(|i| shape.field_go_type(i))
                            }
                            _ => None,
                        };
                        collect(g, p, slot.as_ref(), out);
                    }
                }
                Pat::Con(c, args) => {
                    if let Some(GoType::Opt(repr)) = subj {
                        if g.opt_con(c) == Some(OptCon::Some)
                            && let Some(arg) = args.first()
                        {
                            let inner = repr.payload_go_type();
                            collect(g, arg, inner.as_ref(), out);
                        }
                        return;
                    }
                    if c.rep == Rep::Tagged
                        && let Some(own::Held::Record(shape)) = g.own_held(c)
                    {
                        if let Some(arg) = args.first() {
                            let rec_ty = GoType::Record(shape);
                            collect(g, arg, Some(&rec_ty), out);
                        }
                        return;
                    }
                    let held = (c.rep == Rep::Tagged)
                        .then(|| g.own_held(c))
                        .flatten()
                        .filter(own::Held::has_scalars);
                    for (i, arg) in args.iter().enumerate() {
                        let slot = held
                            .as_ref()
                            .and_then(|h| h.field_native(i))
                            .map(GoType::Scalar);
                        collect(g, arg, slot.as_ref(), out);
                    }
                }
                Pat::Or(alts) => {
                    let mut iter = alts.iter();
                    let Some(first) = iter.next() else {
                        return;
                    };
                    let mut common = Vec::new();
                    collect(g, first, subj, &mut common);
                    for alt in iter {
                        let mut other = Vec::new();
                        collect(g, alt, subj, &mut other);
                        common.retain(|pair| other.contains(pair));
                    }
                    out.extend(common);
                }
            }
        }
        let mut out = Vec::new();
        collect(self, pat, subj, &mut out);
        out
    }

    /// The Go type a non-tail `match` produces when its arms agree on a typed
    /// Go type (or when the match produces an eligible concrete `Option` and at
    /// least one arm already constructs one).
    pub(super) fn match_go_type(
        &self,
        e: &Expr,
        scrutinee_ty: Option<&GoType>,
        arms: &[Arm],
        extra: &[(&str, Option<GoType>)],
    ) -> Option<GoType> {
        let mut arm_types = Vec::with_capacity(arms.len());
        for arm in arms {
            let bound = self.pat_go_types(&arm.pat, scrutinee_ty);
            let mut binders = Vec::new();
            arm.pat.binders(&mut binders);
            let ty = if binders.is_empty() {
                self.predict_go_type(&arm.body, extra)
            } else {
                let mut scoped = Vec::with_capacity(extra.len() + binders.len());
                scoped.extend_from_slice(extra);
                for b in binders {
                    let found = bound
                        .iter()
                        .rev()
                        .find_map(|(k, v)| (*k == b).then(|| v.clone()));
                    scoped.push((b, found));
                }
                self.predict_go_type(&arm.body, &scoped)
            };
            arm_types.push(ty);
        }
        let first = arm_types.first()?.as_ref();
        if let Some(gt) = first
            && arm_types.iter().all(|t| t.as_ref() == Some(gt))
        {
            return Some(gt.clone());
        }
        if let Some(gt) = self.go_type_of(e.ty.as_deref())
            && matches!(gt, GoType::Opt(_) | GoType::Record(_))
            && arm_types.iter().any(|t| t.as_ref() == Some(&gt))
        {
            return Some(gt);
        }
        None
    }
}
