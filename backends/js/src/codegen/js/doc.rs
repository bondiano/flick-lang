//! The code model of the generator. An expression is a `flick-pretty`
//! document together with the few facts about its text that the code around
//! it decides on: whether it is a name or a call, which parentheses its
//! place makes redundant, whether it spans lines or yields. They are fixed
//! as the code is built, so nothing reads the generated text back.

use flick_pretty::{Document, DocumentArena, docvec};

/// The width the printer keeps lines to, where it can.
pub(super) const WIDTH: usize = 100;

/// The indentation of a block, and of a list broken one item per line.
const INDENT: usize = 2;

/// The code spans lines.
pub(super) const MULTILINE: u8 = 1;
/// `yield` occurs in the text, strings included.
pub(super) const YIELD_TEXT: u8 = 2;
/// `yield` occurs in code outside the body of a nested `function*`.
pub(super) const YIELDS: u8 = 4;

/// What precedes an expression where it is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Before {
    /// `(`, `,`, `[` or `:`: an argument, an element or a field value.
    Delimiter,
    /// ` = `: an initializer or an assignment.
    Assign,
    /// `return`.
    Return,
    /// `=>`: an arrow body.
    Arrow,
    /// Nothing: the expression starts a statement.
    Statement,
    /// An operator, or a keyword such as `yield`, `of` or `void`.
    Other,
}

/// What follows an expression where it is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum After {
    /// `)`, `,`, `]` or `}`.
    Delimiter,
    /// `;`.
    Semicolon,
    /// An operator, `?`, `:`, `.` or the arguments of a call.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form<'d> {
    /// A name, a member of a module namespace, or a number or string
    /// literal: evaluating it later is unobservable. With its text.
    Atom(&'d str),
    /// `f(x)`, `f(x)(y)` or `(f)(x)`: called again, it needs no parentheses.
    Call,
    /// `f(x).y`, `(x).y` or `(x)`: called without parentheses too, yet no
    /// call itself.
    Member,
    Other,
}

/// The first characters of code, cursors left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Head {
    /// A letter, a digit, `_` or `$`.
    Ident,
    Brace,
    /// `(`, not followed by another `(`.
    Paren,
    /// `((`.
    Parens,
    Other,
}

/// A JS expression.
#[derive(Debug, Clone, Copy)]
pub(super) struct Js<'d> {
    /// The code, without the parentheses `paren` puts it in.
    pub(super) doc: Document<'d>,
    /// The code goes in parentheses, which a place that delimits it drops.
    pub(super) paren: bool,
    /// Source cursors before the code and its parentheses.
    pub(super) marks: Option<Document<'d>>,
    /// The code when what follows it does not delimit its trailing arrow
    /// body, whose parentheses then stay.
    pub(super) tail: Option<Document<'d>>,
    pub(super) form: Form<'d>,
    /// How `doc` starts.
    pub(super) head: Head,
    /// `doc` starts with a cursor.
    pub(super) marked: bool,
    pub(super) flags: u8,
    /// Of `(yield* effect)`, when the effect neither yields nor spans lines:
    /// the effect as an arrow body, and its flags, for release to run it
    /// without a generator.
    pub(super) effect: Option<(Document<'d>, u8)>,
}

