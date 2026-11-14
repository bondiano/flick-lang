use super::Index;
use std::collections::BTreeMap;

#[test]
fn immutable_versions_preserve_order_shadowing_and_removal() {
    let mut index = Index::new(1024);
    let mut root = 0;
    let mut versions = vec![];
    let mut expected = BTreeMap::new();
    for key in (0..1024).rev() {
        root = index.set(root, key, key * 3);
        expected.insert(key, key * 3);
        versions.push(root);
    }
    assert_eq!(
        index.iter(root).collect::<Vec<_>>(),
        expected.into_iter().collect::<Vec<_>>()
    );
    assert_eq!(index.get(versions[0], 1023), Some(3069));
    assert_eq!(index.get(versions[0], 0), None);
    assert_eq!(index.get(root, usize::MAX), None);
    let shadow = index.set(root, 0, 77);
    assert_eq!(index.get(root, 0), Some(0));
    assert_eq!(index.get(shadow, 0), Some(77));
    assert_eq!(index.set(root, 0, 0), root);
    let (without, removed) = index.remove(root, 1023);
    assert_eq!(removed, Some(3069));
    assert_eq!(index.get(without, 1023), None);
    assert_eq!(index.get(root, 1023), Some(3069));
    assert_eq!(index.remove(without, 1023), (without, None));
}

#[test]
fn unions_share_versions_and_keep_left_leaf_semantics() {
    let mut index = Index::new(17);
    let a = index.set(0, 1, 11);
    let b = index.set(a, 16, 23);
    let mut reject =
        |_: usize, _: usize| -> Result<usize, ()> { panic!("shared roots need no merge") };
    assert_eq!(index.union(b, b, &mut reject), Ok(b));
    assert_eq!(index.union(0, b, &mut reject), Ok(b));
    assert_eq!(index.union(b, 0, &mut reject), Ok(b));
    let c = index.set(0, 1, 99);
    let c = index.set(c, 2, 33);
    let mut visited = vec![];
    let union = index
        .union(b, c, &mut |left, right| {
            visited.push((left, right));
            Ok::<_, ()>(left)
        })
        .expect("compatible union");
    assert_eq!(visited, [(11, 99)]);
    assert_eq!(
        index.iter(union).collect::<Vec<_>>(),
        [(1, 11), (2, 33), (16, 23)]
    );
    assert_eq!(index.iter(b).collect::<Vec<_>>(), [(1, 11), (16, 23)]);
    assert_eq!(
        index.union(b, c, &mut |_, _| Err::<usize, _>("mismatch")),
        Err("mismatch")
    );
}

#[test]
fn zero_and_single_key_domains_have_no_depth_or_drop_recursion() {
    let empty: Index<usize> = Index::new(0);
    assert_eq!(empty.get(0, 0), None);
    let mut single = Index::new(1);
    let root = single.set(0, 0, 7);
    assert_eq!(single.iter(root).collect::<Vec<_>>(), [(0, 7)]);
    assert_eq!(single.remove(root, 0), (0, Some(7)));
}

#[test]
fn bounded_ranges_prune_other_namespaces() {
    let mut index = Index::new(1000);
    let mut root = 0;
    for key in 0..1000 {
        root = index.set(root, key, key);
    }
    assert_eq!(
        index.range(root, 497, 503).collect::<Vec<_>>(),
        (497..503).map(|key| (key, key)).collect::<Vec<_>>()
    );
    assert_eq!(index.range(root, 1000, usize::MAX).next(), None);
    assert_eq!(index.range(root, 0, 0).next(), None);
}

#[test]
fn conflicting_leaf_merges_visit_the_lowest_key_before_other_conflicts() {
    let mut index = Index::new(64);
    let mut left = 0;
    let mut right = 0;
    for key in (0..64).rev() {
        left = index.set(left, key, key);
    }
    for key in 0..64 {
        right = index.set(right, key, key + 64);
    }
    assert_eq!(
        index.union(left, right, &mut |a, b| Err::<usize, _>((a, b))),
        Err((0, 64))
    );
}

#[test]
fn arbitrary_callback_union_remains_uncached() {
    let mut index = Index::new(1);
    let left = index.set(0, 0, 7);
    let right = index.set(0, 0, 7);
    let mut called = 0;
    for _ in 0..3 {
        assert_eq!(
            index.union(left, right, &mut |a, _| {
                called += 1;
                Ok::<_, ()>(a)
            }),
            Ok(left)
        );
    }
    assert_eq!(called, 3);
}
