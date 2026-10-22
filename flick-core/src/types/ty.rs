//! Types as the checker and `.iface` see them. A type in an interface is
//! closed: it holds no unification variables, skolems or error markers.

use serde::{Deserialize, Serialize};

use super::kinds::Kind;

/// Module of the built-in types and constructors every module sees.
pub const PRELUDE: &str = "Prelude";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Type {
    /// A named type constructor: `Number`, `Option`, a service or an error.
    Con {
        module: String,
        name: String,
    },
    /// A variable bound by an enclosing `forall`.
    Var(String),
    App {
        head: Box<Type>,
        arg: Box<Type>,
    },
    /// `a -> b` when `effects` is `None`, `a ->{r; e} b` otherwise; an empty
    /// row never makes a runtime arrow pure.
    Fun {
        param: Box<Type>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effects: Option<Box<Effects>>,
        result: Box<Type>,
    },
    Tuple(Vec<Type>),
    /// An exact record; open when its row has a tail.
    Record(Row),
    /// A row on its own: services, errors, or the tail of a record.
    Row(Row),
    /// `r1 + r2`.
    Union(Box<Type>, Box<Type>),
    Forall {
        vars: Vec<Binder>,
        /// Class constraints, kept for class resolution.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<Type>,
        body: Box<Type>,
    },
    /// A unification variable.
    #[serde(skip)]
    Meta(u32),
    /// A rigid variable: a quantified variable while its body is checked.
    #[serde(skip)]
    Skolem(u32),
    /// Stands for a type that already produced a diagnostic; unifies with anything.
    #[serde(skip)]
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binder {
    pub name: String,
    pub kind: Kind,
}

/// `{services; errors}` of a runtime arrow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effects {
    pub services: Type,
    pub errors: Type,
}

/// Entries sorted by label, each label once. A field row labels entries with
/// field names; a service or error row with the element's qualified name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    pub entries: Vec<(String, Type)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<Box<Type>>,
}

impl Row {
    pub fn closed(mut entries: Vec<(String, Type)>) -> Row {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Row {
            entries,
            tail: None,
        }
    }

    pub fn get(&self, label: &str) -> Option<&Type> {
        self.entries
            .binary_search_by(|(l, _)| l.as_str().cmp(label))
            .ok()
            .map(|i| &self.entries[i].1)
    }
}

/// A row taken apart by [`Store::normalize`](super::unify::Store::normalize).
#[derive(Debug, Clone, Default)]
pub struct Norm {
    /// Sorted by label, each label once.
    pub entries: Vec<(String, Type)>,
    /// Row variables, unknown rows and error markers the row extends.
    pub tails: Vec<Type>,
}

impl Norm {
    pub fn get(&self, label: &str) -> Option<&Type> {
        self.entries
            .binary_search_by(|(l, _)| l.as_str().cmp(label))
            .ok()
            .map(|i| &self.entries[i].1)
    }

    /// The row as one type: `{A, B | r + s}`, or just `r` without entries.
    pub fn into_type(self) -> Type {
        let tail = self
            .tails
            .into_iter()
            .reduce(|a, b| Type::Union(Box::new(a), Box::new(b)));
        match tail {
            Some(tail) if self.entries.is_empty() => tail,
            tail => Type::Row(Row {
                entries: self.entries,
                tail: tail.map(Box::new),
            }),
        }
    }
}

impl Type {
    pub fn con(module: &str, name: &str) -> Type {
        Type::Con {
            module: module.to_owned(),
            name: name.to_owned(),
        }
    }

    pub fn prim(name: &str) -> Type {
        Type::con(PRELUDE, name)
    }

    pub fn is_prim(&self, name: &str) -> bool {
        matches!(self, Type::Con { module, name: n } if module == PRELUDE && n == name)
    }

    pub fn app(head: Type, arg: Type) -> Type {
        Type::App {
            head: Box::new(head),
            arg: Box::new(arg),
        }
    }

    pub fn apps(head: Type, args: impl IntoIterator<Item = Type>) -> Type {
        args.into_iter().fold(head, Type::app)
    }

    pub fn pure_fun(param: Type, result: Type) -> Type {
        Type::Fun {
            param: Box::new(param),
            effects: None,
            result: Box::new(result),
        }
    }

