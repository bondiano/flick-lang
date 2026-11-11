//! The layout of the Go code: documents for calls, composite literals,
//! blocks and closures, and printing a file as gofmt indents, with tabs.
//!
//! A statement is kept with the newline before it, so that a `//line`
//! directive can bring its own newline at column 0, where Go reads it.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::fmt::{self, Write};
use std::iter;

use flick_pretty::{
    Document, DocumentArena, Documentable, Indentation, NextBreakFitsMode, Position, docvec,
};

use super::arith::Native;
use super::shape::Types;

/// The columns of one level of nesting, written as a tab.
pub(super) const INDENT: usize = 4;
/// The width the printer keeps lines to where it can.
const WIDTH: usize = 100;

/// A Go expression, and whether it is atomic: a name or a literal, whose
/// evaluation can move later without anyone noticing.
#[derive(Debug, Clone, Copy)]
pub(super) struct Code<'a> {
    pub(super) doc: Document<'a>,
    pub(super) atomic: bool,
}

impl<'a> Code<'a> {
    /// The code `text` of a name or a literal, copied into the arena.
    pub(super) fn leaf(a: &'a DocumentArena<'a>, text: &str) -> Self {
        Code {
            doc: a.text(a.alloc_str(text)),
            atomic: atomic(text),
        }
    }
}

/// Code built of parts is never atomic.
impl<'a> From<Document<'a>> for Code<'a> {
    fn from(doc: Document<'a>) -> Self {
        Code { doc, atomic: false }
    }
}

impl<'a> Documentable<'a> for Code<'a> {
    fn to_doc(self, _: &'a DocumentArena<'a>) -> Document<'a> {
        self.doc
    }
}

/// Whether moving the evaluation of `go` later is unobservable: a name or
/// a literal.
fn atomic(go: &str) -> bool {
    let name = go
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && go
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    name || go.starts_with('"') && !go[1..].contains('"')
        || go.starts_with("int32(")
        || go.starts_with("float64(")
}

/// `open`, the items separated by commas, then `close`: on one line when
/// they fit, else one per line with the trailing comma Go then requires.
/// Only the last item may hug `close`, as a closure does: the others are
/// measured in full.
pub(super) fn list<'a>(
    a: &'a DocumentArena<'a>,
    open: impl Documentable<'a>,
    items: impl IntoIterator<Item = Document<'a>>,
    close: &'a str,
) -> Document<'a> {
    let mut items = items.into_iter().peekable();
    if items.peek().is_none() {
        return docvec![a, open, close];
    }
    let items = iter::from_fn(|| {
        let item = items.next()?;
        Some(if items.peek().is_some() {
            item.next_break_fits(a, NextBreakFitsMode::Disabled)
        } else {
            item
        })
    });
    let items = a.join(items, a.break_(",", ", "));
    // a closure hugging the parentheses keeps its body at the call's indentation
    let items = docvec![a, a.break_("", ""), items].nest_if_broken(a, INDENT);
    docvec![a, open, items, a.break_(",", ""), close].group(a)
}

/// The call of `f` with `args`.
pub(super) fn call<'a>(
    a: &'a DocumentArena<'a>,
    f: impl Documentable<'a>,
    args: impl IntoIterator<Item = Document<'a>>,
) -> Document<'a> {
    list(a, docvec![a, f, "("], args, ")")
}

/// A statement, on a line of its own.
pub(super) fn statement<'a>(a: &'a DocumentArena<'a>, stmt: impl Documentable<'a>) -> Document<'a> {
    docvec![a, a.line(), stmt]
}

/// A `//line` directive: Go names what follows by `path` and `line`.
pub(super) fn directive<'a>(a: &'a DocumentArena<'a>, path: String, line: u32) -> Document<'a> {
    docvec![a, a.line(), "//line ", path, ":", line].set_nesting(a, 0)
}

/// A block of statements, each on its own line.
pub(super) fn block<'a>(
    a: &'a DocumentArena<'a>,
    stmts: impl IntoIterator<Item = Document<'a>>,
) -> Document<'a> {
    let mut stmts = stmts.into_iter().peekable();
    if stmts.peek().is_none() {
        return a.text("{}");
    }
    docvec![a, "{", a.concat(stmts).nest(a, INDENT), a.line(), "}"]
}

