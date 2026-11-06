//! What an editor shows in a query: what a clause word or a function does,
//! and the row of a table, an alias or the type of a column, from the type
//! of `Sql.Derive.columns T.table` the code gives each table it reads.

use flick_plugin::{Block, Compiler, Completion, Fact, Hover, Span};

use crate::shown::Shown;

use super::parse::{self, E, Query, Tok};

/// The clause words, with what each does.
pub const CLAUSES: [(&str, &str); 19] = [
    ("from", "Introduces a table alias: `from alias in Table`."),
    ("join", "Joins a table; followed by `on condition`."),
    (
        "left_join",
        "Left join: the joined columns may be NULL and read as Option values.",
    ),
    (
        "on",
        "The join condition, checked against the aliases introduced so far.",
    ),
    ("where", "Filters rows before grouping."),
    (
        "select",
        "The result row: `select { label = alias.column }`, columns, or a whole alias.",
    ),
    ("group_by", "Groups rows by these expressions."),
    ("having", "Filters groups; aggregate functions are allowed."),
    (
        "order_by",
        "Orders results; use `asc` or `desc` before an expression.",
    ),
    ("limit", "Limits the number of result rows."),
    ("offset", "Skips result rows."),
    (
        "union",
        "Combines queries with the same result columns, removing duplicates.",
    ),
    (
        "union_all",
        "Combines queries with the same result columns, keeping duplicates.",
    ),
    (
        "insert_into",
        "Inserts into a table with `values { column = value }`.",
    ),
    ("values", "The column values to insert."),
    ("update", "Updates a table with `set { column = value }`."),
    ("set", "The column values to update."),
    ("delete_from", "Deletes rows from a table."),
    (
        "returning",
        "The result row of an insert, update or delete.",
    ),
];

/// The functions a query may call, with what each does.
pub const FUNCTIONS: [(&str, &str); 12] = [
    ("is_null", "Tests whether a SQL value is NULL."),
    ("is_not_null", "Tests whether a SQL value is not NULL."),
    (
        "is_not_distinct_from",
        "Null-safe equality of two SQL values.",
    ),
    ("coalesce", "The first non-NULL value."),
    ("count", "Counts non-NULL values in a group."),
    ("count_all", "Counts all rows in a group; result: BigInt."),
    ("sum", "The sum of a group's values."),
    ("avg", "The average of a group's values."),
    ("min", "The minimum of a group's values."),
    ("max", "The maximum of a group's values."),
    ("exists", "Tests whether a subquery returns any rows."),
    ("is_in", "Tests membership in a single-column subquery."),
];

fn md(code: &str, lang: &str, note: &str) -> String {
    if note.is_empty() {
        format!("```{lang}\n{code}\n```")
    } else {
        format!("```{lang}\n{code}\n```\n\n{note}")
    }
}

/// An alias of the block: its name, the span of its table and whether a
/// left join makes it nullable.
struct Named<'q> {
    alias: &'q Tok,
    table: Span,
    nullable: bool,
}

fn parsed(block: &Block) -> Option<Query> {
    let toks = parse::tokens(block, block.body().rest());
    parse::query(None, &toks, block.whole()).ok()
}

/// Every alias of `q` and its subqueries, and every table it names.
fn aliases<'q>(q: &'q Query, out: &mut Vec<Named<'q>>, tables: &mut Vec<Span>) {
    for c in &q.clauses {
        let writes = matches!(
            c.word.text.as_str(),
            "insert_into" | "update" | "delete_from"
        );
        if let (Some(alias), Some(table)) = (&c.alias, c.items.first()) {
            out.push(Named {
                alias,
                table: table.span(),
                nullable: c.word.text == "left_join",
            });
            tables.push(table.span());
        } else if writes && let Some(E::Path(words, span)) = c.items.first() {
            if let Some(last) = words.last() {
                out.push(Named {
                    alias: last,
                    table: *span,
                    nullable: false,
                });
            }
            tables.push(*span);
        }
        for e in &c.items {
            subqueries(e, out, tables);
        }
    }
}

