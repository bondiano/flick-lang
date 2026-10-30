//! The document tree and the combinators that wrap one document in another.

use crate::arena::DocumentArena;

/// A pretty-printable document: a cheap `Copy` handle to a node that lives
/// in a [`DocumentArena`].
///
/// Documents are immutable. Every combinator returns a new handle and leaves
/// its input untouched, so a document can be shared by several parents.
#[derive(Debug, Clone, Copy)]
pub struct Document<'a>(pub(crate) &'a Node<'a>);

/// The empty document; it needs no arena.
pub(crate) const EMPTY: Document<'static> = Document(&Node::Empty);

/// A single mandatory newline; it needs no arena.
pub(crate) const LINE: Document<'static> = Document(&Node::Line(1));

/// A newline that ignores the current indentation, used for the newlines
/// embedded in a text so the following text keeps its column 0.
pub(crate) const RAW_NEWLINE: Document<'static> = Document(&Node::Nest {
    indent: 0,
    kind: NestKind::Set,
    doc: LINE,
});

#[derive(Debug)]
pub(crate) enum Node<'a> {
    Empty,
    /// Text without newlines, printed verbatim. `width` is its number of
    /// extended grapheme clusters, the length a reader sees, and `utf16` its
    /// length in UTF-16 code units; both are measured once, when the text
    /// becomes a document, because the printer reads them many times.
    Text {
        text: &'a str,
        width: usize,
        utf16: usize,
    },
    /// Text that is printed but takes no room in the layout.
    ZeroWidth {
        text: &'a str,
        utf16: usize,
    },
    /// That many mandatory newlines.
    Line(usize),
    /// A place where a line may break: `unbroken` stays on the line, `broken`
    /// is written before the newline. `width` and `utf16` measure `unbroken`.
    Break {
        broken: &'a str,
        unbroken: &'a str,
        width: usize,
        utf16: usize,
        flex: bool,
    },
    /// Documents printed one after another, with nothing in between.
    Concat(&'a [Document<'a>]),
    Group(Document<'a>),
    Nest {
        indent: usize,
        kind: NestKind,
        doc: Document<'a>,
    },
    ForceBroken(Document<'a>),
    NextBreakFits(Document<'a>, NextBreakFitsMode),
    /// Reports the printer's position to the observer under this mark.
    Cursor(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NestKind {
    /// Indent by this much more than the enclosing document.
    Increase,
    /// Indent by this much more, but only when the enclosing group breaks.
    IfBroken,
    /// Indent by exactly this much, whatever the enclosing indentation.
    Set,
}

/// Whether [`Document::next_break_fits`] turns its behaviour on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextBreakFitsMode {
    Enabled,
    Disabled,
}

/// A value that can become a document: text, numbers, characters and
/// documents themselves. It lets the combinators and [`docvec!`](crate::docvec)
/// take plain strings next to documents.
pub trait Documentable<'a> {
    fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a>;
}

impl<'a> Documentable<'a> for Document<'a> {
    fn to_doc(self, _: &'a DocumentArena<'a>) -> Document<'a> {
        self
    }
}

impl<'a> Documentable<'a> for &'a str {
    fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a> {
        arena.text(self)
    }
}

impl<'a> Documentable<'a> for &'a String {
    fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a> {
        arena.text(self)
    }
}

/// Copies the string into the arena, so the `String` itself can be dropped.
impl<'a> Documentable<'a> for String {
    fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a> {
        arena.text(arena.alloc_str(&self))
    }
}

impl<'a> Documentable<'a> for char {
    fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a> {
        arena.text(arena.alloc_str(self.encode_utf8(&mut [0; 4])))
    }
}

macro_rules! integer_documentable {
    ($($int:ty),*) => {$(
        impl<'a> Documentable<'a> for $int {
            fn to_doc(self, arena: &'a DocumentArena<'a>) -> Document<'a> {
                arena.integer(self)
            }
        }
    )*};
}

integer_documentable!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);

impl<'a> Document<'a> {
    pub(crate) fn is_nil(self) -> bool {
        matches!(self.0, Node::Empty)
    }

    /// Makes this document a unit of layout. The printer keeps a group on a
    /// single line, every `break_` inside rendered unbroken, when its content
    /// fits in the remaining width; otherwise the group breaks and so do all
    /// of its own `break_`s.
    ///
    /// Only the group's own content decides, not what follows it on the line
    /// (this is what makes the printer "strict" and linear). Groups nested in a
    /// broken group decide again for themselves, so outer groups break first.
    /// A `break_` outside of every group never breaks.
    #[must_use]
    pub fn group(self, arena: &'a DocumentArena<'a>) -> Self {
        match self.0 {
            // There is nothing for a group around these to decide.
            Node::Empty
            | Node::Text { .. }
            | Node::ZeroWidth { .. }
            | Node::Cursor(_)
            | Node::Group(_) => self,
            _ => arena.alloc(Node::Group(self)),
        }
    }

