//! The order of the items of a module: each after what it uses, as both
//! backends declare and initialize them.

/// The nodes `0..n` reached from `starts` in depth-first post-order: each
/// after the nodes `deps` gives for it; a cycle is cut where it closes.
pub fn post_order(
    n: usize,
    starts: impl IntoIterator<Item = usize>,
    deps: &mut dyn FnMut(usize) -> Vec<usize>,
) -> Vec<usize> {
    fn visit(
        i: usize,
        deps: &mut dyn FnMut(usize) -> Vec<usize>,
        seen: &mut [bool],
        out: &mut Vec<usize>,
    ) {
        if seen[i] {
            return;
        }
        seen[i] = true;
        for j in deps(i) {
            visit(j, deps, seen, out);
        }
        out.push(i);
    }
    let (mut seen, mut out) = (vec![false; n], Vec::new());
    for i in starts {
        visit(i, deps, &mut seen, &mut out);
    }
    out
}
