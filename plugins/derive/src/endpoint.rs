//! The endpoints of a record that derives `HttpApi`, read from its tokens:
//! `label : Get "/path" { headers : H; payload : B } S {E1, E2}`, maybe
//! inside `Authed (…)` or `MaybeAuthed (…)`. Each type the user wrote is
//! kept as its span, for the code to name it as written.

use flick_plugin::{Block, Span, Token, TokenKind};

/// Why a record is no API, and where.
pub type Refusal = (Span, String);

/// The parts of a request, as `HttpApi.Request` names them.
pub const PARTS: [&str; 4] = ["path", "query", "headers", "payload"];

const METHODS: [&str; 5] = ["Get", "Post", "Put", "Patch", "Delete"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guard {
    None,
    /// `Authed`: the request needs the group's check.
    Required,
    /// `MaybeAuthed`: the check also serves anonymous requests.
    Optional,
}

#[derive(Debug, Clone)]
pub enum Path {
    Text(String),
    /// A route type's constructor, which the compiler writes as its path.
    Route(Span),
}

#[derive(Debug, Clone)]
pub struct Endpoint {
    /// Its field, `label : …`.
    pub at: Span,
    pub label: String,
    pub guard: Guard,
    pub method: String,
    pub path: Path,
    /// The parts of the request, in the order written.
    pub parts: Vec<(&'static str, Span)>,
    /// The success and whether it is `Created`; none for `Unit`.
    pub success: Option<(Span, bool)>,
    /// The errors of its row, `{E1, E2}`.
    pub errors: Vec<Span>,
}

impl Endpoint {
    pub fn part(&self, name: &str) -> Option<Span> {
        self.parts.iter().find(|(p, _)| *p == name).map(|&(_, s)| s)
    }
}

/// The endpoints of the record `me` that `block` writes at `record`.
pub fn read(block: &Block, me: &str, record: Span) -> Result<Vec<Endpoint>, Refusal> {
    let not_record = || -> Refusal { (record, not_a_record(me)) };
    let r = Reader { block };
    let fields = r.inside(record, "{").ok_or_else(not_record)?;
    let mut out = Vec::new();
    for field in r.split(fields, ";") {
        let (label, ty) = r.field(field).ok_or_else(not_record)?;
        out.push(r.endpoint(me, field, label, ty)?);
    }
    Ok(out)
}

struct Reader<'b> {
    block: &'b Block,
}

impl Reader<'_> {
    fn tokens(&self, span: Span) -> Vec<Token> {
        (self.block.tokens.iter())
            .filter(|t| span.start <= t.span.start && t.span.end <= span.end)
            .copied()
            .collect()
    }

    fn text(&self, t: &Token) -> &str {
        self.block.text_of(t.span)
    }

    /// The span inside `open … close` when `span` is just that.
    fn inside(&self, span: Span, open: &str) -> Option<Span> {
        let close = if open == "(" { ")" } else { "}" };
        let tokens = self.tokens(span);
        let (first, last) = (tokens.first()?, tokens.last()?);
        let whole = self.closing(&tokens, 0) == Some(tokens.len() - 1);
        (whole && self.text(first) == open && self.text(last) == close).then_some(Span {
            start: first.span.end,
            end: last.span.start,
        })
    }

    /// The index of the bracket closing the one at `at`.
    fn closing(&self, tokens: &[Token], at: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (i, t) in tokens.iter().enumerate().skip(at) {
            match self.text(t) {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ if depth == 0 => return None,
                _ => {}
            }
        }
        None
    }

    /// `span` without the parentheses around all of it.
    fn bare(&self, span: Span) -> Span {
        self.inside(span, "(")
            .map_or(span, |inner| self.bare(inner))
    }

    /// The pieces of `span` between the separators `sep` outside brackets,
    /// empty ones left out.
    fn split(&self, span: Span, sep: &str) -> Vec<Span> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut start: Option<u32> = None;
        let mut end = span.start;
        for t in self.tokens(span) {
            let text = self.text(&t);
            match text {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" => depth = depth.saturating_sub(1),
                _ if depth == 0 && text == sep => {
                    if let Some(s) = start.take() {
                        out.push(Span { start: s, end });
                    }
                    continue;
                }
                _ => {}
            }
            start.get_or_insert(t.span.start);
            end = t.span.end;
        }
        if let Some(s) = start {
            out.push(Span { start: s, end });
        }
        out
    }

    /// `label : ty`.
    fn field(&self, span: Span) -> Option<(String, Span)> {
        let tokens = self.tokens(span);
        match &tokens[..] {
            [label, colon, rest @ ..]
                if label.kind == TokenKind::Ident && self.text(colon) == ":" =>
            {
                let ty = Span {
                    start: rest.first()?.span.start,
                    end: span.end,
                };
                Some((self.text(label).to_owned(), ty))
            }
            _ => None,
        }
    }

    /// The atoms of an application: names (qualified ones whole), literals
    /// and bracketed groups.
    fn atoms(&self, span: Span) -> Vec<Span> {
        let tokens = self.tokens(self.bare(span));
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let start = tokens[i].span.start;
            let mut last = i;
            if matches!(self.text(&tokens[i]), "(" | "{" | "[") {
                last = self.closing(&tokens, i).unwrap_or(tokens.len() - 1);
            } else {
                // `M.N.T`
                while tokens.get(last + 1).is_some_and(|t| self.text(t) == ".")
                    && tokens
                        .get(last + 2)
                        .is_some_and(|t| matches!(t.kind, TokenKind::Con | TokenKind::Ident))
                {
                    last += 2;
                }
            }
            out.push(Span {
                start,
                end: tokens[last].span.end,
            });
            i = last + 1;
        }
        out
    }

    /// The last part of the name at `span`: `Get` of `HttpApi.Derive.Get`.
    fn last_name(&self, span: Span) -> &str {
        let text = self.block.text_of(span);
        text.rsplit('.').next().unwrap_or(text)
    }

    fn endpoint(&self, me: &str, at: Span, label: String, ty: Span) -> Result<Endpoint, Refusal> {
        let fail = |why: &str| -> Refusal { (at, format!("`{me}.{label}`: {why}")) };
        let mut atoms = self.atoms(ty);
        let guard = match atoms.first().map(|&s| self.last_name(s)) {
            Some("Authed") => Guard::Required,
            Some("MaybeAuthed") => Guard::Optional,
            _ => Guard::None,
        };
        if guard != Guard::None {
            let [_, inner] = atoms[..] else {
                return Err(fail("`Authed` and `MaybeAuthed` take the endpoint"));
            };
            atoms = self.atoms(inner);
        }
        let shape = "an endpoint is `Get`, `Post`, `Put`, `Patch` or `Delete` of `HttpApi.Derive` with a path, the request, the success and the errors";
        let [head, path, request, success, errors] = atoms[..] else {
            return Err(fail(shape));
        };
        let method = self.last_name(head);
        if !METHODS.contains(&method) {
            return Err(fail(shape));
        }
        let path = match &self.tokens(path)[..] {
            [t] if t.kind == TokenKind::String => Path::Text(unquoted(self.text(t))),
            _ => Path::Route(path),
        };
        let parts = self.parts(request).ok_or_else(|| {
            fail("the request is a record of its parts, `{ path : P; payload : B }`, or `{}`")
        })?;
        let mut checked = Vec::new();
        for (part, span) in parts {
            let Some(&known) = PARTS.iter().find(|p| **p == part) else {
                return Err(fail(&format!(
                    "`{part}` is not a part of a request: `path`, `query`, `headers` or `payload`"
                )));
            };
            // a part written twice is the record's error, `FLICK-T014`
            if !checked.iter().any(|(p, _)| *p == known) {
                checked.push((known, span));
            }
        }
        let entries = self.inside(errors, "{").ok_or_else(|| {
            fail("the errors are a row of error types, `{NotFound, Conflict}`, or `{}`")
        })?;
        Ok(Endpoint {
            at,
            label,
            guard,
            method: method.to_owned(),
            path,
            parts: checked,
            success: self.success(success),
            errors: self.split(entries, ","),
        })
    }

    fn parts(&self, request: Span) -> Option<Vec<(String, Span)>> {
        let inner = self.inside(request, "{")?;
        self.split(inner, ";")
            .into_iter()
            .map(|f| self.field(f))
            .collect()
    }

    /// The success: `S`, `Created S`, or none for `Unit`.
    fn success(&self, span: Span) -> Option<(Span, bool)> {
        let atoms = self.atoms(span);
        let (span, created) = match atoms[..] {
            [head, inner] if self.last_name(head) == "Created" => (inner, true),
            _ => (span, false),
        };
        // `Unit`, `()`, `(Unit)`: no body
        let text: String = self.block.text_of(span).split_whitespace().collect();
        let unit = matches!(
            text.trim_start_matches('(').trim_end_matches(')'),
            "Unit" | ""
        );
        (!unit).then_some((span, created))
    }
}

/// Why `me` does not derive `HttpApi`.
pub fn not_a_record(me: &str) -> String {
    format!(
        "`{me}`: it derives `HttpApi`, so it is a record of endpoints, `field : Get \"/path\" {{ … }} Response {{Errors}}`"
    )
}

/// The text of a string literal.
fn unquoted(literal: &str) -> String {
    let inner = literal
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(literal);
    inner.replace("\\\"", "\"").replace("\\\\", "\\")
}
