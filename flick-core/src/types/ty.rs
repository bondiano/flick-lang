//! Types as the checker and `.iface` see them. A type in an interface is
//! closed: it holds no unification variables, skolems or error markers.

use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::elem::{Atom, Elem, Label};
use super::kinds::Kind;
use super::lang::{self, Lang};
use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Type {
    /// A named type constructor: `Float`, `Option`, a service or an error.
    Con {
        module: String,
        name: String,
    },
    /// A variable bound by an enclosing `forall`.
    Var(String),
    /// A label at the type level, of kind `Symbol`: `"name"`.
    Symbol(String),
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
    /// What `row` stands for without the elements named `gone`: the tail of
    /// `{-S | r}`. A row that is known is that row without them, so only a
    /// row variable keeps this form.
    Without {
        row: Box<Type>,
        gone: Vec<Elem>,
    },
    /// `T^c`: a qualifier on a value's captures or a stored type qualification.
    /// A bare type permits only captures exposed by its complete type.
    Captured {
        ty: Box<Type>,
        captures: Box<Type>,
        /// Interpreted at a value boundary after inference, rather than as
        /// an explicitly declared or stored type qualification.
        value: bool,
        #[serde(default, skip_serializing_if = "Elided::is_no")]
        elided: Elided,
    },
    /// A capture bound on a complete generic type, not an extra hidden
    /// qualifier. Substitution must keep the underlying type's qualifiers.
    Bounded {
        ty: Box<Type>,
        captures: Box<Type>,
    },
    Forall {
        vars: Vec<Binder>,
        /// Class constraints, kept for class resolution.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<Type>,
        body: Box<Type>,
    },
    /// `exists K : Sig. T`: a value and a hidden module witness packaged
    /// together. Abstract `K.Member` references in `body` use `$exists:K`.
    Exists {
        binder: String,
        contract: Box<Type>,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Binder {
    pub name: String,
    pub kind: Kind,
}

/// `{services; errors}` of a runtime arrow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effects {
    pub services: Type,
    pub errors: Type,
    /// Whether the stage is [`Stage::Sync`]: decided where the arrow is
    /// written (see [`Effects::new`]), so a row variable solved later to
    /// `Atom.Read` keeps the runtime stage its function was compiled with.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sync: bool,
    /// Calling-convention metadata for intrinsically synchronous rows on the
    /// native runtime. Ordinary closed rows remain preemptible; absence of an
    /// `Async` operation does not promise termination. No part of the type:
    /// `sync` still says what unifies, and runtime calls adapt either stage.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub direct: bool,
    /// Where the row is written, for a fix that adds to it.
    #[serde(skip)]
    pub written: Written,
}

impl PartialEq for Effects {
    fn eq(&self, other: &Effects) -> bool {
        self.services == other.services && self.errors == other.errors && self.sync == other.sync
    }
}

impl Eq for Effects {}

impl Hash for Effects {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.services.hash(state);
        self.errors.hash(state);
        self.sync.hash(state);
    }
}

/// Where an effect row is written: right after its `{`, and what is
/// already there. No part of the type: every two are equal.
#[derive(Debug, Clone, Copy, Default)]
pub struct Written(pub Option<(Span, Shape)>);

impl PartialEq for Written {
    fn eq(&self, _: &Written) -> bool {
        true
    }
}

impl Eq for Written {}

impl Hash for Written {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

/// What the services of a written effect row are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `->{}`.
    Empty,
    /// `->{; E}`.
    NoServices,
    /// `->{A | r; e}`, or a row `{A | r}`.
    Items,
    /// `->{r; e}`.
    TailOnly,
    /// The row `{}`.
    EmptyRow,
    /// The row `none`, which the services replace.
    None,
}

impl Written {
    /// The text that adds `services` to the row, with where it goes.
    pub fn adding(self, services: &str) -> Option<(Span, String)> {
        let (at, shape) = self.0?;
        let text = match shape {
            Shape::Empty | Shape::NoServices | Shape::EmptyRow => services.to_owned(),
            Shape::Items => format!("{services}, "),
            Shape::TailOnly => format!("{services} | "),
            Shape::None => format!("{{{services}}}"),
        };
        Some((at, text))
    }
}

pub use flick_ir::Stage;

