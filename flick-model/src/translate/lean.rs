//! The checker's schemes of the Prelude names the sugar uses as terms of
//! `Elab.Scheme` in Lean (`formal/Formal/Elab/PreludeSchemes.lean`), by the
//! translation of Р188 (`docs/formal/elaboration.md`):
//!
//! 1. `(T ā)^q` is `T q ā` (`appQ`) for a type with hidden storage and for
//!    the class variable over containers; data without storage and a type
//!    variable drop `q`; `(A → B)^q` is `.arr φ A B q`, without `q` `.cnil`;
//!    an arrow with rows is at the stage `R`.
//! 2. `∀v. a^v → …` of a parameter is instantiated `v := a`.
//! 3. A qualifier in a negative position is a variable: a single variable
//!    of the checker stays itself at its first such position, the rest are
//!    new; a missing qualifier of a stored type is new.
//! 4. In a positive position a variable that qualified negative positions
//!    is what those arguments reach, left to right through `cun`: `q ∪ ā` of
//!    data, `q` and the new variables inside of a function.
//! 5. Binders: the class parameter, the checker's `∀` that remain in their
//!    order, then the new variables in the order they appear.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::iter;

use flick_core::iface::Decl;
use flick_core::types::{self as fc, show_type};

/// The names `Sugar` uses, by module.
pub const SUGAR: [(&str, &str); 11] = [
    ("Prelude", "flat_map"),
    ("Prelude", "map2"),
    ("Prelude", "pure"),
    ("Prelude", "nothing"),
    ("Prelude", "iter"),
    ("Seq", "flat_map"),
    ("Seq", "singleton"),
    ("Seq", "empty"),
    ("Seq", "to_list"),
    ("Seq", "each_effect"),
    ("Prelude", "from_result"),
];

/// Types with hidden storage: the qualifier of a value is their argument.
const STORED: [&str; 1] = ["Seq"];

/// A type of `Elab`, its variables by name.
#[derive(Clone)]
enum Ty {
    Var(String),
    Con(String),
    App(Box<Ty>, Box<Ty>),
    /// `.arr φ a b q`, `R` when `true`
    Arr(bool, Box<Ty>, Box<Ty>, Box<Ty>),
    Nil,
    Un(Box<Ty>, Box<Ty>),
    /// a positive qualifier, its variables of the checker, before rule 4
    Pos(Vec<String>),
}

/// `x₁ ∪ … ∪ xₙ`, nested to the left.
fn cun(xs: Vec<Ty>) -> Ty {
    xs.into_iter()
        .reduce(|a, b| Ty::Un(Box::new(a), Box::new(b)))
        .unwrap_or(Ty::Nil)
}

fn app(head: Ty, args: impl IntoIterator<Item = Ty>) -> Ty {
    args.into_iter()
        .fold(head, |h, a| Ty::App(Box::new(h), Box::new(a)))
}

#[derive(Default)]
struct Tr {
    /// the class variable over containers
    container: Option<String>,
    /// `v := a` of rule 2
    inst: BTreeMap<String, String>,
    /// variables that stayed themselves at a negative position
    kept: BTreeSet<String>,
    /// what each qualified argument reaches, by its variable (rule 4)
    reach: BTreeMap<String, Vec<Ty>>,
    fresh: Vec<String>,
    /// the type constructors, in the order they appear
    cons: Vec<String>,
}

/// What a qualified argument reaches besides its qualifier.
enum Site {
    Data(Vec<Ty>),
    /// a function: the new variables from this one on
    Fun(usize),
}

impl Tr {
    fn new_var(&mut self) -> Ty {
        let name = format!("'{}", self.fresh.len());
        self.fresh.push(name.clone());
        Ty::Var(name)
    }

    fn ty(&mut self, t: &fc::Type, neg: bool) -> Ty {
        match t {
            fc::Type::Var(a) => Ty::Var(a.clone()),
            fc::Type::Con { name, .. } => {
                let c = name.to_lowercase();
                if !self.cons.contains(&c) {
                    self.cons.push(c.clone());
                }
                Ty::Con(c)
            }
            fc::Type::App { .. } => self.data(t, None, neg),
            fc::Type::Fun {
                param,
                effects,
                result,
            } => self.arrow(param, effects.is_some(), result, None, neg),
            fc::Type::Captured { ty, captures, .. } | fc::Type::Bounded { ty, captures } => {
                match self.open(ty) {
                    fc::Type::Fun {
                        param,
                        effects,
                        result,
                    } => self.arrow(param, effects.is_some(), result, Some(captures), neg),
                    fc::Type::App { .. } => self.data(ty, Some(captures), neg),
                    // a type variable: its bound is not modelled
                    fc::Type::Var(_) => self.ty(ty, neg),
                    other => panic!("Р188: no qualifier on {}", show_type(other)),
                }
            }
            fc::Type::Forall { .. } => {
                let body = self.open(t);
                self.ty(body, neg)
            }
            other => panic!("Р188: no translation of {}", show_type(other)),
        }
    }

