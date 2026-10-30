//! Laying out and writing a document.
//!
//! Both the layout decisions (`fits`) and the printing loop walk the document
//! with an explicit stack instead of recursion, so generated code nested
//! deeper than the call stack allows still prints.

use std::fmt::{self, Write};

use crate::document::{Document, NestKind, NextBreakFitsMode, Node};

/// Where the printer is in its output, all counts 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    /// The layout column: extended grapheme clusters since the start of the
    /// line, indentation included (a tab as its width) and zero-width text
    /// excluded. This is the
    /// column a reader sees and the one the line limit applies to.
    pub column: usize,
    /// The column in UTF-16 code units of everything on the line, zero-width
    /// text included. This is the column JavaScript source maps use.
    pub utf16_column: usize,
}

/// How the printer writes indentation. Nesting is always counted in
/// columns; this only decides which characters fill them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Indentation {
    /// A space per column.
    #[default]
    Spaces,
    /// A tab per `width` columns, then a space per column left over, as
    /// `gofmt` indents Go. When fitting, a tab counts as `width` columns.
    Tabs { width: usize },
}

impl Indentation {
    /// How many tabs, then spaces, fill `columns` of indentation.
    fn split(self, columns: usize) -> (usize, usize) {
        match self {
            Indentation::Spaces => (0, columns),
            Indentation::Tabs { width } => {
                // a tab of no width would never fill anything
                let width = width.max(1);
                (columns / width, columns % width)
            }
        }
    }
}

/// Told where each [`cursor`](crate::DocumentArena::cursor) document lands in
/// the output. Indentation is written lazily, but a cursor at the start of a
/// line already reports the column after it: where the next text will go.
pub trait CursorPositionObserver {
    fn observe(&mut self, mark: usize, position: Position);
}

impl<F: FnMut(usize, Position)> CursorPositionObserver for F {
    fn observe(&mut self, mark: usize, position: Position) {
        self(mark, position);
    }
}

impl Document<'_> {
    /// Lays the document out in lines of at most `limit` columns, where it
    /// can, and returns it as a string.
    pub fn to_pretty_string(self, limit: usize) -> String {
        let mut output = String::new();
        // Writing into a `String` never fails.
        let _ = self.pretty_print(limit, &mut output);
        output
    }

    /// Lays the document out in lines of at most `limit` columns, where it
    /// can, and writes it into `output`.
    pub fn pretty_print(self, limit: usize, output: &mut impl Write) -> fmt::Result {
        self.pretty_print_observed(limit, output, &mut |_, _| {})
    }

    /// Like [`pretty_print`](Self::pretty_print), and reports to `observer`
    /// the position of every cursor document.
    pub fn pretty_print_observed(
        self,
        limit: usize,
        output: &mut impl Write,
        observer: &mut impl CursorPositionObserver,
    ) -> fmt::Result {
        self.pretty_print_with(limit, Indentation::Spaces, output, observer)
    }

    /// Like [`pretty_print_observed`](Self::pretty_print_observed), writing
    /// indentation as `indentation` says.
    pub fn pretty_print_with(
        self,
        limit: usize,
        indentation: Indentation,
        output: &mut impl Write,
        observer: &mut impl CursorPositionObserver,
    ) -> fmt::Result {
        Printer {
            output,
            observer,
            limit,
            indentation,
            line: 0,
            width: 0,
            utf16: 0,
            indent_pending: 0,
            fits_stack: Vec::new(),
        }
        .print(self)
    }
}

/// How the breaks of the document at hand are laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The enclosing group did not fit: its breaks become newlines.
    Broken,
    /// The breaks stay on the line. Nested groups still check whether they
    /// fit, since the check that chose this mode may have stopped early, at a
    /// newline or at a `next_break_fits` break.
    Flat,
    /// Flat, and the whole enclosing group is known to fit, so nested groups
    /// fit too and are not checked again. Without this, every level of a
    /// deeply nested document would measure all the levels below it.
    Fitted,
    /// While measuring, under `next_break_fits`: the next break ends the
    /// line, and `force_break` is ignored.
    ForcedBroken,
    /// While measuring, under a disabled `next_break_fits`: like `Flat`, but
    /// nested `next_break_fits` are ignored.
    ForcedFlat,
}

