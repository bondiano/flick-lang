//! Immutable ordinal-key indexes. Versions share flat-arena branches; no owning
//! tree pointers, graph-depth recursion, or per-version map copies are needed.

use std::collections::BTreeMap;

/// One arena and one fixed merge policy own this cache. Failures are never stored.
#[derive(Default)]
pub(super) struct Unions {
    successful: BTreeMap<(usize, usize), usize>,
}
#[cfg(test)]
impl Unions {
    pub(super) fn allocated(&self) -> usize {
        self.successful.len()
    }
}

#[derive(Clone, Copy)]
pub(super) enum Node<V> {
    Empty,
    Leaf(usize, V),
    Branch(usize, usize),
}

pub(super) struct Index<V> {
    nodes: Vec<Node<V>>,
    keys: usize,
    bits: u32,
}
impl<V: Copy + Eq> Index<V> {
    pub(super) fn new(keys: usize) -> Self {
        Self {
            nodes: vec![Node::Empty],
            keys,
            bits: usize::BITS - keys.saturating_sub(1).leading_zeros(),
        }
    }
    fn push(&mut self, node: Node<V>) -> usize {
        #[cfg(test)]
        super::metrics::retain(1);
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }
    fn branch(&mut self, original: usize, left: usize, right: usize) -> usize {
        if left == 0 && right == 0 {
            return 0;
        }
        if let Node::Branch(a, b) = self.nodes[original]
            && a == left
            && b == right
        {
            return original;
        }
        self.push(Node::Branch(left, right))
    }
    pub(super) fn get(&self, mut root: usize, key: usize) -> Option<V> {
        let mut depth = self.bits;
        loop {
            #[cfg(test)]
            super::metrics::lookup();
            match self.nodes[root] {
                Node::Empty => return None,
                Node::Leaf(found, value) => return (found == key).then_some(value),
                Node::Branch(left, right) => {
                    depth -= 1;
                    root = if (key >> depth) & 1 == 0 { left } else { right };
                }
            }
        }
    }
    pub(super) fn set(&mut self, root: usize, key: usize, value: V) -> usize {
        assert!(key < self.keys, "interned index key");
        self.set_at(root, key, value, self.bits)
    }
    fn set_at(&mut self, root: usize, key: usize, value: V, depth: u32) -> usize {
        #[cfg(test)]
        super::metrics::merge();
        if depth == 0 {
            if let Node::Leaf(_, old) = self.nodes[root]
                && old == value
            {
                return root;
            }
            return self.push(Node::Leaf(key, value));
        }
        let (mut left, mut right) = match self.nodes[root] {
            Node::Empty => (0, 0),
            Node::Branch(left, right) => (left, right),
            Node::Leaf(_, _) => unreachable!("fixed-depth index"),
        };
        if (key >> (depth - 1)) & 1 == 0 {
            left = self.set_at(left, key, value, depth - 1);
        } else {
            right = self.set_at(right, key, value, depth - 1);
        }
        self.branch(root, left, right)
    }
    pub(super) fn remove(&mut self, root: usize, key: usize) -> (usize, Option<V>) {
        self.remove_at(root, key, self.bits)
    }
    fn remove_at(&mut self, root: usize, key: usize, depth: u32) -> (usize, Option<V>) {
        #[cfg(test)]
        super::metrics::merge();
        match self.nodes[root] {
            Node::Empty => (0, None),
            Node::Leaf(found, value) => {
                if found == key {
                    (0, Some(value))
                } else {
                    (root, None)
                }
            }
            Node::Branch(mut left, mut right) => {
                let removed;
                if (key >> (depth - 1)) & 1 == 0 {
                    (left, removed) = self.remove_at(left, key, depth - 1);
                } else {
                    (right, removed) = self.remove_at(right, key, depth - 1);
                }
                (self.branch(root, left, right), removed)
            }
        }
    }
    #[cfg(test)]
    pub(super) fn union<E>(
        &mut self,
        left: usize,
        right: usize,
        merge: &mut impl FnMut(V, V) -> Result<V, E>,
    ) -> Result<usize, E> {
        #[cfg(test)]
        super::metrics::merge();
        if left == right || right == 0 {
            return Ok(left);
        }
        if left == 0 {
            return Ok(right);
        }
        match (self.nodes[left], self.nodes[right]) {
            (Node::Leaf(key, a), Node::Leaf(_, b)) => {
                let value = merge(a, b)?;
                Ok(if value == a {
                    left
                } else {
                    self.push(Node::Leaf(key, value))
                })
            }
            (Node::Branch(a, b), Node::Branch(c, d)) => {
                let a = self.union(a, c, merge)?;
                let b = self.union(b, d, merge)?;
                Ok(self.branch(left, a, b))
            }
            _ => unreachable!("matching fixed-depth indexes"),
        }
    }
    /// Cached only for a caller that fixes its leaf policy for this arena's
    /// lifetime. The ordinary union deliberately retains arbitrary `FnMut` behavior.
    pub(super) fn union_fixed<E>(
        &mut self,
        left: usize,
        right: usize,
        cache: &mut Unions,
        merge: &mut impl FnMut(V, V) -> Result<V, E>,
    ) -> Result<usize, E> {
        #[cfg(test)]
        super::metrics::merge();
        if left == right || right == 0 {
            return Ok(left);
        }
        if left == 0 {
            return Ok(right);
        }
        #[cfg(test)]
        super::metrics::memo_lookup();
        if let Some(result) = cache.successful.get(&(left, right)) {
            return Ok(*result);
        }
        let result = match (self.nodes[left], self.nodes[right]) {
            (Node::Leaf(key, a), Node::Leaf(_, b)) => {
                let value = merge(a, b)?;
                if value == a {
                    left
                } else {
                    self.push(Node::Leaf(key, value))
                }
            }
            (Node::Branch(a, b), Node::Branch(c, d)) => {
                let a = self.union_fixed(a, c, cache, merge)?;
                let b = self.union_fixed(b, d, cache, merge)?;
                self.branch(left, a, b)
            }
            _ => unreachable!("matching fixed-depth indexes"),
        };
        // Nonempty IDs intrinsically fix their depth and ordinal interval:
        // branches are never lifted/collapsed, and leaf keys remain explicit.
        // Ordered pairs preserve the left representative; arena IDs never reuse.
        cache.successful.insert((left, right), result);
        #[cfg(test)]
        super::metrics::memo_entry();
        Ok(result)
    }
    pub(super) fn node(&self, root: usize) -> Node<V> {
        self.nodes[root]
    }
    pub(super) fn allocated(&self) -> usize {
        self.nodes.len()
    }
    #[cfg(test)]
    pub(super) fn iter(&self, root: usize) -> Iter<'_, V> {
        self.range(root, 0, self.keys)
    }
    pub(super) fn range(&self, root: usize, start: usize, end: usize) -> Iter<'_, V> {
        Iter {
            index: self,
            pending: vec![(root, 0, self.bits)],
            start,
            end,
        }
    }
}
pub(super) struct Iter<'a, V> {
    index: &'a Index<V>,
    pending: Vec<(usize, usize, u32)>,
    start: usize,
    end: usize,
}
impl<V: Copy> Iterator for Iter<'_, V> {
    type Item = (usize, V);
    fn next(&mut self) -> Option<Self::Item> {
        while let Some((id, prefix, depth)) = self.pending.pop() {
            #[cfg(test)]
            super::metrics::range_step();
            let mask = if depth == usize::BITS {
                usize::MAX
            } else {
                (1usize << depth) - 1
            };
            if prefix >= self.end || (prefix | mask) < self.start {
                continue;
            }
            match self.index.nodes[id] {
                Node::Empty => {}
                Node::Leaf(key, value) => return Some((key, value)),
                Node::Branch(left, right) => {
                    self.pending
                        .push((right, prefix | (1 << (depth - 1)), depth - 1));
                    self.pending.push((left, prefix, depth - 1));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
