//! SQL expressions inside `sql { … }`: columns of the query's aliases,
//! parameters, captures and literals as placeholders, comparisons in SQL's
//! three-valued logic, `is_null`, `is_not_null`, null-safe
//! `is_not_distinct_from`, `coalesce`, aggregates with their phase,
//! `exists` and `is_in` over uncorrelated subqueries.

use flick_plugin::{Span, TokenKind};

use super::parse::{E, Op, Tok};
use super::text::{Sql, quote};
use super::{
    AMBIGUOUS, Alias, At, Cx, Form, Known, MISPLACED, NO_COLUMN, NO_TABLE, Phase, Scope, Val, W,
    select,
};
use crate::table::MALFORMED;

const FUNCTIONS: &str = "is_null, is_not_null, is_not_distinct_from, coalesce, count, \
                         count_all, sum, avg, min, max, exists, is_in";

const NOT_SQL: &str =
    "this is not a SQL expression: use columns, parameters, literals, operators and SQL functions";

/// The value of `e`, where aggregates stand as `phase` says; its term
/// stands at `e`, where a type error of it is reported.
pub fn value(cx: &mut Cx, scope: &Scope, e: &E, phase: Phase) -> Val {
    let mut v = value_of(cx, scope, e, phase);
    v.w = W::At(Box::new(v.w), e.span());
    v
}

fn value_of(cx: &mut Cx, scope: &Scope, e: &E, phase: Phase) -> Val {
    match e {
        E::Paren(inner, _) => {
            let mut v = value(cx, scope, inner, phase);
            v.sql = Sql::wrap("(", v.sql, ")");
            v
        }
        E::Literal(t) => literal(cx, t),
        E::Capture { inner, span } => {
            let text = cx.block.text_of(*span).to_owned();
            let k = cx.capture(text, *span, *inner);
            let mut v = Val::plain(Sql::slot(cx.captures[k].slot), W::Capture(k));
            v.at = Some(At::Capture(k));
            v
        }
        E::Path(words, span) => path(cx, scope, words, *span, phase),
        E::Binary(op, l, r, _) => binary(cx, scope, *op, l, r, phase),
        E::Not(inner, _) => {
            let v = value(cx, scope, inner, phase);
            Val {
                sql: Sql::wrap("(NOT ", v.sql, ")"),
                w: W::Call("truth", vec![v.w]),
                form: None,
                at: None,
                known: Known::Unknown,
                ..v
            }
        }
        E::Neg(inner, _) => {
            let v = value(cx, scope, inner, phase);
            Val {
                sql: Sql::wrap("(-", v.sql, ")"),
                w: W::Fact(Box::new(v.w), inner.span()),
                form: None,
                at: None,
                ..v
            }
        }
        E::Apply(name, args, span) => call(cx, scope, name, args, *span, phase),
        E::Sub(q) => {
            cx.error(
                MALFORMED,
                q.span,
                "a subquery goes in `exists`, `is_in` or `union`",
            );
            Val::error()
        }
        E::Record(_, span) => {
            cx.error(MALFORMED, *span, NOT_SQL);
            Val::error()
        }
    }
}

/// A literal: `true` and `false` inline, anything else a placeholder.
fn literal(cx: &mut Cx, t: &Tok) -> Val {
    if t.kind == TokenKind::Keyword {
        let truth = t.text == "true";
        let mut v = Val::plain(
            Sql::text(if truth { "TRUE" } else { "FALSE" }),
            W::Boolean(truth),
        );
        v.known = Known::Kind("bool".into(), "Bool".into());
        return v;
    }
    let k = cx.literal(t.span);
    let mut v = Val::plain(Sql::slot(cx.literals[k].slot), W::Literal(k));
    v.at = Some(At::Literal(k));
    v
}

