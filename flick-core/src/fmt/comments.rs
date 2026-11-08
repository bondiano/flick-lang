//! Comments, found by their position. The tree keeps a comment as trivia of
//! whichever node starts after it, which says little about what it belongs
//! to, so the layout asks for "the comments before this item" and "the
//! comment ending this line" instead, as Gleam's formatter does.

use crate::syntax::cst::Node;

#[derive(Debug, Clone, Copy)]
pub(super) struct Comment<'t> {
    pub text: &'t str,
    pub start: u32,
    /// Only whitespace precedes it on its line.
    pub own_line: bool,
    /// A blank line separates it from what precedes it.
    pub blank: bool,
}

pub(super) struct Comments<'t> {
    text: &'t str,
    list: Vec<Comment<'t>>,
    next: usize,
    /// The end of the last token written: a comment before it was passed
    /// without being written.
    pub cursor: u32,
    /// A comment sits where the layout cannot keep it; the declaration
    /// holding it is left as written.
    pub misplaced: bool,
}

impl<'t> Comments<'t> {
    pub fn new(tree: &Node, text: &'t str) -> Self {
        let list = tree
            .tokens()
            .into_iter()
            .filter(|t| t.kind.is_comment())
            .map(|t| {
                let start = t.start as usize;
                let line = text[..start].rsplit('\n').next().unwrap_or_default();
                // a line comment ends at the line break, its trailing spaces aside
                let body = t.text(text);
                let body = if body.starts_with("//") {
                    body.trim_end()
                } else {
                    body
                };
                Comment {
                    text: body,
                    start: t.start,
                    own_line: line.trim().is_empty(),
                    blank: blank_before(text, t.start),
                }
            })
            .collect();
        Comments {
            text,
            list,
            next: 0,
            cursor: 0,
            misplaced: false,
        }
    }

    pub fn blank_before(&self, at: u32) -> bool {
        blank_before(self.text, at)
    }

    fn peek(&self) -> Option<Comment<'t>> {
        self.list.get(self.next).copied()
    }

    /// Whether a comment starts before `at`.
    pub fn any_before(&self, at: u32) -> bool {
        self.peek().is_some_and(|c| c.start < at)
    }

    /// The comments on their own lines before `before`, to be written on
    /// lines of their own before the item that starts there. It stops at a
    /// comment with code before it on its line: [`inline`](Self::inline)
    /// writes that one, or finds it misplaced.
    pub fn leading(&mut self, before: u32) -> Vec<Comment<'t>> {
        let mut out = Vec::new();
        while let Some(c) = self.peek().filter(|c| c.start < before && c.own_line) {
            self.next += 1;
            if c.start < self.cursor {
                self.misplaced = true;
            }
            out.push(c);
        }
        out
    }

    /// The comments ending the line of the last token written, before
    /// `before`: only whitespace, or the separator `sep` the layout writes
    /// itself, may stand between that token and them, and only whitespace or
    /// a closer after them. A comment with code after it stays for
    /// [`inline`](Self::inline).
    pub fn trailing(&mut self, before: u32, sep: Option<char>) -> Option<&'t str> {
        let mut from = self.cursor as usize;
        let first = self.next;
        while let Some(c) = self.peek().filter(|c| c.start < before && !c.own_line) {
            let gap = self.text[from..c.start as usize].trim();
            let separator = sep.is_some_and(|s| gap.len() == s.len_utf8() && gap.starts_with(s));
            if c.start < self.cursor || !(gap.is_empty() || separator) {
                break;
            }
            self.next += 1;
            from = c.start as usize + c.text.len();
        }
        // a closer after them ends the item too, as in `{ a = 1; /* c */ }`
        let rest = self.text[from..]
            .split('\n')
            .next()
            .unwrap_or_default()
            .trim();
        if !(rest.is_empty() || rest.starts_with([')', ']', '}'])) {
            self.next = first;
        }
        let taken = &self.list[first..self.next];
        match taken {
            [] => None,
            [one] => Some(one.text),
            // a block comment before a line comment on the same line
            [first, .., last] => {
                let end = last.start as usize + last.text.len();
                Some(&self.text[first.start as usize..end])
            }
        }
    }

    /// The comments before a token about to be written. Block comments on
    /// its line stay inline; any other is misplaced.
    pub fn inline(&mut self, before: u32) -> Vec<&'t str> {
        let mut out = Vec::new();
        while let Some(c) = self.peek().filter(|c| c.start < before) {
            self.next += 1;
            let block = c.text.starts_with("/*");
            if c.start < self.cursor || c.own_line || !block {
                self.misplaced = true;
            } else {
                out.push(c.text);
            }
        }
        out
    }

    /// The block comments right after a token ending at `after`, when what
    /// follows them is a token written with no space before it, as `)` or
    /// `,`: they stick to the token, `x /* c */)`.
    pub fn before_closer(&mut self, after: u32) -> Vec<&'t str> {
        let mut from = after as usize;
        let mut end = self.next;
        while let Some(c) = self.list.get(end).filter(|c| {
            !c.own_line
                && c.text.starts_with("/*")
                && self.text[from..c.start as usize].trim().is_empty()
        }) {
            from = c.start as usize + c.text.len();
            end += 1;
        }
        let closer = self.text[from..]
            .trim_start()
            .starts_with([')', ']', '}', ',', ';', '.', '?', '^']);
        if end == self.next || !closer {
            return Vec::new();
        }
        let out = self.list[self.next..end].iter().map(|c| c.text).collect();
        self.next = end;
        self.cursor = u32::try_from(from).unwrap_or(u32::MAX);
        out
    }

    /// Passes the comments before `end` without writing them: they belong to
    /// source text copied as it is.
    pub fn skip(&mut self, end: u32) {
        while self.peek().is_some_and(|c| c.start < end) {
            self.next += 1;
        }
        self.cursor = self.cursor.max(end);
    }

    /// Where the reading stands, to go back after a failed layout.
    pub fn save(&self) -> (usize, u32) {
        (self.next, self.cursor)
    }

    pub fn restore(&mut self, (next, cursor): (usize, u32)) {
        self.next = next;
        self.cursor = cursor;
        self.misplaced = false;
    }
}

/// Whether a blank line lies between `at` and the text before it.
fn blank_before(text: &str, at: u32) -> bool {
    let before = &text[..at as usize];
    let code = before.trim_end().len();
    before[code..].matches('\n').count() >= 2
}
