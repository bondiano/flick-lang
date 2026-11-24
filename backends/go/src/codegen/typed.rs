//! Calls of functions that take or return Go's own typed representations
//! (scalars and allocation-free concrete `Option` layouts), plus typed `Option`
//! constructor emission and scalar instance method lowering.

use flick_backend_support::names as js_names;
use flick_pretty::docvec;

use super::arith::Native;
use super::go_type::{GoType, OptCon, OptRepr, Typed, Want};
use super::layout::{Code, call};
use super::shape::Types;
use super::{Gen, names, own};
use flick_ir::{Constructor, Dict, Expr, ExprKind, IrType, Lang, Stage};

/// A built-in scalar `Eq`/`Ord`/`Hash`/`Show` instance method lowered directly
/// to typed Go operations so derived methods never rebox unboxed scalar fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScalarMethod {
    Eq(Native),
    Compare(Native),
    Hash(Native),
    Show(Native),
}

impl ScalarMethod {
    pub(super) fn arity(self) -> usize {
        match self {
            ScalarMethod::Eq(_) | ScalarMethod::Compare(_) => 2,
            ScalarMethod::Hash(_) | ScalarMethod::Show(_) => 1,
        }
    }

    pub(super) fn result(self) -> Option<GoType> {
        match self {
            ScalarMethod::Eq(_) => Some(GoType::Scalar(Native::Bool)),
            ScalarMethod::Compare(_) => None,
            ScalarMethod::Hash(_) => Some(GoType::Scalar(Native::Int)),
            ScalarMethod::Show(_) => Some(GoType::Scalar(Native::String)),
        }
    }

    pub(super) fn scalar(self) -> Native {
        match self {
            ScalarMethod::Eq(n)
            | ScalarMethod::Compare(n)
            | ScalarMethod::Hash(n)
            | ScalarMethod::Show(n) => n,
        }
    }
}

/// Resolves a built-in scalar `Instances` dictionary, unwrapping `Dict::Super`
/// (e.g. `Eq` of `Ord Float` or `Hash Int`) to its `(class, type)` suffix.
fn scalar_instance_head<'d>(
    dict: &'d Dict,
    lang: &flick_backend_support::Known<'_>,
) -> Option<(&'d str, Native)> {
    let instances_mod = lang.get(Lang::Instances)?.0;
    let (class, suffix) = match dict {
        Dict::Instance { module, name, args } if module == instances_mod && args.is_empty() => {
            name.split_once(' ')?
        }
        Dict::Super { dict: inner, class } => {
            let (_, scalar) = scalar_instance_head(inner, lang)?;
            let short = class.rsplit('.').next().unwrap_or(class);
            return Some((short, scalar));
        }
        _ => return None,
    };
    let scalar = [
        (Native::Int, Lang::Int),
        (Native::Float, Lang::Float),
        (Native::Bool, Lang::Bool),
        (Native::String, Lang::String),
    ]
    .into_iter()
    .find(|(_, role)| lang.get(*role).is_some_and(|(_, n)| n == suffix))?
    .0;
    Some((class, scalar))
}

impl<'a> Gen<'a> {
    pub(super) fn opt_con(&self, c: &Constructor) -> Option<OptCon> {
        match js_names::native(self.input.lang, c) {
            Some(js_names::Native::Some) => Some(OptCon::Some),
            Some(js_names::Native::None) => Some(OptCon::None),
            _ => None,
        }
    }

    pub(super) fn go_type_of(&self, ty: Option<&IrType>) -> Option<GoType> {
        let decls = self.decls?;
        GoType::of(self.input.lang, decls, ty)
    }

    pub(super) fn opt_repr_of(&self, ty: Option<&IrType>) -> Option<OptRepr> {
        let decls = self.decls?;
        OptRepr::of(self.input.lang, decls, ty)
    }

