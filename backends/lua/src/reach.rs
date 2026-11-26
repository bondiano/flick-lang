//! What a program reaches from its roots: the facts of the IR a Lua profile
//! may refuse, each at the source the program reached it from, and the
//! items code generation emits.
//!
//! A root is an entry's `main`, a test root, a host export, or, without an
//! entry or a test root, every export of an own module. A reference into a
//! module that is not the request's own (a built-in one) is blamed on the
//! own expression that left own code, so its facts point at the user's
//! source. An own module that is reached initializes its eager top-level
//! values too, and a reached instance reaches its superclasses and the
//! class defaults it takes.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{BuildRequest, ModuleRole};
use flick_ir::{
    Binding, Dict, Expr, ExprKind, Foreign, InstanceInfo, ItemKind, Program, Span, default_method,
};

pub(crate) enum Fact<'p> {
    /// `await`, which suspends until a task settles.
    Await,
    /// A reference to a foreign function of `module`.
    Foreign {
        module: &'p str,
        foreign: &'p Foreign,
    },
}

/// Where a fact was reached: its span, or the span of the own expression
/// the walk left own code at, with the global it went through.
pub(crate) struct Site {
    pub span: Span,
    pub via: Option<String>,
}

/// The own expression a reached item of a built-in module is blamed on,
/// and the global the walk went through.
#[derive(Clone, Debug)]
pub(crate) struct Blame {
    pub span: Span,
    pub via: String,
}

/// Every reached item, by module and item index, with the blame it was
/// first reached with: `None` for one reached from own code directly.
pub(crate) type Reached = BTreeMap<(usize, usize), Option<Blame>>;

struct Walker<'p, 'f> {
    program: &'p Program,
    own: BTreeSet<&'p str>,
    /// `(module, name)` of a value, a foreign function or a destructured
    /// binder, and of an instance: its module and item index.
    values: BTreeMap<(&'p str, &'p str), (usize, usize)>,
    instances: BTreeMap<(&'p str, &'p str), (usize, usize)>,
    seen: Reached,
    started: BTreeSet<usize>,
    queue: Vec<(usize, usize, Option<Blame>)>,
    report: &'f mut dyn FnMut(Fact<'p>, Site),
}

pub(crate) fn walk<'p>(
    program: &'p Program,
    request: &'p BuildRequest,
    report: &mut dyn FnMut(Fact<'p>, Site),
) -> Reached {
    let mut w = Walker {
        program,
        own: (request.modules.iter())
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect(),
        values: BTreeMap::new(),
        instances: BTreeMap::new(),
        seen: BTreeMap::new(),
        started: BTreeSet::new(),
        queue: Vec::new(),
        report,
    };
    for (mi, module) in program.modules.iter().enumerate() {
        for (ii, item) in module.items.iter().enumerate() {
            let key = |name: &'p str| (module.name.as_str(), name);
            match &item.kind {
                ItemKind::Value { name, .. } => {
                    w.values.insert(key(name), (mi, ii));
                }
                ItemKind::Foreign(foreign) => {
                    w.values.insert(key(&foreign.name), (mi, ii));
                }
                ItemKind::Destructure { pattern, .. } => {
                    let mut names = Vec::new();
                    pattern.binders(&mut names);
                    for name in names {
                        w.values.insert(key(name), (mi, ii));
                    }
                }
                ItemKind::Instance { info, .. } => {
                    w.instances.insert(key(&info.name), (mi, ii));
                }
                ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
            }
        }
    }
    let mut roots: Vec<(&str, &str)> = (request.entries.iter())
        .map(|e| (e.symbol.module.as_str(), e.symbol.name.as_str()))
        .chain(
            request
                .test_roots
                .iter()
                .map(|s| (s.module.as_str(), s.name.as_str())),
        )
        .collect();
    let exported =
        (request.exports.iter()).map(|e| (e.symbol.module.as_str(), e.symbol.name.as_str()));
    if roots.is_empty() {
        roots = (program.modules.iter())
            .filter(|m| w.own.contains(m.name.as_str()))
            .flat_map(|m| {
                m.exports
                    .values
                    .iter()
                    .map(|e| (m.name.as_str(), e.name.as_str()))
            })
            .collect();
    }
    roots.extend(exported);
    for (module, name) in roots {
        if let Some(&(mi, ii)) = w.values.get(&(module, name)) {
            w.enqueue(mi, ii, None);
            w.start(mi);
        }
    }
    while let Some((mi, ii, blame)) = w.queue.pop() {
        let item = &program.modules[mi].items[ii];
        item.each_body(&mut |body| w.expr(body, blame.as_ref()));
        if let ItemKind::Instance { info, .. } = &item.kind {
            w.instance(info, item.span, blame.as_ref());
        }
    }
    w.seen
}