impl<'d> Js<'d> {
    /// Code starting with `head`, made of `parts` as far as flags go.
    pub(super) fn code(doc: Document<'d>, head: Head, parts: &[Js<'d>]) -> Self {
        Js {
            doc,
            paren: false,
            marks: None,
            tail: None,
            form: Form::Other,
            head,
            marked: false,
            flags: flags(parts),
            effect: None,
        }
    }

    /// A name, a literal or a module member, as written.
    pub(super) fn text(a: &'d DocumentArena<'d>, text: &'d str) -> Self {
        let mut flags = 0;
        if text.contains("yield") {
            // a string literal only mentions it
            flags |= YIELD_TEXT | if text.starts_with('"') { 0 } else { YIELDS };
        }
        Js {
            form: if atomic(text) {
                Form::Atom(text)
            } else {
                Form::Other
            },
            flags,
            ..Js::code(a.text(text), head(text), &[])
        }
    }

    /// The code in parentheses that a delimiting place drops.
    pub(super) fn in_parens(self) -> Self {
        Js {
            paren: true,
            ..self
        }
    }

    pub(super) fn call(self) -> Self {
        Js {
            form: Form::Call,
            ..self
        }
    }

    pub(super) fn with(self, flags: u8) -> Self {
        Js {
            flags: self.flags | flags,
            ..self
        }
    }

    /// The text of an atom; a cursor before the code makes it no atom.
    pub(super) fn atom(self) -> Option<&'d str> {
        match self.form {
            Form::Atom(text) if self.marks.is_none() => Some(text),
            _ => None,
        }
    }

    /// The code after the source cursors `marks`.
    pub(super) fn marked_by(self, a: &'d DocumentArena<'d>, marks: Option<Document<'d>>) -> Self {
        let marks = match (marks, self.marks) {
            (Some(new), Some(old)) => Some(docvec![a, new, old]),
            (new, old) => new.or(old),
        };
        Js { marks, ..self }
    }

    /// The code placed between `before` and `after`: its parentheses go
    /// where those delimit it already, as an argument, an initializer, a
    /// returned value, or an arrow body or statement that does not start
    /// with `{`.
    pub(super) fn at(self, a: &'d DocumentArena<'d>, before: Before, after: After) -> Document<'d> {
        let code = if self.paren {
            let braced = self.head == Head::Brace;
            let dropped = after != After::Other
                && match before {
                    Before::Delimiter | Before::Assign | Before::Return => true,
                    Before::Arrow => !braced,
                    Before::Statement => after == After::Semicolon && !braced,
                    Before::Other => false,
                };
            if dropped {
                self.doc
            } else if braced {
                // an object hugs its parentheses, its own braces break
                docvec![a, "(", self.doc, ")"]
            } else {
                // a long condition breaks inside its parentheses first
                let edge = a.break_("", "");
                let inner = docvec![a, edge, self.doc].nest_if_broken(a, INDENT);
                docvec![a, "(", inner, edge, ")"].group(a)
            }
        } else if after == After::Other {
            self.tail.unwrap_or(self.doc)
        } else {
            self.doc
        };
        match self.marks {
            Some(marks) => docvec![a, marks, code],
            None => code,
        }
    }

    /// How the code starts, its parentheses included, its leading cursors
    /// left out.
    pub(super) fn lead(self) -> Head {
        if !self.paren {
            self.head
        } else if !self.marked && matches!(self.head, Head::Paren | Head::Parens) {
            Head::Parens
        } else {
            Head::Paren
        }
    }

    /// Code `doc` that starts with this code as it is produced, its
    /// parentheses included, and is made of `parts`.
    pub(super) fn starting(self, doc: Document<'d>, parts: &[Js<'d>]) -> Js<'d> {
        Js {
            marked: self.marks.is_some() || !self.paren && self.marked,
            ..Js::code(doc, self.lead(), parts)
        }
    }

    /// The code, its parentheses included, starts with `(`, no cursor before.
    pub(super) fn opens(self) -> bool {
        self.marks.is_none()
            && (self.paren || !self.marked && matches!(self.head, Head::Paren | Head::Parens))
    }

    /// The head of `(` followed by this code.
    pub(super) fn after_paren(self) -> Head {
        if self.opens() {
            Head::Parens
        } else {
            Head::Paren
        }
    }

    /// A call, or code starting with `((` as an immediately run arrow does:
    /// top-level code a bundler may drop when nothing uses it.
    pub(super) fn is_pure(self) -> bool {
        match self.lead() {
            Head::Parens => true,
            Head::Ident => self.form == Form::Call,
            _ => false,
        }
    }
}

