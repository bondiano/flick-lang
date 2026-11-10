//! The ```` ```flick ```` blocks of a Markdown page as the checker reads
//! them. The words after `flick` say what a block must do, and a hidden line,
//! `#` alone or starting with `# `, is compiled without that prefix but left
//! off the page (`docs/flick-blocks.mjs` strips it for the site).

use std::cmp::Reverse;
use std::iter;
use std::ops::Range;

use crate::diagnostics::Edit;
use crate::source::{FileId, Span};
use crate::types::RegionMode;

/// What a block's info string asks of the checker: ```` ```flick ```` must
/// check without errors, ```` ```flick keep ```` too but is never rewritten
/// by fixes (it shows the long form a lint would shorten), ```` ```flick
/// ignore ```` is only parsed, and ```` ```flick fail ```` must report at
/// least one error. The word `live` adds the discipline a block checks
/// under, which a build otherwise opts into (`[build] regions`): a ```` ```flick
/// live ```` block must check without errors under it, and ```` ```flick live
/// fail ```` must report one there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Check,
    Keep,
    Ignore,
    Fail,
}

/// A ```` ```flick ```` block of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub mode: Mode,
    /// Whether the info string asks the live discipline of regions.
    pub live: bool,
    /// The 1-based line of the page where the block's code starts.
    pub line: usize,
    /// The code as it is compiled: its hidden lines without their prefix.
    pub text: String,
    /// The lines of the code, then the closing fence.
    lines: Vec<Line>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    /// Where the line starts in the page.
    page: usize,
    /// Where it starts in the block's text.
    text: usize,
    /// The bytes of its hidden prefix: 2 for `# `, 1 for `#` alone, 0 when shown.
    hidden: usize,
}

/// The mode of a block whose info string is ```` ```flick ```` and `words`.
fn mode(words: &str) -> Mode {
    words
        .split_whitespace()
        .fold(Mode::Check, |mode, word| match word {
            "keep" => Mode::Keep,
            "ignore" => Mode::Ignore,
            "fail" => Mode::Fail,
            _ => mode,
        })
}

/// Whether the words ask the live discipline: ```` ```flick live ````.
fn live(words: &str) -> bool {
    words.split_whitespace().any(|word| word == "live")
}

/// The bytes of the hidden prefix of a line of code.
fn hidden(line: &str) -> usize {
    // shortcut: a raw string line starting with `# ` would be hidden too; no example has one, mark such a block `ignore`
    if line == "#" {
        1
    } else if line.starts_with("# ") {
        2
    } else {
        0
    }
}

/// The closed ```` ```flick ```` blocks of `page`, in order.
pub fn blocks(page: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    let mut at = 0;
    for (index, raw) in page.split_inclusive('\n').enumerate() {
        let line = raw
            .strip_suffix('\n')
            .map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l));
        let fence = line.trim_start();
        match &mut open {
            None => {
                let info = fence.strip_prefix("```flick");
                if let Some(words) = info.filter(|w| w.is_empty() || w.starts_with(' ')) {
                    open = Some(Block {
                        mode: mode(words),
                        live: live(words),
                        line: index + 2,
                        text: String::new(),
                        lines: Vec::new(),
                    });
                }
            }
            Some(block) => {
                let hidden = if fence == "```" { 0 } else { hidden(line) };
                let text = block.text.len();
                block.lines.push(Line {
                    page: at,
                    text,
                    hidden,
                });
                if fence == "```" {
                    blocks.extend(open.take());
                } else {
                    block.text.push_str(&line[hidden..]);
                    block.text.push('\n');
                }
            }
        }
        at += raw.len();
    }
    blocks
}

impl Block {
    /// The discipline the block is checked under: a `live` block asks the
    /// live discipline, the one a build opts into with `[build] regions`.
    pub fn regions(&self) -> RegionMode {
        if self.live {
            RegionMode::Live
        } else {
            RegionMode::Captures
        }
    }

    /// Whether `flick check --fix` may rewrite the block. The fixes of a
    /// block are those of its own discipline; a `live` block is left as
    /// written, as only the corpus test checks it.
    pub fn fixable(&self) -> bool {
        self.mode == Mode::Check && !self.live
    }

    /// The bytes of the page that hold the block's code.
    pub fn range(&self) -> Range<usize> {
        let fence = self.lines.last().expect("a closed block has its fence");
        self.lines[0].page..fence.page
    }

    /// The index of the line of offset `at` of the block's text.
    fn line_of(&self, at: usize) -> usize {
        self.lines.partition_point(|l| l.text <= at) - 1
    }

    /// The offset in the page of offset `at` of the block's text.
    pub fn page_offset(&self, at: usize) -> usize {
        let line = self.lines[self.line_of(at)];
        line.page + line.hidden + at - line.text
    }

    /// `span` of the block's text as a span of `page`, the page's file.
    pub fn page_span(&self, span: Span, page: FileId) -> Span {
        let offset = |at: u32| {
            let at = self.page_offset(at as usize);
            u32::try_from(at).expect("page offsets fit in u32")
        };
        Span::new(page, offset(span.start), offset(span.end))
    }

