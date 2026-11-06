//! The editor in a table: its layout, a column a line; what the table, its
//! names, SQL types and attributes mean; and what may come next in a
//! column, a SQL type or an attribute. After `as` and `via` the names in
//! scope are offered, as in the user's code.

use flick_plugin::{Block, Compiler, Completion, Hole, HoleKind, Hover, Piece, Span, Token};

use crate::dialect::{self, ATTRIBUTES, TITLE};
use crate::table::{self, Column, Table};

/// One column a line, one level in; none for a table with a wrong column,
/// which stays as written.
pub fn format(block: &Block, compiler: &mut dyn Compiler) -> Option<Vec<Piece>> {
    let (table, wrong) = table::read(block, Some(compiler));
    let table = table.filter(|_| wrong.is_empty())?;
    let args = block.arguments();
    let text = |t: &str| Piece::Text(t.to_owned());
    let hole = |kind: HoleKind, span: Span| Piece::Hole(Hole { kind, span });
    let sql = block.text_of(args.last()?.span);
    let mut out = vec![
        text(&format!("table {} {sql} {{", table.name)),
        Piece::Indent,
    ];
    for c in &table.columns {
        let attributes = c.attributes.iter().map(|(i, _)| ATTRIBUTES[*i].0);
        let words: Vec<&str> = [c.sql.words].into_iter().chain(attributes).collect();
        out.extend([
            Piece::Line,
            text(&format!("{} : {} as ", c.name, words.join(" "))),
            hole(HoleKind::Ty, c.ty),
        ]);
        if let Some(via) = c.via {
            out.extend([text(" via "), hole(HoleKind::Expr, via)]);
        }
    }
    out.extend([Piece::Dedent, Piece::Line]);
    Some(out)
}

fn md(code: &str, lang: &str, note: &str) -> String {
    format!("```{lang}\n{code}\n```\n\n{note}")
}

/// The type of a column's values, in `Option` when it may be `NULL`.
fn value_type(block: &Block, c: &Column) -> String {
    let ty = block.text_of(c.ty).trim();
    if c.nullable {
        format!("Option {ty}")
    } else {
        ty.to_owned()
    }
}

/// The row record and the companion's type, in the order of the columns.
fn declarations(block: &Block, table: &Table) -> (String, String) {
    let fields: Vec<String> = (table.columns.iter())
        .map(|c| format!("{} : {}", c.name, value_type(block, c)))
        .collect();
    let columns: Vec<String> = (table.columns.iter())
        .map(|c| {
            let null = if c.nullable { "Nullable" } else { "NonNull" };
            let ty = block.text_of(c.ty).trim();
            format!("{} : Sql.Column {ty} Sql.{null} Sql.{}", c.name, c.write)
        })
        .collect();
    let row = format!("type alias {} = {{ {} }}", table.name, fields.join("; "));
    let companion = format!(
        "{}.table : Sql.Table \"{}\" {{ {} }}",
        table.name,
        table.sql,
        columns.join("; ")
    );
    (row, companion)
}

fn covers(span: Span, offset: u32) -> bool {
    span.start <= offset && offset < span.end
}

pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    let (table, _) = table::read(block, None);
    let table = table?;
    let args = block.arguments();
    let (row, companion) = declarations(block, &table);
    let at = |span: Span, markdown: String| Some(Hover { markdown, span });
    if let [word, name, sql] = args {
        if covers(word.span, offset) {
            let doc = format!(
                "Declares the row type `{0}` and the table `{0}.table`, which queries name `{0}`: \
                 `from row in {0}`. Dialect: {TITLE}.",
                table.name
            );
            return at(word.span, md(&format!("{row}\n{companion}"), "flick", &doc));
        }
        if covers(name.span, offset) {
            let doc = format!(
                "A row of the {TITLE} table `{}`; in a query `{}` names the table.",
                table.sql, table.name
            );
            return at(name.span, md(&row, "flick", &doc));
        }
        if covers(sql.span, offset) {
            let doc = format!(
                "The table's name in the database: queries over `{}` read and write `{}`.",
                table.name, table.sql
            );
            return at(sql.span, md(&table.sql, "sql", &doc));
        }
    }
    let c = table.columns.iter().find(|c| covers(c.line, offset))?;
    if covers(c.sql_at, offset) {
        let doc = format!(
            "{TITLE} `{}`: Flick value `{}` (before a `via` codec).",
            c.sql.words, c.sql.flick
        );
        return at(c.sql_at, md(c.sql.words, "sql", &doc));
    }
    if let Some((i, span)) = c.attributes.iter().find(|(_, s)| covers(*s, offset)) {
        let (words, doc, _) = ATTRIBUTES[*i];
        return at(*span, md(words, "sql", doc));
    }
    if covers(c.at, offset) {
        let code = format!("{} : {}", c.name, value_type(block, c));
        return at(c.at, md(&code, "flick", "A column of the table."));
    }
    let shown = block.text_of(c.ty).trim();
    let word = (block.cursor().rest().iter())
        .find(|t| covers(t.span, offset) && covers(c.line, t.span.start))?;
    let doc = match block.text_of(word.span) {
        "as" => format!(
            "The Flick type `{shown}` of the column's values; without `via` it is the type the \
             SQL type gives."
        ),
        "via" => format!(
            "A `PureCodec {} {shown}` between the SQL value and `{shown}`.",
            c.sql.flick
        ),
        _ => return None,
    };
    at(word.span, md(block.text_of(word.span), "flick", &doc))
}

pub fn complete(block: &Block, offset: u32) -> Vec<Completion> {
    let line: Vec<Token> = (block.body().rest().iter())
        .copied()
        .filter(|t| t.span.end <= offset)
        .rev()
        .take_while(|t| {
            let after = &block.text[t.span.end as usize..offset as usize];
            !after.contains('\n') || block.text_of(t.span) == ";"
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let words: Vec<&str> = line.iter().map(|t| block.text_of(t.span)).collect();
    // after `as` and `via`, the names in scope
    if words.iter().any(|w| *w == "as" || *w == "via") {
        return Vec::new();
    }
    let Some(colon) = words.iter().position(|w| *w == ":") else {
        return Vec::new();
    };
    let item = |label: &str, detail: &str| Completion {
        label: label.to_owned(),
        detail: Some(detail.to_owned()),
        documentation: None,
        snippet: None,
    };
    let written = &words[colon + 1..];
    let Some(sql) = dialect::column_type(written) else {
        return (dialect::TYPES.iter())
            .map(|t| item(t.words, t.flick))
            .collect();
    };
    let mut rest = &written[sql.words.split(' ').count()..];
    // the attributes in order; a word that starts none is skipped
    let mut present: Vec<&str> = Vec::new();
    while !rest.is_empty() {
        let found = ATTRIBUTES.iter().find(|(w, ..)| {
            let n = w.split(' ').count();
            rest.len() >= n && rest[..n].join(" ") == *w
        });
        match found {
            Some((w, ..)) => {
                present.push(w);
                rest = &rest[w.split(' ').count()..];
            }
            None => rest = &rest[1..],
        }
    }
    let mut out: Vec<Completion> = (ATTRIBUTES.iter())
        .filter(|(w, _, conflicts)| {
            !present.contains(w) && !conflicts.iter().any(|c| present.contains(c))
        })
        .map(|(w, doc, _)| item(w, doc))
        .collect();
    out.push(item("as", "The Flick type of the column"));
    out
}
