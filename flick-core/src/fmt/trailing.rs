//! Separators at line ends. In a record, record type or record pattern every
//! field that ends a line ends with `;`, including the last one before `}`;
//! a row tail `| r` takes none. In a list, tuple or `exposing (…)` whose
//! closer sits on a later line than its last item, that item ends with `,`.
//! Items sharing a line with the next item or the closer are left as written,
//! so lists on one line keep no trailing separator.

use crate::syntax::cst::{Element, Node};

use super::K;

/// The separator of a list node and whether `child` is one of its items.
fn list(node: &Node, child: &Node) -> Option<&'static str> {
    match node.kind {
        K::RecordExpr | K::RecordUpdate | K::RecordType | K::RecordPat => matches!(
            child.kind,
            K::RecordField | K::RecordTypeField | K::RecordPatField
        )
        .then_some(";"),
        K::ListExpr | K::ListPat | K::Exposing | K::TupleExpr | K::TupleType | K::TuplePat => {
            Some(",")
        }
        _ => None,
    }
}

fn is_trivia(element: &Element) -> bool {
    matches!(element, Element::Token(t) if t.kind.is_trivia())
}

/// Start of the first and end of the last non-trivia token of an element.
fn span(element: &Element) -> Option<(usize, usize)> {
    match element {
        Element::Token(t) if t.kind.is_trivia() => None,
        Element::Token(t) => Some((t.start as usize, t.end as usize)),
        Element::Node(n) => {
            let first = n.children.iter().find_map(span)?.0;
            let last = n.children.iter().rev().find_map(span)?.1;
            Some((first, last))
        }
    }
}

/// Byte offsets after items that end a line without their separator.
fn missing(node: &Node, text: &str, out: &mut Vec<(usize, &'static str)>) {
    let solid: Vec<&Element> = node.children.iter().filter(|c| !is_trivia(c)).collect();
    for (i, element) in solid.iter().enumerate() {
        let Element::Node(child) = element else {
            continue;
        };
        missing(child, text, out);
        let Some(sep) = list(node, child) else {
            continue;
        };
        let next = solid.get(i + 1).copied();
        let separated =
            matches!(next, Some(Element::Token(t)) if matches!(t.kind, K::Comma | K::Semi));
        if let (false, Some((_, end)), Some((start, _))) =
            (separated, span(element), next.and_then(span))
            && text[end..start].contains('\n')
        {
            out.push((end, sep));
        }
    }
}

/// The text with the missing separators inserted, if any are missing and the
/// result is the same program. A separator the parser would read differently
/// is left out.
pub(super) fn add(text: &str, tree: &Node, same_program: impl Fn(&str) -> bool) -> Option<String> {
    let mut inserts = Vec::new();
    missing(tree, text, &mut inserts);
    inserts.sort_unstable();
    let all = insert(text, &inserts);
    if inserts.is_empty() || same_program(&all) {
        return (!inserts.is_empty()).then_some(all);
    }
    inserts.retain(|&one| same_program(&insert(text, &[one])));
    let some = insert(text, &inserts);
    (!inserts.is_empty() && same_program(&some)).then_some(some)
}

fn insert(text: &str, inserts: &[(usize, &str)]) -> String {
    let mut out = String::with_capacity(text.len() + inserts.len());
    let mut seen = 0;
    for &(at, sep) in inserts {
        out.push_str(&text[seen..at]);
        out.push_str(sep);
        seen = at;
    }
    out.push_str(&text[seen..]);
    out
}