impl Effects {
    /// The rows of an arrow as written: Sync when its services are a closed
    /// row of services marked `@sync` and its errors none.
    pub fn new(services: Type, errors: Type) -> Effects {
        let sync = sync_rows(&services, &errors);
        Effects {
            services,
            errors,
            sync,
            direct: false,
            written: Written::default(),
        }
    }

    pub fn stage(&self) -> Stage {
        if self.sync || self.direct {
            Stage::Sync
        } else {
            Stage::Runtime
        }
    }
}

/// The stage of an arrow with `effects`.
pub fn stage_of(effects: Option<&Effects>) -> Stage {
    effects.map_or(Stage::Pure, Effects::stage)
}

/// The labels of a row without a variable or a meta in it.
fn closed_labels(row: &Type) -> Option<Vec<&Elem>> {
    let Type::Row(r) = row else { return None };
    let mut labels: Vec<&Elem> = r.entries.iter().map(|(l, _)| l).collect();
    if let Some(tail) = &r.tail {
        labels.extend(closed_labels(tail)?);
    }
    Some(labels)
}

/// Whether `services` and `errors` make a Sync stage. `Live` is no service:
/// what a stage needs live runs nothing.
pub fn sync_rows(services: &Type, errors: &Type) -> bool {
    closed_labels(services).is_some_and(|mut l| {
        l.retain(|l| **l != Elem::Live);
        !l.is_empty()
            && l.iter().all(|l| {
                let sync = &lang::table().sync;
                sync.iter().any(|s| l.name() == Some(&s.label))
            })
    }) && closed_labels(errors).is_some_and(|l| l.is_empty())
}

/// Entries sorted by label, each label once. A field row labels entries with
/// field names; a service or error row with the element's qualified name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Row {
    pub entries: Vec<(Elem, Type)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<Box<Type>>,
    /// Where the row is written as a type, `none` or `{…}`, for a fix that
    /// adds to it.
    #[serde(skip)]
    pub written: Written,
}

impl Row {
    pub fn closed(mut entries: Vec<(Elem, Type)>) -> Row {
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Row {
            entries,
            tail: None,
            ..Row::default()
        }
    }

    pub fn get(&self, label: &(impl Label + ?Sized)) -> Option<&Type> {
        self.entries
            .binary_search_by(|(l, _)| label.against(l))
            .ok()
            .map(|i| &self.entries[i].1)
    }
}

/// A row taken apart by [`Store::normalize`](super::unify::Store::normalize).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Norm {
    /// Sorted by label, each label once.
    pub entries: Vec<(Elem, Type)>,
    /// Row variables, unknown rows and error markers the row extends.
    pub tails: Vec<Type>,
}

impl Norm {
    pub fn get(&self, label: &(impl Label + ?Sized)) -> Option<&Type> {
        self.entries
            .binary_search_by(|(l, _)| label.against(l))
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
                ..Row::default()
            }),
        }
    }
}

/// `tail` of a row without the elements named `gone`: a tail already less
/// some names is less these too.
pub fn without(tail: Type, gone: &[Elem]) -> Type {
    let (row, mut all) = match tail {
        Type::Error => return Type::Error,
        Type::Without { row, gone } => (row, gone),
        other => (Box::new(other), Vec::new()),
    };
    all.extend(gone.iter().cloned());
    all.sort();
    all.dedup();
    Type::Without { row, gone: all }
}

/// The label of a region, or of a type variable whose captures are meant, in
/// a capture row. It follows the element, which unification may still
/// change, so rows relabel such entries whenever they are taken apart.
pub fn capture_label(t: &Type) -> Option<Elem> {
    Atom::of(t).map(Elem::Capture)
}

/// The label of a type variable as an element of an error row, `{t | e}`.
/// It follows the element until the variable stands for an error type,
/// which is then labelled by its name like any other.
pub fn error_label(t: &Type) -> Option<Elem> {
    match t.spine().0 {
        Type::Con { module, name } => Some(Elem::Named(format!("{module}.{name}"))),
        head => Atom::of(head).map(Elem::Error),
    }
}

/// A service atom supplied by a static module may still be a bound variable.
/// Its row key must follow that variable when the witness is instantiated.
pub fn service_label(t: &Type) -> Option<Elem> {
    match t.spine().0 {
        Type::Con { module, name } => Some(Elem::Named(format!("{module}.{name}"))),
        head => Atom::of(head).map(Elem::Service),
    }
}

