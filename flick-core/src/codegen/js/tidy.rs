//! Readability passes over the generated JS, before source markers become
//! mappings: parentheses that change nothing go in both profiles, and in
//! debug a line wider than [`WIDTH`] breaks at its widest bracket group.
//! Strings, comments and markers are never touched.

use std::iter::repeat_n;

use super::{MARK_END, MARK_START};
use crate::ir::BinOp;
use crate::numeric::{Arith, Num};

/// The width a debug line breaks above, in characters.
const WIDTH: usize = 100;

/// The code characters of `text`: its brackets matched, strings, comments
/// and markers marked so that nothing reads into them.
struct Scan {
    chars: Vec<char>,
    /// The matching bracket of each bracket in code.
    matches: Vec<Option<usize>>,
    /// Whether a character is code, not a string, comment or marker.
    code: Vec<bool>,
    /// Whether a character belongs to a source marker.
    marker: Vec<bool>,
}

impl Scan {
    fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let mut matches = vec![None; chars.len()];
        let mut code = vec![true; chars.len()];
        let mut marker = vec![false; chars.len()];
        let mut open = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let start = i;
            match chars[i] {
                MARK_START => {
                    while i < chars.len() && chars[i] != MARK_END {
                        i += 1;
                    }
                }
                quote @ ('"' | '\'' | '`') => {
                    i += 1;
                    while i < chars.len() && chars[i] != quote {
                        i += if chars[i] == '\\' { 2 } else { 1 };
                    }
                }
                '/' if chars.get(i + 1) == Some(&'*') => {
                    while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                        i += 1;
                    }
                    i += 1;
                }
                '/' if chars.get(i + 1) == Some(&'/') => {
                    while i < chars.len() && chars[i] != '\n' {
                        i += 1;
                    }
                }
                '(' | '[' | '{' => open.push(i),
                ')' | ']' | '}' => {
                    if let Some(o) = open.pop() {
                        matches[o] = Some(i);
                        matches[i] = Some(o);
                    }
                }
                _ => {}
            }
            if i > start {
                let end = (i + 1).min(chars.len());
                code[start..end].iter_mut().for_each(|c| *c = false);
                if chars[start] == MARK_START {
                    marker[start..end].iter_mut().for_each(|m| *m = true);
                }
            }
            i += 1;
        }
        Scan {
            chars,
            matches,
            code,
            marker,
        }
    }

    /// The code character before `i`, skipping whitespace and markers.
    fn before(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&k| self.significant(k))
    }

    /// The code character after `i`, skipping whitespace and markers.
    fn after(&self, i: usize) -> Option<usize> {
        (i + 1..self.chars.len()).find(|&k| self.significant(k))
    }

    /// A string is significant, a marker or whitespace is not.
    fn significant(&self, k: usize) -> bool {
        !self.marker[k] && !self.chars[k].is_whitespace()
    }

    /// Whether a top-level comma separates `(open, close)`.
    fn has_comma(&self, open: usize, close: usize) -> bool {
        let mut k = open + 1;
        while k < close {
            if self.code[k] {
                match self.chars[k] {
                    ',' => return true,
                    '(' | '[' | '{' => k = self.matches[k].unwrap_or(k),
                    _ => {}
                }
            }
            k += 1;
        }
        false
    }

    /// The identifier ending at `end`.
    fn word(&self, end: usize) -> String {
        let mut start = end;
        while start > 0 && is_ident(self.chars[start - 1]) {
            start -= 1;
        }
        self.chars[start..=end].iter().collect()
    }

    /// The word ending at `end` when it can be a keyword: a property such as
    /// `dict.return` is not one.
    fn keyword(&self, end: usize) -> String {
        let word = self.word(end);
        let start = end + 1 - word.chars().count();
        if start > 0 && self.chars[start - 1] == '.' {
            String::new()
        } else {
            word
        }
    }

    fn starts_with(&self, at: usize, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(k, c)| self.chars.get(at + k) == Some(&c))
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// `js` without the parentheses that group an expression where the
/// grammar already delimits one: an argument, an element, a value, an
/// initializer, a `return`, an arrow body or a statement.
pub(super) fn parens(js: &str) -> String {
    let scan = Scan::new(js);
    let mut drop = vec![false; scan.chars.len()];
    for (open, &c) in scan.chars.iter().enumerate() {
        if c != '(' || !scan.code[open] {
            continue;
        }
        let Some(close) = scan.matches[open] else {
            continue;
        };
        if redundant(&scan, open, close) {
            drop[open] = true;
            drop[close] = true;
        }
    }
    scan.chars
        .iter()
        .zip(drop)
        .filter_map(|(&c, d)| (!d).then_some(c))
        .collect()
}

