//! Canonical ordering and compact sorting helpers shared across compiler crates.
//!
//! Record labels follow ECMAScript UTF-16 code-unit order. Routing sorting
//! through one non-generic stable heapsort avoids monomorphizing
//! `slice::sort_by` across every caller's element type and closure in the WASM
//! compiler artifact.

use std::cmp::Ordering;

/// Compares two strings in ECMAScript UTF-16 code-unit order.
#[must_use]
pub fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Returns indices `0..len` sorted stably by `cmp(i, j)`.
#[must_use]
pub fn sort_indices<'a>(len: usize, cmp: &(dyn Fn(usize, usize) -> Ordering + 'a)) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if len < 2 {
        return order;
    }
    let gt = |a: usize, b: usize| cmp(a, b).then_with(|| a.cmp(&b)) == Ordering::Greater;
    let sift = |arr: &mut [usize], mut root: usize, end: usize| {
        loop {
            let left = 2 * root + 1;
            if left >= end {
                break;
            }
            let mut swap = root;
            if gt(arr[left], arr[swap]) {
                swap = left;
            }
            let right = left + 1;
            if right < end && gt(arr[right], arr[swap]) {
                swap = right;
            }
            if swap == root {
                break;
            }
            arr.swap(root, swap);
            root = swap;
        }
    };
    for start in (0..len / 2).rev() {
        sift(&mut order, start, len);
    }
    for end in (1..len).rev() {
        order.swap(0, end);
        sift(&mut order, 0, end);
    }
    order
}

/// Applies the permutation `order` (where `order[dst] = src`) to `items` in place.
pub fn apply_order<T>(items: &mut [T], mut order: Vec<usize>) {
    let n = items.len();
    for i in 0..n {
        let mut cur = i;
        while order[cur] != i {
            let next = order[cur];
            items.swap(cur, next);
            order[cur] = cur;
            cur = next;
        }
        order[cur] = cur;
    }
}

/// Returns indices `0..len` sorted stably in canonical UTF-16 order of `key(i)`.
#[must_use]
pub fn utf16_sorted_indices<'a>(len: usize, key: &(dyn Fn(usize) -> &'a str + 'a)) -> Vec<usize> {
    sort_indices(len, &|i, j| cmp_utf16(key(i), key(j)))
}

/// Sorts `items` stably in place into canonical UTF-16 order of `key(item)`.
pub fn sort_by_utf16<T>(items: &mut [T], key: fn(&T) -> &str) {
    if items.len() < 2 {
        return;
    }
    let order = utf16_sorted_indices(items.len(), &|i| key(&items[i]));
    apply_order(items, order);
}

/// Sorts a slice of strings in place into canonical UTF-16 order.
pub fn sort_strings_utf16(items: &mut [String]) {
    sort_by_utf16(items, String::as_str);
}

/// Sorts `items` stably in place by UTF-8 string key.
pub fn sort_by_str<T>(items: &mut [T], key: fn(&T) -> &str) {
    if items.len() < 2 {
        return;
    }
    let order = sort_indices(items.len(), &|i, j| key(&items[i]).cmp(key(&items[j])));
    apply_order(items, order);
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{cmp_utf16, sort_by_str, sort_by_utf16, sort_strings_utf16, utf16_sorted_indices};

    #[test]
    fn sorts_by_utf16_code_units_and_preserves_stability() {
        let mut labels = vec![
            "\u{E000}".to_owned(),
            "\u{10000}".to_owned(),
            "z".to_owned(),
            "a".to_owned(),
        ];
        sort_strings_utf16(&mut labels);
        // UTF-16: 'a' < 'z' < U+10000 (0xD800 0xDC00) < U+E000 (0xE000)
        assert_eq!(labels, vec!["a", "z", "\u{10000}", "\u{E000}"]);
        assert_eq!(cmp_utf16("\u{10000}", "\u{E000}"), Ordering::Less);

        let mut pairs = vec![("b", 1), ("a", 2), ("b", 3), ("a", 4)];
        sort_by_utf16(&mut pairs, |p| p.0);
        assert_eq!(pairs, vec![("a", 2), ("a", 4), ("b", 1), ("b", 3)]);

        let mut utf8 = vec![("\u{10000}", 1), ("\u{E000}", 2)];
        sort_by_str(&mut utf8, |p| p.0);
        assert_eq!(utf8, vec![("\u{E000}", 2), ("\u{10000}", 1)]);
        assert_eq!(utf16_sorted_indices(0, &|_| ""), Vec::<usize>::new());
    }
}