/// `count_all`, a column, `alias.column`, `params.name` or `Table.column`
/// in a statement on one table.
fn path(cx: &mut Cx, scope: &Scope, words: &[Tok], span: Span, phase: Phase) -> Val {
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    match texts[..] {
        ["count_all"] => {
            aggregate_allowed(cx, phase, span);
            let mut v = Val::plain(Sql::text("COUNT(*)"), W::Call("counted", vec![W::Unit]));
            v.agg = true;
            v.known = Known::Kind("bigint".into(), "BigInt".into());
            v
        }
        [name] if name.starts_with(|c: char| c.is_ascii_lowercase()) => {
            let found: Vec<&Alias> = (scope.aliases.iter())
                .filter(|a| cx.columns(a.table).iter().any(|c| c.name == name))
                .collect();
            match found[..] {
                [alias] => {
                    let alias = alias.clone();
                    column_of(cx, &alias, name)
                }
                [] => {
                    let message = format!("no table of this query has a column `{name}`");
                    cx.error(NO_COLUMN, span, message);
                    Val::error()
                }
                _ => {
                    let names: Vec<String> =
                        found.iter().map(|a| format!("`{}`", a.name)).collect();
                    let message = format!(
                        "`{name}` is a column of {}: name the alias",
                        names.join(" and ")
                    );
                    cx.error(AMBIGUOUS, span, message);
                    Val::error()
                }
            }
        }
        [target, label]
            if cx.binder.as_deref() == Some(target) && words[0].kind == TokenKind::Ident =>
        {
            let i = cx.param(label, span);
            let p = &cx.params[i];
            let mut v = Val::plain(Sql::slot(p.slot), W::Param(i));
            v.null = p.nullable;
            v.form.clone_from(&p.form);
            v.at = Some(At::Param(i));
            v
        }
        [alias, column] => qualified(cx, scope, alias, column, span),
        _ => {
            cx.error(MALFORMED, span, "this is not a column of the query");
            Val::error()
        }
    }
}

/// `alias.column` of an alias of the query.
fn qualified(cx: &mut Cx, scope: &Scope, alias: &str, column: &str, span: Span) -> Val {
    let Some(a) = scope.aliases.iter().find(|a| a.name == alias).cloned() else {
        cx.error(NO_TABLE, span, format!("no table `{alias}` in this query"));
        return Val::error();
    };
    if !cx.columns(a.table).iter().any(|c| c.name == column) {
        let message = format!("`{}` has no column `{column}`", a.table_name);
        cx.error(NO_COLUMN, span, message);
        return Val::error();
    }
    column_of(cx, &a, column)
}

/// A column of an alias that has it.
pub fn column_of(cx: &Cx, alias: &Alias, column: &str) -> Val {
    let col = (cx.columns(alias.table).iter())
        .find(|c| c.name == column)
        .expect("the caller found the column");
    Val {
        sql: Sql::text(format!("{}.{}", quote(&alias.name), quote(column))),
        w: W::Column(alias.var, column.to_owned()),
        null: col.nullable || alias.nullable,
        agg: false,
        bare: vec![(alias.name.clone(), column.to_owned())],
        form: Some(Form {
            table: alias.table,
            column: column.to_owned(),
            via: col.via,
        }),
        at: None,
        known: if col.via {
            Known::Via
        } else {
            Known::Kind(col.kind.clone(), col.base.clone())
        },
    }
}

