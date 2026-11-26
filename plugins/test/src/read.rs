//! The cases of a block, read from its tokens: `suite "name" { … }` holds
//! suites, tests, hooks and fixtures, `test "name" { body }` and
//! `before_each { body }` the user's body and `fixture name = expression`
//! the user's expression, which the compiler parses; a case may be `.skip`
//! or `.only` and have a `timeout`, and a suite may be `parallel`.

use flick_plugin::{Block, Compiler, Diagnostic, Place, Span, Token, TokenKind, string_value};

use crate::{fixtures, rows};

/// A block that is not a suite or a test: the code of its errors.
pub const MALFORMED: &str = "T001";

/// Two suites or tests of one level with the same name.
pub const DUPLICATE: &str = "T002";

/// What a modifier after the word asks of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Run,
    /// `.skip`: reported as skipped, never run.
    Skip,
    /// `.only`: the run keeps only such cases.
    Only,
}

/// A suite, a test or a hook, at spans of the block.
pub struct Item {
    /// The word that starts it, `test`, `suite` or a hook's; none for the
    /// block's own, its head.
    pub word: Option<Span>,
    /// The modifier's word, `skip` of `.skip`, if there is one.
    pub modifier: Option<Span>,
    pub mode: Mode,
    /// The string token naming it; a hook's word, as a hook has no name.
    pub name: Span,
    /// The word `timeout` and its milliseconds, the expression after it.
    pub timeout: Option<(Span, Span)>,
    /// The word `parallel` of a suite whose cases run at once.
    pub parallel: Option<Span>,
    pub kind: Kind,
    /// Where its `}` ends.
    pub end: u32,
}

pub enum Kind {
    /// Its suites, tests and hooks in the order written.
    Suite(Vec<Item>),
    /// Its body, none when the braces are empty.
    Test(Option<Span>),
    /// A hook of the suite it is in, its word one of [`HOOKS`], and its body.
    Hook(Option<Span>),
    /// A fixture of the suite it is in, named by the item's name.
    Fixture(Fixture),
    /// `test.each rows "name" (pattern) { body }`: a case for each row.
    Each(Each),
}

/// The rows, the pattern and the body of a `test.each`.
pub struct Each {
    /// The expression of the rows.
    pub rows: Span,
    /// The pattern each row is matched with, in its parentheses.
    pub pattern: Span,
    /// How many parts a row has: the parts of the pattern's tuple, else one.
    pub parts: usize,
    pub body: Option<Span>,
}

/// `fixture [shared [concurrent]] name = expression`.
pub struct Fixture {
    /// The word `shared` of one made once for the suite, not for each test.
    pub shared: Option<Span>,
    /// The word `concurrent` of a shared one that tests may use at once.
    pub concurrent: Option<Span>,
    /// Its expression, none when nothing follows `=`.
    pub expr: Option<Span>,
}

/// The word that starts a fixture of a suite.
pub const FIXTURE: &str = "fixture";

/// The words that start a hook of a suite, as the functions of `Test`.
pub const HOOKS: [&str; 4] = ["before_all", "before_each", "after_each", "after_all"];

/// What the tokens after a case's word say of it: its modifier, its name
/// and its timeout.
struct Header {
    modifier: Option<Span>,
    mode: Mode,
    parallel: Option<Span>,
    name: Span,
    timeout: Option<(Span, Span)>,
    /// The rows, the pattern and its parts of a `test.each`.
    each: Option<rows::Rows>,
}

/// How a test's body that does not parse is read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Bodies {
    /// It makes the block wrong.
    Strict,
    /// One on lines of its own stands for an empty body, its syntax errors
    /// being the compiler's to report: the block still declares its case,
    /// so the module's `tests`, which names it, finds it. The layout of a
    /// block never reads bodies so, as it would drop the user's code.
    Lenient,
}

/// The suite or test of `block`, the user's bodies parsed.
pub fn block(
    block: &Block,
    compiler: &mut dyn Compiler,
    bodies: Bodies,
) -> Result<Item, Vec<Diagnostic>> {
    let mut item = structure(block)?;
    parse(block, &mut item, compiler, bodies).map_err(|d| vec![d])?;
    Ok(item)
}