/// The union of the flags of `parts`.
pub(super) fn flags(parts: &[Js<'_>]) -> u8 {
    parts.iter().fold(0, |all, part| all | part.flags)
}

/// How `text` starts.
fn head(text: &str) -> Head {
    let mut chars = text.chars();
    match chars.next() {
        Some('(') if chars.next() == Some('(') => Head::Parens,
        Some('(') => Head::Paren,
        Some('{') => Head::Brace,
        Some(c) if c.is_alphanumeric() || c == '_' || c == '$' => Head::Ident,
        _ => Head::Other,
    }
}

/// Whether the expression `js` is a name, a property of a module
/// namespace, or a number or string literal.
pub(super) fn atomic(js: &str) -> bool {
    let name = |s: &str| {
        s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.')
    };
    name(js)
        || js.parse::<f64>().is_ok()
        || (js.starts_with('"') && serde_json::from_str::<String>(js).is_ok())
}

/// A statement.
#[derive(Debug, Clone, Copy)]
pub(super) struct Stmt<'d> {
    pub(super) doc: Document<'d>,
    /// What its text holds, as [`Js::flags`].
    pub(super) flags: u8,
    /// Of `return value;`: the cursors before the value, and the value.
    pub(super) ret: Option<(Option<Document<'d>>, Js<'d>)>,
}

impl<'d> Stmt<'d> {
    pub(super) fn new(doc: Document<'d>, flags: u8) -> Self {
        Stmt {
            doc,
            flags,
            ret: None,
        }
    }

    /// `return value;`.
    pub(super) fn ret(
        a: &'d DocumentArena<'d>,
        marks: Option<Document<'d>>,
        value: Js<'d>,
    ) -> Self {
        let code = value
            .marked_by(a, marks)
            .at(a, Before::Return, After::Semicolon);
        Stmt {
            doc: docvec![a, "return ", code, ";"],
            flags: value.flags,
            ret: Some((marks, value)),
        }
    }

    pub(super) fn multiline(self) -> bool {
        self.flags & MULTILINE != 0
    }
}

/// The flags of code naming `text`, a name or a path.
pub(super) fn text_flags(text: &str) -> u8 {
    if text.contains("yield") {
        YIELD_TEXT | YIELDS
    } else {
        0
    }
}

/// The union of the flags of `stmts`, which span lines when there are two.
pub(super) fn stmt_flags(stmts: &[Stmt<'_>]) -> u8 {
    let lines = if stmts.len() > 1 { MULTILINE } else { 0 };
    stmts.iter().fold(lines, |all, s| all | s.flags)
}

/// `{`, the statements one per line one level in, `}`: lines of their own.
pub(super) fn block<'d>(a: &'d DocumentArena<'d>, stmts: &[Stmt<'d>]) -> Document<'d> {
    let body = a.concat(stmts.iter().map(|s| docvec![a, a.line(), s.doc]));
    docvec![a, "{", body.nest(a, INDENT), a.line(), "}"]
}

/// `{ stmts }` as code: a function body.
pub(super) fn block_js<'d>(a: &'d DocumentArena<'d>, stmts: &[Stmt<'d>]) -> Js<'d> {
    Js::code(block(a, stmts), Head::Brace, &[]).with(stmt_flags(stmts) | MULTILINE)
}

/// Lines one after another.
pub(super) fn lines<'d>(
    a: &'d DocumentArena<'d>,
    docs: impl IntoIterator<Item = Document<'d>>,
) -> Document<'d> {
    a.join(docs, a.line())
}

/// `open`, the items separated by commas, `close`: on one line when it
/// fits, else one item per line, one level in. Two or more items of which
/// one spans lines always go one per line; a lone item spanning lines hugs
/// the brackets, its first line on theirs. `padded` puts spaces inside the
/// brackets of the line.
pub(super) fn list<'d>(
    a: &'d DocumentArena<'d>,
    open: &'d str,
    items: &[Document<'d>],
    close: &'d str,
    padded: bool,
    spans: bool,
) -> Document<'d> {
    if items.is_empty() {
        return docvec![a, open, close];
    }
    if spans && items.len() > 1 {
        let items = a.join(items.iter().copied(), docvec![a, ",", a.line()]);
        return docvec![
            a,
            open,
            docvec![a, a.line(), items].nest(a, INDENT),
            a.line(),
            close
        ];
    }
    let edge = a.break_("", if padded { " " } else { "" });
    let items = a.join(items.iter().copied(), a.break_(",", ", "));
    docvec![
        a,
        open,
        docvec![a, edge, items].nest_if_broken(a, INDENT),
        edge,
        close
    ]
    .group(a)
}