fn binary(cx: &mut Cx, scope: &Scope, op: Op, lhs: &E, rhs: &E, phase: Phase) -> Val {
    let l = value(cx, scope, lhs, phase);
    let r = value(cx, scope, rhs, phase);
    let (sql_op, w, known) = match op {
        Op::And | Op::Or => {
            cx.checks.push(l.w.clone());
            cx.checks.push(r.w.clone());
            let word = if op == Op::And { "AND" } else { "OR" };
            (
                word,
                W::Boolean(true),
                Known::Kind("bool".into(), "Bool".into()),
            )
        }
        Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            cx.adopt(&l, &r);
            let word = match op {
                Op::Eq => "=",
                Op::Ne => "<>",
                Op::Lt => "<",
                Op::Le => "<=",
                Op::Gt => ">",
                _ => ">=",
            };
            let w = W::Call("equal", vec![l.w.clone(), r.w.clone()]);
            (word, w, Known::Kind("bool".into(), "Bool".into()))
        }
        Op::Concat => {
            let w = W::Call("joined", vec![l.w.clone(), r.w.clone()]);
            ("||", w, Known::Kind("text".into(), "String".into()))
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem => {
            cx.adopt(&l, &r);
            let fact = W::Fact(Box::new(l.w.clone()), lhs.span());
            let w = W::Call("either", vec![fact, r.w.clone()]);
            (op.text(), w, l.known.clone())
        }
    };
    let mut bare = l.bare;
    bare.extend(r.bare);
    let mut sql = Sql::wrap("(", l.sql, &format!(" {sql_op} "));
    sql.append(r.sql);
    sql.push(")");
    Val {
        sql,
        w,
        null: l.null || r.null,
        agg: l.agg || r.agg,
        bare,
        form: None,
        at: None,
        known,
    }
}

fn aggregate_allowed(cx: &mut Cx, phase: Phase, span: Span) {
    if phase == Phase::Row {
        let message =
            "an aggregate is computed per group: use it in `select`, `having` or `order_by`";
        cx.error(MISPLACED, span, message);
    }
}

fn call(cx: &mut Cx, scope: &Scope, name: &Tok, args: &[E], span: Span, phase: Phase) -> Val {
    let arity = match name.text.as_str() {
        "is_null" | "is_not_null" | "count" | "sum" | "avg" | "min" | "max" | "exists" => 1,
        "is_not_distinct_from" | "coalesce" | "is_in" => 2,
        _ => {
            let message = format!("`{}` is not a SQL function; known: {FUNCTIONS}", name.text);
            cx.error(MALFORMED, name.span, message);
            return Val::error();
        }
    };
    if args.len() != arity {
        let plural = if arity == 1 { "" } else { "s" };
        let message = format!("`{}` takes {arity} argument{plural}", name.text);
        cx.error(MALFORMED, span, message);
        return Val::error();
    }
    match name.text.as_str() {
        "exists" | "is_in" => membership(cx, scope, &name.text, args, phase),
        "is_null" | "is_not_null" => {
            let v = value(cx, scope, &args[0], phase);
            let test = if name.text == "is_null" {
                " IS NULL)"
            } else {
                " IS NOT NULL)"
            };
            Val {
                sql: Sql::wrap("(", v.sql, test),
                w: W::Call("tested", vec![v.w]),
                null: false,
                form: None,
                at: None,
                known: Known::Kind("bool".into(), "Bool".into()),
                ..v
            }
        }
        "is_not_distinct_from" | "coalesce" => {
            let a = value(cx, scope, &args[0], phase);
            let b = value(cx, scope, &args[1], phase);
            cx.adopt(&a, &b);
            let mut bare = a.bare.clone();
            bare.extend(b.bare.clone());
            let agg = a.agg || b.agg;
            if name.text == "coalesce" {
                let known = if a.known == Known::Unknown {
                    b.known.clone()
                } else {
                    a.known.clone()
                };
                let mut sql = Sql::wrap("COALESCE(", a.sql, ", ");
                sql.append(b.sql);
                sql.push(")");
                Val {
                    sql,
                    w: W::Call("either", vec![a.w, b.w]),
                    null: a.null && b.null,
                    agg,
                    bare,
                    form: a.form.or(b.form),
                    at: None,
                    known,
                }
            } else {
                let mut sql = Sql::wrap("(", a.sql, " IS NOT DISTINCT FROM ");
                sql.append(b.sql);
                sql.push(")");
                Val {
                    sql,
                    w: W::Call("equal", vec![a.w, b.w]),
                    null: false,
                    agg,
                    bare,
                    form: None,
                    at: None,
                    known: Known::Kind("bool".into(), "Bool".into()),
                }
            }
        }
        _ => aggregate(cx, scope, &name.text, &args[0], span, phase),
    }
}