/// The suite or test of `block` as its tokens show it, each test's body
/// the range of its tokens, not parsed yet: what is wrong here the plugin
/// knows before it expands any block.
pub fn structure(block: &Block) -> Result<Item, Vec<Diagnostic>> {
    let suite = block.head == "suite";
    let arguments = block.arguments();
    let braced = before(block, block.body) == Some(b'{');
    let header = header(block, arguments).filter(|_| braced);
    let Some(Header {
        modifier,
        mode,
        parallel,
        name,
        timeout,
        each,
    }) = header.filter(|h| {
        block.place == Place::Declaration
            && (suite || h.parallel.is_none())
            && (!suite || h.each.is_none())
    })
    else {
        let message = if suite {
            "a `suite` block starts a top-level declaration, names its case and holds its tests in braces: `suite \"name\" { … }`, maybe `suite.skip` or `suite.only`, `parallel` before the name and `timeout millis` after it"
        } else {
            "a `test` block starts a top-level declaration, names its case and holds its body in braces: `test \"name\" { … }`, maybe `test.skip` or `test.only`, and `timeout millis` after the name; or a case for each row: `test.each rows \"name\" (pattern) { … }`"
        };
        return Err(vec![malformed(block.whole(), message)]);
    };
    let body = &block.tokens[arguments.len()..];
    let kind = if suite {
        Kind::Suite(items(block, body).map_err(|d| vec![d])?)
    } else {
        case(each, range(body))
    };
    let item = Item {
        word: None,
        modifier,
        mode,
        name,
        timeout,
        parallel,
        kind,
        end: block.whole().end,
    };
    let mut wrong = Vec::new();
    duplicates(block, &item, &mut wrong);
    fixtures::check(block, &item, &mut wrong);
    if wrong.is_empty() {
        Ok(item)
    } else {
        Err(wrong)
    }
}

/// The body of each test and hook under `item`, its range of tokens parsed
/// by the compiler as after the `=` of a `let`: its lines, or one
/// expression; and each timeout's expression.
fn parse(
    block: &Block,
    item: &mut Item,
    compiler: &mut dyn Compiler,
    bodies: Bodies,
) -> Result<(), Diagnostic> {
    if let Some((_, millis)) = &mut item.timeout {
        let hole = compiler.parse_expr(millis.start, millis.end)?;
        if hole.span.end < millis.end {
            let rest = Span {
                start: hole.span.end,
                end: millis.end,
            };
            return Err(malformed(
                rest,
                "expected the `{` after the milliseconds of the timeout",
            ));
        }
        *millis = hole.span;
    }
    match &mut item.kind {
        Kind::Suite(items) => {
            (items.iter_mut()).try_for_each(|i| parse(block, i, compiler, bodies))
        }
        Kind::Each(each) => {
            each.rows = whole(
                compiler.parse_expr(each.rows.start, each.rows.end)?.span,
                each.rows,
                "expected the name of the case after the rows",
            )?;
            let pattern = compiler
                .parse_pat(each.pattern.start, each.pattern.end)?
                .span;
            each.pattern = whole(
                pattern,
                each.pattern,
                "expected the `{` after the pattern of the rows",
            )?;
            body(block, &mut each.body, compiler, bodies)
        }
        Kind::Test(code) | Kind::Hook(code) | Kind::Fixture(Fixture { expr: code, .. }) => {
            body(block, code, compiler, bodies)
        }
    }
}

/// `parsed`, the span the compiler parsed of `written`, when it is all of it.
fn whole(parsed: Span, written: Span, message: &str) -> Result<Span, Diagnostic> {
    if parsed.end < written.end {
        let rest = Span {
            start: parsed.end,
            end: written.end,
        };
        return Err(malformed(rest, message));
    }
    Ok(parsed)
}

/// `code`, the range of a body or a fixture's expression, parsed by the
/// compiler as after the `=` of a `let`.
fn body(
    block: &Block,
    code: &mut Option<Span>,
    compiler: &mut dyn Compiler,
    bodies: Bodies,
) -> Result<(), Diagnostic> {
    let Some(span) = *code else {
        return Ok(());
    };
    let hole = match compiler.parse_body(span.start, span.end) {
        Ok(hole) => hole,
        // lines are a block, which the compiler parses whatever they hold, so
        // it has reported why they do not parse
        Err(_) if bodies == Bodies::Lenient && before(block, span.start) == Some(b'\n') => {
            *code = None;
            return Ok(());
        }
        Err(d) => return Err(d),
    };
    *code = Some(whole(
        hole.span,
        span,
        "expected the end of the body: its lines, or one expression",
    )?);
    Ok(())
}