    /// The bytes for [`Self::range`] of `page` once `edits` of the block's
    /// text, which do not overlap, are made. A line left as it was keeps its
    /// bytes; any other is hidden when the line of its first byte is, a
    /// replacement's bytes being of the line their edit starts on.
    pub fn restore(&self, page: &str, edits: &[&Edit]) -> String {
        let mut edits = edits.to_vec();
        edits.sort_by_key(|e| Reverse((e.span.start, e.span.end)));
        let mut text = self.text.clone();
        // the line each byte comes from, and whether it is still that line's own
        let mut from: Vec<(usize, bool)> = self
            .lines
            .windows(2)
            .enumerate()
            .flat_map(|(i, w)| iter::repeat_n((i, true), w[1].text - w[0].text))
            .collect();
        for edit in edits {
            let range = edit.span.range();
            let line = self.line_of(range.start);
            text.replace_range(range.clone(), &edit.replacement);
            from.splice(range, iter::repeat_n((line, false), edit.replacement.len()));
        }
        let mut out = String::with_capacity(text.len());
        let mut at = 0;
        for code in text.split_inclusive('\n') {
            let (i, own) = from[at];
            at += code.len();
            let line = self.lines[i];
            if own && code == &self.text[line.text..self.lines[i + 1].text] {
                out.push_str(&page[line.page..self.lines[i + 1].page]);
                continue;
            }
            let shown = code.strip_suffix('\n').unwrap_or(code);
            if line.hidden > 0 {
                out.push_str(if shown.is_empty() { "#" } else { "# " });
            }
            out.push_str(shown);
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;

    const PAGE: &str = "# Title\n\n```flick\n# let a = 1\n#\nlet b = a\n```\n\n```flick ignore\nnot yet\n```\n\n  ```flick fail extra\n  let c : Int = \"\"\n  ```\n\n```flick\nunclosed\n";

    fn edit(start: u32, end: u32, replacement: &str) -> Edit {
        let file = SourceMap::default().add("x.md", "");
        Edit {
            span: Span::new(file, start, end),
            replacement: replacement.to_owned(),
        }
    }

    #[test]
    fn blocks_have_their_mode_line_and_compiled_text() {
        let found: Vec<_> = blocks(PAGE)
            .into_iter()
            .map(|b| (b.mode, b.line, b.text))
            .collect();
        assert_eq!(
            found,
            [
                (Mode::Check, 4, "let a = 1\n\nlet b = a\n".to_owned()),
                (Mode::Ignore, 10, "not yet\n".to_owned()),
                (Mode::Fail, 14, "  let c : Int = \"\"\n".to_owned()),
            ]
        );
        // a word that only starts like `flick` opens no block
        assert!(blocks("```flickr\nx\n```\n").is_empty());
        let keep = blocks("```flick keep title=\"a.fl\"\nx\n```\n");
        assert_eq!(keep[0].mode, Mode::Keep);
        let crlf = &blocks("```flick\r\n# a\r\nb\r\n```\r\n")[0];
        assert_eq!(crlf.text, "a\nb\n");
        assert_eq!(crlf.range(), 10..18);
    }

    /// A block says which discipline of regions it checks under: the word
    /// `live`, which leaves the mode to `fail`, `keep` or `ignore`.
    #[test]
    fn a_block_may_ask_the_live_discipline() {
        assert_eq!(blocks(PAGE)[0].regions(), RegionMode::Captures);
        for (words, mode) in [
            ("live", Mode::Check),
            ("live fail", Mode::Fail),
            ("live keep", Mode::Keep),
            ("live ignore", Mode::Ignore),
        ] {
            let block = &blocks(&format!("```flick {words}\nx\n```\n"))[0];
            assert!(block.live, "{words}");
            assert_eq!(block.mode, mode, "{words}");
            assert_eq!(block.regions(), RegionMode::Live, "{words}");
            assert!(!block.fixable(), "{words}");
        }
        let plain = &blocks("```flick keep\nx\n```\n")[0];
        assert_eq!(plain.regions(), RegionMode::Captures);
        assert!(!plain.fixable());
        assert!(blocks("```flick\nx\n```\n")[0].fixable());
    }

    #[test]
    fn offsets_of_the_text_map_into_the_page() {
        let block = &blocks(PAGE)[0];
        assert_eq!(&PAGE[block.range()], "# let a = 1\n#\nlet b = a\n");
        let page = |at| &PAGE[block.page_offset(at)..];
        // past the 2 bytes of `# `
        assert!(page(4).starts_with("a = 1\n#"));
        // the empty line `#` alone compiles to
        assert!(page(10).starts_with("\nlet b"));
        assert!(page(15).starts_with("b = a\n```"));
        // the end of the text is the closing fence
        assert!(page(block.text.len()).starts_with("```"));
        let file = SourceMap::default().add("x.md", PAGE);
        let span = block.page_span(Span::new(file, 4, 5), file);
        assert_eq!(&PAGE[span.range()], "a");
    }

    #[test]
    fn restore_keeps_lines_left_as_they_were_and_hides_new_lines_of_hidden_ones() {
        let page = "```flick\n# let a = 1\n# \nlet b = a\n# let c = b\n```\n";
        let block = &blocks(page)[0];
        assert_eq!(block.text, "let a = 1\n\nlet b = a\nlet c = b\n");
        let restore = |edits: &[Edit]| block.restore(page, &edits.iter().collect::<Vec<_>>());
        assert_eq!(restore(&[]), &page[block.range()], "`# ` keeps its space");
        // on one line, shown and hidden
        assert_eq!(
            restore(&[edit(4, 5, "x"), edit(15, 16, "y")]),
            "# let x = 1\n# \nlet y = a\n# let c = b\n"
        );
        // a new line takes the mark of the line its edit starts on
        assert_eq!(
            restore(&[edit(9, 9, "\nlet z = 2\n")]),
            "# let a = 1\n# let z = 2\n#\n# \nlet b = a\n# let c = b\n"
        );
        // lines an edit joins take the mark of the first
        assert_eq!(restore(&[edit(19, 29, "")]), "# let a = 1\n# \nlet b = b\n");
        // a line deleted whole leaves the next one as it was
        assert_eq!(
            restore(&[edit(11, 21, "")]),
            "# let a = 1\n# \n# let c = b\n"
        );
    }
}
