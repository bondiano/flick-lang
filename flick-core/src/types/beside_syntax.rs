/// The diagnostics of checking a module with syntax errors that do not
/// touch the code the parser could not read, where the checker only guessed.
pub fn beside_syntax_errors(tree: &Node, checked: Vec<Diagnostic>) -> Vec<Diagnostic> {
    fn errors(node: &Node, out: &mut Vec<(u32, u32)>) {
        for child in node.nodes() {
            if child.kind == K::Error {
                out.push(child.range());
            } else {
                errors(child, out);
            }
        }
    }
    let mut broken = Vec::new();
    errors(tree, &mut broken);
    // a type the checker could not finish is no news while code is missing
    let unfinished = |text: &str| text.contains("?`") || text.contains("? ");
    let guessed = |d: &Diagnostic| d.code == codes::T013 || unfinished(&d.message) || d.incomplete;
    checked
        .into_iter()
        .filter(|d| {
            (broken.is_empty() || !guessed(d))
                && d.span
                    .is_none_or(|s| !broken.iter().any(|(b, e)| s.start <= *e && *b <= s.end))
        })
        .collect()
}
