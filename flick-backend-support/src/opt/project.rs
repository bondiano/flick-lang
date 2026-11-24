//! Projections: a pure function of one parameter that reads a path of its
//! fields, as `.address.city` and `fun r -> r.address.city` both are, since
//! they lower to the same lambda. A projection known where it is used is
//! its reads there:
//!
//! - a local bound to one and applied is the reads of its argument;
//! - `List.map` of one, the `list_map` lang item of the build and not a
//!   function that has its name, is a strict `for` over the list whose
//!   element is the reads of each element, without a call per element.
//!
//! A projection of the module or of another module of the build is known
//! by its body, which the build gives with the program; a value whose body
//! is not given stays a call. The argument is evaluated once and first, and
//! each field is read once per use, as the call would: a field of a foreign
//! value may be a getter, so reads are neither repeated nor dropped.

use std::collections::HashMap;
use std::mem;

use super::inline::{Candidates, Foreign};
use crate::Known;
use crate::ir::{Expr, ExprKind, Lit, Param, Pat, Stage, Stmt};
use flick_ir::{IrType, Lang, Span, TypeRef};

/// The longest path of fields a projection may read to be inlined at every
/// use. Each step is one field read: no call, branch or allocation, and
/// about the size of the call it replaces, so a site grows by at most this
/// many reads however many sites name it. Eight covers records nested as
/// deeply as programs write them; a longer path stays a call, under the
/// ordinary budgets.
pub(super) const DEPTH: usize = 8;

/// The labels a projection of `params` and `body` reads, the first one
/// read first: one pure parameter that binds a name, and a body of 1 to
/// [`DEPTH`] field reads over it, with nothing else (no dictionaries, `o?`,
/// calls, branches or captures).
pub(super) fn path(params: &[Param], body: &Expr) -> Option<Vec<String>> {
    let [param] = params else {
        return None;
    };
    let Pat::Bind(x) = &param.pat else {
        return None;
    };
    if param.stage != Stage::Pure {
        return None;
    }
    let mut labels = Vec::new();
    let mut at = body;
    while let ExprKind::Field(inner, label) = &at.kind {
        if labels.len() == DEPTH {
            return None;
        }
        labels.push(label.clone());
        at = inner;
    }
    let reads_param = matches!(&at.kind, ExprKind::Local(l) if l == x);
    (reads_param && !labels.is_empty()).then(|| {
        labels.reverse();
        labels
    })
}

/// [`path`] of a lambda.
fn lambda_path(f: &Expr) -> Option<Vec<String>> {
    match &f.kind {
        ExprKind::Lambda { params, body } => path(params, body),
        _ => None,
    }
}

/// Rewrites the uses of projections known in `e`, a body of `module`, and
/// below; `fresh` numbers the names it binds. Whether anything changed.
pub fn projections(
    e: &mut Expr,
    module: &str,
    candidates: &Candidates,
    foreign: &Foreign,
    lang: &Known<'_>,
    fresh: &mut usize,
) -> bool {
    let mut locals = HashMap::new();
    bound(e, &mut locals);
    let known = Projections {
        module,
        candidates,
        foreign,
        lang,
        locals,
    };
    known.rewrite(e, fresh)
}

/// The projections a body knows: locals are unique in a definition, so a
/// local bound once to one is that one wherever it is read.
struct Projections<'a> {
    module: &'a str,
    candidates: &'a Candidates,
    foreign: &'a Foreign<'a>,
    lang: &'a Known<'a>,
    locals: HashMap<String, Vec<String>>,
}

/// The locals `e` binds with `let` to a projection, with its labels.
fn bound(e: &Expr, out: &mut HashMap<String, Vec<String>>) {
    if let ExprKind::Block(stmts, _) = &e.kind {
        for stmt in stmts {
            if let Stmt::Let {
                pat: Pat::Bind(name),
                value,
                ..
            } = stmt
                && let Some(labels) = lambda_path(value)
            {
                out.insert(name.clone(), labels);
            }
        }
    }
    e.each_child(&mut |c| bound(c, out));
}