fn subqueries<'q>(e: &'q E, out: &mut Vec<Named<'q>>, tables: &mut Vec<Span>) {
    match e {
        E::Sub(q) => aliases(q, out, tables),
        E::Paren(inner, _) | E::Not(inner, _) | E::Neg(inner, _) => subqueries(inner, out, tables),
        E::Binary(_, l, r, _) => {
            subqueries(l, out, tables);
            subqueries(r, out, tables);
        }
        E::Apply(_, args, _) => args.iter().for_each(|a| subqueries(a, out, tables)),
        E::Record(fields, _) => fields.iter().for_each(|(_, v)| subqueries(v, out, tables)),
        E::Path(..) | E::Capture { .. } | E::Literal(_) => {}
    }
}

/// The paths of `q`, with the clause word they stand in.
fn paths<'q>(q: &'q Query, out: &mut Vec<&'q [Tok]>) {
    fn walk<'q>(e: &'q E, out: &mut Vec<&'q [Tok]>) {
        match e {
            E::Path(words, _) => out.push(words),
            E::Sub(q) => paths(q, out),
            E::Paren(inner, _) | E::Not(inner, _) | E::Neg(inner, _) => walk(inner, out),
            E::Binary(_, l, r, _) => {
                walk(l, out);
                walk(r, out);
            }
            E::Apply(_, args, _) => args.iter().for_each(|a| walk(a, out)),
            E::Record(fields, _) => fields.iter().for_each(|(_, v)| walk(v, out)),
            E::Capture { .. } | E::Literal(_) => {}
        }
    }
    for c in &q.clauses {
        c.items.iter().for_each(|e| walk(e, out));
    }
}

/// The columns of the table at `span`, from the fact of its columns: each
/// label and the type of its values.
fn columns(facts: &[Fact], span: Span) -> Option<Vec<(String, String)>> {
    let fact = facts.iter().find(|f| f.span == span)?;
    let fields = fact.fields.as_ref()?;
    Some(
        (fields.iter())
            .map(|f| (f.label.clone(), value_type(&f.ty.shown)))
            .collect(),
    )
}

/// `T` of `Column T NonNull w`, `Option T` of `Column T Nullable w`.
fn value_type(column: &str) -> String {
    let inner = column.strip_prefix("Column ").unwrap_or(column);
    let words: Vec<&str> = inner.rsplitn(3, ' ').collect();
    let [_, null, ty] = words[..] else {
        return column.to_owned();
    };
    if null == "Nullable" {
        option(ty)
    } else {
        ty.to_owned()
    }
}

fn option(ty: &str) -> String {
    if ty.contains(' ') && !ty.starts_with('(') {
        format!("Option ({ty})")
    } else {
        format!("Option {ty}")
    }
}

fn row(columns: &[(String, String)], nullable: bool) -> String {
    let fields: Vec<String> = (columns.iter())
        .map(|(l, t)| {
            let t = if nullable && !t.starts_with("Option ") {
                option(t)
            } else {
                t.clone()
            };
            format!("{l} : {t}")
        })
        .collect();
    format!("{{ {} }}", fields.join("; "))
}