fn redundant(scan: &Scan, open: usize, close: usize) -> bool {
    let next = scan.after(close).map(|k| scan.chars[k]);
    if let Some(k) = scan.after(close)
        && scan.starts_with(k, "=>")
    {
        return false;
    }
    if !matches!(next, None | Some(')' | ',' | ']' | ';' | '}')) || scan.has_comma(open, close) {
        return false;
    }
    let Some(first) = scan.after(open).filter(|&k| k < close) else {
        return false;
    };
    let block_like = scan.chars[first] == '{'
        || ["function", "class", "let ", "async"]
            .iter()
            .any(|w| scan.starts_with(first, w));
    let Some(prev) = scan.before(open) else {
        return next == Some(';') && !block_like;
    };
    match scan.chars[prev] {
        '(' | ',' | '[' | ':' => true,
        ';' | '{' | '}' => next == Some(';') && !block_like,
        '>' => prev > 0 && scan.chars[prev - 1] == '=' && !block_like,
        '=' => prev > 0 && scan.chars[prev - 1].is_whitespace(),
        c if is_ident(c) => matches!(scan.keyword(prev).as_str(), "return" | "throw"),
        _ => false,
    }
}

/// Debug: the code with its bracket groups laid out the way a person would.
/// A group that fits on its line stays there; a wider one, or one whose
/// elements span lines, puts each element on a line of its own. A group of
/// one element spanning lines (`f(() => {…})`) hugs its brackets. Block
/// braces keep the lines the code generator gave them.
pub(super) fn layout(js: &str) -> String {
    let scan = Scan::new(js);
    let mut out = String::with_capacity(js.len() + js.len() / 4);
    Layout {
        scan: &scan,
        out: &mut out,
    }
    .emit(0, scan.chars.len(), 0);
    out
}

struct Layout<'a> {
    scan: &'a Scan,
    out: &'a mut String,
}

