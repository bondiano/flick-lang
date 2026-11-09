//! `sql [params] { clauses }`: a query or statement over aliases of tables,
//! checked clause by clause, its SQL text and how each parameter and result
//! crosses the driver written as a `Sql.Template Pg p r`. What only the
//! types say is the checker's: the code holds a function from the
//! parameters to a row, never called, whose terms state the rules of SQL in
//! Flick (`Sql.Derive.equal`, `value`, `param`, …), and the text takes the
//! casts and kinds a value's type decides from the classes `Sql.Param` and
//! `Sql.Numeric`, which a release build folds into one literal.
//!
//! A `LEFT JOIN` makes its alias nullable, so its columns read as `Option`
//! once, however many joins make them nullable.

mod code;
mod expr;
mod format;
mod hover;
pub mod parse;
mod select;
mod text;
mod write;

use flick_plugin::{Arena, Block, Compiler, Diagnostic, Fact, Place, Span, TokenKind};

use crate::shown::{Shown, ShownColumn};
use crate::table::MALFORMED;
pub use format::format;
pub use hover::{complete, hover};
use parse::{E, Op, Tok};
pub use text::{Piece, Sql};

/// A column given twice in a statement, or a name no table of the query has.
pub const NO_COLUMN: &str = "Q001";
pub const NO_TABLE: &str = "Q002";
pub const AMBIGUOUS: &str = "Q003";
pub const TWICE: &str = "Q004";
pub const UNGROUPED: &str = "Q005";
pub const MISPLACED: &str = "Q006";
pub const MISSING: &str = "Q007";
pub const GENERATED: &str = "Q008";
pub const UNDECLARED: &str = "Q009";

/// Whether `block` is a query: an expression.
pub fn is_query(block: &Block) -> bool {
    block.place == Place::Expression
}

/// A table of the query, as the user names it, and its columns.
pub struct Table {
    pub written: String,
    pub at: Span,
    pub shown: Shown,
}

/// An alias of a table in the scope of a query.
#[derive(Debug, Clone)]
pub struct Alias {
    pub name: String,
    /// How diagnostics name the table.
    pub table_name: String,
    /// The table, by its index in the query's tables, and the variable of
    /// the code that holds its columns.
    pub table: usize,
    pub var: usize,
    /// Joined with `left_join`: every column may be `NULL`.
    pub nullable: bool,
}

/// The aliases a query sees.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub aliases: Vec<Alias>,
}

/// How a value crosses the driver, when a column decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub table: usize,
    pub column: String,
    /// Whether the column holds another type than its SQL type's.
    pub via: bool,
}

/// What the plugin knows of the type of a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Known {
    /// The runtime kind of a value of a SQL type, without a codec, and the
    /// Flick type of the SQL type.
    Kind(String, String),
    /// A column with a codec, or of an enum.
    Via,
    Unknown,
}

impl Known {
    /// The runtime kind, when the plugin knows it.
    pub fn kind(&self) -> Option<&str> {
        match self {
            Known::Kind(kind, _) => Some(kind),
            _ => None,
        }
    }
}

/// A placeholder: a parameter, a capture or a literal, by index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    Param(usize),
    Capture(usize),
    Literal(usize),
}

/// A named parameter of the query.
pub struct Param {
    pub name: String,
    pub span: Span,
    pub slot: usize,
    pub nullable: bool,
    pub form: Option<Form>,
    pub proxy: usize,
}

/// A value of the program, `$name`, `$name.field` or `$(e)`, passed beside
/// the text: its expression and where its `$` is.
pub struct Capture {
    pub text: String,
    /// The whole capture, `$` with it.
    pub span: Span,
    pub inner: Span,
    pub slot: usize,
    pub form: Option<Form>,
    pub proxy: usize,
    /// A `Sql.Template` of the program, spliced into the text in its place.
    pub template: bool,
}

/// A literal passed beside the text.
pub struct Literal {
    pub at: Span,
    pub slot: usize,
    pub proxy: usize,
}

