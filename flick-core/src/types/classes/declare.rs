//! Instance declarations and their coherence: one canonical instance per
//! constraint, without overlap or orphans, with context-assisted coverage of
//! functional dependencies and structurally shrinking instance contexts.
//! Instances are known before any definition is checked; their methods,
//! superclasses and derived fields are checked after.

use std::collections::HashSet;

use super::solve::Outcome;
use super::{Dict, NUMERIC_CLASSES, RECORD_CLASSES, split, vars, which};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::convert::{Free, is_type};
use crate::types::env::Instance;
use crate::types::kinds::Kind;
use crate::types::lang::{self, Lang};
use crate::types::ty::{Binder, Type};
use crate::types::typing::range;
use crate::types::{Checker, Constructor, Form, InstanceInfo, env, passes_dictionary};

/// An instance this module declares, written or derived.
pub struct Declared<'n> {
    pub decl: &'n Node,
    pub span: Span,
    pub instance: Instance,
    /// For a derived instance, the field types its class must cover.
    pub fields: Option<Vec<Type>>,
}

impl<'n> Checker<'_> {
    /// Superclasses must not lead back to their class.
    pub fn declare_classes(&mut self, decls: &[&Node]) {
        let module = self.env.module.clone();
        let classes: Vec<(String, Span)> = decls
            .iter()
            .filter(|d| d.kind == K::ClassDecl)
            .filter_map(|d| Some((self.con_token(d)?, self.span(d))))
            .collect();
        let edges: Vec<Vec<usize>> = classes
            .iter()
            .map(|(name, _)| {
                let supers = self.class_info(&module, name).map(|i| i.supers);
                supers
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| {
                        let ((m, n), _) = split(s)?;
                        (m == module).then(|| classes.iter().position(|(c, _)| c == n))?
                    })
                    .collect()
            })
            .collect();
        let mut reported = vec![false; classes.len()];
        for start in 0..classes.len() {
            if reported[start] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(start, &edges) else {
                continue;
            };
            let names: Vec<&str> = cycle
                .iter()
                .chain([&start])
                .map(|&i| classes[i].0.as_str())
                .collect();
            for &i in &cycle {
                reported[i] = true;
            }
            self.error(
                codes::C008,
                classes[start].1,
                format!(
                    "the superclasses of `{}` lead back to it: {}",
                    classes[start].0,
                    names.join(" -> ")
                ),
            );
        }
    }

    /// Registers the instances of the module in source order: `instance`
    /// declarations and `deriving` clauses.
    pub fn declare_instances(
        &mut self,
        decls: &[&'n Node],
        refined: &[(&Node, Type)],
    ) -> Vec<Declared<'n>> {
        let mut out = Vec::new();
        for &decl in decls {
            match decl.kind {
                K::InstanceDecl => {
                    if let Some(instance) = self.instance_head(decl) {
                        let span = self.span(decl);
                        if self.admit(&instance, span) {
                            out.push(Declared {
                                decl,
                                span,
                                instance,
                                fields: None,
                            });
                        }
                    }
                }
                K::TypeDecl | K::ErrorDecl => out.extend(self.derive(decl, refined)),
                _ => {}
            }
        }
        out
    }

    /// The head and context of an `instance`, over its own variables.
    fn instance_head(&mut self, decl: &Node) -> Option<Instance> {
        let mark = self.tyvars.len();
        let reported = self.diagnostics.len();
        let mut context = Vec::new();
        let mut head = None;
        for part in decl.nodes() {
            match part.kind {
                K::Context => {
                    for c in part.nodes() {
                        context.extend(self.constraints(c, Free::Quantify));
                    }
                }
                kind if is_type(kind) => {
                    head = Some(self.ty(part, &Kind::Constraint, Free::Quantify));
                }
                _ => {}
            }
        }
        let vars: Vec<Binder> = self.tyvars[mark..]
            .iter()
            .map(|v| Binder {
                name: v.name.clone(),
                kind: v.kind.clone(),
            })
            .collect();
        self.tyvars.truncate(mark);
        if self.diagnostics.len() > reported {
            return None;
        }
        let vars = vars
            .into_iter()
            .map(|b| Binder {
                kind: self.kinds.zonk(&b.kind, true),
                name: b.name,
            })
            .collect();
        let head = self.canon(&head?);
        let ((module, name), _) = split(&head)?;
        self.class_info(module, name)?;
        // the name keys the dictionary: types of one name from two modules
        // must not share it
        let shown = self.show(&head);
        let taken = self
            .env
            .own_instances()
            .any(|i| i.module == self.env.module && i.name == shown);
        Some(Instance {
            module: self.env.module.clone(),
            name: if taken {
                self.show_qualified(&head)
            } else {
                shown
            },
            vars,
            context: context.iter().map(|c| self.canon(c)).collect(),
            head,
        })
    }

    /// Checks the coherence rules and makes the instance known; `false`
    /// when it cannot be used.
    pub(super) fn admit(&mut self, instance: &Instance, span: Span) -> bool {
        let Some(((cm, cn), args)) = split(&instance.head) else {
            return false;
        };
        let (cm, cn) = (cm.to_owned(), cn.to_owned());
        let args: Vec<Type> = args.into_iter().cloned().collect();
        let Some(info) = self.class_info(&cm, &cn) else {
            return false;
        };
        let name = &instance.name;
        if which(&RECORD_CLASSES, &cm, &cn).is_some() {
            self.error(
                codes::C012,
                span,
                format!("`{cn}` is a row primitive: the compiler solves it, and a program cannot declare instances of it"),
            );
            return false;
        }
        if which(&NUMERIC_CLASSES, &cm, &cn).is_some()
            && instance.module != lang::item(Lang::Numeric).module
        {
            self.error(
                codes::C012,
                span,
                format!("`{cn}` is closed: the module `Numeric` has every instance of it, for `Int`, `Float` and `BigInt`"),
            );
            return false;
        }
        // `for` iterates its sources as JS and Go containers, without `iter`
        if lang::is(Lang::Iterable, &cm, &cn)
            && instance.module != lang::item(Lang::Instances).module
        {
            self.error(
                codes::C012,
                span,
                "`Iterable` is closed: `for` iterates a `List` or a `Seq` as it is, and the module `Instances` has the instances for them".to_owned(),
            );
            return false;
        }
        if Self::structural(&cm, &cn)
            && matches!(args.first(), Some(Type::Record(_) | Type::Tuple(_)))
        {
            self.diagnostics.push(
                Diagnostic::error(
                    codes::C012,
                    format!("`{name}` is built in: records and tuples have structural `{cn}`, shared by every alias of the same form"),
                    span,
                ).with_fix(|| ("declare a nominal type to give it its own instance", Vec::new())),
            );
            return false;
        }
        self.ownership(instance, &cm, &cn, args.first(), span);
        self.terminates(instance, span);
        let rules = if info.fundeps.is_empty() {
            Vec::new()
        } else {
            self.coverage_rules(&instance.context)
        };
        for fd in &info.fundeps {
            let mut covered: HashSet<String> =
                fd.from.iter().flat_map(|&i| vars(&args[i])).collect();
            loop {
                let mut changed = false;
                for (from, to) in &rules {
                    if from.iter().all(|v| covered.contains(v)) {
                        for v in to {
                            if !covered.contains(v) {
                                covered.insert(v.clone());
                                changed = true;
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            let uncovered = fd
                .to
                .iter()
                .flat_map(|&j| vars(&args[j]))
                .find(|v| !covered.contains(v));
            if let Some(v) = uncovered {
                let dep = fundep_text(&info.params, fd);
                self.error(
                    codes::C005,
                    span,
                    format!("`{v}` in `{name}` is required by `{dep}` but is not determined by the head or its context"),
                );
                return false;
            }
        }
        if self.conflicts(instance, &info, &args, span) {
            return false;
        }
        self.env.add_instance(instance.clone());
        true
    }

    /// Type-variable dependencies from the context and its superclasses.
    fn coverage_rules(&self, context: &[Type]) -> Vec<(Vec<String>, Vec<String>)> {
        fn visit(
            checker: &Checker<'_>,
            constraint: &Type,
            path: &mut Vec<(String, String)>,
            rules: &mut Vec<(Vec<String>, Vec<String>)>,
        ) {
            let constraint = checker.canon(constraint);
            let Some(((module, name), args)) = split(&constraint) else {
                return;
            };
            if path.iter().any(|(m, n)| m == module && n == name) {
                return;
            }
            let Some(info) = checker.class_info(module, name) else {
                return;
            };
            let variables = |positions: &[usize]| {
                let mut out = Vec::new();
                for &i in positions {
                    for v in vars(args.get(i)?) {
                        if !out.contains(&v) {
                            out.push(v);
                        }
                    }
                }
                Some(out)
            };
            for fd in &info.fundeps {
                let (Some(from), Some(to)) = (variables(&fd.from), variables(&fd.to)) else {
                    continue;
                };
                let rule = (from, to);
                if !rules.contains(&rule) {
                    rules.push(rule);
                }
            }
            path.push((module.to_owned(), name.to_owned()));
            for superclass in checker.supers_of(&constraint) {
                visit(checker, &superclass, path, rules);
            }
            path.pop();
        }

        let mut rules = Vec::new();
        let mut path = Vec::new();
        for constraint in context {
            visit(self, constraint, &mut path, &mut rules);
        }
        rules
    }

    /// Only the package of the class or of the nominal type at the head of
    /// the first argument may declare an instance.
    fn ownership(
        &mut self,
        instance: &Instance,
        cm: &str,
        cn: &str,
        first: Option<&Type>,
        span: Span,
    ) {
        let local = self.options.local;
        let first = first.map(|t| t.spine().0);
        if local(cm)
            || first.is_some_and(|h| matches!(h, Type::Con { module, .. } if local(module)))
        {
            return;
        }
        let message = match first {
            Some(Type::Con { name, .. }) => format!(
                "orphan instance `{}`: only the package of class `{cn}` or of type `{name}`, its first type, may declare it",
                instance.name
            ),
            _ => format!(
                "orphan instance `{}`: only the package of class `{cn}` may declare it, as its first type is not a nominal type of this package",
                instance.name
            ),
        };
        self.diagnostics
            .push(Diagnostic::error(codes::C006, message, span).with_fix(|| {
            let fix = match first {
                Some(Type::Con { name, .. }) => format!(
                    "wrap the type in one of this package: `type Local{name} = Local{name} {name}`"
                ),
                _ => "declare a nominal type in this package for the first type".to_owned(),
            };
            (fix, Vec::new())
        }));
    }

    /// Every variable of the context is in the head, and every context
    /// constraint is smaller than the head without repeating a variable more
    /// often. Resolution also has a step/depth budget.
    fn terminates(&mut self, instance: &Instance, span: Span) {
        let head_vars = vars(&instance.head);
        let size = |c: &Type| {
            let mut n = 0;
            split(c).map_or(0, |(_, args)| {
                for a in &args {
                    a.visit(&mut |_| n += 1);
                }
                n
            })
        };
        for c in &instance.context {
            let shown = self.show(c);
            if let Some(v) = vars(c).into_iter().find(|v| !head_vars.contains(v)) {
                self.error(
                    codes::C007,
                    span,
                    format!(
                        "type variable `{v}` of the context `{shown}` is not in the head `{}`",
                        instance.name
                    ),
                );
                continue;
            }
            let more = vars(c).into_iter().find(|v| {
                let count = |vs: &[String]| vs.iter().filter(|x| *x == v).count();
                count(&vars(c)) > count(&head_vars)
            });
            if more.is_some() || size(c) >= size(&instance.head) {
                self.error(
                    codes::C007,
                    span,
                    format!(
                        "the context `{shown}` is not smaller than the head `{}`, so resolving it might never end",
                        instance.name
                    ),
                );
            }
        }
    }

    /// After every instance is known: superclasses hold, derived fields have
    /// instances, and methods match the class.
    pub fn check_instances(&mut self, declared: &[Declared]) {
        for d in declared {
            self.classes.steps = 0;
            let name = d.instance.name.clone();
            let mut supers = Vec::new();
            let declared = self.supers_of(&d.instance.head);
            for (i, s) in declared.iter().enumerate() {
                let shown = self.show(s);
                match self.entails(s, &d.instance.context) {
                    Outcome::Solved(dict) => supers.push((super::super_slot(&declared, i), dict)),
                    Outcome::Budget(_) => self.error(
                        codes::C009,
                        d.span,
                        format!("resolving superclass `{shown}` of `{name}` exceeds the budget"),
                    ),
                    _ => self.diagnostics.push(
                        Diagnostic::error(
                            codes::C001,
                            format!("`{name}` needs an instance `{shown}` of its superclass"),
                            d.span,
                        )
                        .with_fix(|| (format!("declare `instance {shown}`"), Vec::new())),
                    ),
                }
            }
            let (derived, defaults) = if let Some(fields) = &d.fields {
                (Some(self.derived_fields(d, fields)), Vec::new())
            } else {
                (None, self.instance_methods(d.decl, &d.instance))
            };
            self.record_instance(d, supers, derived, defaults);
        }
    }
}

/// `a b -> c` with the class's parameter names.
pub(super) fn fundep_text(params: &[String], fd: &env::Fundep) -> String {
    let names = |is: &[usize]| {
        is.iter()
            .map(|&i| params.get(i).map_or("?", String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!("{} -> {}", names(&fd.from), names(&fd.to))
}

impl Checker<'_> {
    /// What code generation needs to build the dictionary of an instance.
    fn record_instance(
        &mut self,
        d: &Declared,
        supers: Vec<(String, Dict)>,
        fields: Option<Vec<Dict>>,
        defaults: Vec<(String, usize)>,
    ) {
        let Some(((cm, cn), _)) = split(&d.instance.head) else {
            return;
        };
        let class = (cm.to_owned(), cn.to_owned());
        let context = d
            .instance
            .context
            .iter()
            .filter(|c| passes_dictionary(c))
            .map(|c| self.show(&self.canon(c)))
            .collect();
        let derived = fields.map(|fields| {
            let target = d.instance.head.spine().1.first().map(|t| (*t).clone());
            let constructors = target.and_then(|t| self.derived_constructors(&t));
            (fields, constructors)
        });
        let info = InstanceInfo {
            module: d.instance.module.clone(),
            name: d.instance.name.clone(),
            class,
            span: range(d.span),
            context,
            supers,
            derived,
            defaults,
        };
        self.typing.instances.push(info);
    }

    /// The constructors of the type a derived instance is for; `None` when
    /// it has no variants.
    fn derived_constructors(&self, target: &Type) -> Option<Vec<Constructor>> {
        let Type::Con { module, name } = target.spine().0 else {
            return None;
        };
        let names = match &self.env.type_def(module, name)?.form {
            Form::Adt { constructors, .. } => constructors.clone(),
            Form::Error => vec![name.clone()],
            _ => return None,
        };
        Some(
            names
                .into_iter()
                .map(|c| Constructor {
                    module: module.clone(),
                    ty: name.clone(),
                    arity: self.constructor_fields(&c).len(),
                    rep: self.env.constructor_rep(module, name, &c),
                    name: c,
                })
                .collect(),
        )
    }
}
