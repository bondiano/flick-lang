//! A pretty printer: documents describe text and the places where a line may
//! break, and the printer lays them out in lines of a given width.
//!
//! It follows Christian Lindig's "Strictly Pretty" (2000), a strict,
//! linear-time take on Wadler's "A prettier printer", extended with flexible
//! breaks, forced breaks, conditional and absolute nesting, zero-width text
//! and cursor observers.
//!
//! Documents are built in a [`DocumentArena`] and are `Copy` handles into it.
//! A [`group`](Document::group) is laid out on one line if its content fits
//! in the remaining width, and otherwise breaks every
//! [`break_`](DocumentArena::break_) it directly contains; nested groups then
//! decide for themselves, so the outermost groups break first.
//!
//! ```
//! use flick_pretty::{DocumentArena, docvec};
//!
//! let a = &DocumentArena::new();
//! let arguments = a.join([a.text("first"), a.text("second")], a.break_(",", ", "));
//! let indented = docvec![a, a.break_("", ""), arguments].nest(a, 2);
//! let call = docvec![a, "call(", indented, a.break_(",", ""), ")"].group(a);
//! assert_eq!(call.to_pretty_string(80), "call(first, second)");
//! assert_eq!(call.to_pretty_string(10), "call(\n  first,\n  second,\n)");
//! ```
//!
//! Widths are counted in extended grapheme clusters, which is what a reader
//! perceives as characters. Indentation is written lazily, before the next
//! text, so no line ends in whitespace the document did not ask for.

mod arena;
mod document;
mod print;

pub use arena::DocumentArena;
pub use document::{Document, Documentable, NextBreakFitsMode};
pub use print::{CursorPositionObserver, Position};

/// Concatenates documents, or anything [`Documentable`], given after the
/// arena: `docvec![arena, "let ", name, " = ", value]`. Nothing is put
/// between them; only breaks and lines start new lines.
#[macro_export]
macro_rules! docvec {
    ($arena:expr, $($doc:expr),+ $(,)?) => {
        $crate::DocumentArena::concat(
            $arena,
            [$($crate::Documentable::to_doc($doc, $arena)),+],
        )
    };
}

#[cfg(test)]
mod tests;