/// The term of a value in the function of the query's types.
#[derive(Debug, Clone)]
pub enum W {
    /// `Sql.Derive.value a.column` of the alias with that variable.
    Column(usize, String),
    Param(usize),
    Capture(usize),
    Literal(usize),
    Boolean(bool),
    /// A function of `Sql.Derive` applied.
    Call(&'static str, Vec<W>),
    /// `Sql.Derive.sum` or `avg` of a proxy.
    Num(&'static str, usize, Box<W>),
    /// `(w : _)`, its type a fact `check` reads at the span.
    Fact(Box<W>, Span),
    /// A term at the span of the user's expression it stands for.
    At(Box<W>, Span),
    /// The variable of a proxy.
    Proxy(usize),
    /// A variable of the code.
    Raw(String),
    Unit,
    Error,
}

/// An elaborated SQL expression.
#[derive(Debug, Clone)]
pub struct Val {
    pub sql: Sql,
    pub w: W,
    /// Whether it may be `NULL`.
    pub null: bool,
    /// Whether it contains an aggregate.
    pub agg: bool,
    /// Columns used outside any aggregate, as `(alias, column)`.
    pub bare: Vec<(String, String)>,
    pub form: Option<Form>,
    /// The placeholder it is.
    pub at: Option<At>,
    pub known: Known,
}

impl Val {
    pub fn plain(sql: Sql, w: W) -> Val {
        Val {
            sql,
            w,
            null: false,
            agg: false,
            bare: Vec::new(),
            form: None,
            at: None,
            known: Known::Unknown,
        }
    }

    pub fn error() -> Val {
        Val::plain(Sql::default(), W::Error)
    }
}

/// A result of a query.
#[derive(Debug, Clone)]
pub struct Out {
    pub label: String,
    pub w: W,
    pub null: bool,
    pub form: Option<Form>,
    pub known: Known,
}

/// A checked query: its SQL and its results.
pub struct Query {
    pub sql: Sql,
    pub columns: Vec<Out>,
}

/// Where an expression stands: aggregates belong to `select`, `having`
/// and `order_by` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Row,
    Grouped,
}

/// What a query and its subqueries share: the parameters and placeholders,
/// the tables and aliases, the terms that must be `Bool`, what is wrong.
pub struct Cx<'b> {
    pub block: &'b Block,
    pub compiler: &'b mut dyn Compiler,
    pub binder: Option<String>,
    pub tables: Vec<Table>,
    /// The table of each alias variable.
    pub vars: Vec<usize>,
    pub params: Vec<Param>,
    pub captures: Vec<Capture>,
    pub literals: Vec<Literal>,
    pub slots: Vec<At>,
    pub proxies: usize,
    pub checks: Vec<W>,
    pub wrong: Vec<Diagnostic>,
}

impl Cx<'_> {
    pub fn error(&mut self, code: &str, span: Span, message: impl Into<String>) {
        self.wrong
            .push(Diagnostic::error(message, span).with_code(code));
    }

    fn proxy(&mut self) -> usize {
        self.proxies += 1;
        self.proxies - 1
    }

    /// The named parameter `name`, added at its first use.
    pub fn param(&mut self, name: &str, span: Span) -> usize {
        if let Some(i) = self.params.iter().position(|p| p.name == name) {
            return i;
        }
        let proxy = self.proxy();
        self.params.push(Param {
            name: name.to_owned(),
            span,
            slot: self.slots.len(),
            nullable: false,
            form: None,
            proxy,
        });
        self.slots.push(At::Param(self.params.len() - 1));
        self.params.len() - 1
    }

    /// A capture of the expression at `inner`, `$` and it at `span`.
    pub fn capture(&mut self, text: String, span: Span, inner: Span) -> usize {
        let proxy = self.proxy();
        self.captures.push(Capture {
            text,
            span,
            inner,
            slot: self.slots.len(),
            form: None,
            proxy,
            template: false,
        });
        self.slots.push(At::Capture(self.captures.len() - 1));
        self.captures.len() - 1
    }

    /// A template of the program used as a subquery: its type says no kind
    /// or cast, so it has no proxy.
    pub fn template(&mut self, text: String, span: Span, inner: Span) -> usize {
        self.captures.push(Capture {
            text,
            span,
            inner,
            slot: self.slots.len(),
            form: None,
            proxy: usize::MAX,
            template: true,
        });
        self.slots.push(At::Capture(self.captures.len() - 1));
        self.captures.len() - 1
    }

    /// A literal at `at`, passed beside the text.
    pub fn literal(&mut self, at: Span) -> usize {
        let proxy = self.proxy();
        self.literals.push(Literal {
            at,
            slot: self.slots.len(),
            proxy,
        });
        self.slots.push(At::Literal(self.literals.len() - 1));
        self.literals.len() - 1
    }

    /// A placeholder compared with a column passes through its codec.
    pub fn adopt(&mut self, a: &Val, b: &Val) {
        for (p, other) in [(a, b), (b, a)] {
            let Some(form) = &other.form else {
                continue;
            };
            match p.at {
                Some(At::Param(i)) if self.params[i].form.is_none() => {
                    self.params[i].form = Some(form.clone());
                }
                Some(At::Capture(i)) if self.captures[i].form.is_none() => {
                    self.captures[i].form = Some(form.clone());
                }
                _ => {}
            }
        }
    }

    /// The columns of the table `i`.
    pub fn columns(&self, table: usize) -> &[ShownColumn] {
        &self.tables[table].shown.columns
    }
}

