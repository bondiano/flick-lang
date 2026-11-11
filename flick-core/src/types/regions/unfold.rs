//! A walk over a whole type unfolds the aliases in it, and an opaque alias
//! may name itself inside its module: `opaque type alias Stream = Option
//! (Float, Stream)`. The walk keeps the aliases it is unfolding; an alias met
//! again inside its own unfolding adds nothing of its own, only what its
//! arguments add, the least solution of the equation the alias is.

use crate::types::Checker;
use crate::types::env::Form;
use crate::types::ty::Type;
use crate::types::unify::{Mismatch, Unified};

/// An alias by its module and name, as a walk keeps it.
pub(in crate::types) type AliasKey = (String, String);

/// What a walk meets at the head of a type.
pub(in crate::types) enum Unfold {
    /// No alias.
    Plain,
    /// An alias the walk now unfolds; [`leave_alias`] takes it off again.
    Entered(AliasKey),
    /// An alias met again inside its own unfolding.
    Again,
}

impl Checker<'_> {
    /// The opaque alias at the head of `t`, if it unfolds: only a saturated
    /// one does (`Checker::borrowed_head`). A transparent alias is never kept:
    /// transparent aliases are acyclic (`break_alias_cycles`), so unfolding
    /// one ends, and keeping it would leave `L (L a)` folded and make every
    /// later walk unfold it again.
    pub(in crate::types) fn alias_key(&self, t: &Type) -> Option<AliasKey> {
        // what unification asks of every pair: no copy of the spine
        let (mut head, mut args) = (self.store.head(t), 0);
        while let Type::App { head: h, .. } = head {
            head = self.store.head(h);
            args += 1;
        }
        let Type::Con { module, name } = head else {
            return None;
        };
        matches!(
            self.env.type_def(module, name).map(|d| &d.form),
            Some(Form::Alias { params, opaque: true, .. }) if args >= params.len()
        )
        .then(|| (module.clone(), name.clone()))
    }

    /// Enters the alias at the head of `t` on `path`, the aliases a walk is
    /// unfolding (it may hold other keys too).
    pub(in crate::types) fn enter_alias(&self, t: &Type, path: &mut Vec<AliasKey>) -> Unfold {
        let Some(key) = self.alias_key(t) else {
            return Unfold::Plain;
        };
        if path.contains(&key) {
            return Unfold::Again;
        }
        path.push(key.clone());
        Unfold::Entered(key)
    }

    /// `compare` of `a` and `b`, which may unfold an alias on both sides: one
    /// met again inside its own unfolding is equal by its arguments, as the
    /// nominal type an importer sees.
    pub(in crate::types) fn compare_unfolding(
        &mut self,
        a: &Type,
        b: &Type,
        compare: impl FnOnce(&mut Self) -> Unified,
    ) -> Unified {
        let key = match self.alias_key(a) {
            Some(x) if self.alias_key(b).is_some_and(|y| x == y) => x,
            _ => return compare(self),
        };
        if self.unfolding.contains(&key) {
            let (a, b) = (self.store.resolve(a), self.store.resolve(b));
            let ((_, xs), (_, ys)) = (a.spine(), b.spine());
            if xs.len() != ys.len() {
                return Err(Mismatch::Types(a.clone(), b.clone()));
            }
            return xs.iter().zip(&ys).try_for_each(|(x, y)| self.unify(x, y));
        }
        self.unfolding.push(key);
        let compared = compare(self);
        self.unfolding.pop();
        compared
    }

    /// `t` with `head` taken at every node, which expands the aliases in it:
    /// an alias met again inside its own expansion stays as written, its
    /// arguments expanded.
    pub(in crate::types) fn expand_all(
        &self,
        t: &Type,
        head: &dyn Fn(&Type) -> Type,
        path: &mut Vec<AliasKey>,
    ) -> Type {
        match self.enter_alias(t, path) {
            Unfold::Again => t.map_children(&mut |c| self.expand_all(c, head, path)),
            unfold => {
                let out = head(t).map_children(&mut |c| self.expand_all(c, head, path));
                leave_alias(path, unfold);
                out
            }
        }
    }
}

/// Takes an alias the walk has unfolded off `path`.
pub(in crate::types) fn leave_alias(path: &mut Vec<AliasKey>, unfold: Unfold) {
    if let Unfold::Entered(key) = unfold
        && let Some(i) = path.iter().rposition(|k| *k == key)
    {
        path.remove(i);
    }
}