impl Layout<'_> {
    /// Emits `start..end`, each line shifted by `shift` columns.
    fn emit(&mut self, start: usize, end: usize, shift: isize) {
        let chars = &self.scan.chars;
        let mut k = start;
        while k < end {
            let c = chars[k];
            if c == '\n' && self.scan.code[k] {
                self.out.push('\n');
                let mut spaces = 0;
                while k + 1 < end && chars[k + 1] == ' ' {
                    spaces += 1;
                    k += 1;
                }
                let indent = usize::try_from(spaces + shift).unwrap_or(0);
                self.out.extend(repeat_n(' ', indent));
                k += 1;
                continue;
            }
            if self.scan.code[k]
                && matches!(c, '(' | '[' | '{')
                && let Some(close) = self.scan.matches[k].filter(|&m| m < end)
                && self.breaks(k, close)
            {
                self.group(k, close);
                k = close + 1;
                continue;
            }
            self.out.push(c);
            k += 1;
        }
    }

    /// Whether the group `open..=close` goes one element per line.
    fn breaks(&self, open: usize, close: usize) -> bool {
        let scan = self.scan;
        if close == open + 1 || !self.is_list(open) {
            return false;
        }
        let inner = open + 1..close;
        let flat = !inner.clone().any(|k| scan.chars[k] == '\n');
        if flat {
            let text: String = scan.chars[open..=close].iter().collect();
            self.column() + width(&text) > WIDTH
        } else {
            scan.has_comma(open, close)
        }
    }

    /// Whether the bracket at `open` holds a list: arguments, elements or
    /// fields, not a block, a statement header or a `for`.
    fn is_list(&self, open: usize) -> bool {
        let scan = self.scan;
        let close = scan.matches[open].unwrap_or(open);
        if (open + 1..close)
            .any(|k| scan.code[k] && scan.chars[k] == ';' && depth_zero(scan, open, k))
        {
            return false;
        }
        if scan.chars[open] != '{' {
            return true;
        }
        let Some(prev) = scan.before(open) else {
            return false;
        };
        match scan.chars[prev] {
            '(' | ',' | '[' | '?' => true,
            '=' => prev > 0 && scan.chars[prev - 1].is_whitespace(),
            ':' => {
                // a label or a `case` opens a block
                let line_start = scan.chars[..prev]
                    .iter()
                    .rposition(|&c| c == '\n')
                    .map_or(0, |n| n + 1);
                let head: String = scan.chars[line_start..prev].iter().collect();
                let head = head.trim();
                !(head.starts_with("case ") || head == "default" || head.chars().all(is_ident))
            }
            c if is_ident(c) => scan.keyword(prev) == "return",
            _ => false,
        }
    }

    /// Emits the group `open..=close` one element per line.
    fn group(&mut self, open: usize, close: usize) {
        let scan = self.scan;
        let indent = self.indent();
        let mut items = Vec::new();
        let mut start = open + 1;
        let mut k = open + 1;
        while k < close {
            if scan.code[k] {
                match scan.chars[k] {
                    ',' => {
                        items.push((start, k));
                        start = k + 1;
                    }
                    '(' | '[' | '{' => k = scan.matches[k].unwrap_or(k),
                    _ => {}
                }
            }
            k += 1;
        }
        items.push((start, close));
        self.out.push(scan.chars[open]);
        let last = items.len() - 1;
        for (n, (mut a, mut b)) in items.into_iter().enumerate() {
            while a < b && scan.chars[a].is_whitespace() {
                a += 1;
            }
            while b > a && scan.chars[b - 1].is_whitespace() {
                b -= 1;
            }
            self.out.push('\n');
            self.out.extend(repeat_n(' ', indent + 2));
            // the element's later lines move with its first one
            let shift = isize::try_from(indent + 2).unwrap_or(0)
                - isize::try_from(source_indent(scan, a)).unwrap_or(0);
            self.emit(a, b, shift);
            if n < last {
                self.out.push(',');
            }
        }
        self.out.push('\n');
        self.out.extend(repeat_n(' ', indent));
        self.out.push(scan.chars[close]);
    }

    /// The column the output is at.
    fn column(&self) -> usize {
        width(self.out.rsplit('\n').next().unwrap_or(""))
    }

    /// The indentation of the output's current line.
    fn indent(&self) -> usize {
        let line = self.out.rsplit('\n').next().unwrap_or("");
        line.len() - line.trim_start_matches(' ').len()
    }
}

/// Whether `k` is directly inside the group opened at `open`.
fn depth_zero(scan: &Scan, open: usize, k: usize) -> bool {
    let mut j = open + 1;
    while j < k {
        if scan.code[j] && matches!(scan.chars[j], '(' | '[' | '{') {
            match scan.matches[j] {
                Some(m) if m >= k => return false,
                Some(m) => j = m,
                None => return false,
            }
        }
        j += 1;
    }
    true
}

/// The indentation of the source line holding `k`.
fn source_indent(scan: &Scan, k: usize) -> usize {
    let start = scan.chars[..k]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |n| n + 1);
    scan.chars[start..]
        .iter()
        .take_while(|&&c| c == ' ')
        .count()
}

/// The width of `text` as seen: markers take none.
fn width(text: &str) -> usize {
    let mut n = 0;
    let mut marker = false;
    for c in text.chars() {
        match c {
            MARK_START => marker = true,
            MARK_END => marker = false,
            _ if !marker => n += 1,
            _ => {}
        }
    }
    n
}

/// `js` without the parentheses around all of it, its markers kept.
pub(super) fn unwrap(js: &str) -> String {
    let scan = Scan::new(js);
    let Some(open) = (0..scan.chars.len()).find(|&k| scan.significant(k)) else {
        return js.to_owned();
    };
    let last = scan.chars.len() - 1;
    if scan.chars[open] != '(' || scan.matches[open] != Some(last) {
        return js.to_owned();
    }
    scan.chars
        .iter()
        .enumerate()
        .filter_map(|(k, &c)| (k != open && k != last).then_some(c))
        .collect()
}