impl<'p> Walker<'p, '_> {
    fn enqueue(&mut self, mi: usize, ii: usize, blame: Option<Blame>) {
        if let Entry::Vacant(seen) = self.seen.entry((mi, ii)) {
            seen.insert(blame.clone());
            self.queue.push((mi, ii, blame));
        }
    }

    /// What an instance's dictionary is built from besides its methods:
    /// its superclasses, the dictionaries its derived methods and its
    /// defaults take, and the class's default methods.
    fn instance(&mut self, info: &'p InstanceInfo, span: Span, blame: Option<&Blame>) {
        let derived = info.derived.iter().flat_map(|(fields, _)| fields);
        let extra = info.defaults.iter().flat_map(|(_, _, extra)| extra);
        for dict in (info.supers.iter().map(|(_, d)| d))
            .chain(derived)
            .chain(extra)
        {
            self.dict(dict, span, blame);
        }
        let (module, class) = &info.class;
        for (name, _, _) in &info.defaults {
            self.global(module, &default_method(class, name), span, blame);
        }
    }

    /// The eager top-level values of a reached own module.
    fn start(&mut self, mi: usize) {
        if !self.started.insert(mi) {
            return;
        }
        let program = self.program;
        for (ii, item) in program.modules[mi].items.iter().enumerate() {
            let eager = match &item.kind {
                ItemKind::Value { dicts, body, .. } => {
                    dicts.is_empty() && !matches!(body.kind, ExprKind::Lambda { .. })
                }
                ItemKind::Destructure { .. } => true,
                _ => false,
            };
            if eager {
                self.enqueue(mi, ii, None);
            }
        }
    }

    fn fact(&mut self, fact: Fact<'p>, span: Span, blame: Option<&Blame>) {
        let site = Site {
            span: blame.map_or(span, |b| b.span),
            via: blame.map(|b| b.via.clone()),
        };
        (self.report)(fact, site);
    }

    fn expr(&mut self, e: &'p Expr, blame: Option<&Blame>) {
        if let Some((head, name)) = effect_test_parts(e)
            && self.is_effect_test(head)
        {
            self.expr(head, blame);
            self.expr(name, blame);
            return;
        }
        match &e.kind {
            ExprKind::Await(_) => self.fact(Fact::Await, e.span, blame),
            ExprKind::Global { module, name, .. } => self.global(module, name, e.span, blame),
            ExprKind::Method { dict, .. } => self.dict(dict, e.span, blame),
            ExprKind::Dicts(_, dicts) => {
                for dict in dicts {
                    self.dict(dict, e.span, blame);
                }
            }
            _ => {}
        }
        e.each_child(&mut |child| self.expr(child, blame));
    }

    /// Whether `head` names the kernel's `testEffect`.
    fn is_effect_test(&self, head: &Expr) -> bool {
        let ExprKind::Global { module, name, .. } = &head.kind else {
            return false;
        };
        let Some(&(mi, ii)) = self.values.get(&(module.as_str(), name.as_str())) else {
            return false;
        };
        matches!(&self.program.modules[mi].items[ii].kind, ItemKind::Foreign(f) if is_effect_test(f))
    }

    /// A reference to `module.name`: its item is reached, and a foreign
    /// function is a fact at the reference.
    fn global(&mut self, module: &str, name: &str, span: Span, blame: Option<&Blame>) {
        let Some(&(mi, ii)) = self.values.get(&(module, name)) else {
            return;
        };
        let program = self.program;
        let target = &program.modules[mi];
        if let ItemKind::Foreign(foreign) = &target.items[ii].kind {
            let module = target.name.as_str();
            self.fact(Fact::Foreign { module, foreign }, span, blame);
        }
        self.reach(mi, ii, span, blame);
    }

    fn reach(&mut self, mi: usize, ii: usize, span: Span, blame: Option<&Blame>) {
        let program = self.program;
        let module = &program.modules[mi];
        let own = self.own.contains(module.name.as_str());
        let blame = blame.cloned().or_else(|| {
            (!own).then(|| Blame {
                span,
                via: format!("{}.{}", module.name, item_name(&module.items[ii].kind)),
            })
        });
        self.enqueue(mi, ii, blame);
        if own {
            self.start(mi);
        }
    }

    fn dict(&mut self, dict: &'p Dict, span: Span, blame: Option<&Blame>) {
        match dict {
            Dict::Instance { module, name, args } => {
                if let Some(&(mi, ii)) = self.instances.get(&(module.as_str(), name.as_str())) {
                    self.reach(mi, ii, span, blame);
                }
                for arg in args {
                    self.dict(arg, span, blame);
                }
            }
            Dict::Value { module, name } => self.global(module, name, span, blame),
            Dict::Super { dict, .. } => self.dict(dict, span, blame),
            Dict::Structural { fields, .. } => {
                for field in fields {
                    self.dict(field, span, blame);
                }
            }
            Dict::Given(_)
            | Dict::Opened { .. }
            | Dict::Field(_)
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}

/// Whether `foreign` is the kernel's `testEffect`, whose task the Lua runner
/// reports skipped until the Lua runtime exists.
pub(crate) fn is_effect_test(foreign: &Foreign) -> bool {
    matches!(&foreign.binding, Binding::Module { from, name } if from == "flick:rt" && name == "testEffect")
}

/// `f name task` applied in full: `f` and `name`. Of `testEffect`, only
/// these are reached and generated, never the task.
pub(crate) fn effect_test_parts(e: &Expr) -> Option<(&Expr, &Expr)> {
    let ExprKind::App { func, .. } = &e.kind else {
        return None;
    };
    let ExprKind::App {
        func: head,
        arg: name,
        ..
    } = &func.kind
    else {
        return None;
    };
    Some((head, name))
}

fn item_name(kind: &ItemKind) -> &str {
    match kind {
        ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => name,
        ItemKind::Foreign(foreign) => &foreign.name,
        ItemKind::Instance { info, .. } => &info.name,
        ItemKind::Constructor(_) | ItemKind::Destructure { .. } => "?",
    }
}
