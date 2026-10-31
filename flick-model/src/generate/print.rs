//! A program of the generator as Flick source
//! (`docs/src/content/docs/reference/grammar.md`): `Gen.fl`, then
//! `program : σ` and its `let`. Only the forms the generator makes are
//! written. Layout: a block on lines of its own, two columns deeper than
//! the line it belongs to; the arms of `match` and `try` two columns
//! deeper than that line, their bodies two more; every compound part of
//! an expression in parentheses.

use std::fmt::Write as _;

use super::{PRELUDE, Program};
use crate::syntax::{Arm, Const, Head, Lit, Pat, Prim, Term};
use crate::types::{Row, Scheme, Stage, Type, Var};

use flick_core::numeric::Arith;

/// The source flick-core checks: the prelude and the program.
pub fn source(p: &Program) -> String {
    let head = match &p.term {
        Term::Lam(x, body) => format!("let program {x} =\n{}", block(body, 2)),
        e => format!("let program =\n{}", block(e, 2)),
    };
    format!("{PRELUDE}\nprogram : {}\n{head}\n", scheme(&p.scheme))
}

/// `e` as the statements of a block at column `col`.
fn block(e: &Term, col: usize) -> String {
    let pad = " ".repeat(col);
    match e {
        // `s` before the rest of a block is `let _ : Unit = s` (§1.4)
        Term::Let(x, Some(_), s, rest) if &**x == "_" => {
            format!("{pad}{}\n{}", expr(s, col), block(rest, col))
        }
        Term::Let(x, sig, rhs, rest) => {
            format!(
                "{}\n{}",
                binding(x, sig.as_deref(), rhs, col, ""),
                block(rest, col)
            )
        }
        Term::LetRec(f, sig, lam, rest) => {
            format!(
                "{}\n{}",
                binding(f, sig.as_deref(), lam, col, "rec "),
                block(rest, col)
            )
        }
        e => format!("{pad}{}", expr(e, col)),
    }
}

/// `let x = e`, `let x : σ = e`, or for a function its signature on the
/// line above `let f x y =` (`FLICK-T004`).
fn binding(x: &str, sig: Option<&Scheme>, rhs: &Term, col: usize, rec: &str) -> String {
    let pad = " ".repeat(col);
    match sig {
        Some(s) if matches!(s.body, Type::Fun(..)) => {
            let mut params = String::new();
            let mut body = rhs;
            // the parameters of one `let` are distinct: a name again is a λ
            // of its own in the body
            while let Term::Lam(p, b) = body
                && !params.split(' ').any(|q| q == &**p)
            {
                params.push(' ');
                params.push_str(p);
                body = b;
            }
            format!(
                "{pad}{x} : {}\n{pad}let {rec}{x}{params} =\n{}",
                scheme(s),
                block(body, col + 2)
            )
        }
        Some(s) => format!(
            "{pad}let {rec}{x} : {} =\n{}",
            scheme(s),
            block(rhs, col + 2)
        ),
        None => format!("{pad}let {rec}{x} =\n{}", block(rhs, col + 2)),
    }
}

/// `e` as one expression whose first line follows what is before it on
/// the line, in a statement at column `col`: an atom or parenthesized.
fn expr(e: &Term, col: usize) -> String {
    let inner = col + 2;
    match e {
        Term::Var(x) => x.to_string(),
        Term::Lit(l) => lit(l),
        Term::Lam(x, body) => format!("(fun {x} ->\n{})", block(body, inner)),
        Term::Tuple(xs) => format!("({})", list(xs.iter().map(|x| expr(x, col)), ", ")),
        Term::Record(fs) => {
            let fields: Vec<String> = fs
                .iter()
                .map(|(l, x)| format!("{l} = {}", expr(x, inner)))
                .collect();
            if fields.iter().any(|f| f.contains('\n')) {
                let pad = " ".repeat(inner);
                format!("{{\n{pad}{} }}", fields.join(&format!("\n{pad}")))
            } else {
                format!("{{ {} }}", fields.join("; "))
            }
        }
        Term::Field(r, l) if matches!(**r, Term::Var(_)) => format!("{}.{l}", expr(r, col)),
        // `0.a` would read as a number
        Term::Field(r, l) => format!("({}).{l}", expr(r, col)),
        Term::Task(body) => format!("(task {{\n{} }})", block(body, inner)),
        Term::Await(x) => format!("(await {})", expr(x, col)),
        Term::Match(s, arms) => format!("(match {} with{})", expr(s, col), cases(arms, col)),
        Term::Try(body, arms) => format!(
            "(try\n{}\n{}with{})",
            block(body, col + 4),
            " ".repeat(inner),
            cases(arms, col)
        ),
        Term::Provide(l, body) => {
            // a record after `provide` would read as its block
            let layer = match &**l {
                Term::Record(_) => format!("({})", expr(l, col)),
                _ => expr(l, col),
            };
            format!("(provide {layer} {{\n{} }})", block(body, inner))
        }
        Term::App(..) | Term::Apply(..) => app(e, col),
        other => panic!("the generator makes no {other:?} outside a block"),
    }
}

/// The arms of a `match` or `try` in a statement at column `col`.
fn cases(arms: &[Arm], col: usize) -> String {
    let bar = " ".repeat(col + 2);
    let mut out = String::new();
    for a in arms {
        let guard = a
            .guard
            .as_ref()
            .map(|g| format!(" when {}", expr(g, col + 2)))
            .unwrap_or_default();
        let _ = write!(
            out,
            "\n{bar}| {}{guard} ->\n{}",
            pat(&a.pat, false),
            block(&a.body, col + 4)
        );
    }
    out
}