/// The arguments of a call, or the elements of an array, between `open`
/// and `close`.
pub(super) fn items<'d>(
    a: &'d DocumentArena<'d>,
    open: &'d str,
    items: &[Js<'d>],
    close: &'d str,
) -> Document<'d> {
    let docs: Vec<Document<'d>> = items
        .iter()
        .map(|item| item.at(a, Before::Delimiter, After::Delimiter))
        .collect();
    list(a, open, &docs, close, false, flags(items) & MULTILINE != 0)
}

/// `callee(args)`.
pub(super) fn call<'d>(a: &'d DocumentArena<'d>, callee: Js<'d>, args: &[Js<'d>]) -> Js<'d> {
    let callable = callee.paren || matches!(callee.form, Form::Atom(_) | Form::Call | Form::Member);
    let doc = docvec![
        a,
        callee.at(a, Before::Other, After::Other),
        items(a, "(", args, ")")
    ];
    let js = callee.starting(doc, args).with(callee.flags);
    if callable { js.call() } else { js }
}

/// `[items]`.
pub(super) fn array<'d>(a: &'d DocumentArena<'d>, items: &[Js<'d>]) -> Js<'d> {
    Js::code(self::items(a, "[", items, "]"), Head::Other, items)
}

/// `{ entries }`, the entries made of `parts`.
pub(super) fn object<'d>(
    a: &'d DocumentArena<'d>,
    entries: &[Document<'d>],
    parts: &[Js<'d>],
) -> Js<'d> {
    let spans = flags(parts) & MULTILINE != 0;
    Js::code(list(a, "{", entries, "}", true, spans), Head::Brace, parts)
}

#[cfg(test)]
mod tests {
    use super::{After, Before, Js};
    use flick_pretty::DocumentArena;

    #[test]
    fn parentheses_go_only_where_the_place_delimits_the_code() {
        let a = &DocumentArena::new();
        let sum = Js::code(a.text("a + b"), super::Head::Ident, &[]).in_parens();
        let at = |before, after| sum.at(a, before, after).to_pretty_string(80);
        assert_eq!(at(Before::Delimiter, After::Delimiter), "a + b");
        assert_eq!(at(Before::Return, After::Semicolon), "a + b");
        assert_eq!(at(Before::Statement, After::Semicolon), "a + b");
        assert_eq!(at(Before::Delimiter, After::Other), "(a + b)");
        assert_eq!(at(Before::Other, After::Delimiter), "(a + b)");
        let object = Js::code(a.text("{ a: 1 }"), super::Head::Brace, &[]).in_parens();
        assert_eq!(
            object
                .at(a, Before::Arrow, After::Delimiter)
                .to_pretty_string(80),
            "({ a: 1 })"
        );
        assert_eq!(
            object
                .at(a, Before::Assign, After::Semicolon)
                .to_pretty_string(80),
            "{ a: 1 }"
        );
    }

    #[test]
    fn a_wide_list_puts_each_item_on_a_line_and_a_lone_block_hugs() {
        let a = &DocumentArena::new();
        let long = "x".repeat(60);
        let items = [a.text(&long), a.text(&long)];
        assert_eq!(
            super::list(a, "f(", &items, ")", false, false).to_pretty_string(100),
            format!("f(\n  {long},\n  {long}\n)")
        );
        let block = flick_pretty::docvec![a, "() => {", a.line(), "}"];
        assert_eq!(
            super::list(a, "f(", &[block], ")", false, true).to_pretty_string(100),
            "f(() => {\n})"
        );
    }
}
