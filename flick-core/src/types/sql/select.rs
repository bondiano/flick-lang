//! The clauses of a `SELECT` and of `INSERT`, `UPDATE` and `DELETE`.
//! Clauses may be written in any order after the first; they are checked
//! in SQL's order: tables, `where`, `group_by`, `select`, `having`,
//! `order_by`, `limit`, `offset`, then `union`.

use std::fmt::Write;

use super::super::Checker;
use super::super::ty::Type;
use super::super::typing::SqlForm;
use super::expr::{Place, Val, inner, quote};
use super::query::{Alias, Cx, Out, Query, Scope};
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::lang::Lang;

type Clauses<'n> = [(String, &'n Node)];

/// The clauses of one word.
fn all<'a, 'n>(clauses: &'a Clauses<'n>, word: &'a str) -> impl Iterator<Item = &'n Node> + 'a {
    clauses
        .iter()
        .filter(move |(w, _)| w == word)
        .map(|(_, n)| *n)
}

impl Checker<'_> {
    fn sql_alias_of(&mut self, cx: &mut Cx, clause: &Node) -> Option<Alias> {
        let name = clause.child_tokens().skip(1).find(|t| t.kind == K::Ident)?;
        let table = clause.nodes().next()?;
        let mut alias = self.sql_table_alias(cx, name.text(self.text), table)?;
        alias.nullable = clause
            .child_tokens()
            .next()
            .is_some_and(|t| t.text(self.text) == "left_join");
        self.note_type(
            Span::new(self.file, name.start, name.end),
            &alias.row_type(),
        );
        Some(alias)
    }

    /// `WHERE`-like conditions: a `Bool` without aggregates.
    fn sql_condition(&mut self, cx: &mut Cx, scope: &Scope, node: &Node, place: Place) -> Val {
        let v = self.sql_value(cx, scope, node, place);
        self.expect(&v.ty, &Type::lang(Lang::Bool), self.span(node));
        v
    }

    fn sql_misplaced(&mut self, clauses: &Clauses, allowed: &[&str], what: &str) {
        for (word, node) in clauses.iter().skip(1) {
            if !allowed.contains(&word.as_str()) {
                self.error(
                    codes::Q010,
                    self.span(node),
                    format!("`{word}` has no place in {what}"),
                );
            }
        }
    }

    pub(super) fn sql_select(&mut self, cx: &mut Cx, clauses: &Clauses) -> Option<Query> {
        self.sql_misplaced(
            clauses,
            &[
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
            ],
            "a `from` query",
        );
        let (scope, mut sql) = self.sql_from(cx, clauses)?;
        let conditions: Vec<String> = all(clauses, "where")
            .flat_map(Node::nodes)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| self.sql_condition(cx, &scope, n, Place::Row).sql)
            .collect();
        if !conditions.is_empty() {
            let _ = write!(sql, " WHERE {}", conditions.join(" AND "));
        }
        let groups: Vec<Val> = all(clauses, "group_by")
            .flat_map(Node::nodes)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| self.sql_value(cx, &scope, n, Place::Row))
            .collect();
        if !groups.is_empty() {
            let keys: Vec<&str> = groups.iter().map(|g| g.sql.as_str()).collect();
            let _ = write!(sql, " GROUP BY {}", keys.join(", "));
        }
        let selects: Vec<&Node> = all(clauses, "select").collect();
        let [select] = selects[..] else {
            let at = selects.get(1).copied().unwrap_or(clauses[0].1);
            self.error(
                codes::Q010,
                self.span(at),
                "a query has exactly one `select`",
            );
            return None;
        };
        let items = self.sql_projection(cx, &scope, select, Place::Grouped);
        let having: Vec<(Val, Span)> = all(clauses, "having")
            .flat_map(Node::nodes)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| {
                (
                    self.sql_condition(cx, &scope, n, Place::Grouped),
                    self.span(n),
                )
            })
            .collect();
        let order: Vec<(Val, &str, Span)> = all(clauses, "order_by")
            .flat_map(Node::nodes)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| self.sql_order(cx, &scope, n))
            .collect();
        let grouped = !groups.is_empty() || !having.is_empty() || items.iter().any(|i| i.2.agg);
        if grouped {
            let checked = items
                .iter()
                .map(|(_, _, v, s)| (v, *s))
                .chain(having.iter().map(|(v, s)| (v, *s)))
                .chain(order.iter().map(|(v, _, s)| (v, *s)));
            for (v, span) in checked.collect::<Vec<_>>() {
                self.sql_grouped(v, &groups, span);
            }
        }
        let projection: Vec<&str> = items.iter().map(|i| i.1.as_str()).collect();
        let mut text = format!("SELECT {}{sql}", projection.join(", "));
        if !having.is_empty() {
            let conds: Vec<&str> = having.iter().map(|h| h.0.sql.as_str()).collect();
            let _ = write!(text, " HAVING {}", conds.join(" AND "));
        }
        if !order.is_empty() {
            let keys: Vec<String> = order
                .iter()
                .map(|(v, d, _)| format!("{}{d}", v.sql))
                .collect();
            let _ = write!(text, " ORDER BY {}", keys.join(", "));
        }
        let columns = items.into_iter().map(|i| i.0).collect();
        Some(self.sql_paging(cx, &scope, clauses, text, columns))
    }

    /// `from` and the joins: the aliases and the `FROM` clause.
    fn sql_from(&mut self, cx: &mut Cx, clauses: &Clauses) -> Option<(Scope, String)> {
        let mut scope = Scope::default();
        let first = self.sql_alias_of(cx, clauses[0].1)?;
        let mut sql = format!(
            " FROM {} AS {}",
            quote(&first.table_sql),
            quote(&first.name)
        );
        scope.aliases.push(first);
        for (i, (word, clause)) in clauses.iter().enumerate().skip(1) {
            match word.as_str() {
                "join" | "left_join" => {
                    let Some(alias) = self.sql_alias_of(cx, clause) else {
                        continue;
                    };
                    if scope.aliases.iter().any(|a| a.name == alias.name) {
                        self.error(
                            codes::Q004,
                            self.span(clause),
                            format!("the query already has a table `{}`", alias.name),
                        );
                        continue;
                    }
                    let keyword = if alias.nullable { "LEFT JOIN" } else { "JOIN" };
                    let _ = write!(
                        sql,
                        " {keyword} {} AS {}",
                        quote(&alias.table_sql),
                        quote(&alias.name)
                    );
                    scope.aliases.push(alias);
                    match clauses.get(i + 1) {
                        Some((on, node)) if on == "on" => {
                            let cond = node.nodes().next()?;
                            let v = self.sql_condition(cx, &scope, cond, Place::Row);
                            let _ = write!(sql, " ON {}", v.sql);
                        }
                        _ => self.error(
                            codes::Q010,
                            self.span(clause),
                            format!("`{word}` needs `on` with the join condition"),
                        ),
                    }
                }
                "on" if !matches!(clauses[i - 1].0.as_str(), "join" | "left_join") => {
                    self.error(
                        codes::Q010,
                        self.span(clause),
                        "`on` belongs right after a `join`",
                    );
                }
                "from" => {
                    self.error(
                        codes::Q010,
                        self.span(clause),
                        "a query has one `from`: add tables with `join`",
                    );
                }
                _ => {}
            }
        }
        Some((scope, sql))
    }

    /// `limit`, `offset`, then the unions.
    fn sql_paging(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        clauses: &Clauses,
        mut text: String,
        mut columns: Vec<Out>,
    ) -> Query {
        for word in ["limit", "offset"] {
            for clause in all(clauses, word) {
                let Some(n) = clause.nodes().next() else {
                    continue;
                };
                let v = self.sql_value(cx, scope, n, Place::Row);
                self.expect(&v.ty, &Type::lang(Lang::Int), self.span(n));
                let _ = write!(text, " {} {}", word.to_uppercase(), v.sql);
            }
        }
        for (word, clause) in clauses.iter().filter(|(w, _)| w.starts_with("union")) {
            let Some(sub) = clause.nodes().next() else {
                continue;
            };
            let Some(right) = self.sql_subquery(cx, sub) else {
                continue;
            };
            self.sql_union(&mut columns, &right.columns, self.span(sub));
            let all = if word == "union_all" { " ALL" } else { "" };
            text = format!("({text}) UNION{all} ({})", right.sql);
        }
        Query { sql: text, columns }
    }

    /// `order_by e`, `order_by asc e` or `order_by desc e`.
    fn sql_order<'s>(&mut self, cx: &mut Cx, scope: &Scope, node: &Node) -> (Val, &'s str, Span) {
        let node = inner(node);
        let children: Vec<&Node> = node.nodes().collect();
        if node.kind == K::AppExpr && children.len() == 2 {
            let head = self.text_of(children[0]);
            if head == "asc" || head == "desc" {
                let v = self.sql_value(cx, scope, children[1], Place::Grouped);
                let dir = if head == "asc" { " ASC" } else { " DESC" };
                return (v, dir, self.span(node));
            }
        }
        (
            self.sql_value(cx, scope, node, Place::Grouped),
            "",
            self.span(node),
        )
    }

    /// In a grouped query a column outside an aggregate must be grouped.
    fn sql_grouped(&mut self, v: &Val, groups: &[Val], span: Span) {
        if groups.iter().any(|g| g.sql == v.sql) {
            return;
        }
        for (alias, column) in &v.bare {
            let grouped = groups.iter().any(|g| {
                g.bare.len() == 1 && g.bare[0] == (alias.clone(), column.clone()) && !g.agg
            });
            if !grouped {
                self.error(
                    codes::Q005,
                    span,
                    format!("`{alias}.{column}` is neither grouped nor aggregated: add it to `group_by` or aggregate it"),
                );
                return;
            }
        }
    }

    /// Both sides of a union give the same results in the same order.
    fn sql_union(&mut self, left: &mut [Out], right: &[Out], span: Span) {
        let same =
            left.len() == right.len() && left.iter().zip(right).all(|(l, r)| l.label == r.label);
        if !same {
            let names = |cols: &[Out]| {
                cols.iter()
                    .map(|c| c.label.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            self.error(
                codes::Q010,
                span,
                format!(
                    "both sides of a union select the same results in the same order: {} and {}",
                    names(left),
                    names(right)
                ),
            );
            return;
        }
        for (l, r) in left.iter_mut().zip(right) {
            self.expect(&r.ty, &l.ty, span);
            l.null |= r.null;
        }
    }

    /// `insert_into T values { … }`, `update T set { … } where …` and
    /// `delete_from T where …`, each with an optional `returning`.
    pub(super) fn sql_write(&mut self, cx: &mut Cx, clauses: &Clauses) -> Option<Query> {
        let (word, head) = (clauses[0].0.as_str(), clauses[0].1);
        let allowed: &[&str] = match word {
            "insert_into" => &["values", "returning"],
            "update" => &["set", "where", "returning"],
            _ => &["where", "returning"],
        };
        self.sql_misplaced(clauses, allowed, &format!("`{word}`"));
        let table = head.nodes().next()?;
        let name = table
            .child_tokens()
            .filter(|t| t.kind == K::Con)
            .last()
            .map_or_else(|| "t".to_owned(), |t| t.text(self.text).to_owned());
        let alias = self.sql_table_alias(cx, &name, table)?;
        let target = format!("{} AS {}", quote(&alias.table_sql), quote(&alias.name));
        let scope = Scope {
            aliases: vec![alias],
        };
        let mut text = match word {
            "insert_into" => {
                let values = all(clauses, "values").next();
                let Some(values) = values else {
                    self.error(
                        codes::Q010,
                        self.span(head),
                        "`insert_into` needs `values { … }`",
                    );
                    return None;
                };
                let given = self.sql_assignments(cx, &scope, values);
                self.sql_required(&scope.aliases[0], &given, self.span(values));
                let names: Vec<String> = given.iter().map(|(c, _)| quote(c)).collect();
                let vals: Vec<&str> = given.iter().map(|(_, v)| v.as_str()).collect();
                format!(
                    "INSERT INTO {target} ({}) VALUES ({})",
                    names.join(", "),
                    vals.join(", ")
                )
            }
            "update" => {
                let Some(set) = all(clauses, "set").next() else {
                    self.error(codes::Q010, self.span(head), "`update` needs `set { … }`");
                    return None;
                };
                let given = self.sql_assignments(cx, &scope, set);
                let sets: Vec<String> = given
                    .iter()
                    .map(|(c, v)| format!("{} = {v}", quote(c)))
                    .collect();
                format!("UPDATE {target} SET {}", sets.join(", "))
            }
            _ => format!("DELETE FROM {target}"),
        };
        let conditions: Vec<String> = all(clauses, "where")
            .flat_map(Node::nodes)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|n| self.sql_condition(cx, &scope, n, Place::Row).sql)
            .collect();
        if !conditions.is_empty() {
            let _ = write!(text, " WHERE {}", conditions.join(" AND "));
        }
        let mut columns = Vec::new();
        if let Some(returning) = all(clauses, "returning").next() {
            let items = self.sql_projection(cx, &scope, returning, Place::Row);
            let list: Vec<&str> = items.iter().map(|i| i.1.as_str()).collect();
            let _ = write!(text, " RETURNING {}", list.join(", "));
            columns = items.into_iter().map(|i| i.0).collect();
        }
        Some(Query { sql: text, columns })
    }

    /// The fields of `values { … }` or `set { … }`: each a column the
    /// database does not generate, with a value of its type.
    fn sql_assignments(
        &mut self,
        cx: &mut Cx,
        scope: &Scope,
        clause: &Node,
    ) -> Vec<(String, String)> {
        let alias = &scope.aliases[0];
        let record = clause
            .nodes()
            .next()
            .map(inner)
            .filter(|n| n.kind == K::RecordExpr);
        let Some(record) = record else {
            self.error(
                codes::Q010,
                self.span(clause),
                "expected a record `{ column = value; … }`",
            );
            return Vec::new();
        };
        let mut given: Vec<(String, String)> = Vec::new();
        for field in record.nodes().filter(|n| n.kind == K::RecordField) {
            let label = field.child_tokens().find(|t| t.kind == K::Ident);
            let (Some(label), Some(value)) = (label, field.nodes().next()) else {
                continue;
            };
            let label = label.text(self.text).to_owned();
            let span = self.span(field);
            let Some(col) = alias.columns.iter().find(|c| c.name == label) else {
                self.error(
                    codes::Q001,
                    span,
                    format!("`{}` has no column `{label}`", alias.table_name),
                );
                continue;
            };
            if given.iter().any(|(c, _)| *c == label) {
                self.error(codes::Q004, span, format!("`{label}` is given twice"));
                continue;
            }
            if col.write == "Generated" {
                self.error(
                    codes::Q008,
                    span,
                    format!("the database generates `{label}`: it cannot be written"),
                );
                continue;
            }
            let v = self.sql_value(cx, scope, value, Place::Row);
            self.expect(&v.ty, &col.ty, self.span(value));
            if let Some(i) = v.param {
                cx.params[i].nullable |= col.nullable;
                if cx.params[i].form.is_none() {
                    cx.params[i].form = Some(SqlForm::Column(alias.table, label.clone()));
                }
            }
            given.push((label, v.sql));
        }
        given
    }

    /// Every required column has a value.
    fn sql_required(&mut self, alias: &Alias, given: &[(String, String)], span: Span) {
        let missing: Vec<String> = alias
            .columns
            .iter()
            .filter(|c| c.write == "Required" && !given.iter().any(|(g, _)| *g == c.name))
            .map(|c| format!("`{}`", c.name))
            .collect();
        if !missing.is_empty() {
            self.error(
                codes::Q007,
                span,
                format!(
                    "the insert misses the required column {}",
                    missing.join(", ")
                ),
            );
        }
    }
}