/// The JS precedence of an operator as emitted; a call form binds tightest.
#[allow(
    clippy::match_same_arms,
    reason = "the forms of `Int` come before the JS operators"
)]
pub(super) fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 3,
        BinOp::And => 4,
        BinOp::Same | BinOp::NotSame => 8,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 9,
        // `l + r | 0`
        BinOp::Arith(Arith::Add | Arith::Sub, Num::Int) => 5,
        // a runtime or `Math` call
        BinOp::Arith(_, Num::Int) | BinOp::Arith(Arith::Div | Arith::Rem, Num::BigInt) => 17,
        BinOp::Concat | BinOp::Arith(Arith::Add | Arith::Sub, _) => 11,
        BinOp::Arith(Arith::Mul | Arith::Div | Arith::Rem, _) => 12,
        BinOp::Arith(Arith::Pow, _) => 13,
        BinOp::Eq | BinOp::NotEq | BinOp::ListConcat => 17,
    }
}

/// The precedence an operand of `op` must exceed to need no parentheses:
/// that of the JS operator it is written beside.
pub(super) fn operand_precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Arith(Arith::Add | Arith::Sub, Num::Int) => 11,
        _ => precedence(op),
    }
}

/// Whether `js` is a call, `f(x)` or `f(x)(y)`, that can be called again
/// without parentheses: no arrow or operator outside its brackets.
pub(super) fn is_call(js: &str) -> bool {
    let scan = Scan::new(js);
    let Some(last) = (0..scan.chars.len()).rev().find(|&k| scan.significant(k)) else {
        return false;
    };
    let Some(open) = scan.matches[last].filter(|_| scan.chars[last] == ')') else {
        return false;
    };
    let callee = scan
        .before(open)
        .is_some_and(|p| matches!(scan.chars[p], ')' | ']') || is_ident(scan.chars[p]));
    let mut k = 0;
    while k < open {
        if scan.code[k] {
            match scan.chars[k] {
                '(' | '[' | '{' => k = scan.matches[k].unwrap_or(k),
                c if !(is_ident(c) || c == '.' || c == ')' || c == ']') => return false,
                _ => {}
            }
        }
        k += 1;
    }
    callee
}

#[cfg(test)]
mod tests {
    use super::{is_call, layout, parens};

    #[test]
    fn parens_go_only_where_the_grammar_delimits_the_expression() {
        let cases = [
            ("return (a + b);", "return a + b;"),
            ("const x = (yield* f(y));", "const x = yield* f(y);"),
            ("(yield* g);", "yield* g;"),
            ("f((a + b), [(c)], { k: (d) });", "f(a + b, [c], { k: d });"),
            ("const f = (x) => (x + 1);", "const f = (x) => x + 1;"),
            // an object body, a comma expression, a member or a call keep them
            (
                "const f = (x) => ({ a: x });",
                "const f = (x) => ({ a: x });",
            ),
            ("if ((n = p, true)) {}", "if ((n = p, true)) {}"),
            ("return (a + b).length;", "return (a + b).length;"),
            ("return (() => 1)();", "return (() => 1)();"),
            ("return a * (b + c);", "return a * (b + c);"),
            ("const s = \"(x)\";", "const s = \"(x)\";"),
            // a property named like a keyword is called, not returned
            ("f(d.return(20));", "f(d.return(20));"),
        ];
        for (js, tidy) in cases {
            assert_eq!(parens(js), tidy, "{js}");
        }
    }

    #[test]
    fn a_call_is_called_again_without_parentheses() {
        assert!(is_call("f(a)(b)"));
        assert!(is_call("$m.g({ on: (x) => y })"));
        assert!(!is_call("(x) => f(x)"));
        assert!(!is_call("a + f(b)"));
    }

    #[test]
    fn a_wide_group_puts_each_element_on_a_line() {
        let wide = format!(
            "  return f([{}, g(() => {{\n    return 1;\n  }})]);",
            "\"x\"".repeat(40)
        );
        let out = layout(&wide);
        assert!(out.starts_with("  return f([\n    \"x\""), "{out}");
        // the callback's lines move with it
        assert!(
            out.contains("    g(() => {\n      return 1;\n    })\n  ]);"),
            "{out}"
        );
        assert_eq!(layout("return f(a, b);"), "return f(a, b);");
    }
}
