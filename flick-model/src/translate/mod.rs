//! Checked IR of flick-core to λFlick: each top-level item becomes a
//! global, as the nested `let`s of `docs/formal/core.md` §2.6. Classes
//! become dictionaries (§1.6): an instance is a function of its context's
//! dictionaries to the record of its methods and superclasses.
//!
//! A node without a λFlick form becomes `Term::Stuck` naming it, so a
//! program that never reaches it still runs.

mod applications;
mod callbacks;
mod collection;
mod control_handlers;
mod derive;
mod expr;
#[cfg(test)]
mod expr_tests;
pub mod lean;
mod loops;
mod mutation;
mod mutation_errors;
mod mutation_handlers;
mod resumptions;
pub mod types;

use std::{collections::HashMap, rc::Rc};

use flick_core::ir::{self, ItemKind};
use flick_core::types::{Dict, InstanceInfo, OpClass, default_method};

use self::expr::pat;
use crate::decls::Decls;
use crate::eval::Program;
use crate::native;
use crate::subst::binders;
use crate::syntax::{Arm, Const, GETTER, Lit, Name, Path, RegionOp, Term};
use crate::types::Scheme;

fn control(e: &ir::Expr) -> bool {
    if let ir::ExprKind::Handle(h) = &e.kind
        && h.effects
            .iter()
            .flat_map(|e| &e.operations)
            .any(|o| o.class == OpClass::Control)
    {
        return true;
    }
    let mut found = false;
    e.each_child(&mut |c| found |= control(c));
    found
}

/// The program of `modules`: those of a build and the built-in ones.
pub fn program<'a>(modules: impl IntoIterator<Item = &'a ir::Module>) -> Program {
    program_with_decls(modules, &Decls::default())
}

