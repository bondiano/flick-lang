//! The import graph of the loaded modules: their levels, each module after
//! those it imports, and running one level over the available cores.

use super::Root;

/// The modules by level of the import graph, each after every module it
/// imports; `None` when imports form a cycle.
pub(in crate::project::load) fn levels(edges: &[Vec<usize>]) -> Option<Vec<Vec<usize>>> {
    let mut waiting: Vec<usize> = edges.iter().map(Vec::len).collect();
    let mut importers: Vec<Vec<usize>> = vec![Vec::new(); edges.len()];
    for (i, targets) in edges.iter().enumerate() {
        for &j in targets {
            importers[j].push(i);
        }
    }
    let mut level: Vec<usize> = (0..edges.len()).filter(|&i| waiting[i] == 0).collect();
    let mut levels = Vec::new();
    let mut placed = 0;
    while !level.is_empty() {
        placed += level.len();
        let mut next = Vec::new();
        for &i in &level {
            for &k in &importers[i] {
                waiting[k] -= 1;
                if waiting[k] == 0 {
                    next.push(k);
                }
            }
        }
        next.sort_unstable();
        levels.push(level);
        level = next;
    }
    (placed == edges.len()).then_some(levels)
}

/// `f` of each item, in order, spread over the available cores.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn parallel<T: Send>(items: &[usize], f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    use std::panic;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    // the checker recurses deeply, like on the main thread
    const STACK: usize = 8 << 20;
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(items.len());
    if workers <= 1 {
        return items.iter().map(|&i| f(i)).collect();
    }
    let next = AtomicUsize::new(0);
    let mut results: Vec<(usize, T)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                thread::Builder::new()
                    .stack_size(STACK)
                    .spawn_scoped(scope, || {
                        let mut done = Vec::new();
                        loop {
                            let k = next.fetch_add(1, Ordering::Relaxed);
                            let Some(&i) = items.get(k) else {
                                return done;
                            };
                            done.push((k, f(i)));
                        }
                    })
                    .expect("spawn a checker thread")
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_else(|e| panic::resume_unwind(e)))
            .collect()
    });
    results.sort_unstable_by_key(|(k, _)| *k);
    results.into_iter().map(|(_, t)| t).collect()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn parallel<T>(items: &[usize], f: impl Fn(usize) -> T) -> Vec<T> {
    items.iter().map(|&i| f(i)).collect()
}

/// The library a module of `root` belongs to; `None` for the project's own.
pub(super) fn package_of(root: &Root) -> Option<&str> {
    match root {
        Root::Package(name) => Some(name),
        _ => None,
    }
}