impl Mode {
    fn is_flat(self) -> bool {
        matches!(self, Mode::Flat | Mode::Fitted | Mode::ForcedFlat)
    }
}

/// The answer to "does it fit on the line?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    No,
    /// It fits up to where the line ends anyway; the rest was not measured.
    UpToBreak,
    /// It fits entirely, measured in full.
    Whole,
}

#[derive(Debug, Clone, Copy)]
struct Item<'a> {
    indent: usize,
    mode: Mode,
    doc: Document<'a>,
}

struct Printer<'a, 'w, W, O> {
    output: &'w mut W,
    observer: &'w mut O,
    limit: usize,
    indentation: Indentation,
    line: usize,
    /// The layout column, see [`Position::column`].
    width: usize,
    /// The UTF-16 column, see [`Position::utf16_column`].
    utf16: usize,
    /// The indentation of the current line, not written yet: it is written
    /// before the first text, so blank lines carry no trailing spaces.
    indent_pending: usize,
    /// Reused by every `fits` call, so measuring allocates nothing.
    fits_stack: Vec<(Mode, Document<'a>)>,
}

impl<'a, W: Write, O: CursorPositionObserver> Printer<'a, '_, W, O> {
    fn print(mut self, doc: Document<'a>) -> fmt::Result {
        // Breaks outside of every group stay on the line: only groups break.
        let mut stack = vec![Item {
            indent: 0,
            mode: Mode::Flat,
            doc,
        }];
        while let Some(Item { indent, mode, doc }) = stack.pop() {
            match doc.0 {
                Node::Empty => {}
                Node::Text { text, width, utf16 } => self.write(text, *width, *utf16)?,
                Node::ZeroWidth { text, utf16 } => self.write(text, 0, *utf16)?,
                Node::Line(count) => self.newlines(*count, indent)?,
                Node::Break {
                    broken,
                    unbroken,
                    width,
                    utf16,
                    flex,
                } => {
                    // A strict break follows its group; a flex break in a
                    // broken group stays on the line if what follows it, up
                    // to the next place a line ends, still fits.
                    let flat = mode.is_flat()
                        || *flex && self.fits(self.width + width, None, &stack) != Fit::No;
                    if flat {
                        self.write(unbroken, *width, *utf16)?;
                    } else {
                        self.write(broken, 0, 0)?;
                        self.newlines(1, indent)?;
                    }
                }
                Node::Concat(docs) => {
                    stack.extend(docs.iter().rev().map(|&doc| Item { indent, mode, doc }));
                }
                Node::Group(inner) => {
                    let mode = if mode == Mode::Fitted {
                        Mode::Fitted
                    } else {
                        match self.fits(self.width, Some(*inner), &[]) {
                            Fit::No => Mode::Broken,
                            Fit::UpToBreak => Mode::Flat,
                            Fit::Whole => Mode::Fitted,
                        }
                    };
                    stack.push(Item {
                        indent,
                        mode,
                        doc: *inner,
                    });
                }
                Node::Nest {
                    indent: by,
                    kind,
                    doc,
                } => {
                    let indent = match kind {
                        NestKind::Increase => indent + by,
                        NestKind::IfBroken if mode == Mode::Broken => indent + by,
                        NestKind::IfBroken => indent,
                        NestKind::Set => *by,
                    };
                    stack.push(Item {
                        indent,
                        mode,
                        doc: *doc,
                    });
                }
                // These only change how a group is measured.
                Node::ForceBroken(inner) | Node::NextBreakFits(inner, _) => stack.push(Item {
                    indent,
                    mode,
                    doc: *inner,
                }),
                Node::Cursor(mark) => self.observer.observe(
                    *mark,
                    Position {
                        line: self.line,
                        column: self.width,
                        utf16_column: self.utf16,
                    },
                ),
            }
        }
        Ok(())
    }