    /// The body of `∀v. a^v → …`, with `v := a` (rule 2).
    fn open<'t>(&mut self, t: &'t fc::Type) -> &'t fc::Type {
        let fc::Type::Forall {
            vars,
            constraints,
            body,
        } = t
        else {
            return t;
        };
        assert!(constraints.is_empty(), "Р188: a context of rank 2");
        for v in vars {
            let a = bound(body, &v.name)
                .unwrap_or_else(|| panic!("Р188: `{}` bounds no variable", v.name));
            self.inst.insert(v.name.clone(), a);
        }
        body
    }

    fn arrow(
        &mut self,
        param: &fc::Type,
        runtime: bool,
        result: &fc::Type,
        q: Option<&fc::Type>,
        neg: bool,
    ) -> Ty {
        let start = self.fresh.len();
        let a = self.ty(param, !neg);
        let b = self.ty(result, neg);
        let q = match q {
            Some(q) => self.qual(q, neg, Site::Fun(start)),
            None => Ty::Nil,
        };
        Ty::Arr(runtime, Box::new(a), Box::new(b), Box::new(q))
    }

    /// `T ā` with the qualifier `q` of its value.
    fn data(&mut self, t: &fc::Type, q: Option<&fc::Type>, neg: bool) -> Ty {
        let (head, args) = t.spine();
        let stored = match head {
            fc::Type::Var(m) => self.container.as_ref() == Some(m),
            fc::Type::Con { name, .. } => STORED.contains(&name.as_str()),
            _ => false,
        };
        let h = self.ty(head, neg);
        let args: Vec<Ty> = args.into_iter().map(|a| self.ty(a, neg)).collect();
        if !stored {
            return app(h, args);
        }
        let q = match q {
            Some(q) => self.qual(q, neg, Site::Data(args.clone())),
            None => self.new_var(),
        };
        app(h, iter::once(q).chain(args))
    }

    fn qual(&mut self, q: &fc::Type, neg: bool, site: Site) -> Ty {
        let mut atoms = Vec::new();
        self.atoms(q, &mut atoms);
        if !neg {
            return Ty::Pos(atoms);
        }
        let var = match &atoms[..] {
            [v] if self.kept.insert(v.clone()) => Ty::Var(v.clone()),
            _ => self.new_var(),
        };
        if let [v] = &atoms[..] {
            let rest = match site {
                Site::Data(args) => args,
                Site::Fun(start) => self.fresh[start..].iter().cloned().map(Ty::Var).collect(),
            };
            let reach = cun(iter::once(var.clone()).chain(rest).collect());
            self.reach.entry(v.clone()).or_default().push(reach);
        }
        var
    }

    fn atoms(&self, q: &fc::Type, out: &mut Vec<String>) {
        match q {
            fc::Type::Var(v) => out.push(self.inst.get(v).unwrap_or(v).clone()),
            fc::Type::Union(a, b) => {
                self.atoms(a, out);
                self.atoms(b, out);
            }
            other => panic!("Р188: no qualifier {}", show_type(other)),
        }
    }

    /// Rule 4 on the positive qualifiers.
    fn resolve(&self, t: Ty) -> Ty {
        let go = |t: Box<Ty>| Box::new(self.resolve(*t));
        match t {
            Ty::Pos(atoms) => cun(atoms
                .into_iter()
                .map(|v| match self.reach.get(&v) {
                    Some(sites) => cun(sites.clone()),
                    None => Ty::Var(v),
                })
                .collect()),
            Ty::App(a, b) => Ty::App(go(a), go(b)),
            Ty::Arr(r, a, b, q) => Ty::Arr(r, go(a), go(b), go(q)),
            Ty::Un(a, b) => Ty::Un(go(a), go(b)),
            leaf => leaf,
        }
    }
}

/// The type variable `a` of `a^v` in `t`.
fn bound(t: &fc::Type, v: &str) -> Option<String> {
    match t {
        fc::Type::Bounded { ty, captures } => match (&**ty, &**captures) {
            (fc::Type::Var(a), fc::Type::Var(w)) if w == v => Some(a.clone()),
            _ => bound(ty, v),
        },
        fc::Type::Fun { param, result, .. } => bound(param, v).or_else(|| bound(result, v)),
        fc::Type::Captured { ty, .. } => bound(ty, v),
        _ => None,
    }
}

fn occurs(t: &Ty, v: &str) -> bool {
    match t {
        Ty::Var(w) => w == v,
        Ty::App(a, b) | Ty::Un(a, b) => occurs(a, v) || occurs(b, v),
        Ty::Arr(_, a, b, q) => occurs(a, v) || occurs(b, v) || occurs(q, v),
        _ => false,
    }
}