/// `exists q` and `is_in e q` over an uncorrelated subquery.
fn membership(cx: &mut Cx, scope: &Scope, name: &str, args: &[E], phase: Phase) -> Val {
    if name == "exists" {
        let Some(sub) = select::subquery(cx, &args[0]) else {
            return Val::error();
        };
        let mut v = Val::plain(Sql::wrap("EXISTS (", sub.sql, ")"), W::Boolean(true));
        v.known = Known::Kind("bool".into(), "Bool".into());
        return v;
    }
    let v = value(cx, scope, &args[0], phase);
    let Some(sub) = select::subquery(cx, &args[1]) else {
        return Val::error();
    };
    if let [only] = &sub.columns[..] {
        cx.checks
            .push(W::Call("equal", vec![v.w.clone(), only.w.clone()]));
    } else {
        let message = "the subquery of `is_in` selects exactly one result";
        cx.error(MALFORMED, args[1].span(), message);
    }
    let null = v.null || sub.columns.iter().any(|c| c.null);
    let mut sql = Sql::wrap("(", v.sql, " IN (");
    sql.append(sub.sql);
    sql.push("))");
    Val {
        sql,
        w: W::Boolean(true),
        null,
        agg: v.agg,
        bare: v.bare,
        form: None,
        at: None,
        known: Known::Kind("bool".into(), "Bool".into()),
    }
}

/// `count`, `sum`, `avg`, `min`, `max`: `NULL` over no rows except
/// `count`; sums and averages cast to the type their result has.
fn aggregate(cx: &mut Cx, scope: &Scope, name: &str, arg: &E, span: Span, phase: Phase) -> Val {
    aggregate_allowed(cx, phase, span);
    let v = value(cx, scope, arg, Phase::Row);
    let upper = name.to_uppercase();
    let sql = Sql::wrap(&format!("{upper}("), v.sql, ")");
    let (sql, w, null, form, known) = match name {
        "count" => (
            sql,
            W::Call("counted", vec![v.w]),
            false,
            None,
            Known::Kind("bigint".into(), "BigInt".into()),
        ),
        "min" | "max" => (sql, v.w, true, v.form, v.known),
        _ => {
            // a column without a codec says its type: SQL sums only numbers
            let known = match &v.known {
                Known::Kind(kind, base) => {
                    let Some(result) = numeric(name, kind) else {
                        let message = format!(
                            "`{name}` takes `Float`, `BigInt` or `BigDecimal`, not `{base}`"
                        );
                        cx.error(MALFORMED, arg.span(), message);
                        return Val::error();
                    };
                    Known::Kind(result.0.into(), result.1.into())
                }
                _ => Known::Unknown,
            };
            let proxy = cx.proxies;
            cx.proxies += 1;
            let mut sql = sql;
            let cast = if name == "sum" {
                "sum_cast"
            } else {
                "avg_cast"
            };
            sql.0.push(super::Piece::Cast(cast, proxy));
            (
                sql,
                W::Num(
                    if name == "sum" { "sum" } else { "avg" },
                    proxy,
                    Box::new(v.w),
                ),
                true,
                None,
                known,
            )
        }
    };
    Val {
        sql,
        w,
        null,
        agg: true,
        bare: Vec::new(),
        form,
        at: None,
        known,
    }
}

/// The runtime kind and type of `sum` or `avg` over values of `kind`.
fn numeric(name: &str, kind: &str) -> Option<(&'static str, &'static str)> {
    Some(match (name, kind) {
        (_, "number") => ("number", "Float"),
        ("sum", "bigint") => ("bigint", "BigInt"),
        (_, "bigint" | "numeric") => ("numeric", "BigDecimal"),
        _ => return None,
    })
}
