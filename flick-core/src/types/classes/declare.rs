//! Instance declarations and their coherence: one canonical instance per
//! constraint, without overlap or orphans, with strict coverage of
//! functional dependencies and a context that shrinks, so resolution ends.
//! Instances are known before any definition is checked; their methods,
//! superclasses and derived fields are checked after.

use super::matching::{Subst, rename, resolve, unify_heads};
use super::prims::PRIMITIVES;
use super::solve::Outcome;
use super::{ClassInfo, RECORD, split, vars};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::convert::{Free, is_type};
use crate::types::env::Instance;
use crate::types::kinds::Kind;
use crate::types::ty::{Binder, Type};

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
            let Some(cycle) = crate::resolve::shortest_cycle(start, &edges) else {
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
        Some(Instance {
            module: self.env.module.clone(),
            name: self.show(&head),
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
        if cm == RECORD && PRIMITIVES.contains(&cn.as_str()) {
            self.error(
                codes::C012,
                span,
                format!("`{cn}` is a row primitive: the compiler solves it, and a program cannot declare instances of it"),
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
                )
                .with_fix("declare a nominal type to give it its own instance", Vec::new()),
            );
            return false;
        }
        self.ownership(instance, &cm, &cn, args.first(), span);
        self.terminates(instance, span);
        for fd in &info.fundeps {
            let from: Vec<String> = fd.from.iter().flat_map(|&i| vars(&args[i])).collect();
            let uncovered = fd
                .to
                .iter()
                .flat_map(|&j| vars(&args[j]))
                .find(|v| !from.contains(v));
            if let Some(v) = uncovered {
                let dep = fundep_text(&info.params, fd);
                self.error(
                    codes::C005,
                    span,
                    format!("`{v}` in `{name}` is determined by `{dep}`, so it must appear in the types that determine it"),
                );
                return false;
            }
        }
        if self.conflicts(instance, &info, &args, span) {
            return false;
        }
        self.env.instances.push(instance.clone());
        true
    }

    /// Reports another instance of the class that this one overlaps or
    /// disagrees with on a functional dependency.
    fn conflicts(
        &mut self,
        instance: &Instance,
        info: &ClassInfo,
        args: &[Type],
        span: Span,
    ) -> bool {
        let name = &instance.name;
        let class = split(&instance.head).map(|(k, _)| k);
        let others: Vec<Instance> = self
            .env
            .instances
            .iter()
            .filter(|i| split(&i.head).map(|(k, _)| k) == class)
            .cloned()
            .collect();
        for other in others {
            let names: Vec<String> = other.vars.iter().map(|v| v.name.clone()).collect();
            let theirs = rename(&other.head, &names);
            let Some((_, theirs)) = split(&theirs) else {
                continue;
            };
            let from = |other: &Instance| {
                if other.module == self.env.module {
                    String::new()
                } else {
                    format!(" of module `{}`", other.module)
                }
            };
            for fd in &info.fundeps {
                let mut subst = Subst::new();
                let same = fd
                    .from
                    .iter()
                    .all(|&i| unify_heads(&args[i], theirs[i], &mut subst));
                if same
                    && !fd
                        .to
                        .iter()
                        .all(|&j| unify_heads(&args[j], theirs[j], &mut subst))
                {
                    let dep = fundep_text(&info.params, fd);
                    self.error(
                        codes::C004,
                        span,
                        format!(
                            "`{name}` and `{}`{} break the functional dependency `{dep}`: the same types determine different ones",
                            other.name,
                            from(&other)
                        ),
                    );
                    return true;
                }
            }
            let mut subst = Subst::new();
            if theirs
                .iter()
                .zip(args)
                .all(|(a, b)| unify_heads(a, b, &mut subst))
            {
                let common = self.show(&resolve(&instance.head, &subst));
                self.error(
                    codes::C003,
                    span,
                    format!(
                        "`{name}` overlaps `{}`{}: both apply to `{common}`, and instances may not overlap",
                        other.name,
                        from(&other)
                    ),
                );
                return true;
            }
        }
        false
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
        let (message, fix) = match first {
            Some(Type::Con { name, .. }) => (
                format!(
                    "orphan instance `{}`: only the package of class `{cn}` or of type `{name}`, its first type, may declare it",
                    instance.name
                ),
                format!(
                    "wrap the type in one of this package: `type Local{name} = Local{name} {name}`"
                ),
            ),
            _ => (
                format!(
                    "orphan instance `{}`: only the package of class `{cn}` may declare it, as its first type is not a nominal type of this package",
                    instance.name
                ),
                "declare a nominal type in this package for the first type".to_owned(),
            ),
        };
        self.diagnostics
            .push(Diagnostic::error(codes::C006, message, span).with_fix(fix, Vec::new()));
    }

    /// Every variable of the context is in the head, and every context
    /// constraint is smaller than the head without repeating a variable more
    /// often, so resolution ends.
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
            for s in self.supers_of(&d.instance.head) {
                let shown = self.show(&s);
                match self.entails(&s, &d.instance.context) {
                    Outcome::Solved(_) => {}
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
                        .with_fix(format!("declare `instance {shown}`"), Vec::new()),
                    ),
                }
            }
            match &d.fields {
                Some(fields) => self.derived_fields(d, fields),
                None => self.instance_methods(d.decl, &d.instance),
            }
        }
    }
}

/// `a b -> c` with the class's parameter names.
fn fundep_text(params: &[String], fd: &crate::types::env::Fundep) -> String {
    let names = |is: &[usize]| {
        is.iter()
            .map(|&i| params.get(i).map_or("?", String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!("{} -> {}", names(&fd.from), names(&fd.to))
}
