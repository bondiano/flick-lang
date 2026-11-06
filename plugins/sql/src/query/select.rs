//! The clauses of a `SELECT` and of `INSERT`, `UPDATE` and `DELETE`.
//! Clauses may be written in any order after the first; they are checked
//! in SQL's order: tables, `where`, `group_by`, `select`, `having`,
//! `order_by`, `limit`, `offset`, then `union`.

use flick_plugin::Span;

use super::expr::{column_of, value};
use super::parse::{self, Clause, E};
use super::text::{Sql, quote};
use super::write::write;
use super::{Alias, Cx, Out, Phase, Query, Scope, TWICE, Table, UNGROUPED, Val, W};
use crate::shown::Shown;
use crate::table::MALFORMED;

/// The clauses of one word.
pub(super) fn all<'a>(
    clauses: &'a [Clause],
    word: &'a str,
) -> impl Iterator<Item = &'a Clause> + 'a {
    clauses.iter().filter(move |c| c.word.text == word)
}

/// A whole query or statement, nested or not.
pub fn statement(cx: &mut Cx, q: &parse::Query) -> Option<Query> {
    let Some(first) = q.clauses.first() else {
        cx.error(MALFORMED, q.span, "an empty `sql` block");
        return None;
    };
    match first.word.text.as_str() {
        "from" => select(cx, &q.clauses),
        "insert_into" | "update" | "delete_from" => write(cx, &q.clauses),
        other => {
            let message = format!(
                "a query starts with `from`, `insert_into`, `update` or `delete_from`, not `{other}`"
            );
            cx.error(MALFORMED, first.span, message);
            None
        }
    }
}

/// A subquery: its own scope, the parameters of the enclosing query.
pub fn subquery(cx: &mut Cx, e: &E) -> Option<Query> {
    let inner = match e {
        E::Paren(inner, _) => inner,
        other => other,
    };
    let E::Sub(q) = inner else {
        cx.error(MALFORMED, inner.span(), "expected a subquery `sql { … }`");
        return None;
    };
    if q.binder.is_some() {
        let message = "a subquery uses the parameters of its query and names none of its own";
        cx.error(MALFORMED, q.span, message);
    }
    statement(cx, q)
}

/// The table of `from x in T`: a `sql table` or a table of migrations, as
/// the module names it.
pub(super) fn table_alias(cx: &mut Cx, alias: &str, table: &E) -> Option<Alias> {
    let span = table.span();
    let named = match table {
        E::Path(words, _)
            if words
                .last()
                .is_some_and(|w| w.kind == flick_plugin::TokenKind::Con) =>
        {
            Some(words)
        }
        _ => None,
    };
    let Some(words) = named else {
        let message = "a query reads a table by its name: a `sql table` or a table of migrations";
        cx.error(MALFORMED, span, message);
        return None;
    };
    let written: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    let written = written.join(".");
    let published = cx.compiler.published(&written);
    let shown: Option<Shown> = published.and_then(|d| serde_json::from_slice(&d).ok());
    let Some(shown) = shown else {
        let name = words.last().map_or("", |w| w.text.as_str());
        cx.error(MALFORMED, span, format!("`{name}` is not a `sql table`"));
        return None;
    };
    let table_name = words.last().map_or(String::new(), |w| w.text.clone());
    cx.tables.push(Table {
        written,
        at: span,
        shown,
    });
    cx.vars.push(cx.tables.len() - 1);
    Some(Alias {
        name: alias.to_owned(),
        table_name,
        table: cx.tables.len() - 1,
        var: cx.vars.len() - 1,
        nullable: false,
    })
}

fn alias_of(cx: &mut Cx, clause: &Clause) -> Option<Alias> {
    let name = clause.alias.as_ref()?;
    let mut alias = table_alias(cx, &name.text, clause.items.first()?)?;
    alias.nullable = clause.word.text == "left_join";
    Some(alias)
}

/// `WHERE`-like conditions: a `Bool` without aggregates.
pub(super) fn condition(cx: &mut Cx, scope: &Scope, e: &E, phase: Phase) -> Val {
    let v = value(cx, scope, e, phase);
    cx.checks.push(W::Call("truth", vec![v.w.clone()]));
    v
}

pub(super) fn misplaced(cx: &mut Cx, clauses: &[Clause], allowed: &[&str], what: &str) {
    for c in clauses.iter().skip(1) {
        if !allowed.contains(&c.word.text.as_str()) {
            cx.error(
                MALFORMED,
                c.span,
                format!("`{}` has no place in {what}", c.word.text),
            );
        }
    }
}