fn lean(t: &Ty, vars: &[String], out: &mut String) {
    let arg = |t: &Ty, out: &mut String| {
        if matches!(t, Ty::Nil) {
            lean(t, vars, out);
        } else {
            out.push('(');
            lean(t, vars, out);
            out.push(')');
        }
    };
    match t {
        Ty::Var(v) => {
            let i = vars.iter().position(|w| w == v).expect("bound");
            let _ = write!(out, ".var {i}");
        }
        Ty::Con(c) => {
            let _ = write!(out, ".con {c}");
        }
        Ty::App(a, b) => {
            out.push_str(".app ");
            arg(a, out);
            out.push(' ');
            arg(b, out);
        }
        Ty::Arr(r, a, b, q) => {
            out.push_str(if *r { ".arr .R " } else { ".arr .P " });
            for (i, t) in [a, b, q].into_iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                arg(t, out);
            }
        }
        Ty::Nil => out.push_str(".cnil"),
        Ty::Un(a, b) => {
            out.push_str(".cun ");
            arg(a, out);
            out.push(' ');
            arg(b, out);
        }
        Ty::Pos(_) => unreachable!("resolved"),
    }
}

/// The `def` of one scheme: its parameters are the class of a method, then
/// the type constructors in the order they appear.
fn def(module: &str, name: &str, scheme: &fc::Type, class: Option<&str>) -> String {
    let fc::Type::Forall {
        vars,
        constraints,
        body,
    } = scheme
    else {
        panic!("Р188: {module}.{name} is not generic");
    };
    let mut tr = Tr::default();
    let mut ctx = Vec::new();
    if let Some(class) = class {
        let [c] = &constraints[..] else {
            panic!("Р188: {module}.{name} has a context besides its class");
        };
        let (_, args) = c.spine();
        let [fc::Type::Var(f)] = &args[..] else {
            panic!("Р188: the class of {module}.{name} is not over a variable");
        };
        tr.container = Some(f.clone());
        ctx.push((class.to_lowercase(), f.clone()));
    } else if !constraints.is_empty() {
        panic!("Р188: {module}.{name} has a context");
    }
    let t = tr.ty(body, false);
    let t = tr.resolve(t);
    let binders: Vec<String> = tr
        .container
        .iter()
        .chain(
            vars.iter()
                .map(|b| &b.name)
                .filter(|v| tr.container.as_ref() != Some(*v)),
        )
        .filter(|v| occurs(&t, v))
        .chain(&tr.fresh)
        .cloned()
        .collect();
    let params: Vec<&str> = ctx
        .iter()
        .map(|(c, _)| c.as_str())
        .chain(tr.cons.iter().map(String::as_str))
        .collect();
    let mut out = format!("/-- `{module}.{name} : {}` -/\n", show_type(scheme));
    let lean_name = if module == "Prelude" {
        name.to_string()
    } else {
        format!("{module}.{name}")
    };
    let _ = write!(
        out,
        "def {lean_name} ({} : Nat) : Scheme :=\n  ⟨{}, [",
        params.join(" "),
        binders.len()
    );
    for (i, (c, f)) in ctx.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let f = binders.iter().position(|v| v == f).expect("bound");
        let _ = write!(out, "⟨{c}, [.var {f}]⟩");
    }
    out.push_str("],\n    ");
    lean(&t, &binders, &mut out);
    out.push_str(", .cnil⟩\n");
    out
}

/// The text of `formal/Formal/Elab/PreludeSchemes.lean`.
pub fn prelude_schemes() -> String {
    let mut out = String::from(
        "-- Code generated by `just formal-schemes`. DO NOT EDIT.\n\n\
         import Formal.Elab.Syntax\n\n\
         /-!\n\
         # Elaboration: the schemes of the Prelude names the sugar uses\n\n\
         The checker's types of `Prelude.fl` and `Seq.fl` translated by Р188 (`docs/formal/elaboration.md`)\n\
         in `flick-model/src/translate/lean.rs`. A `def` takes the numbers of the class of a method and of\n\
         the type constructors it names.\n\
         -/\n\n\
         namespace Flick.Elab.PreludeSchemes\n",
    );
    for (module, name) in SUGAR {
        let export = fc::builtins()
            .iter()
            .filter(|i| i.module == module)
            .flat_map(|i| &i.exports)
            .find(|e| e.entity.name == name && e.scheme.is_some())
            .unwrap_or_else(|| panic!("no {module}.{name}"));
        let scheme = export.scheme.as_ref().expect("checked");
        out.push('\n');
        let class = export
            .parent
            .as_deref()
            .filter(|_| export.decl == Decl::Method);
        out.push_str(&def(module, name, scheme, class));
    }
    out.push_str("\nend Flick.Elab.PreludeSchemes\n");
    out
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    /// The committed file is what the checker's Prelude gives; with
    /// `FLICK_FORMAL_SCHEMES` set the test writes it.
    #[test]
    fn prelude_schemes_are_generated() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../formal/Formal/Elab/PreludeSchemes.lean"
        );
        let text = super::prelude_schemes();
        if env::var_os("FLICK_FORMAL_SCHEMES").is_some() {
            fs::write(path, text).expect("write PreludeSchemes.lean");
        } else {
            let committed = fs::read_to_string(path).unwrap_or_default();
            assert!(
                committed == text,
                "PreludeSchemes.lean is stale: run `just formal-schemes`"
            );
        }
    }
}
