//! The cases of a block, read from its tokens: `suite "name" { … }` holds
//! suites and tests, `test "name" { body }` the user's body, which the
//! compiler parses, and either may be `.skip` or `.only`.

use flick_plugin::{Block, Compiler, Diagnostic, Place, Span, Token, TokenKind, string_value};

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

/// A suite or a test, at spans of the block.
pub struct Item {
    /// The word that starts it, `test` or `suite`; none for the block's
    /// own, its head.
    pub word: Option<Span>,
    /// The modifier's word, `skip` of `.skip`, if there is one.
    pub modifier: Option<Span>,
    pub mode: Mode,
    /// The string token naming it.
    pub name: Span,
    pub kind: Kind,
    /// Where its `}` ends.
    pub end: u32,
}

pub enum Kind {
    /// Its suites and tests in the order written.
    Suite(Vec<Item>),
    /// Its body, none when the braces are empty.
    Test(Option<Span>),
}

/// The words that start an item, as the heads of the blocks.
pub const WORDS: [&str; 2] = ["suite", "test"];

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
    let (modifier, mode, name) = match header {
        Some(header) if block.place == Place::Declaration => header,
        _ => {
            let message = if suite {
                "a `suite` block starts a top-level declaration, names its case and holds its tests in braces: `suite \"name\" { … }`, maybe `suite.skip` or `suite.only`"
            } else {
                "a `test` block starts a top-level declaration, names its case and holds its body in braces: `test \"name\" { … }`, maybe `test.skip` or `test.only`"
            };
            return Err(vec![malformed(block.whole(), message)]);
        }
    };
    let body = &block.tokens[arguments.len()..];
    let kind = if suite {
        Kind::Suite(items(block, body).map_err(|d| vec![d])?)
    } else {
        Kind::Test(range(body))
    };
    let item = Item {
        word: None,
        modifier,
        mode,
        name,
        kind,
        end: block.whole().end,
    };
    let mut wrong = Vec::new();
    duplicates(block, &item, &mut wrong);
    if wrong.is_empty() {
        Ok(item)
    } else {
        Err(wrong)
    }
}

/// The body of each test under `item`, its range of tokens parsed by the
/// compiler as after the `=` of a `let`: its lines, or one expression.
fn parse(
    block: &Block,
    item: &mut Item,
    compiler: &mut dyn Compiler,
    bodies: Bodies,
) -> Result<(), Diagnostic> {
    let kind = &mut item.kind;
    match kind {
        Kind::Suite(items) => {
            (items.iter_mut()).try_for_each(|i| parse(block, i, compiler, bodies))
        }
        Kind::Test(None) => Ok(()),
        Kind::Test(Some(body)) => {
            let hole = match compiler.parse_body(body.start, body.end) {
                Ok(hole) => hole,
                // lines are a block, which the compiler parses whatever they hold, so
                // it has reported why they do not parse
                Err(_) if bodies == Bodies::Lenient && before(block, body.start) == Some(b'\n') => {
                    *kind = Kind::Test(None);
                    return Ok(());
                }
                Err(d) => return Err(d),
            };
            if hole.span.end < body.end {
                let rest = Span {
                    start: hole.span.end,
                    end: body.end,
                };
                let message = "expected the end of the test's body: its lines, or one expression";
                return Err(malformed(rest, message));
            }
            *body = hole.span;
            Ok(())
        }
    }
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
fn range(tokens: &[Token]) -> Option<Span> {
    let (first, last) = (tokens.first()?, tokens.last()?);
    Some(Span {
        start: first.span.start,
        end: last.span.end,
    })
}

/// The modifier and the name of a case, from the tokens after its word:
/// `"name"`, `.skip "name"` or `.only "name"`.
fn header(block: &Block, tokens: &[Token]) -> Option<(Option<Span>, Mode, Span)> {
    let (modifier, name) = match tokens {
        [name] => (None, name),
        [dot, word, name] if block.text_of(dot.span) == "." => (Some(word.span), name),
        _ => return None,
    };
    let mode = match modifier.map(|m| block.text_of(m)) {
        None => Mode::Run,
        Some("skip") => Mode::Skip,
        Some("only") => Mode::Only,
        Some(_) => return None,
    };
    (name.kind == TokenKind::String).then_some((modifier, mode, name.span))
}

/// The suites and tests of a suite's body, `tokens`.
fn items(block: &Block, tokens: &[Token]) -> Result<Vec<Item>, Diagnostic> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(word) = tokens.get(i) {
        let w = block.text_of(word.span);
        if w == ";" {
            i += 1;
            continue;
        }
        // the header runs to the `{` of the body, which the parser balanced
        let open = (tokens[i..].iter()).position(|t| block.text_of(t.span) == "{");
        let found = open.filter(|_| WORDS.contains(&w)).and_then(|open| {
            let header = header(block, &tokens[i + 1..i + open])?;
            Some((header, i + open, closing(block, tokens, i + open)?))
        });
        let Some(((modifier, mode, name), open, close)) = found else {
            let message = "a suite holds `test \"name\" { … }` and `suite \"name\" { … }`, maybe with `.skip` or `.only`";
            return Err(malformed(word.span, message));
        };
        let inner = &tokens[open + 1..close];
        let kind = if w == "suite" {
            Kind::Suite(items(block, inner)?)
        } else {
            Kind::Test(range(inner))
        };
        out.push(Item {
            word: Some(word.span),
            modifier,
            mode,
            name,
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
    let names: Vec<String> = (items.iter())
        .map(|i| string_value(block.text_of(i.name)))
        .collect();
    for (inner, name) in items.iter().zip(&names) {
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
    header(block, block.arguments()).map(|(_, _, name)| string_value(block.text_of(name)))
}