/// A block of the one statement `stmt`.
pub(super) fn single<'a>(a: &'a DocumentArena<'a>, stmt: impl Documentable<'a>) -> Document<'a> {
    block(a, [statement(a, stmt)])
}

/// `if test { then } else { other }`, of a test that is a Go `bool`.
pub(super) fn if_else<'a>(
    a: &'a DocumentArena<'a>,
    test: Document<'a>,
    then: Vec<Document<'a>>,
    other: Vec<Document<'a>>,
) -> Document<'a> {
    docvec![
        a,
        "if ",
        test,
        " ",
        block(a, then),
        " else ",
        block(a, other)
    ]
}

/// `{ s1; s2 }`, or the statements on lines of their own when they do not
/// fit. The statements carry no newline of their own.
pub(super) fn inline<'a>(
    a: &'a DocumentArena<'a>,
    stmts: impl IntoIterator<Item = Document<'a>>,
) -> Document<'a> {
    let stmts = a.join(stmts, a.break_("", "; "));
    let body = docvec![a, a.break_("", " "), stmts].nest(a, INDENT);
    docvec![a, "{", body, a.break_("", " "), "}"].group(a)
}

/// The function literal `head { stmts }` of an `inline` body. As the last
/// argument of a call it hugs the parentheses: the call stays on its line
/// and only the body breaks.
pub(super) fn closure<'a>(
    a: &'a DocumentArena<'a>,
    head: impl Documentable<'a>,
    stmts: impl IntoIterator<Item = Document<'a>>,
) -> Document<'a> {
    docvec![a, head, " ", inline(a, stmts)].next_break_fits(a, NextBreakFitsMode::Enabled)
}

/// Conditions joined by `&&`, the line broken after each operator when
/// they do not fit; the conditions after the first are indented.
pub(super) fn conjunction<'a>(a: &'a DocumentArena<'a>, conds: Vec<Document<'a>>) -> Document<'a> {
    let mut conds = conds.into_iter();
    let first = conds.next().unwrap_or(a.empty());
    let rest = a.concat(conds.map(|cond| docvec![a, " &&", a.break_("", " "), cond]));
    docvec![a, first, rest.nest(a, INDENT)].group(a)
}

/// The parameter list of a Go function taking the hidden fiber, then
/// `names`, each a `Value`, independently of its language stages.
pub(super) fn signature<S: Borrow<str>>(names: &[S]) -> String {
    let params = param_list(names);
    if params.is_empty() {
        "fb *flickrt.Fiber".to_owned()
    } else {
        format!("fb *flickrt.Fiber, {params}")
    }
}

/// The parameter list of a Go function taking the hidden fiber, `dicts`
/// dictionaries, then parameters of `types`: each of the Go type it has
/// there, a `Value` when it has none.
pub(super) fn typed_signature(names: &[&str], dicts: usize, types: Types) -> String {
    if !types.any() {
        return signature(names);
    }
    let params = names.iter().enumerate().map(|(i, name)| {
        let native = i.checked_sub(dicts).and_then(|p| types.param(p));
        format!(", {name} {}", native.map_or("flickrt.Value", Native::go))
    });
    format!("fb *flickrt.Fiber{}", params.collect::<String>())
}

/// The parameter list of Go function taking `names`, each a `Value`.
pub(super) fn param_list<S: Borrow<str>>(names: &[S]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("{} flickrt.Value", names.join(", "))
    }
}

/// The `//line` directive naming the Go file `file` again for what follows.
/// The number of the line after it is only known while printing: `print`
/// writes it at the cursor.
pub(super) fn named_again<'a>(a: &'a DocumentArena<'a>, file: String) -> Document<'a> {
    docvec![a, a.line(), "//line ", file, ":", a.cursor(0)]
}

/// The text of a Go file.
pub(super) fn print(doc: Document<'_>) -> String {
    let out = RefCell::new(String::new());
    // the cursors of `named_again`: the 1-based number of the next line
    let mut number = |_, at: Position| {
        let _ = write!(out.borrow_mut(), "{}", at.line + 2);
    };
    // writing into a `String` never fails
    let _ = doc.pretty_print_with(
        WIDTH,
        Indentation::Tabs { width: INDENT },
        &mut Shared(&out),
        &mut number,
    );
    out.into_inner()
}

/// The output `print` and its observer both write into.
struct Shared<'s>(&'s RefCell<String>);

impl Write for Shared<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0.borrow_mut().push_str(text);
        Ok(())
    }
}
