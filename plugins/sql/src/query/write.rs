//! `insert_into T values { … }`, `update T set { … } where …` and
//! `delete_from T where …`, each with an optional `returning`: the table's
//! name is its alias, `Users.id`.

use flick_plugin::{Span, TokenKind};

use super::expr::{column_of, value};
use super::parse::{Clause, E};
use super::select::{all, condition, misplaced, projection, table_alias};
use super::text::{Sql, quote};
use super::{Alias, At, Cx, Form, GENERATED, MISSING, NO_COLUMN, Phase, Query, Scope, TWICE, W};
use crate::table::MALFORMED;

/// `insert_into T values { … }`, `update T set { … } where …` and
/// `delete_from T where …`, each with an optional `returning`.
pub fn write(cx: &mut Cx, clauses: &[Clause]) -> Option<Query> {
    let (word, head) = (clauses[0].word.text.as_str(), &clauses[0]);
    let allowed: &[&str] = match word {
        "insert_into" => &["values", "returning"],
        "update" => &["set", "where", "returning"],
        _ => &["where", "returning"],
    };
    misplaced(cx, clauses, allowed, &format!("`{word}`"));
    let table = head.items.first()?;
    let name = match table {
        E::Path(words, _) => (words.iter().rev())
            .find(|w| w.kind == TokenKind::Con)
            .map_or_else(|| "t".to_owned(), |w| w.text.clone()),
        _ => "t".to_owned(),
    };
    let alias = table_alias(cx, &name, table)?;
    let sql_name = &cx.tables[alias.table].shown.sql;
    let target = format!("{} AS {}", quote(sql_name), quote(&alias.name));
    let scope = Scope {
        aliases: vec![alias],
    };
    let mut text = match word {
        "insert_into" => {
            let Some(values) = all(clauses, "values").next() else {
                cx.error(MALFORMED, head.span, "`insert_into` needs `values { … }`");
                return None;
            };
            let given = assignments(cx, &scope, values);
            required(cx, &scope.aliases[0], &given, values.span);
            let names: Vec<String> = given.iter().map(|(c, _)| quote(c)).collect();
            let mut sql = Sql::text(format!(
                "INSERT INTO {target} ({}) VALUES (",
                names.join(", ")
            ));
            sql.append(Sql::join(given.into_iter().map(|(_, v)| v), ", "));
            sql.push(")");
            sql
        }
        "update" => {
            let Some(set) = all(clauses, "set").next() else {
                cx.error(MALFORMED, head.span, "`update` needs `set { … }`");
                return None;
            };
            let given = assignments(cx, &scope, set);
            let sets = given.into_iter().map(|(c, v)| {
                let mut s = Sql::text(format!("{} = ", quote(&c)));
                s.append(v);
                s
            });
            let mut sql = Sql::text(format!("UPDATE {target} SET "));
            sql.append(Sql::join(sets, ", "));
            sql
        }
        _ => Sql::text(format!("DELETE FROM {target}")),
    };
    let conditions: Vec<Sql> = (all(clauses, "where").flat_map(|c| &c.items))
        .map(|n| condition(cx, &scope, n, Phase::Row).sql)
        .collect();
    if !conditions.is_empty() {
        text.push(" WHERE ");
        text.append(Sql::join(conditions, " AND "));
    }
    let mut columns = Vec::new();
    if let Some(returning) = all(clauses, "returning").next() {
        let items = projection(cx, &scope, returning, Phase::Row);
        text.push(" RETURNING ");
        text.append(Sql::join(items.iter().map(|i| i.1.clone()), ", "));
        columns = items.into_iter().map(|i| i.0).collect();
    }
    Some(Query { sql: text, columns })
}

/// The fields of `values { … }` or `set { … }`: each a column the database
/// does not generate, with a value of its type.
fn assignments(cx: &mut Cx, scope: &Scope, clause: &Clause) -> Vec<(String, Sql)> {
    let alias = scope.aliases[0].clone();
    let Some(E::Record(fields, _)) = clause.items.first() else {
        cx.error(
            MALFORMED,
            clause.span,
            "expected a record `{ column = value; … }`",
        );
        return Vec::new();
    };
    let mut given: Vec<(String, Sql)> = Vec::new();
    for (label, value_of) in fields {
        let span = Span {
            start: label.span.start,
            end: value_of.span().end,
        };
        let name = label.text.clone();
        let Some(col) = cx
            .columns(alias.table)
            .iter()
            .find(|c| c.name == name)
            .cloned()
        else {
            let message = format!("`{}` has no column `{name}`", alias.table_name);
            cx.error(NO_COLUMN, span, message);
            continue;
        };
        if given.iter().any(|(c, _)| *c == name) {
            cx.error(TWICE, span, format!("`{name}` is given twice"));
            continue;
        }
        if col.write == "Generated" {
            let message = format!("the database generates `{name}`: it cannot be written");
            cx.error(GENERATED, span, message);
            continue;
        }
        let v = value(cx, scope, value_of, Phase::Row);
        let column = column_of(cx, &alias, &name);
        cx.checks
            .push(W::Call("equal", vec![column.w, v.w.clone()]));
        let form = Form {
            table: alias.table,
            column: name.clone(),
            via: col.via,
        };
        match v.at {
            Some(At::Param(i)) => {
                cx.params[i].nullable |= col.nullable;
                cx.params[i].form.get_or_insert(form);
            }
            Some(At::Capture(i)) => {
                cx.captures[i].form.get_or_insert(form);
            }
            _ => {}
        }
        given.push((name, v.sql));
    }
    given
}

/// Every required column has a value.
fn required(cx: &mut Cx, alias: &Alias, given: &[(String, Sql)], span: Span) {
    let missing: Vec<String> = (cx.columns(alias.table).iter())
        .filter(|c| c.write == "Required" && !given.iter().any(|(g, _)| *g == c.name))
        .map(|c| format!("`{}`", c.name))
        .collect();
    if !missing.is_empty() {
        let message = format!(
            "the insert misses the required column {}",
            missing.join(", ")
        );
        cx.error(MISSING, span, message);
    }
}