    fn write(&mut self, text: &str, width: usize, utf16: usize) -> fmt::Result {
        const SPACES: &str = "                                ";
        const TABS: &str = "\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t";
        if text.is_empty() {
            return Ok(());
        }
        let (tabs, spaces) = self.indentation.split(self.indent_pending);
        self.indent_pending = 0;
        for (fill, mut count) in [(TABS, tabs), (SPACES, spaces)] {
            while count > 0 {
                let chunk = count.min(fill.len());
                self.output.write_str(&fill[..chunk])?;
                count -= chunk;
            }
        }
        self.output.write_str(text)?;
        self.width += width;
        self.utf16 += utf16;
        Ok(())
    }

    fn newlines(&mut self, count: usize, indent: usize) -> fmt::Result {
        for _ in 0..count {
            self.output.write_char('\n')?;
        }
        self.line += count;
        self.width = indent;
        let (tabs, spaces) = self.indentation.split(indent);
        self.utf16 = tabs + spaces;
        self.indent_pending = indent;
        Ok(())
    }

    /// Whether `first`, then the documents of `rest` from its top, fit on the
    /// line from column `width` on.
    fn fits(&mut self, width: usize, first: Option<Document<'a>>, rest: &[Item<'a>]) -> Fit {
        self.fits_stack.clear();
        self.fits_stack.extend(first.map(|doc| (Mode::Flat, doc)));
        fits(self.limit, width, &mut self.fits_stack, rest)
    }
}

/// Measures documents as if laid out flat, until the line overflows (they do
/// not fit), a newline is reached (what precedes it fits) or nothing is left
/// (everything fits). `stack` holds the documents to measure first, `rest`
/// those that follow them, top last; `rest` is only read.
fn fits<'a>(
    limit: usize,
    mut width: usize,
    stack: &mut Vec<(Mode, Document<'a>)>,
    rest: &[Item<'a>],
) -> Fit {
    let mut rest = rest.iter().rev();
    // Cleared when a `force_break` was skipped: then a nested group measured
    // on its own might not fit, so the answer is no longer `Whole`.
    let mut in_full = true;
    loop {
        if width > limit {
            return Fit::No;
        }
        let Some((mode, doc)) = stack
            .pop()
            .or_else(|| rest.next().map(|item| (item.mode, item.doc)))
        else {
            return if in_full { Fit::Whole } else { Fit::UpToBreak };
        };
        match doc.0 {
            Node::Empty | Node::ZeroWidth { .. } | Node::Cursor(_) => {}
            Node::Text { width: text, .. } => width += text,
            Node::Line(_) => return Fit::UpToBreak,
            Node::Break { width: text, .. } => {
                if !mode.is_flat() {
                    return Fit::UpToBreak;
                }
                width += text;
            }
            Node::Concat(docs) => stack.extend(docs.iter().rev().map(|&doc| (mode, doc))),
            // A group inside a broken one is measured flat: it is that group
            // which must fit, not the broken one around it.
            Node::Group(inner) => {
                let mode = if mode == Mode::Broken {
                    Mode::Flat
                } else {
                    mode
                };
                stack.push((mode, *inner));
            }
            // Indentation only matters after a newline, where measuring stops.
            Node::Nest { doc: inner, .. } => stack.push((mode, *inner)),
            Node::ForceBroken(inner) => {
                if mode != Mode::ForcedBroken {
                    return Fit::No;
                }
                in_full = false;
                stack.push((mode, *inner));
            }
            Node::NextBreakFits(inner, NextBreakFitsMode::Disabled) => {
                stack.push((Mode::ForcedFlat, *inner));
            }
            Node::NextBreakFits(inner, NextBreakFitsMode::Enabled) => {
                let mode = if mode == Mode::ForcedFlat {
                    mode
                } else {
                    Mode::ForcedBroken
                };
                stack.push((mode, *inner));
            }
        }
    }
}