fn select(cx: &mut Cx, clauses: &[Clause]) -> Option<Query> {
    let allowed = [
        "join",
        "left_join",
        "on",
        "where",
        "group_by",
        "having",
        "select",
        "order_by",
        "limit",
        "offset",
        "union",
        "union_all",
    ];
    misplaced(cx, clauses, &allowed, "a `from` query");
    let (scope, mut sql) = from(cx, clauses)?;
    let conditions: Vec<Sql> = (all(clauses, "where").flat_map(|c| &c.items))
        .map(|n| condition(cx, &scope, n, Phase::Row).sql)
        .collect();
    if !conditions.is_empty() {
        sql.push(" WHERE ");
        sql.append(Sql::join(conditions, " AND "));
    }
    let groups: Vec<Val> = (all(clauses, "group_by").flat_map(|c| &c.items))
        .map(|n| value(cx, &scope, n, Phase::Row))
        .collect();
    if !groups.is_empty() {
        sql.push(" GROUP BY ");
        sql.append(Sql::join(groups.iter().map(|g| g.sql.clone()), ", "));
    }
    let selects: Vec<&Clause> = all(clauses, "select").collect();
    let [select] = selects[..] else {
        let at = selects.get(1).map_or(clauses[0].span, |c| c.span);
        cx.error(MALFORMED, at, "a query has exactly one `select`");
        return None;
    };
    let items = projection(cx, &scope, select, Phase::Grouped);
    let having: Vec<(Val, Span)> = (all(clauses, "having").flat_map(|c| &c.items))
        .map(|n| (condition(cx, &scope, n, Phase::Grouped), n.span()))
        .collect();
    let order: Vec<(Val, &str, Span)> = (all(clauses, "order_by").flat_map(|c| &c.items))
        .map(|n| order_by(cx, &scope, n))
        .collect();
    let grouped = !groups.is_empty() || !having.is_empty() || items.iter().any(|i| i.2.agg);
    if grouped {
        let checked: Vec<(&Val, Span)> = (items.iter().map(|(_, _, v, s)| (v, *s)))
            .chain(having.iter().map(|(v, s)| (v, *s)))
            .chain(order.iter().map(|(v, _, s)| (v, *s)))
            .collect();
        for (v, span) in checked {
            in_groups(cx, v, &groups, span);
        }
    }
    let mut text = Sql::text("SELECT ");
    text.append(Sql::join(items.iter().map(|i| i.1.clone()), ", "));
    text.append(sql);
    if !having.is_empty() {
        text.push(" HAVING ");
        text.append(Sql::join(having.iter().map(|h| h.0.sql.clone()), " AND "));
    }
    if !order.is_empty() {
        text.push(" ORDER BY ");
        let keys = order.iter().map(|(v, d, _)| {
            let mut key = v.sql.clone();
            key.push(d);
            key
        });
        text.append(Sql::join(keys, ", "));
    }
    let columns = items.into_iter().map(|i| i.0).collect();
    Some(paging(cx, &scope, clauses, text, columns))
}

/// `from` and the joins: the aliases and the `FROM` clause.
fn from(cx: &mut Cx, clauses: &[Clause]) -> Option<(Scope, Sql)> {
    let mut scope = Scope::default();
    let first = alias_of(cx, &clauses[0])?;
    let table = &cx.tables[first.table].shown.sql;
    let mut sql = Sql::text(format!(" FROM {} AS {}", quote(table), quote(&first.name)));
    scope.aliases.push(first);
    for (i, clause) in clauses.iter().enumerate().skip(1) {
        let word = clause.word.text.as_str();
        match word {
            "join" | "left_join" => {
                let Some(alias) = alias_of(cx, clause) else {
                    continue;
                };
                if scope.aliases.iter().any(|a| a.name == alias.name) {
                    let message = format!("the query already has a table `{}`", alias.name);
                    cx.error(TWICE, clause.span, message);
                    continue;
                }
                let keyword = if alias.nullable { "LEFT JOIN" } else { "JOIN" };
                let table = &cx.tables[alias.table].shown.sql;
                sql.push(&format!(
                    " {keyword} {} AS {}",
                    quote(table),
                    quote(&alias.name)
                ));
                scope.aliases.push(alias);
                match clauses.get(i + 1) {
                    Some(on) if on.word.text == "on" => {
                        let cond = on.items.first()?;
                        let v = condition(cx, &scope, cond, Phase::Row);
                        sql.push(" ON ");
                        sql.append(v.sql);
                    }
                    _ => {
                        let message = format!("`{word}` needs `on` with the join condition");
                        cx.error(MALFORMED, clause.span, message);
                    }
                }
            }
            "on" if !matches!(clauses[i - 1].word.text.as_str(), "join" | "left_join") => {
                cx.error(MALFORMED, clause.span, "`on` belongs right after a `join`");
            }
            "from" => {
                let message = "a query has one `from`: add tables with `join`";
                cx.error(MALFORMED, clause.span, message);
            }
            _ => {}
        }
    }
    Some((scope, sql))
}