    /// Indents by `indent` more columns every line that starts inside this
    /// document. Only newlines are affected: the first line keeps whatever
    /// column it starts at.
    #[must_use]
    pub fn nest(self, arena: &'a DocumentArena<'a>, indent: usize) -> Self {
        self.nested(arena, indent, NestKind::Increase)
    }

    /// Like [`nest`](Self::nest), but only when the group around it is broken.
    /// When that group stays flat (its measuring stopped at a mandatory
    /// newline or at a `next_break_fits` break) the lines started inside, for
    /// instance by a nested group that breaks, keep the enclosing indentation.
    /// A value can so be indented under `let x =` when the assignment is
    /// split, while a block hugging the `=` keeps its body one level in.
    #[must_use]
    pub fn nest_if_broken(self, arena: &'a DocumentArena<'a>, indent: usize) -> Self {
        self.nested(arena, indent, NestKind::IfBroken)
    }

    /// Sets the indentation of the lines that start inside this document to
    /// exactly `indent` columns, ignoring the enclosing nesting. With `0` it
    /// puts something at the start of the line in the middle of nested code,
    /// such as a Go `//line` directive.
    #[must_use]
    pub fn set_nesting(self, arena: &'a DocumentArena<'a>, indent: usize) -> Self {
        if self.is_nil() {
            return self;
        }
        arena.alloc(Node::Nest {
            indent,
            kind: NestKind::Set,
            doc: self,
        })
    }

    fn nested(self, arena: &'a DocumentArena<'a>, indent: usize, kind: NestKind) -> Self {
        if indent == 0 || self.is_nil() {
            return self;
        }
        arena.alloc(Node::Nest {
            indent,
            kind,
            doc: self,
        })
    }

    /// Makes every enclosing group that reaches this document while checking
    /// whether it fits break, as if the line were already too long. A group
    /// stops looking at its first newline, so a `force_break` after a
    /// mandatory `line` does not affect it. Flex breaks that would have to
    /// fit this document on their line break too.
    ///
    /// This is how a formatter keeps a list broken because the source had it
    /// broken, or because it contains a comment.
    #[must_use]
    pub fn force_break(self, arena: &'a DocumentArena<'a>) -> Self {
        arena.alloc(Node::ForceBroken(self))
    }

    /// With [`NextBreakFitsMode::Enabled`], an enclosing group checking whether
    /// it fits treats the first break inside this document as the end of its
    /// line: what comes before that break must fit, what comes after is not
    /// measured, and `force_break`s in between are ignored. Printing itself
    /// is unchanged; nested groups still decide for themselves.
    ///
    /// It lets the last argument of a call "hug" the parentheses: in
    /// `f(a, fn() {` the call stays on one line and only the function body
    /// breaks, instead of the call breaking around its arguments.
    ///
    /// [`NextBreakFitsMode::Disabled`] switches off every `Enabled` document
    /// nested inside, so an outer group measures the content in full.
    #[must_use]
    pub fn next_break_fits(self, arena: &'a DocumentArena<'a>, mode: NextBreakFitsMode) -> Self {
        arena.alloc(Node::NextBreakFits(self, mode))
    }

    /// This document followed by `next`.
    #[must_use]
    pub fn append(self, arena: &'a DocumentArena<'a>, next: impl Documentable<'a>) -> Self {
        let next = next.to_doc(arena);
        if next.is_nil() {
            return self;
        }
        if self.is_nil() {
            return next;
        }
        arena.slice(&[self, next])
    }

    /// This document between `open` and `close`.
    #[must_use]
    pub fn surround(
        self,
        arena: &'a DocumentArena<'a>,
        open: impl Documentable<'a>,
        close: impl Documentable<'a>,
    ) -> Self {
        arena.concat([open.to_doc(arena), self, close.to_doc(arena)])
    }

    /// Whether this document prints nothing in any layout. Cursors count as
    /// empty; breaks and lines do not, since they can print a newline.
    pub fn is_empty(self) -> bool {
        // A loop with a worklist rather than recursion: documents can be
        // nested deeper than the call stack allows.
        let mut pending = Vec::new();
        let mut doc = self;
        loop {
            match doc.0 {
                Node::Text { .. } | Node::ZeroWidth { .. } | Node::Line(_) | Node::Break { .. } => {
                    return false;
                }
                Node::Group(inner)
                | Node::Nest { doc: inner, .. }
                | Node::ForceBroken(inner)
                | Node::NextBreakFits(inner, _) => {
                    doc = *inner;
                    continue;
                }
                Node::Concat(docs) => pending.extend(docs.iter().copied()),
                Node::Empty | Node::Cursor(_) => {}
            }
            match pending.pop() {
                Some(next) => doc = next,
                None => return true,
            }
        }
    }
}
