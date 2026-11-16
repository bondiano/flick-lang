use crate::types::Elem;
use crate::types::ty::{Norm, Type};

/// A row variable and the names it is taken without, if any.
pub(super) fn less(tail: &Type) -> (&Type, &[Elem]) {
    match tail {
        Type::Without { row, gone } => (row, gone),
        other => (other, &[]),
    }
}

/// The variable of a tail still to be inferred, maybe less some names.
pub(super) fn unknown(tail: &Type) -> Option<u32> {
    match less(tail).0 {
        Type::Meta(meta) => meta.inference_type_root(),
        _ => None,
    }
}

/// The names `declared` takes out of the variable of `tail` that neither
/// the tail is less too nor `declared` gives back, or `None` when it does
/// not hold the variable: `s` is within `{L | s - L}`, whatever `s` is.
/// Only a name without arguments gives it back: `s` may hold `S b`, which
/// `{S a | s - S}` does not.
pub(super) fn owed(declared: &Norm, tail: &Type) -> Option<Vec<Elem>> {
    let (variable, gone) = less(tail);
    let gives_back = |l: &Elem| declared.get(l).is_some_and(|t| !l.is_instance(t));
    (declared.tails.iter().map(less))
        .filter(|(held, _)| *held == variable)
        .map(|(_, taken)| {
            (taken.iter())
                .filter(|l| !gone.contains(l) && !gives_back(l))
                .cloned()
                .collect::<Vec<_>>()
        })
        .min_by_key(Vec::len)
}

/// Whether `declared` allows all a `tail` stands for.
pub(super) fn allows(declared: &Norm, tail: &Type) -> bool {
    owed(declared, tail).is_some_and(|names| names.is_empty())
}