/// The code of the query `block`, or what is wrong with it.
pub fn expand(block: &Block, compiler: &mut dyn Compiler) -> Result<Arena, Vec<Diagnostic>> {
    let toks = parse::tokens(block, block.body().rest());
    let binder = (block.arguments().first())
        .filter(|t| t.kind == TokenKind::Ident)
        .map(|t| Tok {
            text: block.text_of(t.span).to_owned(),
            span: t.span,
            kind: t.kind,
        });
    let parsed = parse::query(binder, &toks, block.whole()).map_err(|d| vec![d])?;
    let mut cx = Cx {
        block,
        compiler,
        binder: parsed.binder.as_ref().map(|b| b.text.clone()),
        tables: Vec::new(),
        vars: Vec::new(),
        params: Vec::new(),
        captures: Vec::new(),
        literals: Vec::new(),
        slots: Vec::new(),
        proxies: 0,
        checks: Vec::new(),
        wrong: Vec::new(),
    };
    let query = select::statement(&mut cx, &parsed);
    let declared = declared(block);
    if let Some(declared) = &declared {
        for p in &cx.params {
            if !declared.contains(&p.name) {
                let message = format!("the template declares no parameter `{}`", p.name);
                cx.wrong
                    .push(Diagnostic::error(message, p.span).with_code(UNDECLARED));
            }
        }
    }
    match query {
        Some(query) if cx.wrong.is_empty() => Ok(code::arena(&mut cx, &query, declared.as_deref())),
        _ => Err(cx.wrong),
    }
}

/// The parameters the signature above the `let` declares, when it writes
/// the template's parameters as a record: `Sql.Template Pg { x : T } r`.
fn declared(block: &Block) -> Option<Vec<String>> {
    declared_types(block).map(|ps| ps.into_iter().map(|(l, _)| l).collect())
}

/// The parameters the signature declares, each with its type as written.
pub fn declared_types(block: &Block) -> Option<Vec<(String, String)>> {
    let signature = block.signature.as_ref()?;
    let text = |s: Span| &signature.text[s.start as usize..s.end as usize];
    let tokens = &signature.tokens;
    let at = tokens.iter().rposition(|t| text(t.span) == "Template")?;
    // `Template Pg {` …
    tokens.get(at + 2).filter(|t| text(t.span) == "{")?;
    let mut depth = 0;
    let mut out: Vec<(String, String)> = Vec::new();
    let mut ty: Option<(String, Span)> = None;
    let flush = |ty: &mut Option<(String, Span)>, end: u32, out: &mut Vec<(String, String)>| {
        if let Some((label, start)) = ty.take() {
            let written = signature.text[start.end as usize..end as usize].trim();
            out.push((label, written.to_owned()));
        }
    };
    for (i, t) in tokens.iter().enumerate().skip(at + 2) {
        match text(t.span) {
            "{" | "(" | "[" => depth += 1,
            "}" | ")" | "]" => {
                depth -= 1;
                if depth == 0 {
                    flush(&mut ty, t.span.start, &mut out);
                    return Some(out);
                }
            }
            ";" if depth == 1 => flush(&mut ty, t.span.start, &mut out),
            ":" if depth == 1 => {
                if let Some(label) = i.checked_sub(1).and_then(|j| tokens.get(j)) {
                    ty = Some((text(label.span).to_owned(), t.span));
                }
            }
            _ => {}
        }
    }
    None
}

