//! Top-level recursion that is not declared: a function that uses itself
//! without `let rec`, functions that use each other outside one `rec`
//! group, and values that recurse at all.

use std::collections::VecDeque;

use super::Item;
use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::source::Span;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;

impl Checker<'_> {
    /// Reports recursion that is not declared with `rec`.
    pub(super) fn recursion(&mut self) {
        let n = self.top.items.len();
        let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(from, to, _) in &self.top.edges {
            if !next[from].contains(&to) {
                next[from].push(to);
            }
        }
        let mut reported = vec![false; n];
        // Multi-item invalid cycles first, so a function that is both
        // self-recursive and mutually recursive is reported as needing one
        // `rec { … }` group rather than `let rec`.
        for start in 0..n {
            if reported[start] {
                continue;
            }
            let Some(cycle) =
                shortest_invalid_cycle(start, &next, |u, v| invalid_edge(&self.top.items, u, v))
            else {
                continue;
            };
            for &i in &cycle {
                reported[i] = true;
            }
            self.report_cycle(&cycle);
        }
        // Single-item invalid self-cycles (`let f = ... f ...` or recursive
        // values not already reported at their `let rec` declaration).
        for start in 0..n {
            if reported[start]
                || !next[start].contains(&start)
                || !invalid_edge(&self.top.items, start, start)
            {
                continue;
            }
            if self.top.items[start].rec_value_reported {
                continue;
            }
            reported[start] = true;
            self.report_cycle(&[start]);
        }
    }

    fn report_cycle(&mut self, cycle: &[usize]) {
        let items = &self.top.items;
        let start = cycle[0];
        let after = cycle.get(1).copied().unwrap_or(start);
        let span = self
            .top
            .edges
            .iter()
            .find(|(f, t, _)| *f == start && *t == after)
            .map(|e| e.2)
            .expect("an edge of the cycle");
        let names: Vec<String> = cycle
            .iter()
            .map(|&i| format!("`{}`", items[i].names.join("`, `")))
            .collect();
        let values = cycle.iter().any(|&i| !items[i].function);
        let (code, message) = match (values, &names[..]) {
            (true, _) => (
                codes::T012,
                format!(
                    "{} would be evaluated while being defined; only functions can be recursive",
                    names.join(" and ")
                ),
            ),
            (false, [one]) => (
                codes::T017,
                format!("{one} uses itself: write `let rec` for a recursive function"),
            ),
            (false, _) => (
                codes::T017,
                format!(
                    "{} use each other: define mutually recursive functions in one `rec {{ … }}` group",
                    names.join(", ")
                ),
            ),
        };
        let mut diagnostic = Diagnostic::error(code, message, span);
        if let ([only], false) = (cycle, values) {
            let let_kw = items[*only]
                .decl
                .child_tokens()
                .find(|t| t.kind == K::LetKw);
            if let Some(kw) = let_kw {
                diagnostic = diagnostic.with_fix(|| {
                    (
                        "write `let rec`",
                        vec![Edit {
                            span: Span::new(span.file, kw.end, kw.end),
                            replacement: " rec".to_owned(),
                        }],
                    )
                });
            }
        }
        self.diagnostics.push(diagnostic);
    }
}