/// `f a b`: the constants of Γ₀ as the source writes them (§1.4).
fn app(e: &Term, col: usize) -> String {
    let mut args = Vec::new();
    let mut head = e;
    while let Term::App(f, a) = head {
        args.push(expr(a, col));
        head = f;
    }
    args.reverse();
    // what is applied to more arguments than it takes (a change of
    // `mutate`) is applied to the rest
    let name = match head {
        Term::Apply(Head::Const(c), xs) if xs.is_empty() => match c {
            Const::Prim(p) if args.len() >= 2 => {
                let op = format!("({} {} {})", args[0], operator(*p), args[1]);
                args.drain(..2);
                op
            }
            Const::Raise if !args.is_empty() => format!("(from_result (Err {}))", args.remove(0)),
            Const::RefMake => "Ref.make".into(),
            Const::RefGet => "Ref.get".into(),
            Const::RefSet => "Ref.set".into(),
            Const::LayerSucceed(s) => format!("Layer.succeed {}", s.name),
            Const::LayerEffect(s) => format!("Layer.effect {}", s.name),
            other => panic!("the generator makes no {other:?}"),
        },
        Term::Apply(Head::Con(c), xs) if xs.is_empty() => c.name.to_string(),
        // `(ask_S ()).op` is the operation `S.op`
        Term::Field(ask, op) => match &**ask {
            Term::App(f, _) => match &**f {
                Term::Apply(Head::Const(Const::Ask(s)), _) => format!("{}.{op}", s.name),
                _ => expr(head, col),
            },
            _ => expr(head, col),
        },
        other => expr(other, col),
    };
    if args.is_empty() {
        return name;
    }
    format!("({name} {})", args.join(" "))
}

fn operator(p: Prim) -> &'static str {
    match p {
        Prim::Arith(Arith::Add, _) => "+",
        Prim::Arith(Arith::Sub, _) => "-",
        Prim::Lt => "<",
        Prim::Concat => "++",
        other => panic!("the generator makes no {other:?}"),
    }
}

fn lit(l: &Lit) -> String {
    match l {
        Lit::Int(n) => n.to_string(),
        Lit::Float(x) => format!("{x:?}"),
        Lit::String(s) => format!("{s:?}"),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "()".into(),
    }
}

/// A pattern; one with arguments in parentheses where `nested`.
fn pat(p: &Pat, nested: bool) -> String {
    match p {
        Pat::Wild => "_".into(),
        Pat::Var(x) => x.to_string(),
        Pat::Lit(l) => lit(l),
        Pat::Con(c, ps) if ps.is_empty() => c.name.to_string(),
        Pat::Con(c, ps) => {
            let s = format!("{} {}", c.name, list(ps.iter().map(|p| pat(p, true)), " "));
            if nested { format!("({s})") } else { s }
        }
        Pat::Tuple(ps) => format!("({})", list(ps.iter().map(|p| pat(p, false)), ", ")),
        Pat::Record(fs) => format!(
            "{{ {} }}",
            list(
                fs.iter().map(|(l, p)| format!("{l} = {}", pat(p, false))),
                "; "
            )
        ),
        other => panic!("the generator makes no pattern {other:?}"),
    }
}

fn list(items: impl Iterator<Item = String>, sep: &str) -> String {
    items.collect::<Vec<_>>().join(sep)
}

/// `σ`, its variables under an explicit `forall`, so a recursive binding
/// is polymorphic in its own body for both checkers (σ° of (Let-Rec)).
fn scheme(s: &Scheme) -> String {
    if s.vars.is_empty() {
        return ty(&s.body);
    }
    let vars = list(s.vars.iter().map(|(v, _)| v.to_string()), " ");
    format!("forall {vars}. {}", ty(&s.body))
}

fn ty(t: &Type) -> String {
    match t {
        Type::Fun(p, s, r) => format!("{} {} {}", ty_atom(p), stage(s), ty(r)),
        Type::Eff(r, e, a) => format!("Eff {} {} {}", row_arg(r), row_arg(e), ty_atom(a)),
        Type::App(f, x) => format!("{} {}", ty(f), ty_atom(x)),
        Type::Ref(a) => format!("Ref {}", ty_atom(a)),
        _ => ty_atom(t),
    }
}

fn ty_atom(t: &Type) -> String {
    match t {
        Type::Var(v) => v.to_string(),
        Type::Con(p) => p.name.to_string(),
        Type::Tuple(ts) => format!("({})", list(ts.iter().map(ty), ", ")),
        Type::Record(f) => format!(
            "{{ {} }}",
            list(
                f.fields.iter().map(|(l, t)| format!("{l} : {}", ty(t))),
                "; "
            )
        ),
        _ => format!("({})", ty(t)),
    }
}

/// `->`, `->{}`, `->{Log; Oops}`.
fn stage(s: &Stage) -> String {
    match s {
        Stage::Pure => "->".into(),
        Stage::Runtime(r, e) if r.is_none() && e.is_none() => "->{}".into(),
        Stage::Runtime(r, e) => format!("->{{{}; {}}}", items(r), items(e)),
    }
}

/// A row as a type argument: `none`, `r`, `{Log, Counter}`.
fn row_arg(r: &Row) -> String {
    match (r.elems.is_empty(), r.vars.len()) {
        (true, 0) => "none".into(),
        (true, 1) => items(r),
        _ => format!("{{{}}}", items(r)),
    }
}

/// The elements of a row, then its variable: `Log, Counter | r`.
fn items(r: &Row) -> String {
    let elems = list(r.elems.keys().map(|p| p.name.to_string()), ", ");
    match r.vars.iter().next() {
        Some(Var::Named(v)) if elems.is_empty() => v.to_string(),
        Some(Var::Named(v)) => format!("{elems} | {v}"),
        _ => elems,
    }
}