/// The last byte of `block` before `at` that is no space or tab.
pub fn before(block: &Block, at: u32) -> Option<u8> {
    let text = block.text.as_bytes().get(..at as usize)?;
    text.iter()
        .rev()
        .find(|b| !matches!(b, b' ' | b'\t'))
        .copied()
}

/// The range of `tokens`, a test's body, from its first token to its last;
/// none when there are none.
pub fn range(tokens: &[Token]) -> Option<Span> {
    let (first, last) = (tokens.first()?, tokens.last()?);
    Some(Span {
        start: first.span.start,
        end: last.span.end,
    })
}

/// The modifier, the name and the timeout of a case, from the tokens after
/// its word: `"name"`, `.skip "name"` or `.only "name"`, maybe after
/// `parallel` and followed by `timeout millis`.
fn header(block: &Block, tokens: &[Token]) -> Option<Header> {
    let (modifier, rest) = match tokens {
        [dot, word, rest @ ..] if block.text_of(dot.span) == "." => (Some(word.span), rest),
        rest => (None, rest),
    };
    if modifier.is_some_and(|m| block.text_of(m) == "each") {
        return each(block, modifier, rest);
    }
    let (parallel, rest) = match rest {
        [word, rest @ ..] if block.text_of(word.span) == "parallel" => (Some(word.span), rest),
        rest => (None, rest),
    };
    let (name, rest) = rest.split_first()?;
    let timeout = match rest {
        [] => None,
        [word, millis @ ..] if block.text_of(word.span) == "timeout" => {
            Some((word.span, range(millis)?))
        }
        _ => return None,
    };
    let mode = match modifier.map(|m| block.text_of(m)) {
        None => Mode::Run,
        Some("skip") => Mode::Skip,
        Some("only") => Mode::Only,
        Some(_) => return None,
    };
    (name.kind == TokenKind::String).then_some(Header {
        modifier,
        mode,
        parallel,
        name: name.span,
        timeout,
        each: None,
    })
}

/// The header of a `test.each` after `.each`, as [`rows::header`] reads it.
fn each(block: &Block, modifier: Option<Span>, tokens: &[Token]) -> Option<Header> {
    let (name, timeout, each) = rows::header(block, tokens)?;
    Some(Header {
        modifier,
        mode: Mode::Run,
        parallel: None,
        name,
        timeout,
        each: Some(each),
    })
}

/// A test's kind: a case for each row of `each`, else one case of `body`.
fn case(each: Option<rows::Rows>, body: Option<Span>) -> Kind {
    match each {
        Some((rows, pattern, parts)) => Kind::Each(Each {
            rows,
            pattern,
            parts,
            body,
        }),
        None => Kind::Test(body),
    }
}

/// The fixture whose word is `tokens[at]`, and where the next item starts:
/// its expression runs to the next word of an item that starts a line.
fn fixture(block: &Block, tokens: &[Token], at: usize) -> Option<(Item, usize)> {
    let word = tokens[at].span;
    let text = |i: usize| tokens.get(i).map(|t| block.text_of(t.span));
    let mut i = at + 1;
    let flag = |name: &str, i: &mut usize| {
        // a word followed by `=` is the fixture's name
        (text(*i) == Some(name) && text(*i + 1) != Some("=")).then(|| {
            *i += 1;
            tokens[*i - 1].span
        })
    };
    let shared = flag("shared", &mut i);
    let concurrent = shared.and_then(|_| flag("concurrent", &mut i));
    let name = tokens.get(i).filter(|t| t.kind == TokenKind::Ident)?;
    if text(i + 1) != Some("=") {
        return None;
    }
    let starts = |t: &Token| {
        let w = block.text_of(t.span);
        let item = crate::owns(w) || HOOKS.contains(&w) || w == FIXTURE || w == ";";
        item && (w == ";" || before(block, t.span.start) == Some(b'\n'))
    };
    let end = (i + 2..tokens.len())
        .find(|&j| starts(&tokens[j]))
        .unwrap_or(tokens.len());
    let item = Item {
        word: Some(word),
        modifier: None,
        mode: Mode::Run,
        name: name.span,
        timeout: None,
        parallel: None,
        kind: Kind::Fixture(Fixture {
            shared,
            concurrent,
            expr: range(&tokens[i + 2..end]),
        }),
        end: tokens[end - 1].span.end,
    };
    Some((item, end))
}

