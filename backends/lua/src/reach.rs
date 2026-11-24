//! What a program reaches from its roots: the facts of the IR a Lua profile
//! may refuse, each at the source the program reached it from.
//!
//! A root is an entry's `main`, a test root, or, without either, every
//! export of an own module. A reference into a module that is not the
//! request's own (a built-in one) is blamed on the own expression that left
//! own code, so its facts point at the user's source. An own module that is
//! reached initializes its eager top-level values too.

use std::collections::{BTreeMap, BTreeSet};

use flick_backend::{BuildRequest, ModuleRole};
use flick_ir::{
    BinOp, Dict, Expr, ExprKind, Foreign, IrType, ItemKind, Lit, Namespace, Num, Program, Span,
};

/// A numeric type no Lua number holds exactly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Wide {
    BigInt,
    Int64,
    UInt64,
}

impl Wide {
    pub fn name(self) -> &'static str {
        match self {
            Wide::BigInt => "BigInt",
            Wide::Int64 => "Int64",
            Wide::UInt64 => "UInt64",
        }
    }

    fn of_num(num: Num) -> Option<Wide> {
        match num {
            Num::BigInt => Some(Wide::BigInt),
            Num::Int64 => Some(Wide::Int64),
            Num::UInt64 => Some(Wide::UInt64),
            _ => None,
        }
    }
}

pub(crate) enum Fact<'p> {
    /// A literal, an operator or a foreign function of the type.
    Number(Wide),
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

#[derive(Clone)]
struct Blame {
    span: Span,
    via: String,
}

struct Walker<'p, 'f> {
    program: &'p Program,
    own: BTreeSet<&'p str>,
    /// `(module, name)` of a value, a foreign function or a destructured
    /// binder, and of an instance: its module and item index.
    values: BTreeMap<(&'p str, &'p str), (usize, usize)>,
    instances: BTreeMap<(&'p str, &'p str), (usize, usize)>,
    /// The type of each wide numeric lang item.
    types: BTreeMap<(&'p str, &'p str), Wide>,
    seen: BTreeSet<(usize, usize)>,
    started: BTreeSet<usize>,
    queue: Vec<(usize, usize, Option<Blame>)>,
    report: &'f mut dyn FnMut(Fact<'p>, Site),
}

pub(crate) fn walk<'p>(
    program: &'p Program,
    request: &'p BuildRequest,
    report: &mut dyn FnMut(Fact<'p>, Site),
) {
    let mut w = Walker {
        program,
        own: (request.modules.iter())
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| m.name.as_str())
            .collect(),
        values: BTreeMap::new(),
        instances: BTreeMap::new(),
        types: BTreeMap::new(),
        seen: BTreeSet::new(),
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
    for item in &program.lang {
        let wide = match item.lang.as_str() {
            "bigint" => Wide::BigInt,
            "int64" => Wide::Int64,
            "uint64" => Wide::UInt64,
            _ => continue,
        };
        if item.namespace == Namespace::Type {
            w.types
                .insert((item.module.as_str(), item.name.as_str()), wide);
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
    for (module, name) in roots {
        if let Some(&(mi, ii)) = w.values.get(&(module, name)) {
            w.enqueue(mi, ii, None);
            w.start(mi);
        }
    }
    while let Some((mi, ii, blame)) = w.queue.pop() {
        program.modules[mi].items[ii].each_body(&mut |body| w.expr(body, blame.as_ref()));
    }
}

impl<'p> Walker<'p, '_> {
    fn enqueue(&mut self, mi: usize, ii: usize, blame: Option<Blame>) {
        if self.seen.insert((mi, ii)) {
            self.queue.push((mi, ii, blame));
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
        let number = match &e.kind {
            ExprKind::Lit(Lit::BigInt(_)) => Some(Wide::BigInt),
            ExprKind::Lit(Lit::Int64(_)) => Some(Wide::Int64),
            ExprKind::Lit(Lit::UInt64(_)) => Some(Wide::UInt64),
            ExprKind::Binary(BinOp::Arith(_, num), ..) | ExprKind::Neg(num, _) => {
                Wide::of_num(*num)
            }
            _ => None,
        };
        if let Some(wide) = number {
            self.fact(Fact::Number(wide), e.span, blame);
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

    /// A reference to `module.name`: its item is reached, and a foreign
    /// function is a fact at the reference.
    fn global(&mut self, module: &str, name: &str, span: Span, blame: Option<&Blame>) {
        let Some(&(mi, ii)) = self.values.get(&(module, name)) else {
            return;
        };
        let program = self.program;
        let target = &program.modules[mi];
        if let ItemKind::Foreign(foreign) = &target.items[ii].kind {
            let wide = foreign.ty.as_deref().and_then(|ty| self.wide_type(ty));
            if let Some(wide) = wide {
                self.fact(Fact::Number(wide), span, blame);
            }
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

    /// The wide numeric type a foreign function's type mentions.
    fn wide_type(&self, ty: &IrType) -> Option<Wide> {
        if let IrType::Named { module, name, .. } = ty
            && let Some(&wide) = self.types.get(&(module.as_str(), name.as_str()))
        {
            return Some(wide);
        }
        let mut found = None;
        ty.each_child(&mut |child| {
            if found.is_none() {
                found = self.wide_type(child);
            }
        });
        found
    }
}

fn item_name(kind: &ItemKind) -> &str {
    match kind {
        ItemKind::Value { name, .. } | ItemKind::Service { name, .. } => name,
        ItemKind::Foreign(foreign) => &foreign.name,
        ItemKind::Instance { info, .. } => &info.name,
        ItemKind::Constructor(_) | ItemKind::Destructure { .. } => "?",
    }
}