/// Whether `from -> to` is NOT declared recursion within a single `rec` group
/// of functions. Every invalid cycle contains at least one such edge, and any
/// cycle containing such an edge is invalid.
fn invalid_edge(items: &[Item<'_>], from: usize, to: usize) -> bool {
    !(items[from].function
        && items[to].function
        && items[from].group.is_some()
        && items[from].group == items[to].group)
}

/// Shortest simple cycle `start -> v -> … -> start` whose first step
/// `start -> v` is an invalid edge to another item (`v != start`).
fn shortest_invalid_cycle(
    start: usize,
    next: &[Vec<usize>],
    invalid: impl Fn(usize, usize) -> bool,
) -> Option<Vec<usize>> {
    let mut previous: Vec<Option<usize>> = vec![None; next.len()];
    let mut queue = VecDeque::new();
    for &v in &next[start] {
        if v != start && invalid(start, v) {
            previous[v] = Some(start);
            queue.push_back(v);
        }
    }
    while let Some(node) = queue.pop_front() {
        for &to in &next[node] {
            if to == start {
                let mut path = vec![node];
                while let Some(p) = previous[*path.last().expect("non-empty")] {
                    path.push(p);
                }
                path.reverse();
                return Some(path);
            }
            if previous[to].is_none() && to != start {
                previous[to] = Some(node);
                queue.push_back(to);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::diagnostics::{Diagnostic, Severity, codes};
    use crate::source::SourceMap;

    fn errors(body: &str) -> Vec<Diagnostic> {
        let text = format!("module App.CycleProbe exposing (..)\n\n{body}");
        let mut sources = SourceMap::default();
        let file = sources.add("CycleProbe.fl", text);
        crate::check(&sources, file)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect()
    }

    #[test]
    fn issue_686_self_and_mutually_recursive_let_rec_reports_single_t017() {
        let diagnostics = errors(
            "\
f : List Int -> Result String Int
let rec f xs =
  match xs with
  | [] -> Err \"e\"
  | [x, ...rest] ->
    match f rest with
    | Err e -> Err e
    | Ok n -> g (n + x)

g : Int -> Result String Int
let rec g n =
  match g 0 with
  | Err e -> Err e
  | Ok m -> f [n + m]
",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T017);
        assert!(
            diagnostics[0].message.contains("`f`, `g` use each other"),
            "{}",
            diagnostics[0].message
        );
        assert!(diagnostics[0].fixes.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn single_self_recursive_let_keeps_quick_fix() {
        let body = "\
loop : Int -> Int
let loop n = loop n
";
        let text = format!("module App.CycleProbe exposing (..)\n\n{body}");
        let mut sources = SourceMap::default();
        let file = sources.add("CycleProbe.fl", text.clone());
        let diagnostics: Vec<_> = crate::check(&sources, file)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T017);
        assert!(
            diagnostics[0].message.contains("`loop` uses itself"),
            "{}",
            diagnostics[0].message
        );
        assert_eq!(diagnostics[0].fixes.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].fixes[0].message, "write `let rec`");
        let edit = &diagnostics[0].fixes[0].edits[0];
        let mut fixed = text;
        fixed.replace_range(edit.span.range(), &edit.replacement);
        assert!(fixed.contains("let rec loop n = loop n"), "{fixed}");
    }

    #[test]
    fn figure_eight_across_groups_reports_simple_cycle_without_duplicate_names() {
        let diagnostics = errors(
            "\
rec {
  a : Int -> Int
  let a n = b n

  b : Int -> Int
  let b n = a n + c n
}

c : Int -> Int
let rec c n = c n + b n
",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T017);
        assert!(
            diagnostics[0].message.contains("`b`, `c` use each other"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn mutual_functions_and_value_cycle_both_reported() {
        let diagnostics = errors(
            "\
f : Int -> Int
let rec f n = if n <= 0 then 0 else f (n - 1) + g n

g : Int -> Int
let rec g n = if n <= 0 then v else g (n - 1) + f n

let v = f 1
",
        );
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == codes::T017 && d.message.contains("`f`, `g` use each other")),
            "{diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == codes::T012 && d.message.contains("`v`")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn rec_group_three_item_value_cycle_closes_through_valid_intra_group_edge() {
        let diagnostics = errors(
            "\
rec {
  f : Int -> Int
  let f n = if n <= 0 then g 0 else v

  g : Int -> Int
  let g n = if n <= 0 then 0 else f (n - 1)

  v : Int
  let v = g 1
}
",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T012);
        assert!(
            diagnostics[0]
                .message
                .contains("`f` and `v` and `g` would be evaluated while being defined"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn self_recursive_value_inside_rec_group_reports_t012() {
        let diagnostics = errors(
            "\
rec {
  a : Int
  let a = a + 1
}
",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T012);
        assert!(
            diagnostics[0].message.contains("`a` would be evaluated"),
            "{}",
            diagnostics[0].message
        );
    }

    #[test]
    fn let_rec_pattern_cycle_reports_t012() {
        let diagnostics = errors("let rec (a, b) = (b, a)\n");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T012);
    }

    #[test]
    fn let_rec_unsigned_value_reports_t012_only_once() {
        let diagnostics = errors("let rec xs = [1] ++ xs\n");
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T012);
    }

    #[test]
    fn plain_lets_that_are_self_and_mutually_recursive_report_mutual_t017() {
        let diagnostics = errors(
            "\
f : Int -> Int
let f n = if n <= 0 then 0 else f (n - 1) + g n

g : Int -> Int
let g n = if n <= 0 then 0 else g (n - 1) + f n
",
        );
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert_eq!(diagnostics[0].code, codes::T017);
        assert!(
            diagnostics[0].message.contains("`f`, `g` use each other"),
            "{}",
            diagnostics[0].message
        );
        assert!(diagnostics[0].fixes.is_empty(), "{diagnostics:?}");
    }
}