/// The suites, tests, hooks and fixtures of a suite's body, `tokens`.
fn items(block: &Block, tokens: &[Token]) -> Result<Vec<Item>, Diagnostic> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(word) = tokens.get(i) {
        let w = block.text_of(word.span);
        if w == ";" {
            i += 1;
            continue;
        }
        if w == FIXTURE {
            let Some((fixture, next)) = fixture(block, tokens, i) else {
                let message = "a fixture is `fixture name = expression`, maybe `fixture shared` or `fixture shared concurrent`";
                return Err(malformed(word.span, message));
            };
            out.push(fixture);
            i = next;
            continue;
        }
        // the header runs to the `{` of the body, which the parser balanced
        let open = rows::brace(block, &tokens[i..]);
        let hook = HOOKS.contains(&w);
        let found = open.and_then(|open| {
            let after = &tokens[i + 1..i + open];
            let header = match after {
                [] if hook => Header {
                    modifier: None,
                    mode: Mode::Run,
                    parallel: None,
                    name: word.span,
                    timeout: None,
                    each: None,
                },
                _ if crate::owns(w) => header(block, after).filter(|h| {
                    (w == "suite" || h.parallel.is_none()) && (w == "test" || h.each.is_none())
                })?,
                _ => return None,
            };
            Some((header, i + open, closing(block, tokens, i + open)?))
        });
        let Some((header, open, close)) = found else {
            let message = "a suite holds `test \"name\" { … }`, `suite \"name\" { … }`, maybe with `.skip` or `.only` and `timeout millis`, `test.each rows \"name\" (pattern) { … }`, the hooks `before_all`, `before_each`, `after_each` and `after_all { … }`, and `fixture name = expression`";
            return Err(malformed(word.span, message));
        };
        let inner = &tokens[open + 1..close];
        let kind = match w {
            "suite" => Kind::Suite(items(block, inner)?),
            "test" => case(header.each, range(inner)),
            _ => Kind::Hook(range(inner)),
        };
        out.push(Item {
            word: Some(word.span),
            modifier: header.modifier,
            mode: header.mode,
            name: header.name,
            timeout: header.timeout,
            parallel: header.parallel,
            kind,
            end: tokens[close].span.end,
        });
        i = close + 1;
    }
    Ok(out)
}

/// The index of the `}` that closes the `{` at `open`.
fn closing(block: &Block, tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, t) in tokens.iter().enumerate().skip(open) {
        match block.text_of(t.span) {
            "{" => depth += 1,
            "}" if depth == 1 => return Some(i),
            "}" => depth -= 1,
            _ => {}
        }
    }
    None
}

/// The names of suites and tests under `item` that a sibling has too, as
/// errors in `out`: a full name, `Suite > test`, stands for one case.
fn duplicates(block: &Block, item: &Item, out: &mut Vec<Diagnostic>) {
    let Kind::Suite(items) = &item.kind else {
        return;
    };
    let cases = || {
        (items.iter()).filter(|i| matches!(i.kind, Kind::Suite(_) | Kind::Test(_) | Kind::Each(_)))
    };
    let names: Vec<String> = cases()
        .map(|i| string_value(block.text_of(i.name)))
        .collect();
    for (inner, name) in cases().zip(&names) {
        if names.iter().filter(|n| *n == name).count() > 1 {
            out.push(duplicate(block, inner.name));
        }
        duplicates(block, inner, out);
    }
}

/// The error of a name another case of the same level has too.
pub fn duplicate(block: &Block, name: Span) -> Diagnostic {
    let message = format!(
        "another case at this level is named {} too: a full name stands for one case",
        block.text_of(name)
    );
    Diagnostic::error(message, name).with_code(DUPLICATE)
}

fn malformed(span: Span, message: &str) -> Diagnostic {
    Diagnostic::error(message, span).with_code(MALFORMED)
}

/// The name of a top-level block, as its string's value; none for a block
/// that does not name its case.
pub fn name_of(block: &Block) -> Option<String> {
    header(block, block.arguments()).map(|h| string_value(block.text_of(h.name)))
}