/// [`program`], attaching local `let` / `let rec` signatures recorded in `decls`.
pub fn program_with_decls<'a>(
    modules: impl IntoIterator<Item = &'a ir::Module>,
    decls: &Decls,
) -> Program {
    let modules: Vec<_> = modules.into_iter().collect();
    let needs_functions = modules.iter().any(|m| {
        m.items
            .iter()
            .any(|i| matches!(&i.kind, ItemKind::Value { body, .. } if control(body)))
    });
    let functions = if needs_functions {
        Rc::new(
            modules
                .iter()
                .flat_map(|m| {
                    m.items.iter().filter_map(|i| {
                        if let ItemKind::Value { name, dicts, body } = &i.kind
                            && dicts.is_empty()
                        {
                            Some((Path::new(&m.name, name), body.clone()))
                        } else {
                            None
                        }
                    })
                })
                .collect(),
        )
    } else {
        Rc::default()
    };
    let mut program = Program::default();
    for module in modules {
        let mut tr = Tr::with_local_schemes(&module.name, Some(&decls.local_schemes));
        tr.functions = Rc::clone(&functions);
        for item in &module.items {
            tr.item(item, &mut program);
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

struct Tr<'a> {
    module: String,
    current_item: String,
    local_schemes: Option<&'a HashMap<(Name, String, Name), Scheme>>,
    fresh: usize,
    /// Each `let mut` variable in scope and the name of its current value,
    /// innermost last.
    versions: Vec<(String, Name)>,
    cells: Vec<(String, Name)>,
    resumptions: Vec<(String, String)>,
    functions: Rc<HashMap<Path, ir::Expr>>,
    control: Option<String>,
    error_action: Option<Name>,
    pub(super) control_loops: Vec<String>,
}

impl<'a> Tr<'a> {
    #[cfg(test)]
    fn new(module: &str) -> Tr<'static> {
        Tr::with_local_schemes(module, None)
    }

    fn with_local_schemes(
        module: &str,
        local_schemes: Option<&'a HashMap<(Name, String, Name), Scheme>>,
    ) -> Self {
        Tr {
            module: module.to_owned(),
            current_item: String::new(),
            local_schemes,
            fresh: 0,
            versions: Vec::new(),
            cells: Vec::new(),
            resumptions: Vec::new(),
            functions: Rc::default(),
            control: None,
            error_action: None,
            control_loops: Vec::new(),
        }
    }

    pub(super) fn local_scheme(&self, name: &str) -> Option<Rc<Scheme>> {
        let schemes = self.local_schemes?;
        if self.current_item.is_empty() {
            return None;
        }
        schemes
            .get(&(
                Name::from(self.module.as_str()),
                self.current_item.clone(),
                Name::from(name),
            ))
            .cloned()
            .map(Rc::new)
    }

    fn fresh(&mut self, what: &str) -> Name {
        self.fresh += 1;
        format!("${what}{}", self.fresh).into()
    }

    fn path(&self, name: &str) -> Path {
        Path::new(&self.module, name)
    }

    fn item(&mut self, item: &ir::Item, program: &mut Program) {
        self.current_item.clear();
        let globals = &mut program.globals;
        match &item.kind {
            ItemKind::Value { name, dicts, body } => {
                self.current_item.clone_from(name);
                let value = lams(dicts.iter().map(|d| given(d)), self.expr(body));
                globals.insert(self.path(name), value);
            }
            ItemKind::Destructure { pattern, body } => {
                self.current_item = format!("destructure {}..{}", item.span.start, item.span.end);
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
                let path = self.path(&f.name);
                let source = self.path(f.source_name.as_deref().unwrap_or(&f.name));
                globals.insert(path, foreign(source, f));
            }
            ItemKind::Instance { info, methods } => {
                let path = instance(&info.module, &info.name);
                if info.derived.is_some() {
                    program.derived.insert(path.clone());
                }
                let value = self.instance(info, methods);
                program.globals.insert(path, value);
            }
            ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }

    /// `λctx. { methods, $Super = …, defaults }` (§1.6).
    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, ir::Expr)]) -> Term {
        let context: Vec<Name> = info.context.iter().map(|d| given(d)).collect();
        let this = Term::apps(
            Term::Global(instance(&info.module, &info.name)),
            context.iter().map(|d| Term::Var(d.clone())),
        );
        // A derived instance whose methods lowering already wrote out is
        // translated from those methods, as both backends and the counted path
        // do; an `InstanceInfo.derived` without method bodies falls back to
        // `derive::methods`.
        let mut fields: Vec<(Name, Term)> = if methods.is_empty()
            && let Some((dicts, constructors)) = &info.derived
        {
            let dicts: Vec<Term> = dicts.iter().map(dict).collect();
            derive::methods(&info.class.1, &dicts, constructors.as_deref())
        } else {
            methods
                .iter()
                .map(|(m, e)| {
                    self.current_item = format!("instance {}::{m}", info.name);
                    (Name::from(m.as_str()), self.expr(e))
                })
                .collect()
        };
        for (class, d) in &info.supers {
            fields.push((format!("${class}").into(), dict(d)));
        }
        let (class_module, class) = &info.class;
        for (method, own, extra) in &info.defaults {
            let default = Term::Global(Path::new(class_module, &default_method(class, method)));
            // after the class's dictionary, those its default signature needs
            let call = Term::apps(Term::app(default, this.clone()), extra.iter().map(dict));
            // a getter, so the dictionary is not built while it is built and
            // a value default stays a value
            let field = if *own == 0 {
                Term::lam(GETTER, call)
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
        Dict::Value { module, name } => Term::Global(Path::new(module, name)),
        Dict::Opened { binder, field } => Term::Field(
            Rc::new(Term::Var(binder.as_str().into())),
            field.as_str().into(),
        ),
        // the pattern of the constructor whose value holds it binds it
        Dict::Field(local) => Term::Var(local.as_str().into()),
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
        // the record of `CallSite`'s two values, in label order
        Dict::Site { place, definition } => Term::Record(
            [("definition", definition), ("place", place)]
                .into_iter()
                .map(|(l, v)| (Name::from(l), Term::Lit(Lit::String(v.as_str().into()))))
                .collect(),
        ),
        Dict::Error => Term::Stuck("a dictionary of a type with an error".into()),
    }
}

/// The constant a foreign binding stands for: a constant of Γ₀ or a
/// native, with every argument the binding takes in Flick. The bindings
/// of regions, and those of `./res.mjs`, the resource of the probes of
/// `docs/formal/regions.md` (§6), are steps of the machine (§3.2).
fn foreign(path: Path, f: &ir::Foreign) -> Term {
    let arity = f.dicts + f.stages.len();
    let module = f.binding.from.as_deref().unwrap_or("global");
    // Keep the internal default-owner primitive operational even when it
    // is reached through an alias instead of the direct @lang call path.
    if module == "flick:rt" && f.binding.name == "scopeCurrent" {
        return Term::lam("_", Term::DefaultOwner);
    }
    if module == "flick:rt" && f.binding.name == "layerRun" {
        return Term::lam(
            "$layer",
            Term::lam(
                "$body",
                Term::Provide(
                    Rc::new(Term::Var("$layer".into())),
                    Rc::new(Term::app(Term::Var("$body".into()), Term::unit())),
                ),
            ),
        );
    }
    if module == "flick:rt" && f.binding.name == "scopeRun" {
        return Term::lam(
            "$body",
            Term::Scope(
                "$scope".into(),
                Rc::new(Term::app(
                    Term::Var("$body".into()),
                    Term::Var("$scope".into()),
                )),
            ),
        );
    }
    let core = match (module, f.binding.name.as_str()) {
        ("flick:rt", "refMake") => Some(Const::RefMake),
        ("flick:rt", "refGet") => Some(Const::RefGet),
        ("flick:rt", "refSet") => Some(Const::RefSet),
        (m, e) => region_op(m, e).map(|op| Const::Region(path, op)),
    };
    match core {
        Some(c) if c.arity() != arity => {
            Term::Stuck(format!("{c:?} takes {} arguments, the binding {arity}", c.arity()).into())
        }
        Some(c) => Term::konst(c),
        None => match native::find(module, &f.binding.name) {
            Some(n) if n.arity == arity => Term::konst(Const::Native(n)),
            Some(n) => Term::Stuck(
                format!(
                    "the native {n:?} takes {} arguments, the binding {arity}",
                    n.arity
                )
                .into(),
            ),
            None => {
                let name = f.source_name.as_deref().unwrap_or(&f.name);
                let unsupported = Term::Stuck(
                    format!("no native for `{name}` ({module} {})", f.binding.name).into(),
                );
                // Lookup of a callable does not invoke its host. Stop only
                // once its dictionaries and actual host-call arguments arrive;
                // later source arrows describe the unknown returned value.
                // A data constant (no stages) still fails at lookup.
                lams(
                    (0..f.dicts + f.taken()).map(|i| Name::from(format!("$foreign{i}"))),
                    unsupported,
                )
            }
        },
    }
}

fn region_op(module: &str, export: &str) -> Option<RegionOp> {
    Some(match (module, export) {
        ("./res.mjs", "open") => RegionOp::Acquire,
        ("./res.mjs", "use") => RegionOp::Use,
        ("./res.mjs", "raw") => RegionOp::Make,
        ("./res.mjs", "close") => RegionOp::Release,
        ("flick:rt", "fiberSpawn") => RegionOp::Spawn,
        ("flick:rt", "acquireRelease") => RegionOp::AcquireRelease,
        ("flick:rt", "arenaMake") => RegionOp::ArenaMake,
        ("flick:rt", "arenaAlloc") => RegionOp::ArenaAlloc,
        ("flick:rt", "arenaGet") => RegionOp::ArenaGet,
        ("flick:Scope", "flickAddFinalizer") => RegionOp::AddFinalizer,
        _ => return None,
    })
}

#[cfg(test)]
mod foreign_staging_tests {
    use crate::eval::{Machine, Outcome, Program};
    use crate::syntax::{Path, Term};
    use flick_core::ir::{Adapter, Binding, Foreign, ForeignMode};

    #[test]
    fn unsupported_foreign_waits_for_dictionaries_and_host_arguments_only() {
        for mode in [
            ForeignMode::Pure,
            ForeignMode::Sync,
            ForeignMode::Async,
            ForeignMode::Effect,
        ] {
            for (stages, arity, dicts) in [
                (vec![], 0, 0),
                (vec![true], 0, 0),
                (vec![false, true], 1, 1),
            ] {
                let foreign = Foreign {
                    name: "$raw_source".to_owned(),
                    source_name: Some("source".to_owned()),
                    mode,
                    binding: Binding {
                        from: Some("./unknown.mjs".to_owned()),
                        name: "hostExport".to_owned(),
                    },
                    stages,
                    dicts,
                    arity,
                    adapter: Adapter::default(),
                    catch: None,
                };
                let mut term = super::foreign(Path::new("Probe", "$raw_source"), &foreign);
                let program = Program::default();
                let arguments = foreign.dicts + foreign.taken();
                for _ in 0..arguments {
                    assert!(matches!(
                        Machine::new(&program, 100).run(term.clone()),
                        Outcome::Value(Term::Lam(..))
                    ));
                    term = Term::app(term, Term::unit());
                }
                assert!(
                    matches!(Machine::new(&program, 100).run(term), Outcome::Stuck(message) if message == "no native for `source` (./unknown.mjs hostExport)")
                );
            }
        }
    }

    #[test]
    fn derived_instance_without_ir_methods_falls_back_to_derive_methods() {
        use std::rc::Rc;

        use flick_core::ir::{Item, ItemKind};
        use flick_core::source::{FileId, Span};
        use flick_core::types::{Constructor, InstanceInfo, Rep};

        use crate::syntax::Lit;

        let con = Constructor {
            module: "App.Main".to_owned(),
            ty: "Flag".to_owned(),
            name: "Flag".to_owned(),
            arity: 0,
            rep: Rep::Tagged,
            dicts: 0,
        };
        let info = InstanceInfo {
            module: "App.Main".to_owned(),
            name: "Show Flag".to_owned(),
            class: ("Classes".to_owned(), "Show".to_owned()),
            span: (0, 0),
            context: Vec::new(),
            supers: Vec::new(),
            derived: Some((Vec::new(), Some(vec![con.clone()]))),
            defaults: Vec::new(),
            generic: None,
            enumerates: None,
        };
        let mut program = Program::default();
        super::Tr::new("App.Main").item(
            &Item {
                kind: ItemKind::Instance {
                    info: Box::new(info),
                    methods: Vec::new(),
                },
                span: Span::new(FileId::new(0), 0, 0),
            },
            &mut program,
        );
        let dict_path = super::instance("App.Main", "Show Flag");
        assert!(program.derived.contains(&dict_path));
        let call = Term::app(
            Term::Field(Rc::new(Term::Global(dict_path)), "show".into()),
            Term::konst_con(super::expr::con(&con)),
        );
        assert!(matches!(
            Machine::new(&program, 100).run(call),
            Outcome::Value(Term::Lit(Lit::String(text))) if &*text == "Flag"
        ));
    }
}
