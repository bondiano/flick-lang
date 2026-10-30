//! The arena that owns documents, and the constructors of the leaf documents.

use std::fmt::{self, Display, Write};

use typed_arena::Arena;
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{Document, EMPTY, LINE, Node, RAW_NEWLINE};

/// Owns every document built from it; documents are handles that borrow it.
///
/// All memory is freed at once when the arena is dropped, which makes
/// building a document tree a matter of bumping a pointer: there is no heap
/// allocation per document, and owned strings are copied into the arena too.
#[derive(Default)]
pub struct DocumentArena<'a> {
    nodes: Arena<Node<'a>>,
    slices: Arena<Document<'a>>,
    strings: Arena<u8>,
}

/// How many documents one concatenation node holds. Concatenating an
/// iterator builds a chain of such nodes, so it needs no scratch buffer.
const CHUNK: usize = 8;

impl<'a> DocumentArena<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn alloc(&'a self, node: Node<'a>) -> Document<'a> {
        Document(self.nodes.alloc(node))
    }

    pub(crate) fn slice(&'a self, docs: &[Document<'a>]) -> Document<'a> {
        self.alloc(Node::Concat(self.slices.alloc_extend(docs.iter().copied())))
    }

    /// Copies `string` into the arena, so a document can borrow it after the
    /// original is gone.
    pub fn alloc_str(&'a self, string: &str) -> &'a str {
        self.strings.alloc_str(string)
    }

    /// The document that prints nothing.
    pub fn empty(&self) -> Document<'a> {
        EMPTY
    }

    /// Text printed verbatim. A newline inside it starts a new line at column
    /// 0, without the current indentation, as the text's own content.
    pub fn text(&'a self, text: &'a str) -> Document<'a> {
        if !text.contains('\n') {
            return self.line_text(text);
        }
        self.join(
            text.split('\n').map(|line| self.line_text(line)),
            RAW_NEWLINE,
        )
    }

    fn line_text(&'a self, text: &'a str) -> Document<'a> {
        if text.is_empty() {
            return EMPTY;
        }
        let (width, utf16) = measure(text);
        self.alloc(Node::Text { text, width, utf16 })
    }

    pub(crate) fn integer(&'a self, number: impl Display) -> Document<'a> {
        let mut digits = Digits::default();
        // The longest integer, `i128::MIN`, takes 40 bytes: it always fits.
        let _ = write!(digits, "{number}");
        let digits = str::from_utf8(&digits.bytes[..digits.len]).unwrap_or_default();
        self.line_text(self.alloc_str(digits))
    }

    /// Text that is printed but takes no room in the layout, such as an ANSI
    /// colour code or an HTML tag around highlighted code, or a comment that
    /// must not make its group break. It still moves the observed UTF-16
    /// column, since it is in the output. It should not contain newlines.
    pub fn zero_width(&'a self, text: &'a str) -> Document<'a> {
        if text.is_empty() {
            return EMPTY;
        }
        let utf16 = measure(text).1;
        self.alloc(Node::ZeroWidth { text, utf16 })
    }

    /// A mandatory newline. The next line starts at the current indentation,
    /// which is only written once text follows: a line is never left with
    /// trailing spaces.
    pub fn line(&self) -> Document<'a> {
        LINE
    }

    /// `count` mandatory newlines; `lines(2)` leaves one blank line.
    pub fn lines(&'a self, count: usize) -> Document<'a> {
        match count {
            0 => EMPTY,
            1 => LINE,
            _ => self.alloc(Node::Line(count)),
        }
    }

    /// A place where the enclosing group may break the line. When the group
    /// fits, this prints `unbroken`; when it breaks, it prints `broken` and
    /// then a newline at the current indentation. Neither string should
    /// contain a newline.
    ///
    /// `break_("", " ")` is a space or a newline; `break_(",", ", ")` a comma
    /// followed by a space or by a newline.
    pub fn break_(&'a self, broken: &'a str, unbroken: &'a str) -> Document<'a> {
        self.break_node(broken, unbroken, false)
    }

    /// Like [`break_`](Self::break_), but in a broken group each flex break
    /// decides for itself: it stays unbroken when what follows it, up to the
    /// next possible newline, still fits on the line. A run of flex breaks
    /// fills lines the way words fill a paragraph.
    pub fn flex_break(&'a self, broken: &'a str, unbroken: &'a str) -> Document<'a> {
        self.break_node(broken, unbroken, true)
    }

    fn break_node(&'a self, broken: &'a str, unbroken: &'a str, flex: bool) -> Document<'a> {
        let (width, utf16) = measure(unbroken);
        self.alloc(Node::Break {
            broken,
            unbroken,
            width,
            utf16,
            flex,
        })
    }

    /// A document that prints nothing; when the printer reaches it, it calls
    /// the [`CursorPositionObserver`](crate::CursorPositionObserver) with
    /// `mark` and the current position. A code generator puts one before the
    /// code of each source node, with the node's index as the mark, to build
    /// a source map.
    pub fn cursor(&'a self, mark: usize) -> Document<'a> {
        self.alloc(Node::Cursor(mark))
    }

    /// The documents one after another, with nothing in between.
    pub fn concat(&'a self, docs: impl IntoIterator<Item = Document<'a>>) -> Document<'a> {
        // The iterator may itself build documents in this arena, so it is
        // drained in chunks before each allocation rather than streamed into
        // the arena. Each chunk starts with the documents gathered so far.
        let mut docs = docs.into_iter().filter(|doc| !doc.is_nil());
        let mut result = EMPTY;
        loop {
            let mut chunk = [EMPTY; CHUNK];
            let mut len = usize::from(!result.is_nil());
            chunk[0] = result;
            let start = len;
            for doc in docs.by_ref() {
                chunk[len] = doc;
                len += 1;
                if len == CHUNK {
                    break;
                }
            }
            if len == start {
                return result;
            }
            result = if len == 1 {
                chunk[0]
            } else {
                self.slice(&chunk[..len])
            };
            if len < CHUNK {
                return result;
            }
        }
    }

    /// The documents with `separator` between each pair.
    pub fn join(
        &'a self,
        docs: impl IntoIterator<Item = Document<'a>>,
        separator: Document<'a>,
    ) -> Document<'a> {
        let mut first = true;
        self.concat(docs.into_iter().flat_map(|doc| {
            let before = if first { EMPTY } else { separator };
            first = false;
            [before, doc]
        }))
    }
}

/// The width of a text in extended grapheme clusters, and its length in
/// UTF-16 code units.
fn measure(text: &str) -> (usize, usize) {
    if text.is_ascii() {
        (text.len(), text.len())
    } else {
        (text.graphemes(true).count(), text.encode_utf16().count())
    }
}

/// A buffer on the stack to format an integer without a heap allocation.
struct Digits {
    bytes: [u8; 40],
    len: usize,
}

impl Default for Digits {
    fn default() -> Self {
        Self {
            bytes: [0; 40],
            len: 0,
        }
    }
}

impl Write for Digits {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