    pub(super) fn go_type_name(&self, ty: Option<&GoType>) -> &'a str {
        match ty {
            Some(gt) => self.a.alloc_str(&gt.go_type()),
            None => "flickrt.Value",
        }
    }

    /// Converts `t` into an erased `flickrt.Value`-compatible expression.
    pub(super) fn to_value(&self, t: Typed<'a>) -> Code<'a> {
        match t.ty {
            Some(GoType::Opt(r)) => r.box_doc(self.a, t.code.doc).into(),
            Some(GoType::Scalar(_) | GoType::Record(_)) | None => t.code,
        }
    }

    /// Converts `t` into the exact requested representation (`None` means erased
    /// `flickrt.Value`).
    pub(super) fn coerce(&self, t: Typed<'a>, want: Option<&GoType>) -> Code<'a> {
        if t.ty.as_ref() == want {
            return t.code;
        }
        let val = self.to_value(t);
        match want {
            Some(gt) => gt.unbox_doc(self.a, val.doc).into(),
            None => val,
        }
    }

    /// Spills `t` into a temporary variable of its exact Go type unless atomic.
    pub(super) fn spill_typed(&mut self, t: Typed<'a>) -> Typed<'a> {
        if t.code.atomic {
            return t;
        }
        if let Some(gt) = &t.ty {
            self.register_go_type(gt);
        }
        let temp = self.temp();
        let go_ty = self.go_type_name(t.ty.as_ref());
        self.emit(docvec![self.a, "var ", temp, " ", go_ty, " = ", t.code]);
        Typed {
            code: self.code(temp),
            ty: t.ty,
        }
    }

    /// Declares the local `name` with `value` and its optional typed Go type.
    pub(super) fn bind_typed(&mut self, name: &str, value: Code<'a>, ty: Option<GoType>) {
        if let Some(gt) = &ty {
            self.register_go_type(gt);
        }
        let l = self.a.alloc_str(&names::local(name));
        let go_ty = self.go_type_name(ty.as_ref());
        self.emit(docvec![self.a, "var ", l, " ", go_ty, " = ", value]);
        self.emit(docvec![self.a, "_ = ", l]);
        match ty {
            Some(gt) => {
                self.go_types.insert(name.to_owned(), gt);
            }
            None => {
                self.go_types.remove(name);
            }
        }
    }

    fn want_opt_repr(&self, e: &Expr, want: Want<'_>) -> Option<OptRepr> {
        match want {
            Want::Exact(GoType::Opt(r)) => Some(r.clone()),
            Want::Any => self.opt_repr_of(e.ty.as_deref()),
            Want::Value | Want::Exact(GoType::Scalar(_) | GoType::Record(_)) => None,
        }
    }

    /// Emits a `None` constructor or a saturated `Some x` application directly
    /// in the requested representation (`Want::Value` stays erased `*Data`,
    /// `Want::Any` / `Want::Exact(Opt(_))` uses the allocation-free typed form).
    pub(super) fn try_opt_con(&mut self, e: &Expr, want: Want<'_>) -> Option<Typed<'a>> {
        let a = self.a;
        match &e.kind {
            ExprKind::Con(c) if self.opt_con(c) == Some(OptCon::None) => {
                Some(if let Some(r) = self.want_opt_repr(e, want) {
                    Typed {
                        code: r.none_code(a),
                        ty: Some(GoType::Opt(r)),
                    }
                } else {
                    Typed {
                        code: a.text(self.once("flickrt.Nullary(\"None\")")).into(),
                        ty: None,
                    }
                })
            }
            ExprKind::App { func, arg, .. } if matches!(&func.kind, ExprKind::Con(c) if self.opt_con(c) == Some(OptCon::Some)) => {
                Some(if let Some(r) = self.want_opt_repr(e, want) {
                    let payload = self.expr_into(arg, r.payload_go_type().as_ref());
                    Typed {
                        code: r.some_code(a, payload.doc),
                        ty: Some(GoType::Opt(r)),
                    }
                } else {
                    let payload = self.expr(arg);
                    Typed {
                        code: call(a, "flickrt.NewData", [a.text("\"Some\""), payload.doc]).into(),
                        ty: None,
                    }
                })
            }
            _ => None,
        }
    }

    /// Computes the desired Go type for the last (`arg_first`) argument of a
    /// piped call `arg |> func`, when `func` is a direct full application of a
    /// typed function, scalar method, or unboxed-scalar constructor.
    pub(super) fn piped_arg_want(&self, func: &Expr) -> Option<GoType> {
        let (head, args) = func.spine();
        let total = args.len() + 1;
        let last = total - 1;
        if let ExprKind::Con(c) = &head.kind
            && c.arity == total
        {
            match self.own_held(c) {
                Some(own::Held::Fields(slots)) if slots.iter().any(Option::is_some) => {
                    return slots.get(last).copied().flatten().map(GoType::Scalar);
                }
                Some(own::Held::Record(shape)) if last == 0 => {
                    return Some(GoType::Record(shape));
                }
                _ => {}
            }
        }
        if let Some(m) = self.scalar_method_head(head)
            && total == m.arity()
        {
            return Some(GoType::Scalar(m.scalar()));
        }
        let (_, types, params) = self.typed_head(head)?;
        (total >= params && last < params)
            .then(|| types.param(last).cloned())
            .flatten()
    }

    /// Recognizes a built-in scalar `Eq`/`Ord`/`Hash`/`Show` method on `head`.
    pub(super) fn scalar_method_head(&self, head: &Expr) -> Option<ScalarMethod> {
        let ExprKind::Method { dict, name, .. } = &head.kind else {
            return None;
        };
        let lang = self.input.lang;
        let (class, scalar) = scalar_instance_head(dict, lang)?;
        if lang.get(Lang::Eq).is_some_and(|(_, c)| c == class) && name == "eq" {
            Some(ScalarMethod::Eq(scalar))
        } else if lang.get(Lang::Ord).is_some_and(|(_, c)| c == class) && name == "compare" {
            Some(ScalarMethod::Compare(scalar))
        } else if lang.get(Lang::Hash).is_some_and(|(_, c)| c == class) && name == "hash" {
            Some(ScalarMethod::Hash(scalar))
        } else if lang.get(Lang::Show).is_some_and(|(_, c)| c == class)
            && lang.get(Lang::ShowMethod).is_some_and(|(_, m)| m == name)
        {
            Some(ScalarMethod::Show(scalar))
        } else {
            None
        }
    }

    /// Emits a full application of a built-in scalar `Eq`/`Ord`/`Hash`/`Show`
    /// method over operands already compiled in `method.scalar()`.
    pub(super) fn scalar_method_call(&self, method: ScalarMethod, all: &[Code<'a>]) -> Typed<'a> {
        let a = self.a;
        let code = match method {
            ScalarMethod::Eq(Native::Float) => {
                call(a, "flickrt.EqFloat", [all[0].doc, all[1].doc]).into()
            }
            ScalarMethod::Eq(_) => docvec![a, "(", all[0].doc, " == ", all[1].doc, ")"].into(),
            ScalarMethod::Compare(n) => call(
                a,
                docvec![a, "flickrt.Compare", n.name()],
                [all[0].doc, all[1].doc],
            )
            .into(),
            ScalarMethod::Hash(n) => {
                call(a, docvec![a, "flickrt.Hash", n.name()], [all[0].doc]).into()
            }
            ScalarMethod::Show(n) => {
                call(a, docvec![a, "flickrt.Show", n.name()], [all[0].doc]).into()
            }
        };
        Typed {
            code,
            ty: method.result(),
        }
    }

    /// Emits a full application of an owned constructor with unboxed scalar
    /// fields over operands already compiled in their slot types.
    pub(super) fn typed_con_call(&self, make: &'a str, all: &[Code<'a>]) -> Typed<'a> {
        Typed {
            code: call(self.a, make, all.iter().map(|arg| arg.doc)).into(),
            ty: None,
        }
    }

    /// The Go function the head of an application names, its Go types and
    /// how many parameters it takes, when a call of it with all its
    /// arguments is a call of that function.
    pub(super) fn typed_head(&self, head: &Expr) -> Option<(&'a str, Types, usize)> {
        let (module, name, dicts) = match &head.kind {
            ExprKind::Global { module, name, .. } => (module, name, 0),
            ExprKind::Dicts(inner, dicts) => match &inner.kind {
                ExprKind::Global { module, name, .. } => (module, name, dicts.len()),
                _ => return None,
            },
            _ => return None,
        };
        let (params, _, types) = self.direct_shape(module, name, dicts)?;
        Some((self.a.alloc_str(&names::func(module, name)), types, params))
    }

    /// The Go type of the application `e`, when `app` compiles it to a call
    /// of a known function or scalar instance method, of all its arguments,
    /// that returns one.
    pub(super) fn typed_result(&self, e: &Expr) -> Option<GoType> {
        let mut head = e;
        let mut given = 0;
        if let ExprKind::App {
            func,
            arg_first: true,
            ..
        } = &e.kind
        {
            (head, given) = (func, 1);
        }
        while let ExprKind::App {
            func,
            arg_first: false,
            ..
        } = &head.kind
        {
            (head, given) = (func, given + 1);
        }
        if let Some(method) = self.scalar_method_head(head)
            && given == method.arity()
        {
            return method.result();
        }
        let (_, types, params) = self.typed_head(head)?;
        (given == params).then(|| types.result().cloned()).flatten()
    }

    /// The call `e` of the function `typed` with all its arguments, `all`
    /// (already compiled into each parameter's target `GoType`).
    pub(super) fn typed_call(
        &mut self,
        (f, types, params): (&'a str, Types, usize),
        all: &[Code<'a>],
        exprs: &[&Expr],
        e: &Expr,
    ) -> Typed<'a> {
        let a = self.a;
        self.register_types(&types);
        let dicts = all.len() - params;
        // the Go types of a binding are written in a table: they must be
        // those of the types the checker gave the call
        let differs = |go: Option<&GoType>, of: &Expr| {
            go.zip(self.go_type_of(of.ty.as_deref()).as_ref())
                .is_some_and(|(go, flick)| go != flick)
        };
        let mut wrong = differs(types.result(), e);
        let mut args = vec![self.context()];
        for (i, arg) in all.iter().enumerate() {
            if let Some(p) = i.checked_sub(dicts)
                && let (Some(go_ty), Some(of)) = (types.param(p), exprs.get(p))
            {
                wrong |= differs(Some(go_ty), of);
            }
            args.push(arg.doc);
        }
        if wrong {
            let message = format!("the Go types of `{f}` are not those of its Flick type");
            self.errors.push((message, Some(e.span)));
        }
        Typed {
            code: call(a, f, args).into(),
            ty: types.result().cloned(),
        }
    }

    /// `v` applied to the arguments `rest` that its head did not take, each
    /// with the stage of its application and whether it is quiet.
    pub(super) fn applied(
        &mut self,
        mut v: Code<'a>,
        rest: Vec<(Code<'a>, Stage)>,
        quiet: Vec<bool>,
    ) -> Code<'a> {
        let a = self.a;
        let mut rest = rest.into_iter().zip(quiet).peekable();
        while let Some(((arg, mut stage), _)) = rest.next() {
            let mut group = vec![self.context(), v.doc, arg.doc];
            while group.len() < 5 && stage == Stage::Pure {
                let Some(((next, after), _)) = rest.next_if(|(_, quiet)| *quiet) else {
                    break;
                };
                group.push(next.doc);
                stage = after;
            }
            let apply = ["flickrt.Apply", "flickrt.Apply2", "flickrt.Apply3"][group.len() - 3];
            v = call(a, apply, group).into();
            if stage != Stage::Pure {
                v = self.run(v);
            }
        }
        v
    }
}
