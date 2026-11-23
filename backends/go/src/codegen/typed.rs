//! Calls of functions that take or return Go's own types. A function of
//! the top level whose parameters or result are `Int`, `Float`, `Bool` or
//! `String` is a Go function of `int32`, `float64`, `bool` or `string`
//! there: a number passed to it or returned by it is no value made for the
//! call. Where such a function is a value, a function of values around it
//! converts them (`closures.rs`).

use flick_pretty::docvec;

use super::arith::Native;
use super::layout::{Code, call};
use super::shape::Types;
use super::{Gen, names};
use flick_ir::{Dict, Expr, ExprKind, Lang, Stage};

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

    pub(super) fn result(self) -> Option<Native> {
        match self {
            ScalarMethod::Eq(_) => Some(Native::Bool),
            ScalarMethod::Compare(_) => None,
            ScalarMethod::Hash(_) => Some(Native::Int),
            ScalarMethod::Show(_) => Some(Native::String),
        }
    }

    fn scalar(self) -> Native {
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
    /// method directly over unboxed Go scalar operands.
    pub(super) fn scalar_method_call(
        &self,
        method: ScalarMethod,
        all: &[Code<'a>],
        exprs: &[&Expr],
    ) -> Code<'a> {
        let a = self.a;
        let scalar = method.scalar();
        let arg = |i: usize| match exprs.get(i) {
            Some(of) => self.unboxed(of, all[i], scalar).doc,
            None => scalar.unbox(a, all[i].doc),
        };
        match method {
            ScalarMethod::Eq(Native::Float) => call(a, "flickrt.EqFloat", [arg(0), arg(1)]).into(),
            ScalarMethod::Eq(_) => docvec![a, "(", arg(0), " == ", arg(1), ")"].into(),
            ScalarMethod::Compare(n) => {
                call(a, docvec![a, "flickrt.Compare", n.name()], [arg(0), arg(1)]).into()
            }
            ScalarMethod::Hash(n) => call(a, docvec![a, "flickrt.Hash", n.name()], [arg(0)]).into(),
            ScalarMethod::Show(n) => call(a, docvec![a, "flickrt.Show", n.name()], [arg(0)]).into(),
        }
    }

    /// Emits a full application of an owned constructor with unboxed scalar
    /// fields, passing already-native arguments without boxing.
    pub(super) fn typed_con_call(
        &self,
        make: &'a str,
        slots: &[Option<Native>],
        all: &[Code<'a>],
        exprs: &[&Expr],
    ) -> Code<'a> {
        let a = self.a;
        let args = all.iter().enumerate().map(|(i, arg)| match slots[i] {
            Some(native) => match exprs.get(i) {
                Some(of) => self.unboxed(of, *arg, native).doc,
                None => native.unbox(a, arg.doc),
            },
            None => arg.doc,
        });
        call(a, make, args).into()
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
    pub(super) fn typed_result(&self, e: &Expr) -> Option<Native> {
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
        types.result().filter(|_| given == params)
    }

    /// The call `e` of the function `typed` with all its arguments, `all`:
    /// the dictionaries, then the values of `exprs`, then one taken before
    /// them. Each is in the Go type of its parameter, with no value made of
    /// one that is of it already.
    pub(super) fn typed_call(
        &mut self,
        (f, types, params): (&'a str, Types, usize),
        all: &[Code<'a>],
        exprs: &[&Expr],
        e: &Expr,
    ) -> Code<'a> {
        let a = self.a;
        let dicts = all.len() - params;
        // the Go types of a binding are written in a table: they must be
        // those of the types the checker gave the call
        let differs = |go: Option<Native>, of: &Expr| {
            go.zip(Native::of(self.input.lang, of.ty.as_deref()))
                .is_some_and(|(go, flick)| go != flick)
        };
        let mut wrong = differs(types.result(), e);
        let mut args = vec![self.context()];
        for (i, arg) in all.iter().enumerate() {
            let native = i.checked_sub(dicts).and_then(|p| types.param(p));
            args.push(match (native, exprs.get(i.wrapping_sub(dicts))) {
                (Some(native), Some(of)) => {
                    wrong |= differs(Some(native), of);
                    self.unboxed(of, *arg, native).doc
                }
                // the argument taken before the others is a value by now
                (Some(native), None) => native.unbox(a, arg.doc),
                (None, _) => arg.doc,
            });
        }
        if wrong {
            let message = format!("the Go types of `{f}` are not those of its Flick type");
            self.errors.push((message, Some(e.span)));
        }
        call(a, f, args).into()
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
            // an argument of a pure stage goes with the next, up to three
            // at once: a function that takes them so is called once. The
            // next one must be quiet: a function may fail, or not return,
            // once it has an argument, and that comes before the next
            // argument is evaluated
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
