//! `sql table Name "table" { column : sql type words as T [via codec] }`:
//! the row type `Name` and the companion `Name.table`, a
//! `Sql.Table "table" { column : Sql.Column T n w, … }` whose indices say
//! whether the column may be `NULL` and what an `INSERT` must give it. Each
//! column holds the values of its SQL type, or what its `via` codec makes
//! of them; an enum of that type's values needs no codec.

use flick_plugin::{Block, Compiler, Diagnostic, Span, Token, TokenKind};

use crate::dialect::{self, ATTRIBUTES, ColumnType};

/// A table the block describes.
pub struct Table {
    /// The name of its row type and of the table in queries.
    pub name: String,
    /// The table's name in the database.
    pub sql: String,
    pub columns: Vec<Column>,
}

/// A column of a table.
pub struct Column {
    pub name: String,
    /// Its name in the block.
    pub at: Span,
    /// Its SQL type and the span of its words.
    pub sql: &'static ColumnType,
    pub sql_at: Span,
    /// Its attributes, by their index in [`ATTRIBUTES`], and their spans.
    pub attributes: Vec<(usize, Span)>,
    pub nullable: bool,
    /// `Required`, `Defaulted` or `Generated`.
    pub write: &'static str,
    /// The user's type after `as`.
    pub ty: Span,
    /// The user's codec after `via`.
    pub via: Option<Span>,
    /// The whole line.
    pub line: Span,
}

/// The code a problem of the block is reported with.
pub const DUPLICATE: &str = "Q001";
pub const MALFORMED: &str = "Q010";

/// The table of `block` and what is wrong in it: a column that is wrong is
/// left out, so the table is still declared.
/// Without `compiler`, the user's types and codecs are their tokens.
pub fn read(
    block: &Block,
    mut compiler: Option<&mut dyn Compiler>,
) -> (Option<Table>, Vec<Diagnostic>) {
    let mut wrong = Vec::new();
    let args = block.arguments();
    let text = |t: &Token| block.text_of(t.span);
    let (name, sql) = match args {
        [table, name, sql]
            if text(table) == "table"
                && name.kind == TokenKind::Con
                && sql.kind == TokenKind::String =>
        {
            (text(name).to_owned(), flick_plugin::string_value(text(sql)))
        }
        _ => {
            let message = "a table is `sql table Name \"table\" { column : type as T }`";
            wrong.push(Diagnostic::error(message, block.whole()).with_code(MALFORMED));
            return (None, wrong);
        }
    };
    let mut columns: Vec<Column> = Vec::new();
    for line in lines(block) {
        match column(block, &line, &mut compiler) {
            Ok(c) if columns.iter().any(|o| o.name == c.name) => {
                let message = format!("the table has two columns `{}`", c.name);
                wrong.push(Diagnostic::error(message, c.line).with_code(DUPLICATE));
            }
            Ok(c) => columns.push(c),
            Err(d) => wrong.push(d),
        }
    }
    (Some(Table { name, sql, columns }), wrong)
}

/// The tokens of the body, a column a line; `;` ends one too.
fn lines(block: &Block) -> Vec<Vec<Token>> {
    let mut out: Vec<Vec<Token>> = Vec::new();
    let mut previous: Option<Token> = None;
    for &t in block.body().rest() {
        let breaks = previous.is_none_or(|p| {
            block.text_of(p.span) == ";"
                || block.text[p.span.end as usize..t.span.start as usize].contains('\n')
        });
        if breaks {
            out.push(Vec::new());
        }
        if block.text_of(t.span) != ";" {
            out.last_mut().expect("a line").push(t);
        }
        previous = Some(t);
    }
    out.retain(|l| !l.is_empty());
    out
}

