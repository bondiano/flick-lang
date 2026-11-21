//! The variance of each parameter of a declared type (`docs/formal/regions.md`
//! §2.2.1, Lean `Sig.var` with `Sig.var_ok`): the fixed point over the
//! declarations, read from the fields of the constructors, as the model reads
//! it (`flick-model/src/variance.rs`). Subtyping compares the arguments of
//! data by it, and records and tuples field by field.

mod class_solve;
mod class_walk;
mod format;
mod solve;

pub use format::{
    decl_param_names, format_interface_summary, format_markdown, format_parameter_markdown,
    format_plain,
};

use std::collections::HashMap;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::Checker;
use super::env::{Form, TypeDef};
use super::kinds::Kind;
use super::lang::Lang;
use super::regions::{Hold, Visible};
use super::report::uncaptured;
use super::ty::Type;
use super::unify::Unified;
use crate::iface::{Entity, Namespace};
use crate::logic::RULES;
use crate::numeric::{Num, NumLang};
use crate::source::Span;
use solve::{bare, fields};

/// The variance of a type or class parameter: `+` (covariant), `-`
/// (contravariant), `inv` (invariant) or `bi` (bivariant / unused).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variance {
    Bivariant,
    Covariant,
    Contravariant,
    Invariant,
}

impl Variance {
    /// Least upper bound in the variance lattice (`bi < {+, -} < inv`).
    #[must_use]
    pub fn join(self, other: Variance) -> Variance {
        match (self, other) {
            (Variance::Bivariant, s) | (s, Variance::Bivariant) => s,
            (a, b) if a == b => a,
            _ => Variance::Invariant,
        }
    }

    /// Short symbol used in developer tools (`+`, `-`, `inv`, `bi`).
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Variance::Covariant => "+",
            Variance::Contravariant => "-",
            Variance::Invariant => "inv",
            Variance::Bivariant => "bi",
        }
    }

    /// Lowercase name matching the `.iface` serialization.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Variance::Covariant => "covariant",
            Variance::Contravariant => "contravariant",
            Variance::Invariant => "invariant",
            Variance::Bivariant => "bivariant",
        }
    }
}

/// A type by its module and name.
pub(super) type Key = (String, String);

/// The signs found so far, by type; `None` for a type without them.
pub(super) type Signs = HashMap<Key, Option<Rc<[Variance]>>>;

/// The kinds of the parameters a kind takes.
pub(crate) fn param_kinds(mut k: &Kind) -> Vec<&Kind> {
    let mut out = Vec::new();
    while let Kind::Arrow(p, r) = k {
        out.push(&**p);
        k = r;
    }
    out
}

/// Validates serialized parameter variance against `kinds`.
///
/// Only parameterized declarations with exact arity and `Invariant` on every
/// non-`Type` parameter are trusted; unknown or malformed metadata stays
/// conservative (`None`).
pub(crate) fn valid_exported_variance<'a>(
    kinds: &[Kind],
    variance: &'a [Variance],
) -> Option<&'a [Variance]> {
    if kinds.is_empty() || variance.len() != kinds.len() {
        return None;
    }
    if kinds
        .iter()
        .zip(variance)
        .any(|(kind, v)| *kind != Kind::Type && *v != Variance::Invariant)
    {
        return None;
    }
    Some(variance)
}

/// Built-in scalar types subtyping moves neither up nor down (Lean `Ty.Closed`).
fn is_closed_scalar(t: &Type) -> bool {
    const SCALARS: [Lang; 7] = [
        Lang::Int,
        Lang::Float,
        Lang::BigInt,
        Lang::String,
        Lang::Bool,
        Lang::Unit,
        Lang::Never,
    ];
    SCALARS.iter().any(|&lang| t.is_lang(lang)) || Num::ALL.iter().any(|num| t.is_lang(num.lang()))
}