/// What the editor shows at `offset` of the query `block`.
pub fn hover(block: &Block, offset: u32, facts: &[Fact]) -> Option<Hover> {
    let q = parsed(block)?;
    let toks = parse::tokens(block, block.body().rest());
    let t = toks
        .iter()
        .find(|t| t.span.start <= offset && offset < t.span.end)?;
    let at = |markdown: String| {
        Some(Hover {
            markdown,
            span: t.span,
        })
    };
    if let Some((_, doc)) = CLAUSES.iter().find(|(w, _)| *w == t.text) {
        return at(md(&t.text, "sql", doc));
    }
    if let Some((_, doc)) = FUNCTIONS.iter().find(|(w, _)| *w == t.text) {
        return at(md(&t.text, "sql", doc));
    }
    let mut named = Vec::new();
    let mut tables = Vec::new();
    aliases(&q, &mut named, &mut tables);
    if let Some(table) = tables.iter().find(|s| s.start <= offset && offset < s.end) {
        let cols = columns(facts, *table)?;
        return at(md(
            &format!("{} : {}", block.text_of(*table), row(&cols, false)),
            "flick",
            "",
        ));
    }
    let alias_of = |name: &str| {
        (named.iter())
            .rev()
            .find(|n| n.alias.text == name && n.alias.span.start <= offset)
    };
    if let Some(n) = named.iter().find(|n| n.alias.span == t.span) {
        let cols = columns(facts, n.table)?;
        return at(md(
            &format!("{} : {}", t.text, row(&cols, n.nullable)),
            "flick",
            "",
        ));
    }
    let mut all = Vec::new();
    paths(&q, &mut all);
    let words = all.iter().find(|w| w.iter().any(|x| x.span == t.span))?;
    match &words[..] {
        [binder, label]
            if block.arguments().first().map(|a| block.text_of(a.span))
                == Some(binder.text.as_str()) =>
        {
            let span = Span {
                start: binder.span.start,
                end: label.span.end,
            };
            let fact = facts.iter().find(|f| f.span == span)?;
            at(md(
                &format!("{}.{} : {}", binder.text, label.text, fact.ty.shown),
                "flick",
                "",
            ))
        }
        [alias, column] if column.span == t.span || alias.span == t.span => {
            let n = alias_of(&alias.text)?;
            let cols = columns(facts, n.table)?;
            if alias.span == t.span {
                return at(md(
                    &format!("{} : {}", alias.text, row(&cols, n.nullable)),
                    "flick",
                    "",
                ));
            }
            let (_, ty) = cols.iter().find(|(l, _)| *l == column.text)?;
            let ty = if n.nullable && !ty.starts_with("Option ") {
                option(ty)
            } else {
                ty.clone()
            };
            at(md(
                &format!("{}.{} : {ty}", alias.text, column.text),
                "flick",
                "",
            ))
        }
        _ => None,
    }
}

/// What an editor may insert in a query: after `alias.` the columns of
/// its table, after `params.` the parameters the signature declares,
/// where a table goes the tables the module sees; elsewhere the aliases,
/// the clause words and the functions. In a capture the usual completion
/// offers the names of the program.
pub fn complete(
    block: &Block,
    offset: u32,
    member: bool,
    compiler: &mut dyn Compiler,
) -> Vec<Completion> {
    let item = |label: &str, detail: Option<&str>| Completion {
        label: label.to_owned(),
        detail: detail.map(str::to_owned),
        documentation: None,
        snippet: None,
    };
    let toks = parse::tokens(block, block.body().rest());
    let before = toks.iter().rev().find(|t| t.span.end <= offset);
    // a query being written does not parse yet: its aliases are its words
    if parsed(block).is_some_and(|q| in_capture(&q, offset))
        || before.is_some_and(|t| t.text == "$")
    {
        return Vec::new();
    }
    let named = written_aliases(&toks);
    if member {
        let Some(word) = before.filter(|t| t.span.end == offset) else {
            return Vec::new();
        };
        let binder = block.arguments().first().map(|t| block.text_of(t.span));
        if binder == Some(word.text.as_str()) {
            let declared = super::declared_types(block).unwrap_or_default();
            return declared.iter().map(|(l, t)| item(l, Some(t))).collect();
        }
        let Some((_, table, nullable)) = named.iter().rev().find(|(a, _, _)| *a == word.text)
        else {
            return Vec::new();
        };
        let cols = published(table, compiler).unwrap_or_default();
        return (cols.iter())
            .map(|(l, t)| {
                let t = if *nullable && !t.starts_with("Option ") {
                    option(t)
                } else {
                    t.clone()
                };
                item(l, Some(&t))
            })
            .collect();
    }
    // where a table goes, the tables the module sees
    let table = matches!(
        before.map(|t| t.text.as_str()),
        Some("in" | "insert_into" | "update" | "delete_from")
    );
    if table {
        return (compiler.published_names().iter())
            .map(|n| item(n, Some("sql table")))
            .collect();
    }
    let mut out: Vec<Completion> = Vec::new();
    for (alias, table, nullable) in &named {
        if !out.iter().any(|c| c.label == *alias) {
            let detail = published(table, compiler).map(|cols| row(&cols, *nullable));
            out.push(item(alias, detail.as_deref()));
        }
    }
    out.extend(CLAUSES.iter().map(|(w, d)| item(w, Some(d))));
    out.extend(FUNCTIONS.iter().map(|(w, d)| item(w, Some(d))));
    out
}