fn column(
    block: &Block,
    line: &[Token],
    compiler: &mut Option<&mut dyn Compiler>,
) -> Result<Column, Diagnostic> {
    let text = |t: &Token| block.text_of(t.span);
    let whole = Span {
        start: line[0].span.start,
        end: line[line.len() - 1].span.end,
    };
    let malformed = |message: String, at: Span| Diagnostic::error(message, at).with_code(MALFORMED);
    let [name, colon, rest @ ..] = line else {
        return Err(malformed(
            "expected a column `name : type as T`".into(),
            whole,
        ));
    };
    if name.kind != TokenKind::Ident || text(colon) != ":" {
        return Err(malformed(
            "expected a column `name : type as T`".into(),
            whole,
        ));
    }
    let Some(as_at) = rest.iter().position(|t| text(t) == "as") else {
        return Err(malformed(
            "a column names its Flick type: `as T`".into(),
            whole,
        ));
    };
    let (words, after) = (&rest[..as_at], &rest[as_at + 1..]);
    let texts: Vec<&str> = words.iter().map(text).collect();
    let words_at = match (words.first(), words.last()) {
        (Some(a), Some(b)) => Span {
            start: a.span.start,
            end: b.span.end,
        },
        _ => whole,
    };
    let Some(sql) = dialect::column_type(&texts) else {
        let known: Vec<&str> = dialect::TYPES.iter().map(|t| t.words).collect();
        let message = format!(
            "unknown SQL type `{}`; known: {}",
            texts.first().unwrap_or(&""),
            known.join(", ")
        );
        return Err(malformed(message, words_at));
    };
    let n = sql.words.split(' ').count();
    let sql_at = Span {
        start: words[0].span.start,
        end: words[n - 1].span.end,
    };
    let (attributes, nullable, write) = attributes(&texts, &words[n..], sql)
        .map_err(|word| malformed(format!("unknown column attribute `{word}`"), words_at))?;
    let via_at = after.iter().position(|t| text(t) == "via");
    let ty_tokens = &after[..via_at.unwrap_or(after.len())];
    let (Some(first), Some(last)) = (ty_tokens.first(), ty_tokens.last()) else {
        return Err(malformed(
            "expected the Flick type after `as`".into(),
            whole,
        ));
    };
    let tokens = Span {
        start: first.span.start,
        end: last.span.end,
    };
    let ty = match compiler {
        Some(c) => c.parse_type(tokens.start, tokens.end)?.span,
        None => tokens,
    };
    let via = match via_at.map(|i| &after[i + 1..]) {
        Some([]) => return Err(malformed("expected a codec after `via`".into(), whole)),
        Some(codec) => {
            let tokens = Span {
                start: codec[0].span.start,
                end: codec[codec.len() - 1].span.end,
            };
            Some(match compiler {
                Some(c) => c.parse_expr(tokens.start, tokens.end)?.span,
                None => tokens,
            })
        }
        None => None,
    };
    Ok(Column {
        name: text(name).to_owned(),
        at: name.span,
        sql,
        sql_at,
        attributes,
        nullable,
        write,
        ty,
        via,
        line: whole,
    })
}

/// The attributes of a column, by their index in [`ATTRIBUTES`] with their
/// spans, whether it may be `NULL` and what an insert gives it.
type Attributes = (Vec<(usize, Span)>, bool, &'static str);

/// The attributes after a column's SQL type; the first unknown word else.
fn attributes<'t>(
    texts: &[&'t str],
    words: &[Token],
    sql: &ColumnType,
) -> Result<Attributes, &'t str> {
    let offset = texts.len() - words.len();
    let mut at = 0;
    let mut found = Vec::new();
    let mut nullable = true;
    let mut write = if sql.words.ends_with("serial") {
        "Defaulted"
    } else {
        "Required"
    };
    while at < words.len() {
        let rest = &texts[offset + at..];
        let Some(i) = ATTRIBUTES.iter().position(|(w, ..)| {
            let n = w.split(' ').count();
            rest.len() >= n && rest[..n].join(" ") == *w
        }) else {
            return Err(rest[0]);
        };
        let n = ATTRIBUTES[i].0.split(' ').count();
        match ATTRIBUTES[i].0 {
            "not null" | "primary key" => nullable = false,
            "default" | "generated by default" => write = "Defaulted",
            "generated always" => write = "Generated",
            _ => {}
        }
        let span = Span {
            start: words[at].span.start,
            end: words[at + n - 1].span.end,
        };
        found.push((i, span));
        at += n;
    }
    if nullable && write == "Required" {
        write = "Defaulted";
    }
    Ok((found, nullable, write))
}