/// What only the types of the code show: arithmetic on a type SQL cannot
/// compute, and a template in `is_in` whose row is not one column of the
/// value's type.
pub fn check(block: &Block, facts: &[Fact]) -> Vec<Diagnostic> {
    let toks = parse::tokens(block, block.body().rest());
    let Ok(parsed) = parse::query(None, &toks, block.whole()) else {
        return Vec::new();
    };
    let mut operands = Vec::new();
    let mut members = Vec::new();
    for clause in &parsed.clauses {
        for e in &clause.items {
            arithmetic(e, &mut operands);
            templates(e, &mut members);
        }
    }
    let mut out = Vec::new();
    let fact = |span: Span| facts.iter().find(|f| f.span == span);
    for (word, at) in members {
        let (Some(value), Some(row)) = (fact(word), fact(at)) else {
            continue;
        };
        let Some(fields) = &row.fields else {
            continue;
        };
        let [only] = &fields[..] else {
            let message = format!(
                "the template of `is_in` selects exactly one result, not {}",
                fields.len()
            );
            out.push(Diagnostic::error(message, at).with_code(MALFORMED));
            continue;
        };
        let (Some(a), Some(b)) = (&value.ty.exact, &only.ty.exact) else {
            continue;
        };
        // a column that may be `NULL` is an `Option` in the row
        let same =
            a == b || *b == format!("Prelude.Option {a}") || *b == format!("Prelude.Option ({a})");
        if !same {
            let message = format!(
                "`is_in` compares `{}` with a template of `{}`",
                value.ty.shown, only.ty.shown
            );
            out.push(Diagnostic::error(message, at).with_code(MALFORMED));
        }
    }
    for (span, op) in operands {
        let Some(fact) = facts.iter().find(|f| f.span == span) else {
            continue;
        };
        let Some(exact) = &fact.ty.exact else {
            continue;
        };
        let numeric = matches!(
            exact.as_str(),
            "Prelude.Int" | "Prelude.Float" | "Prelude.BigInt" | "BigDecimal.BigDecimal"
        );
        if !numeric {
            let message = format!(
                "`{op}` in SQL takes `Float`, `BigInt` or `BigDecimal`, not `{}`",
                fact.ty.shown
            );
            out.push(Diagnostic::error(message, span).with_code(MALFORMED));
        }
    }
    out
}

/// Each `is_in e $(t)`: the span of its word and of the template.
fn templates(e: &E, out: &mut Vec<(Span, Span)>) {
    match e {
        E::Apply(name, args, _) => {
            if let [_, sub] = &args[..]
                && name.text == "is_in"
            {
                let sub = match sub {
                    E::Paren(inner, _) => inner,
                    other => other,
                };
                if let E::Capture { span, .. } = sub {
                    out.push((name.span, *span));
                }
            }
            for a in args {
                templates(a, out);
            }
        }
        E::Binary(_, l, r, _) => {
            templates(l, out);
            templates(r, out);
        }
        E::Not(inner, _) | E::Neg(inner, _) | E::Paren(inner, _) => templates(inner, out),
        E::Record(fields, _) => fields.iter().for_each(|(_, v)| templates(v, out)),
        E::Sub(q) => (q.clauses.iter())
            .flat_map(|c| &c.items)
            .for_each(|i| templates(i, out)),
        E::Path(..) | E::Capture { .. } | E::Literal(_) => {}
    }
}

/// The operands whose type arithmetic checks, with the operator.
fn arithmetic(e: &E, out: &mut Vec<(Span, &'static str)>) {
    match e {
        E::Binary(op, l, r, _) => {
            if matches!(op, Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Rem) {
                out.push((l.span(), op.text()));
            }
            arithmetic(l, out);
            arithmetic(r, out);
        }
        E::Neg(inner, _) => {
            out.push((inner.span(), "-"));
            arithmetic(inner, out);
        }
        E::Not(inner, _) | E::Paren(inner, _) => arithmetic(inner, out),
        E::Apply(_, args, _) => args.iter().for_each(|a| arithmetic(a, out)),
        E::Record(fields, _) => fields.iter().for_each(|(_, v)| arithmetic(v, out)),
        E::Sub(q) => (q.clauses.iter())
            .flat_map(|c| &c.items)
            .for_each(|i| arithmetic(i, out)),
        E::Path(..) | E::Capture { .. } | E::Literal(_) => {}
    }
}