    pub fn empty_row() -> Type {
        Type::Row(Row::default())
    }

    /// The head constructor and arguments of an application.
    pub fn spine(&self) -> (&Type, Vec<&Type>) {
        let mut args = Vec::new();
        let mut head = self;
        while let Type::App { head: h, arg } = head {
            args.push(&**arg);
            head = h;
        }
        args.reverse();
        (head, args)
    }

    /// Applies `f` to every direct child type.
    #[must_use]
    pub fn map_children(&self, f: &mut impl FnMut(&Type) -> Type) -> Type {
        let row = |row: &Row, f: &mut dyn FnMut(&Type) -> Type| Row {
            entries: row.entries.iter().map(|(l, t)| (l.clone(), f(t))).collect(),
            tail: row.tail.as_ref().map(|t| Box::new(f(t))),
        };
        match self {
            Type::App { head, arg } => Type::app(f(head), f(arg)),
            Type::Fun {
                param,
                effects,
                result,
            } => Type::Fun {
                param: Box::new(f(param)),
                effects: effects.as_ref().map(|e| {
                    Box::new(Effects {
                        services: f(&e.services),
                        errors: f(&e.errors),
                    })
                }),
                result: Box::new(f(result)),
            },
            Type::Tuple(items) => Type::Tuple(items.iter().map(&mut *f).collect()),
            Type::Record(r) => Type::Record(row(r, f)),
            Type::Row(r) => Type::Row(row(r, f)),
            Type::Union(a, b) => Type::Union(Box::new(f(a)), Box::new(f(b))),
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars: vars.clone(),
                constraints: constraints.iter().map(&mut *f).collect(),
                body: Box::new(f(body)),
            },
            Type::Con { .. } | Type::Var(_) | Type::Meta(_) | Type::Skolem(_) | Type::Error => {
                self.clone()
            }
        }
    }

    /// Calls `f` on every direct child type.
    pub fn for_each_child(&self, f: &mut impl FnMut(&Type)) {
        let row = |row: &Row, f: &mut dyn FnMut(&Type)| {
            row.entries.iter().for_each(|(_, t)| f(t));
            row.tail.iter().for_each(|t| f(t));
        };
        match self {
            Type::App { head, arg } => {
                f(head);
                f(arg);
            }
            Type::Fun {
                param,
                effects,
                result,
            } => {
                f(param);
                if let Some(e) = effects {
                    f(&e.services);
                    f(&e.errors);
                }
                f(result);
            }
            Type::Tuple(items) => items.iter().for_each(f),
            Type::Record(r) | Type::Row(r) => row(r, f),
            Type::Union(a, b) => {
                f(a);
                f(b);
            }
            Type::Forall {
                constraints, body, ..
            } => {
                constraints.iter().for_each(&mut *f);
                f(body);
            }
            Type::Con { .. } | Type::Var(_) | Type::Meta(_) | Type::Skolem(_) | Type::Error => {}
        }
    }

    /// Calls `f` on every type inside, this one first.
    pub fn visit(&self, f: &mut impl FnMut(&Type)) {
        f(self);
        self.for_each_child(&mut |t| t.visit(f));
    }

    /// Replaces the free variables named in `subst`; stops at a `forall`
    /// that rebinds a name.
    #[must_use]
    pub fn substitute(&self, subst: &[(String, Type)]) -> Type {
        if subst.is_empty() {
            return self.clone();
        }
        match self {
            Type::Var(name) => subst
                .iter()
                .find(|(n, _)| n == name)
                .map_or_else(|| self.clone(), |(_, t)| t.clone()),
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let inner: Vec<(String, Type)> = subst
                    .iter()
                    .filter(|(n, _)| !vars.iter().any(|v| &v.name == n))
                    .cloned()
                    .collect();
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints.iter().map(|c| c.substitute(&inner)).collect(),
                    body: Box::new(body.substitute(&inner)),
                }
            }
            _ => self.map_children(&mut |t| t.substitute(subst)),
        }
    }

    pub fn contains(&self, pred: &mut impl FnMut(&Type) -> bool) -> bool {
        let mut found = false;
        self.visit(&mut |t| found |= pred(t));
        found
    }
}
