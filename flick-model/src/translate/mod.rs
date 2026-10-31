//! Checked IR of flick-core to λFlick: each top-level item becomes a
//! global, as the nested `let`s of `docs/formal/core.md` §2.6. Classes
//! become dictionaries (§1.6): an instance is a function of its context's
//! dictionaries to the record of its methods and superclasses.
//!
//! A node without a λFlick form becomes `Term::Stuck` naming it, so a
//! program that never reaches it still runs.

mod derive;
mod expr;
pub mod types;

use std::rc::Rc;

use flick_core::ir::{self, ItemKind};
use flick_core::types::{Dict, InstanceInfo, default_method};

use self::expr::pat;
use crate::eval::Program;
use crate::native;
use crate::subst::binders;
use crate::syntax::{Arm, Const, Name, Path, Term};

/// The program of `modules`: those of a build and the built-in ones.
pub fn program<'a>(modules: impl IntoIterator<Item = &'a ir::Module>) -> Program {
    let mut program = Program::default();
    for module in modules {
        let mut tr = Tr::new(&module.name);
        for item in &module.items {
            tr.item(&item.kind, &mut program);
        }
    }
    program
}

/// The global an instance's dictionary is, by the head it is written with.
pub fn instance(module: &str, name: &str) -> Path {
    Path::new(module, &format!("instance {name}"))
}

/// The variable a dictionary of a given constraint is bound to.
pub fn given(name: &str) -> Name {
    format!("?{name}").into()
}

/// `λx₁. … λxₙ. body`.
pub fn lams(names: impl IntoIterator<Item = Name>, body: Term) -> Term {
    let names: Vec<Name> = names.into_iter().collect();
    names
        .into_iter()
        .rev()
        .fold(body, |body, x| Term::Lam(x, Rc::new(body)))
}

struct Tr {
    module: String,
    fresh: usize,
}

impl Tr {
    fn new(module: &str) -> Self {
        Tr {
            module: module.to_owned(),
            fresh: 0,
        }
    }

    fn fresh(&mut self, what: &str) -> Name {
        self.fresh += 1;
        format!("${what}{}", self.fresh).into()
    }

    fn path(&self, name: &str) -> Path {
        Path::new(&self.module, name)
    }

    fn item(&mut self, item: &ItemKind, program: &mut Program) {
        let globals = &mut program.globals;
        match item {
            ItemKind::Value { name, dicts, body } => {
                let value = lams(dicts.iter().map(|d| given(d)), self.expr(body));
                globals.insert(self.path(name), value);
            }
            ItemKind::Destructure { pattern, body } => {
                let value = Rc::new(self.expr(body));
                let pat = pat(pattern);
                let mut names = Vec::new();
                binders(&pat, &mut names);
                for x in names {
                    let arm = Arm {
                        pat: pat.clone(),
                        guard: None,
                        body: Term::Var(x.clone()),
                    };
                    globals.insert(self.path(&x), Term::Match(value.clone(), Rc::from([arm])));
                }
            }
            ItemKind::Foreign(f) => {
                globals.insert(self.path(&f.name), foreign(f));
            }
            ItemKind::Instance { info, methods } => {
                let path = instance(&info.module, &info.name);
                if info.derived.is_some() {
                    program.derived.insert(path.clone());
                }
                let value = self.instance(info, methods);
                program.globals.insert(path, value);
            }
            ItemKind::Constructor(_)
            | ItemKind::Service { .. }
            | ItemKind::Refined { .. }
            | ItemKind::Schema(_) => {}
        }
    }

    /// `λctx. { methods, $Super = …, defaults }` (§1.6).
    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, ir::Expr)]) -> Term {
        let context: Vec<Name> = info.context.iter().map(|d| given(d)).collect();
        let this = Term::apps(
            Term::Global(instance(&info.module, &info.name)),
            context.iter().map(|d| Term::Var(d.clone())),
        );
        let mut fields: Vec<(Name, Term)> = match &info.derived {
            Some((dicts, constructors)) => {
                let dicts: Vec<Term> = dicts.iter().map(dict).collect();
                derive::methods(&info.class.1, &dicts, constructors.as_deref())
            }
            None => methods
                .iter()
                .map(|(m, e)| (Name::from(m.as_str()), self.expr(e)))
                .collect(),
        };
        for (class, d) in &info.supers {
            fields.push((format!("${class}").into(), dict(d)));
        }
        let (class_module, class) = &info.class;
        for (method, own) in &info.defaults {
            let default = Term::Global(Path::new(class_module, &default_method(class, method)));
            let call = Term::app(default, this.clone());
            // shortcut: a default without dictionaries of its own is taken to be a function; eta-expanded so the dictionary is not built while it is built
            let field = if *own == 0 {
                let x = self.fresh("x");
                Term::lam(&x, Term::app(call, Term::Var(x.clone())))
            } else {
                let own: Vec<Name> = (0..*own).map(|_| self.fresh("d")).collect();
                lams(
                    own.clone(),
                    Term::apps(call, own.into_iter().map(Term::Var)),
                )
            };
            fields.push((method.as_str().into(), field));
        }
        lams(context, Term::Record(fields.into()))
    }
}

/// The term of a dictionary flick-core inserted.
fn dict(d: &Dict) -> Term {
    match d {
        Dict::Instance { module, name, args } => {
            let args: Vec<Term> = args.iter().map(dict).collect();
            Term::apps(Term::Global(instance(module, name)), args)
        }
        Dict::Given(name) => Term::Var(given(name)),
        Dict::Super { dict: inner, class } => {
            Term::Field(Rc::new(dict(inner)), format!("${class}").into())
        }
        Dict::Structural { class, fields } => {
            let fields: Vec<Term> = fields.iter().map(dict).collect();
            derive::structural(class, fields)
        }
        Dict::Primitive { constraint, .. } => {
            Term::Stuck(format!("the row primitive `{constraint}`").into())
        }
        Dict::Error => Term::Stuck("a dictionary of a type with an error".into()),
    }
}

/// The constant a foreign binding stands for: a constant of Γ₀ or a
/// native, with every argument the binding takes in Flick.
fn foreign(f: &ir::Foreign) -> Term {
    let arity = f.dicts + f.stages.len();
    let module = f.module.as_deref().unwrap_or("global");
    let core = match (module, f.export.as_str()) {
        ("@flick-lang/runtime", "refMake") => Some(Const::RefMake),
        ("@flick-lang/runtime", "refGet") => Some(Const::RefGet),
        ("@flick-lang/runtime", "refSet") => Some(Const::RefSet),
        _ => None,
    };
    match core {
        Some(c) => Term::konst(c),
        None => match native::find(module, &f.export) {
            Some(n) if n.arity == arity => Term::konst(Const::Native(n)),
            Some(n) => Term::Stuck(
                format!(
                    "the native {n:?} takes {} arguments, the binding {arity}",
                    n.arity
                )
                .into(),
            ),
            None => {
                Term::Stuck(format!("no native for `{}` ({module} {})", f.name, f.export).into())
            }
        },
    }
}