/// The aliases the words of a query introduce: `from x in T`, `join x in
/// T`, `left_join x in T`, and the table of a statement, each with the
/// table as written and whether a left join makes it nullable.
fn written_aliases(toks: &[Tok]) -> Vec<(String, String, bool)> {
    let path_at = |i: usize| {
        let mut words = Vec::new();
        let mut j = i;
        while let Some(t) = toks.get(j) {
            if !matches!(
                t.kind,
                flick_plugin::TokenKind::Ident | flick_plugin::TokenKind::Con
            ) {
                break;
            }
            words.push(t.text.as_str());
            if toks.get(j + 1).is_some_and(|d| d.text == ".") {
                j += 2;
            } else {
                break;
            }
        }
        words
    };
    let mut out = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        match t.text.as_str() {
            "from" | "join" | "left_join" => {
                let alias = toks
                    .get(i + 1)
                    .filter(|a| a.kind == flick_plugin::TokenKind::Ident);
                let inn = toks.get(i + 2).is_some_and(|n| n.text == "in");
                if let (Some(alias), true) = (alias, inn) {
                    let table = path_at(i + 3);
                    if !table.is_empty() {
                        out.push((alias.text.clone(), table.join("."), t.text == "left_join"));
                    }
                }
            }
            "insert_into" | "update" | "delete_from" => {
                let table = path_at(i + 1);
                if let Some(last) = table.last() {
                    out.push(((*last).to_owned(), table.join("."), false));
                }
            }
            _ => {}
        }
    }
    out
}

/// The columns of `table`, as its plugin published them.
fn published(table: &str, compiler: &mut dyn Compiler) -> Option<Vec<(String, String)>> {
    let data = compiler.published(table)?;
    let shown: Shown = serde_json::from_slice(&data).ok()?;
    Some(
        (shown.columns.iter())
            .map(|c| {
                let ty = if c.nullable {
                    option(&c.ty)
                } else {
                    c.ty.clone()
                };
                (c.name.clone(), ty)
            })
            .collect(),
    )
}

/// Whether `offset` is in the code of a capture of `q`.
fn in_capture(q: &Query, offset: u32) -> bool {
    fn walk(e: &E, offset: u32) -> bool {
        match e {
            E::Capture { inner, .. } => inner.start <= offset && offset <= inner.end,
            E::Sub(q) => in_capture(q, offset),
            E::Paren(inner, _) | E::Not(inner, _) | E::Neg(inner, _) => walk(inner, offset),
            E::Binary(_, l, r, _) => walk(l, offset) || walk(r, offset),
            E::Apply(_, args, _) => args.iter().any(|a| walk(a, offset)),
            E::Record(fields, _) => fields.iter().any(|(_, v)| walk(v, offset)),
            E::Path(..) | E::Literal(_) => false,
        }
    }
    (q.clauses.iter()).any(|c| c.items.iter().any(|e| walk(e, offset)))
}

#[cfg(test)]
mod tests {
    use super::value_type;

    #[test]
    fn a_column_reads_as_its_value_maybe_null() {
        assert_eq!(value_type("Column String NonNull Required"), "String");
        assert_eq!(
            value_type("Column UserId Nullable Defaulted"),
            "Option UserId"
        );
        assert_eq!(
            value_type("Column (List Int) Nullable Defaulted"),
            "Option (List Int)"
        );
    }
}