/// A declared type: the kinds of its parameters, its constructors' schemes
/// (`None` when any constructor is hidden or opaque), and validated serialized
/// variance when imported or when constructors are unavailable.
pub(super) struct Data<'t> {
    pub(super) kinds: Vec<Kind>,
    pub(super) ctors: Option<Vec<&'t Type>>,
    pub(super) exported: Option<&'t [Variance]>,
}

impl Checker<'_> {
    /// Computes and records inferred parameter variances on this module's own
    /// data and class declarations so exports, support types and local
    /// declarations all carry them.
    pub(super) fn populate_own_variances(&mut self) {
        let module = self.env.module.clone();
        let mut data_candidates: Vec<String> = Vec::new();
        let mut class_candidates: Vec<String> = Vec::new();
        for (name, def) in self.env.own_types() {
            match def.form {
                Form::Adt { .. } | Form::Error => data_candidates.push(name.to_owned()),
                Form::Nominal { .. } if Type::con(&module, name).is_lang(Lang::List) => {
                    data_candidates.push(name.to_owned());
                }
                Form::Class { .. } => class_candidates.push(name.to_owned()),
                _ => {}
            }
        }
        data_candidates.sort();
        for name in data_candidates {
            if let Some(signs) = self.signs(&module, &name)
                && !signs.is_empty()
                && let Some(def) = self.env.type_def_mut(&name)
            {
                def.variance = signs.to_vec();
            }
        }
        class_candidates.sort();
        for name in class_candidates {
            if let Some(signs) = self.class_signs(&module, &name)
                && !signs.is_empty()
                && let Some(def) = self.env.type_def_mut(&name)
            {
                def.variance = signs.to_vec();
            }
        }
    }

    /// Variance of `module.name` for export/declaration projection without
    /// mutating the checker cache.
    pub(super) fn exported_variance(
        &self,
        module: &str,
        name: &str,
        def: &TypeDef,
    ) -> Vec<Variance> {
        let kinds: Vec<Kind> = param_kinds(&def.kind).into_iter().cloned().collect();
        if kinds.is_empty() {
            return Vec::new();
        }
        match &def.form {
            Form::Adt { .. } | Form::Error => {
                if valid_exported_variance(&kinds, &def.variance).is_some() {
                    return def.variance.clone();
                }
                self.compute_signs(module, name)
                    .map_or_else(Vec::new, |s| s.to_vec())
            }
            Form::Class { .. } => {
                if valid_exported_variance(&kinds, &def.variance).is_some() {
                    return def.variance.clone();
                }
                self.compute_class_signs(module, name)
                    .map_or_else(Vec::new, |s| s.to_vec())
            }
            Form::Nominal { .. } => {
                if valid_exported_variance(&kinds, &def.variance).is_some() {
                    return def.variance.clone();
                }
                if Type::con(module, name).is_lang(Lang::List) && kinds == [Kind::Type] {
                    vec![Variance::Covariant]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// The sign of each parameter of class `module.name`, inferred from its
    /// superclasses and methods for local classes or read from validated `.iface`
    /// metadata for imported classes (falling back conservatively to `Invariant`).
    pub(super) fn class_signs(&mut self, module: &str, name: &str) -> Option<Rc<[Variance]>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.class_signs.get(&key) {
            return found.clone();
        }
        let solved = self.solve_class_group(&key);
        for (k, s) in solved {
            self.class_signs.insert(k, Some(s.into()));
        }
        let found = self.class_signs.get(&key).cloned().flatten();
        self.class_signs.insert(key, found.clone());
        found
    }

    /// Pure `&self` class-sign lookup for export projection.
    pub(super) fn compute_class_signs(&self, module: &str, name: &str) -> Option<Rc<[Variance]>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.class_signs.get(&key) {
            return found.clone();
        }
        let solved = self.solve_class_group(&key);
        solved.get(&key).map(|s| Rc::from(s.as_slice()))
    }

    /// The sign of each parameter of the type `module.name`, `None` but for
    /// a declared type, a nominal type with validated exported variance, or
    /// the built-in list.
    pub(super) fn signs(&mut self, module: &str, name: &str) -> Option<Rc<[Variance]>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.signs.get(&key) {
            return found.clone();
        }
        let solved = self.solve_group(&key);
        for (k, s) in solved {
            self.signs.insert(k, Some(s.into()));
        }
        let found = self.signs.get(&key).cloned().flatten().or_else(|| {
            let list = Type::Con {
                module: module.to_owned(),
                name: name.to_owned(),
            };
            // the built-in list is immutable and has no declaration
            list.is_lang(Lang::List)
                .then(|| Rc::from([Variance::Covariant]))
        });
        self.signs.insert(key, found.clone());
        found
    }

    /// Pure `&self` sign lookup for projection paths that cannot mutate `self`.
    fn compute_signs(&self, module: &str, name: &str) -> Option<Rc<[Variance]>> {
        let key = (module.to_owned(), name.to_owned());
        if let Some(found) = self.signs.get(&key) {
            return found.clone();
        }
        let solved = self.solve_group(&key);
        if let Some(s) = solved.get(&key) {
            return Some(Rc::from(s.as_slice()));
        }
        let list = Type::Con {
            module: module.to_owned(),
            name: name.to_owned(),
        };
        list.is_lang(Lang::List)
            .then(|| Rc::from([Variance::Covariant]))
    }

    pub(super) fn data(&self, (module, name): &Key) -> Option<Data<'_>> {
        let def = self.env.type_def(module, name)?;
        let kinds: Vec<Kind> = param_kinds(&def.kind).into_iter().cloned().collect();
        let imported = module != &self.env.module;
        match &def.form {
            Form::Adt { constructors, .. } => {
                let ctors = constructors
                    .iter()
                    .map(|c| {
                        self.env.value(&Entity {
                            module: module.clone(),
                            namespace: Namespace::Constructor,
                            name: c.clone(),
                        })
                    })
                    .collect::<Option<Vec<_>>>();
                let exported = (imported || ctors.is_none())
                    .then(|| valid_exported_variance(&kinds, &def.variance))
                    .flatten();
                Some(Data {
                    kinds,
                    ctors,
                    exported,
                })
            }
            Form::Error => {
                let ctors = self
                    .env
                    .value(&Entity {
                        module: module.clone(),
                        namespace: Namespace::Constructor,
                        name: name.clone(),
                    })
                    .map(|scheme| vec![scheme]);
                let exported = (imported || ctors.is_none())
                    .then(|| valid_exported_variance(&kinds, &def.variance))
                    .flatten();
                Some(Data {
                    kinds,
                    ctors,
                    exported,
                })
            }
            Form::Nominal { .. } => {
                let exported = valid_exported_variance(&kinds, &def.variance)?;
                Some(Data {
                    kinds,
                    ctors: None,
                    exported: Some(exported),
                })
            }
            _ => None,
        }
    }

    /// A type subtyping moves neither up nor down (Lean `Ty.Closed`): a
    /// built-in scalar, or a declared data type without parameters whose
    /// visible constructor fields are such types (a cycle of them is).
    /// Opaque, abstract and module-parameter nominal types stay conservatively
    /// non-closed because their representations are unavailable.
    pub(super) fn closed(&self, t: &Type, seen: &mut Vec<Key>) -> bool {
        let Type::Con { module, name } = t else {
            return false;
        };
        let Some(def) = self.env.type_def(module, name) else {
            return false;
        };
        if !param_kinds(&def.kind).is_empty() {
            return false;
        }
        let key = (module.clone(), name.clone());
        match &def.form {
            Form::Nominal { .. } => is_closed_scalar(t),
            Form::Adt { .. } | Form::Error if seen.contains(&key) => true,
            Form::Adt { .. } | Form::Error => {
                let Some(Data {
                    ctors: Some(ctors), ..
                }) = self.data(&key)
                else {
                    return false;
                };
                seen.push(key);
                let all =
                    (ctors.iter()).all(|s| fields(s).0.iter().all(|f| self.closed(bare(f), seen)));
                seen.pop();
                all
            }
            _ => false,
        }
    }

    /// `⌈x⌉ ⊆ ⌈y⌉` of a `−` argument, which narrows by its type but keeps
    /// what it reaches.
    fn keeps_reach(&mut self, x: &Type, y: &Type, span: Span) {
        self.hold(Hold {
            held: vec![(None, x.clone())],
            visible: Visible::Late(y.clone()),
            bound: Type::empty_row(),
            expected: Some(y.clone()),
            written: true,
            what: "value",
            span,
        });
    }

    /// `T ū ≤ T ū'` by the sign of each parameter (≤-Data), and a record or
    /// a tuple field by field; else `≡`.
    pub(super) fn subsume_data(&mut self, actual: &Type, expected: &Type, span: Span) -> Unified {
        match (actual, expected) {
            (Type::Tuple(args), Type::Tuple(expected_args))
                if args.len() == expected_args.len() =>
            {
                args.iter()
                    .zip(expected_args)
                    .try_for_each(|(x, y)| self.subsume(x, y, span))
            }
            (Type::Record(r), Type::Record(s))
                if r.tail.is_none()
                    && s.tail.is_none()
                    && r.entries.len() == s.entries.len()
                    && r.entries.iter().all(|(label, _)| s.get(label).is_some()) =>
            {
                r.entries.iter().try_for_each(|(label, x)| {
                    s.get(label)
                        .map_or(Ok(()), |y| self.subsume(x, &y.clone(), span))
                })
            }
            // a proof is of values, whose captures its logic does not read
            (Type::App { head, .. }, Type::App { .. }) if matches!(&**head, Type::Con { module, .. } if module == RULES) =>
            {
                let (x, y) = (self.zonk(actual), self.zonk(expected));
                self.unify(&uncaptured(&x), &uncaptured(&y))
            }
            (Type::App { .. }, Type::App { .. }) => {
                let ((head, args), (other_head, expected_args)) =
                    (actual.spine(), expected.spine());
                let signs = match (head, other_head) {
                    (
                        Type::Con { module, name },
                        Type::Con {
                            module: other_module,
                            name: other_name,
                        },
                    ) if module == other_module
                        && name == other_name
                        && args.len() == expected_args.len() =>
                    {
                        if let Some(class_name) = name.strip_suffix("$Dict")
                            && self.class_info(module, class_name).is_some()
                        {
                            self.class_signs(module, class_name)
                                .filter(|s| s.len() == args.len())
                        } else {
                            self.signs(module, name).filter(|s| s.len() == args.len())
                        }
                    }
                    _ => None,
                };
                let Some(signs) = signs else {
                    return self.unify(actual, expected);
                };
                let (args, expected_args): (Vec<Type>, Vec<Type>) = (
                    args.into_iter().cloned().collect(),
                    expected_args.into_iter().cloned().collect(),
                );
                for ((x, y), s) in args.iter().zip(&expected_args).zip(signs.iter()) {
                    // an argument inference has yet to solve takes the other
                    // as it is, its bound too, as `≡` of the whole type did
                    let open = |t: &Type| self.whnf(t).inference_meta_root().is_some();
                    match s {
                        Variance::Covariant if !open(x) && open(y) => {
                            self.bind_subsumed_meta(x, y, false)
                                .unwrap_or_else(|| self.unify(x, y))?;
                        }
                        _ if open(x) || open(y) => self.unify(x, y)?,
                        Variance::Covariant => self.subsume(x, y, span)?,
                        Variance::Contravariant => {
                            self.subsume(y, x, span)?;
                            self.keeps_reach(x, y, span);
                        }
                        Variance::Invariant | Variance::Bivariant => self.unify(x, y)?,
                    }
                }
                Ok(())
            }
            _ => self.unify(actual, expected),
        }
    }
}
