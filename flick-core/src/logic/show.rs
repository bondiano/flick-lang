//! Normal forms as Flick source, for messages.

use std::fmt::Write as _;

use flick_ir::Lit;

use super::eval::{Nf, NfPat};
use super::term::Literal;

pub fn show(nf: &Nf) -> String {
    let mut out = String::new();
    write(nf, &mut out, false, 0);
    out
}

/// The name of a variable as the source spells it: the lowering numbers
/// names it repeats, `x$1`.
fn source_name(x: &str) -> &str {
    x.split('$').next().filter(|n| !n.is_empty()).unwrap_or(x)
}

fn literal(l: &Literal) -> String {
    match &l.0 {
        Lit::Int(n) => n.to_string(),
        Lit::Float(x) => format!("{x:?}"),
        Lit::BigInt(n) => format!("{n}n"),
        Lit::String(s) => format!("{s:?}"),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "()".into(),
    }
}

/// Writes `nf`, under `depth` bound variables, parenthesized as an argument.
fn write(nf: &Nf, out: &mut String, argument: bool, depth: u32) {
    let compound = match nf {
        Nf::Con(_, args) => !args.is_empty(),
        Nf::App(..) | Nf::Lam(_) | Nf::Match(..) => true,
        _ => false,
    };
    if argument && compound {
        out.push('(');
    }
    match nf {
        Nf::Sym(x) => out.push_str(source_name(x)),
        Nf::Bound(n) => {
            let _ = write!(out, "x{n}");
        }
        Nf::Lit(l) => out.push_str(&literal(l)),
        Nf::Con(c, args) => {
            out.push_str(&c.name);
            for a in args {
                out.push(' ');
                write(a, out, true, depth);
            }
        }
        Nf::App(f, a) => {
            write(f, out, matches!(**f, Nf::Lam(_) | Nf::Match(..)), depth);
            out.push(' ');
            write(a, out, true, depth);
        }
        Nf::Lam(body) => {
            let _ = write!(out, "fun x{depth} -> ");
            write(body, out, false, depth + 1);
        }
        Nf::Match(s, arms) => {
            out.push_str("match ");
            write(s, out, false, depth);
            out.push_str(" with");
            for (p, body) in arms {
                out.push_str(" | ");
                let mut at = depth;
                pattern(p, out, &mut at);
                out.push_str(" -> ");
                write(body, out, false, at);
            }
        }
    }
    if argument && compound {
        out.push(')');
    }
}

fn pattern(p: &NfPat, out: &mut String, at: &mut u32) {
    match p {
        NfPat::Wild => out.push('_'),
        NfPat::Bind => {
            let _ = write!(out, "x{at}");
            *at += 1;
        }
        NfPat::Lit(l) => out.push_str(&literal(l)),
        NfPat::Con(c, fields) => {
            out.push_str(&c.name);
            for f in fields {
                out.push(' ');
                let parens = matches!(f, NfPat::Con(_, xs) if !xs.is_empty());
                if parens {
                    out.push('(');
                }
                pattern(f, out, at);
                if parens {
                    out.push(')');
                }
            }
        }
    }
}