impl Type {
    pub fn con(module: &str, name: &str) -> Type {
        Type::Con {
            module: module.to_owned(),
            name: name.to_owned(),
        }
    }

    /// The type constructor the lang item `item` names.
    pub fn lang(item: Lang) -> Type {
        let item = lang::item(item);
        Type::con(&item.module, &item.name)
    }

    /// Whether this is the type constructor the lang item `item` names.
    pub fn is_lang(&self, item: Lang) -> bool {
        matches!(self, Type::Con { module, name } if lang::is(item, module, name))
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

    /// Where this row is written, when it is a row written as a type.
    pub fn written(&self) -> Written {
        match self {
            Type::Row(r) => r.written,
            _ => Written::default(),
        }
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
    pub fn map_children(&self, f: &mut dyn FnMut(&Type) -> Type) -> Type {
        let row = |row: &Row, f: &mut dyn FnMut(&Type) -> Type| Row {
            entries: row.entries.iter().map(|(l, t)| (l.clone(), f(t))).collect(),
            tail: row.tail.as_ref().map(|t| Box::new(f(t))),
            written: row.written,
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
                        sync: e.sync,
                        direct: e.direct,
                        written: e.written,
                    })
                }),
                result: Box::new(f(result)),
            },
            Type::Tuple(items) => Type::Tuple(items.iter().map(&mut *f).collect()),
            Type::Record(r) => Type::Record(row(r, f)),
            Type::Row(r) => Type::Row(row(r, f)),
            Type::Union(a, b) => Type::Union(Box::new(f(a)), Box::new(f(b))),
            Type::Without { row, gone } => Type::Without {
                row: Box::new(f(row)),
                gone: gone.clone(),
            },
            Type::Captured {
                ty,
                captures,
                value,
                elided,
            } => Type::Captured {
                ty: Box::new(f(ty)),
                captures: Box::new(f(captures)),
                value: *value,
                elided: *elided,
            },
            Type::Bounded { ty, captures } => Type::Bounded {
                ty: Box::new(f(ty)),
                captures: Box::new(f(captures)),
            },
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars: vars.clone(),
                constraints: constraints.iter().map(&mut *f).collect(),
                body: Box::new(f(body)),
            },
            Type::Exists {
                binder,
                contract,
                body,
            } => Type::Exists {
                binder: binder.clone(),
                contract: Box::new(f(contract)),
                body: Box::new(f(body)),
            },
            Type::Con { .. }
            | Type::Var(_)
            | Type::Symbol(_)
            | Type::Meta(_)
            | Type::Skolem(_)
            | Type::Error => self.clone(),
        }
    }

    /// Calls `f` on every direct child type.
    pub fn for_each_child(&self, f: &mut dyn FnMut(&Type)) {
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
            Type::Without { row, .. } => f(row),
            Type::Union(a, b)
            | Type::Captured {
                ty: a, captures: b, ..
            }
            | Type::Bounded { ty: a, captures: b } => {
                f(a);
                f(b);
            }
            Type::Forall {
                constraints, body, ..
            } => {
                constraints.iter().for_each(&mut *f);
                f(body);
            }
            Type::Exists { contract, body, .. } => {
                f(contract);
                f(body);
            }
            Type::Con { .. }
            | Type::Var(_)
            | Type::Symbol(_)
            | Type::Meta(_)
            | Type::Skolem(_)
            | Type::Error => {}
        }
    }

    /// Calls `f` on every type inside, this one first.
    pub fn visit(&self, f: &mut dyn FnMut(&Type)) {
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
            Type::Var(name) => {
                if let Some((_, ty)) = subst.iter().find(|(n, _)| n == name) {
                    return ty.clone();
                }
                if let Some(class) = name.strip_suffix("$Dict")
                    && let Some((_, identity)) = subst.iter().find(|(n, _)| n == class)
                {
                    return match identity {
                        Type::Con { module, name } => Type::con(module, &format!("{name}$Dict")),
                        Type::Var(name) => Type::Var(format!("{name}$Dict")),
                        _ => self.clone(),
                    };
                }
                self.clone()
            }
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

    pub fn contains(&self, pred: &mut dyn FnMut(&Type) -> bool) -> bool {
        let mut found = false;
        self.visit(&mut |t| found |= pred(t));
        found
    }
}

mod elided;
pub use elided::Elided;

#[cfg(test)]
mod tests;
