//! Functions and calls: a known top-level function applied to all its
//! arguments is a direct Go call; any other application goes through the
//! curried value.

use std::iter;

use flick_pretty::Document;

use super::adt::AdtShape;
use super::adt_derive::AdtMethod;
use super::closures::{Call, at_once, callee, curried, quiet, returning};
use super::expr::enum_literal;
use super::go_type::{GoType, Typed, Want};
use super::layout::{Code, call, closure};
use super::shape::Types;
use super::{Gen, Shape, names, own};
use flick_ir::{Constructor, Expr, ExprKind, IrType, Rep, Stage, operation_arity};

impl<'a> Gen<'a> {
    /// Builds a value of the constructor `c` from its fields, holding the
    /// dictionaries `held` after them.
    fn construct(&mut self, c: &Constructor, held: Vec<Document<'a>>) -> Call<'a> {
        let a = self.a;
        let tag = a.alloc_str(&names::string(&self.tag(c)));
        match &c.rep {
            Rep::Literal(l) => {
                let lit = a.alloc_str(&enum_literal(l));
                Box::new(move |_, _: &[Code<'a>]| a.text(lit).into())
            }
            Rep::Tagged if c.arity == 0 && held.is_empty() => {
                if let Some((ty, own::Held::Zero)) = self.own_type(c) {
                    let value = a.alloc_str(&format!("flickrt.Value({ty}{{}})"));
                    return Box::new(move |_, _: &[Code<'a>]| a.text(value).into());
                }
                let value = self.once(&format!("flickrt.Nullary({tag})"));
                Box::new(move |_, _: &[Code<'a>]| a.text(value).into())
            }
            Rep::Tagged => {
                let own = held
                    .is_empty()
                    .then(|| self.own_type(c))
                    .flatten()
                    .and_then(|(ty, held)| match held {
                        own::Held::Zero => None,
                        own::Held::Fields(slots) => {
                            Some((a.alloc_str(&own::make(&ty)), Some(slots)))
                        }
                        own::Held::Record(_) => Some((a.alloc_str(&own::of(&ty)), None)),
                    });
                Box::new(move |_, args: &[Code<'a>]| match &own {
                    // a Go type of its own, which is its tag; a curried value
                    // unboxes scalar fields nil-safely so `tagOf` can probe it
                    Some((make, Some(slots))) if slots.iter().any(Option::is_some) => {
                        let args = args.iter().enumerate().map(|(i, arg)| match slots[i] {
                            Some(native) => native.field_unbox(a, arg.doc),
                            None => arg.doc,
                        });
                        call(a, *make, args).into()
                    }
                    Some((make, _)) => {
                        let args = args.iter().map(|arg| arg.doc).chain(held.iter().copied());
                        call(a, *make, args).into()
                    }
                    None => {
                        let args = args.iter().map(|arg| arg.doc).chain(held.iter().copied());
                        call(a, "flickrt.NewData", iter::once(a.text(tag)).chain(args)).into()
                    }
                })
            }
            Rep::Unboxed | Rep::Typeof(_) => Box::new(|_, args: &[Code<'a>]| args[0]),
        }
    }

    /// A top-level value of `module` with `dicts` given: a function of
    /// parameters becomes a curried value.
    pub(super) fn global_value(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<Code<'a>>,
    ) -> Code<'a> {
        let a = self.a;
        self.reads(module);
        let shape = self.shape_of(module, name);
        match shape {
            Shape::Var => self.code(&names::var(module, name)),
            Shape::Func {
                dicts: n,
                params,
                effects,
                types,
            } if dicts.len() == n => {
                self.register_types(&types);
                let f = names::func(module, name);
                // a function of values is a value as it is; one that takes
                // or gives a Go type is wrapped in one that converts them
                let plain = n == 0 && effects == 0 && !types.any();
                match params {
                    0 => call(
                        a,
                        f,
                        iter::once(self.context()).chain(dicts.into_iter().map(|d| d.doc)),
                    )
                    .into(),
                    1 if plain => self.code(&f),
                    // the Go function itself is the value: nothing wraps it
                    2 | 3 if plain => call(
                        a,
                        at_once(params, 0).expect("two or three"),
                        [a.text(a.alloc_str(&f))],
                    )
                    .into(),
                    _ => curried(
                        a,
                        &callee(a, &f, n, types),
                        dicts,
                        params,
                        effects,
                        &mut || self.temp(),
                    ),
                }
            }
            // a constrained value passed on before its dictionaries
            Shape::Func {
                dicts: n,
                params,
                effects,
                types,
            } => {
                self.register_types(&types);
                let given: Vec<Code<'a>> = (0..n).map(|i| self.code(&format!("ds[{i}]"))).collect();
                let f = names::func(module, name);
                let inner = if params == 0 {
                    call(
                        a,
                        f,
                        iter::once(a.text("fb")).chain(given.into_iter().map(|d| d.doc)),
                    )
                } else {
                    curried(
                        a,
                        &callee(a, &f, n, types),
                        given,
                        params,
                        effects,
                        &mut || self.temp(),
                    )
                    .doc
                };
                let f = closure(
                    a,
                    "func(fb *flickrt.Fiber, ds ...flickrt.Value) flickrt.Value",
                    returning(a, inner),
                );
                call(a, "flickrt.Poly", [f]).into()
            }
        }
    }
}

struct AppTarget<'a> {
    make: Call<'a>,
    given: Vec<Code<'a>>,
    arity: usize,
    effects: u64,
    adt_con: Option<(AdtShape, usize)>,
    typed_con: Option<(&'a str, Vec<Option<GoType>>)>,
    scalar_method: Option<super::typed::ScalarMethod>,
    adt_method: Option<AdtMethod>,
}

impl<'a> AppTarget<'a> {
    fn arg_want(
        &self,
        typed: Option<&(&'a str, Types, usize)>,
        total_args: usize,
        index: usize,
    ) -> Option<GoType> {
        if total_args < self.arity {
            return None;
        }
        if let Some((shape, vi)) = &self.adt_con {
            shape.field_go_type(*vi, index)
        } else if let Some((_, slots)) = &self.typed_con {
            slots.get(index).cloned().flatten()
        } else if let Some(m) = self.scalar_method {
            (index < m.arity()).then_some(GoType::Scalar(m.scalar()))
        } else if let Some(m) = &self.adt_method {
            (index < m.arity()).then(|| GoType::Adt(m.shape().clone()))
        } else if let Some((_, types, params)) = typed {
            (index < *params)
                .then(|| types.param(index).cloned())
                .flatten()
        } else {
            None
        }
    }
}

impl<'a> Gen<'a> {
    fn con_target(
        &mut self,
        c: &Constructor,
        total_args: usize,
        e_ty: Option<&IrType>,
        want: Want<'_>,
    ) -> AppTarget<'a> {
        if c.arity == total_args
            && let Some((shape, vi)) = self.adt_con_target(c, e_ty, want)
        {
            return AppTarget {
                make: Box::new(|_, _: &[Code<'a>]| unreachable!("full by-value ADT constructor")),
                given: Vec::new(),
                arity: c.arity,
                effects: 0,
                adt_con: Some((shape, vi)),
                typed_con: None,
                scalar_method: None,
                adt_method: None,
            };
        }
        let a = self.a;
        let typed_con = match self.own_type(c) {
            Some((ty, own::Held::Fields(slots))) if slots.iter().any(Option::is_some) => Some((
                a.alloc_str(&own::make(&ty)),
                slots.into_iter().map(|s| s.map(GoType::Scalar)).collect(),
            )),
            Some((ty, own::Held::Record(shape))) => Some((
                a.alloc_str(&own::make(&ty)),
                vec![Some(GoType::Record(shape))],
            )),
            _ => None,
        };
        AppTarget {
            make: self.construct(c, Vec::new()),
            given: Vec::new(),
            arity: c.arity,
            effects: 0,
            adt_con: None,
            typed_con,
            scalar_method: None,
            adt_method: None,
        }
    }

    fn app_target(
        &mut self,
        head: &Expr,
        total_args: usize,
        first_arg: Option<&Expr>,
        first_ty: Option<&GoType>,
        e_ty: Option<&IrType>,
        want: Want<'_>,
    ) -> AppTarget<'a> {
        if let ExprKind::Con(c) = &head.kind {
            return self.con_target(c, total_args, e_ty, want);
        }
        let mut scalar_method = None;
        let mut adt_method = None;
        let (make, given, arity, effects): (Call<'a>, Vec<Code<'a>>, usize, u64) = match &head.kind
        {
            // a constructor of a GADT with the dictionaries its value holds
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Con(_)) => {
                let ExprKind::Con(c) = &inner.kind else {
                    unreachable!("a constructor, as the guard tested");
                };
                let carried = dicts.iter().map(|d| self.dict(d).doc).collect();
                (self.construct(c, carried), Vec::new(), c.arity, 0)
            }
            ExprKind::Global { module, name, .. } => self.direct(module, name, Vec::new()),
            // an operation that runs here once it has its arguments
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } if self.fiber.is_some()
                && stages.iter().any(|&s| s != Stage::Pure)
                && total_args >= operation_arity(stages) =>
            {
                let arity = operation_arity(stages);
                (self.perform(service, name), Vec::new(), arity, 0)
            }
            ExprKind::Dicts(inner, dicts) if matches!(inner.kind, ExprKind::Global { .. }) => {
                if let Some(m) = self.adt_method_head(head, first_arg, first_ty)
                    && total_args >= m.arity()
                {
                    let arity = m.arity();
                    adt_method = Some(m);
                    (
                        Box::new(|_, _: &[Code<'a>]| unreachable!("full ADT Ord comparison")),
                        Vec::new(),
                        arity,
                        0,
                    )
                } else {
                    let ExprKind::Global { module, name, .. } = &inner.kind else {
                        unreachable!("a global, as the guard tested");
                    };
                    let dicts: Vec<Code<'a>> = dicts.iter().map(|d| self.dict(d)).collect();
                    self.direct(module, name, dicts)
                }
            }
            ExprKind::Method { .. }
                if self
                    .scalar_method_head(head)
                    .is_some_and(|m| total_args >= m.arity()) =>
            {
                let m = self.scalar_method_head(head).expect("tested by the guard");
                scalar_method = Some(m);
                (
                    Box::new(|_, _: &[Code<'a>]| unreachable!("full scalar method call")),
                    Vec::new(),
                    m.arity(),
                    0,
                )
            }
            ExprKind::Method { .. }
                if self
                    .adt_method_head(head, first_arg, first_ty)
                    .is_some_and(|m| total_args >= m.arity()) =>
            {
                let m = self
                    .adt_method_head(head, first_arg, first_ty)
                    .expect("tested by the guard");
                let arity = m.arity();
                adt_method = Some(m);
                (
                    Box::new(|_, _: &[Code<'a>]| unreachable!("full ADT derived method call")),
                    Vec::new(),
                    arity,
                    0,
                )
            }
            ExprKind::Method { dict, name, .. } if self.direct_method(dict, name).is_some() => {
                let (module, name) = self.direct_method(dict, name).expect("tested by the guard");
                let dicts = self.context_dicts(dict);
                self.direct(&module, &name, dicts)
            }
            _ => {
                let f = self.expr(head);
                let f = self.spill(f);
                (Box::new(move |_, _: &[Code<'a>]| f), Vec::new(), 0, 0)
            }
        };
        AppTarget {
            make,
            given,
            arity,
            effects,
            adt_con: None,
            typed_con: None,
            scalar_method,
            adt_method,
        }
    }

    /// An application, or a constructor: a known function of all its
    /// arguments is called directly; an application of an effectful stage
    /// of any other value runs the task it gives.
    pub(super) fn app_typed(&mut self, e: &Expr, want: Want<'_>) -> Typed<'a> {
        if let Some(opt) = self.try_opt_con(e, want) {
            return opt;
        }
        let a = self.a;
        let mut head = e;
        let mut first = None;
        if let ExprKind::App {
            func,
            arg,
            arg_first: true,
            stage,
        } = &e.kind
        {
            let first_want = self.piped_arg_want(func, arg, e.ty.as_deref(), want);
            let v = self.eval(arg, Want::of(first_want.as_ref()));
            first = Some((self.spill_typed(v), *stage));
            head = func;
        }
        let (head, args) = head.spine();
        let total_args = args.len() + usize::from(first.is_some());
        let first_arg = args.first().map(|(a, _)| *a);
        let first_ty = if args.is_empty() {
            first.as_ref().and_then(|(v, _)| v.ty.as_ref())
        } else {
            None
        };
        let target = self.app_target(head, total_args, first_arg, first_ty, e.ty.as_deref(), want);
        let typed = self.typed_head(head).filter(|(_, types, _)| types.any());
        let exprs: Vec<&Expr> = args.iter().map(|(arg, _)| *arg).collect();
        let wants: Vec<Option<GoType>> = (0..exprs.len())
            .map(|i| target.arg_want(typed.as_ref(), total_args, i))
            .collect();
        let mut values: Vec<(Code<'a>, Stage)> = self
            .ordered_into(&exprs, &wants)
            .into_iter()
            .zip(args.iter().map(|(_, s)| *s))
            .collect();
        // whether the value of each argument is ready with nothing to see:
        // a name, a literal or a function, and the one taken before the rest
        let mut quiet: Vec<bool> = exprs.iter().map(|e| quiet(e)).collect();
        quiet.extend(first.iter().map(|_| true));
        if let Some((first_val, stage)) = first {
            let first_want = target.arg_want(typed.as_ref(), total_args, args.len());
            values.push((self.coerce(first_val, first_want.as_ref()), stage));
        }
        if values.len() < target.arity {
            // a partial application keeps what it was given
            let taken = values.len();
            let given: Vec<Code<'a>> = target
                .given
                .into_iter()
                .chain(values.into_iter().map(|(v, _)| v))
                .map(|v| self.spill(v))
                .collect();
            return Typed {
                code: curried(
                    a,
                    &target.make,
                    given,
                    target.arity - taken,
                    target.effects >> taken,
                    &mut || self.temp(),
                ),
                ty: None,
            };
        }
        let rest = values.split_off(target.arity);
        let quiet = quiet.split_off(target.arity.min(quiet.len()));
        let all: Vec<Code<'a>> = target
            .given
            .into_iter()
            .chain(values.into_iter().map(|(v, _)| v))
            .collect();
        let v = if let Some((shape, vi)) = target.adt_con {
            self.adt_con_call(&shape, vi, &all)
        } else if let Some((make, _)) = target.typed_con {
            self.typed_con_call(make, &all)
        } else if let Some(method) = target.scalar_method {
            self.scalar_method_call(method, &all)
        } else if let Some(method) = target.adt_method {
            self.adt_method_call(&method, &all)
        } else if let Some(typed) = typed {
            self.typed_call(typed, &all, &exprs, e)
        } else {
            Typed {
                code: (target.make)(self.context.unwrap_or("nil"), &all),
                ty: None,
            }
        };
        if rest.is_empty() {
            v
        } else {
            let fn_val = self.to_value(v);
            Typed {
                code: self.applied(fn_val, rest, quiet),
                ty: None,
            }
        }
    }

    /// The task `task` run in the fiber in scope.
    pub(super) fn run(&self, task: Code<'a>) -> Code<'a> {
        let fb = self.fiber.expect("effects run where a fiber is in scope");
        call(self.a, "flickrt.Run", [self.a.text(fb), task.doc]).into()
    }

    /// How a known function of `module` is called with `dicts` dictionaries
    /// and all its arguments, when that is a call of its Go function: how
    /// many parameters it takes, which run effects, and their Go types.
    pub(super) fn direct_shape(
        &self,
        module: &str,
        name: &str,
        dicts: usize,
    ) -> Option<(usize, u64, Types)> {
        let shape = self.shape_of(module, name);
        let effectful = shape.effectful();
        match shape {
            Shape::Func {
                dicts: n,
                params,
                effects,
                types,
            } if n == dicts && params > 0 && (self.fiber.is_some() || !effectful) => {
                Some((params, effects, types))
            }
            _ => None,
        }
    }

    /// How to call a top-level value of `module`: its Go function, how many
    /// parameters it takes after `dicts` and which run effects, or the
    /// value to apply.
    fn direct(
        &mut self,
        module: &str,
        name: &str,
        dicts: Vec<Code<'a>>,
    ) -> (Call<'a>, Vec<Code<'a>>, usize, u64) {
        self.reads(module);
        if let Some((params, effects, types)) = self.direct_shape(module, name, dicts.len()) {
            self.register_types(&types);
            let f = names::func(module, name);
            let make = callee(self.a, &f, dicts.len(), types);
            (make, dicts, params, effects)
        } else {
            let f = self.global_value(module, name, dicts);
            (Box::new(move |_, _: &[Code<'a>]| f), Vec::new(), 0, 0)
        }
    }
}