impl Projections<'_> {
    /// The labels of the projection `f` is: written there, a local bound to
    /// one, or a top-level one of the build whose body is given.
    fn of(&self, f: &Expr) -> Option<Vec<String>> {
        match &f.kind {
            ExprKind::Lambda { params, body } => path(params, body),
            ExprKind::Local(name) => self.locals.get(name).cloned(),
            ExprKind::Global { module, name, .. } => {
                let (_, params, body) = if module == self.module {
                    self.candidates.get(name).cloned()?
                } else {
                    self.foreign.body(module, name, false)?
                };
                path(&params, &body)
            }
            _ => None,
        }
    }

    fn rewrite(&self, e: &mut Expr, fresh: &mut usize) -> bool {
        let mut changed = false;
        e.each_child_mut(&mut |c| changed |= self.rewrite(c, fresh));
        changed | self.applied(e) | self.mapped(e, fresh)
    }

    /// `get a` of a local projection `get`: the reads of `a`. Reading the
    /// local shows nothing, so `a |> get` is the same.
    fn applied(&self, e: &mut Expr) -> bool {
        let ExprKind::App {
            func,
            arg,
            stage: Stage::Pure,
            ..
        } = &mut e.kind
        else {
            return false;
        };
        let ExprKind::Local(name) = &func.kind else {
            return false;
        };
        let Some(labels) = self.locals.get(name) else {
            return false;
        };
        let arg = mem::replace(&mut **arg, unit(e.span));
        *e = reads(arg, labels, e.ty.clone());
        true
    }

    /// `List.map f xs` of a projection `f`, or `xs |> List.map f`: a strict
    /// `for` over `xs` whose element is the reads of each element. `f` shows
    /// nothing when evaluated, so `xs` is still evaluated once, where the
    /// call is. Over a strict `for`, `f` becomes a lambda fusion then runs
    /// in its loop.
    fn mapped(&self, e: &mut Expr, fresh: &mut usize) -> bool {
        let span = e.span;
        let out_ty = e.ty.clone();
        let ExprKind::App {
            func,
            arg: xs,
            stage: Stage::Pure,
            ..
        } = &mut e.kind
        else {
            return false;
        };
        let ExprKind::App {
            func: head,
            arg: f,
            stage: Stage::Pure,
            arg_first: false,
        } = &mut func.kind
        else {
            return false;
        };
        let ExprKind::Global {
            module,
            name,
            type_args,
        } = &head.kind
        else {
            return false;
        };
        if !self.lang.is(Lang::ListMap, module, name) {
            return false;
        }
        // fusion applies a lambda written there in the loop of the `for`
        let over_for = strict_for(xs);
        if over_for && matches!(f.kind, ExprKind::Lambda { .. }) {
            return false;
        }
        let Some(labels) = self.of(f) else {
            return false;
        };
        let (elem_ty, res_ty) = match &type_args[..] {
            [a, b] => (Some(a.clone()), Some(b.clone())),
            _ => (element(xs.ty.as_ref()), element(out_ty.as_ref())),
        };
        *fresh += 1;
        let v = format!("$elem{fresh}");
        let local = Expr::with_ty(ExprKind::Local(v.clone()), span, elem_ty.clone());
        let read = reads(local, &labels, res_ty);
        let binder_types: Vec<(String, TypeRef)> = (elem_ty.clone())
            .map(|t| vec![(v.clone(), t)])
            .unwrap_or_default();
        if over_for {
            let param = Param {
                pat: Pat::Bind(v),
                stage: Stage::Pure,
                ty: elem_ty,
                binder_types,
            };
            let lambda = ExprKind::Lambda {
                params: vec![param],
                body: Box::new(read),
            };
            **f = Expr::with_ty(lambda, f.span, f.ty.clone());
            return true;
        }
        let source = mem::replace(&mut **xs, unit(span));
        let each = ExprKind::Each {
            stop: None,
            pat: Pat::Bind(v),
            binder_types,
            source: Box::new(source),
            body: Box::new(Expr::new(ExprKind::Yield(Box::new(read)), span)),
        };
        let generate = ExprKind::Generate {
            body: Box::new(Expr::new(each, span)),
            strict: true,
            sync: false,
            limit: None,
        };
        *e = Expr::with_ty(generate, span, out_ty);
        true
    }
}

/// Whether `e` is a `for` that fusion takes a `List.map` into.
fn strict_for(e: &Expr) -> bool {
    matches!(
        &e.kind,
        ExprKind::Generate {
            strict: true,
            sync: false,
            limit: None,
            ..
        }
    )
}

/// `root.l1.….ln`: `root` evaluated once, then each field read once; the
/// last read has the type `ty`, the others that of their field in the type
/// of what they read, when it is a record.
fn reads(root: Expr, labels: &[String], ty: Option<TypeRef>) -> Expr {
    let span = root.span;
    let (last, path) = labels.split_last().expect("a projection reads a field");
    let mut value = root;
    for label in path {
        let field_ty = field(value.ty.as_ref(), label);
        let kind = ExprKind::Field(Box::new(value), label.clone());
        value = Expr::with_ty(kind, span, field_ty);
    }
    Expr::with_ty(ExprKind::Field(Box::new(value), last.clone()), span, ty)
}

/// The type of `label` in the record type `ty`.
fn field(ty: Option<&TypeRef>, label: &str) -> Option<TypeRef> {
    match ty.map(|t| &**t) {
        Some(IrType::Record { fields, .. }) => (fields.iter())
            .find(|(l, _)| l == label)
            .map(|(_, t)| t.clone()),
        _ => None,
    }
}

/// The element type of the list type `ty`.
fn element(ty: Option<&TypeRef>) -> Option<TypeRef> {
    match ty.map(|t| &**t) {
        Some(IrType::Named { args, .. }) if args.len() == 1 => Some(args[0].clone()),
        _ => None,
    }
}

fn unit(span: Span) -> Expr {
    Expr::new(ExprKind::Lit(Lit::Unit), span)
}