/// `limit`, `offset`, then the unions.
fn paging(
    cx: &mut Cx,
    scope: &Scope,
    clauses: &[Clause],
    mut text: Sql,
    mut columns: Vec<Out>,
) -> Query {
    for word in ["limit", "offset"] {
        for clause in all(clauses, word) {
            let Some(n) = clause.items.first() else {
                continue;
            };
            let v = value(cx, scope, n, Phase::Row);
            cx.checks.push(W::Call("whole", vec![v.w.clone()]));
            text.push(&format!(" {} ", word.to_uppercase()));
            text.append(v.sql);
        }
    }
    for clause in clauses.iter().filter(|c| c.word.text.starts_with("union")) {
        let Some(sub) = clause.items.first() else {
            continue;
        };
        let Some(right) = subquery(cx, sub) else {
            continue;
        };
        union(cx, &mut columns, &right.columns, sub.span());
        let all = if clause.word.text == "union_all" {
            " ALL"
        } else {
            ""
        };
        let mut joined = Sql::wrap("(", text, &format!(") UNION{all} ("));
        joined.append(right.sql);
        joined.push(")");
        text = joined;
    }
    Query { sql: text, columns }
}

/// `order_by e`, `order_by asc e` or `order_by desc e`.
fn order_by(cx: &mut Cx, scope: &Scope, e: &E) -> (Val, &'static str, Span) {
    if let E::Apply(head, args, span) = e
        && let [arg] = &args[..]
        && (head.text == "asc" || head.text == "desc")
    {
        let v = value(cx, scope, arg, Phase::Grouped);
        let dir = if head.text == "asc" { " ASC" } else { " DESC" };
        return (v, dir, *span);
    }
    (value(cx, scope, e, Phase::Grouped), "", e.span())
}

/// In a grouped query a column outside an aggregate must be grouped.
fn in_groups(cx: &mut Cx, v: &Val, groups: &[Val], span: Span) {
    if groups.iter().any(|g| g.sql.key() == v.sql.key()) {
        return;
    }
    for (alias, column) in &v.bare {
        let grouped = (groups.iter())
            .any(|g| g.bare.len() == 1 && g.bare[0] == (alias.clone(), column.clone()) && !g.agg);
        if !grouped {
            let message = format!(
                "`{alias}.{column}` is neither grouped nor aggregated: add it to `group_by` or aggregate it"
            );
            cx.error(UNGROUPED, span, message);
            return;
        }
    }
}

/// Both sides of a union give the same results in the same order.
fn union(cx: &mut Cx, left: &mut [Out], right: &[Out], span: Span) {
    let same = left.len() == right.len() && left.iter().zip(right).all(|(l, r)| l.label == r.label);
    if !same {
        let names = |cols: &[Out]| {
            let labels: Vec<&str> = cols.iter().map(|c| c.label.as_str()).collect();
            labels.join(", ")
        };
        let message = format!(
            "both sides of a union select the same results in the same order: {} and {}",
            names(left),
            names(right)
        );
        cx.error(MALFORMED, span, message);
        return;
    }
    for (l, r) in left.iter_mut().zip(right) {
        cx.checks
            .push(W::Call("equal", vec![l.w.clone(), r.w.clone()]));
        l.null |= r.null;
    }
}

/// `select { label = e; … }`, `select a.x, b.y` or `select alias`; the same
/// forms after `returning`.
pub(super) fn projection(
    cx: &mut Cx,
    scope: &Scope,
    clause: &Clause,
    phase: Phase,
) -> Vec<(Out, Sql, Val, Span)> {
    let mut items: Vec<(String, Val, Span)> = Vec::new();
    for e in &clause.items {
        if let E::Record(fields, _) = e {
            for (label, field) in fields {
                let v = value(cx, scope, field, phase);
                let span = Span {
                    start: label.span.start,
                    end: field.span().end,
                };
                items.push((label.text.clone(), v, span));
            }
            continue;
        }
        let lone = match e {
            E::Path(words, _) if words.len() == 1 => {
                scope.aliases.iter().find(|a| a.name == words[0].text)
            }
            _ => None,
        };
        if let Some(alias) = lone.cloned() {
            let names: Vec<String> = cx
                .columns(alias.table)
                .iter()
                .map(|c| c.name.clone())
                .collect();
            for name in names {
                let v = column_of(cx, &alias, &name);
                items.push((name, v, e.span()));
            }
            continue;
        }
        let label = match e {
            E::Path(words, _)
                if words.len() > 1 || words[0].kind == flick_plugin::TokenKind::Ident =>
            {
                words.last().map(|w| w.text.clone())
            }
            _ => None,
        };
        let v = value(cx, scope, e, phase);
        match label {
            Some(label) => items.push((label, v, e.span())),
            None => cx.error(
                MALFORMED,
                e.span(),
                "name this result: `select { label = … }`",
            ),
        }
    }
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (label, v, span) in items {
        if seen.contains(&label) {
            let message = format!("two results are named `{label}`: give each its own label");
            cx.error(TWICE, span, message);
            continue;
        }
        seen.push(label.clone());
        let mut sql = v.sql.clone();
        sql.push(&format!(" AS {}", quote(&label)));
        let o = Out {
            label,
            w: v.w.clone(),
            null: v.null,
            form: v.form.clone(),
            known: v.known.clone(),
        };
        out.push((o, sql, v, span));
    }
    out
}
