//! Definition admission (`admit_v2`) and intra-module contract dependency
//! ordering & cycle detection (Tarjan SCC) for `PureTotalV2`.

use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::rc::Rc;

use flick_ir::{Dict, Expr, ExprKind, ItemKind};

use crate::logic::{Def, Program, Refused};
use crate::source::Span;

use super::term::{LangTypes, Term, TermKind, extract, extract_numeric_int_method};

/// The non-contracted `PureTotalV2` definitions admitted for a contract (in
/// dependency order), plus every intra-module contracted callee referenced
/// directly or through admitted helpers.
#[derive(Debug, Default, Clone)]
pub struct AdmittedV2 {
    defs: Vec<(Rc<Def>, Term)>,
    local: Vec<bool>,
    contracted_deps: Vec<(String, String)>,
}

impl AdmittedV2 {
    pub fn defs(&self) -> &[(Rc<Def>, Term)] {
        &self.defs
    }

    pub fn body(&self, d: &Def) -> Option<&Term> {
        self.defs.iter().find(|(k, _)| **k == *d).map(|(_, t)| t)
    }

    /// Whether `d` was defined in the same module as the contract being
    /// verified (so its extracted spans belong to the caller's own `FileId`).
    pub fn is_local(&self, d: &Def) -> bool {
        self.defs
            .iter()
            .zip(&self.local)
            .find(|((k, _), _)| **k == *d)
            .is_some_and(|(_, &loc)| loc)
    }

    /// Contracted `(module, name)` callees reached from the roots, in discovery
    /// order without duplicates.
    pub fn contracted_deps(&self) -> &[(String, String)] {
        &self.contracted_deps
    }
}

fn refuse<T>(span: Span, why: String) -> Result<T, Refused> {
    Err(Refused { span, why })
}

/// Admits every non-contracted definition reachable from `roots` into
/// `PureTotalV2`.
///
/// References to functions in `contracted_in_module` are recorded as
/// dependencies without unfolding or admitting their bodies (verified callees
/// are summarized by their contracts rather than unfolded).
pub fn admit_v2<S: BuildHasher>(
    roots: &[&Term],
    program: &dyn Program,
    contracted_in_module: &HashSet<(String, String), S>,
    lang: &LangTypes,
    span: Span,
) -> Result<AdmittedV2, Refused> {
    admit_v2_for_module(roots, None, program, contracted_in_module, lang, span)
}

/// Like [`admit_v2`], clamping extracted spans and errors from any module
/// other than `owner_module` to `span` so foreign `FileId`s (from built-ins,
/// precompiled packages, or other files) never leak into diagnostics.
pub fn admit_v2_for_module<S: BuildHasher>(
    roots: &[&Term],
    owner_module: Option<&str>,
    program: &dyn Program,
    contracted_in_module: &HashSet<(String, String), S>,
    lang: &LangTypes,
    span: Span,
) -> Result<AdmittedV2, Refused> {
    let mut admitting = AdmittingV2 {
        owner_module,
        program,
        contracted_in_module,
        lang,
        span,
        out: AdmittedV2::default(),
        path: Vec::new(),
    };
    for root in roots {
        admitting.visit(root)?;
    }
    Ok(admitting.out)
}

struct AdmittingV2<'a, S> {
    owner_module: Option<&'a str>,
    program: &'a dyn Program,
    contracted_in_module: &'a HashSet<(String, String), S>,
    lang: &'a LangTypes,
    span: Span,
    out: AdmittedV2,
    path: Vec<Rc<Def>>,
}

impl<S: BuildHasher> AdmittingV2<'_, S> {
    fn visit(&mut self, t: &Term) -> Result<(), Refused> {
        match &t.kind {
            TermKind::Var(_)
            | TermKind::IntLit(_)
            | TermKind::BoolLit(_)
            | TermKind::UnitLit
            | TermKind::Con(_) => Ok(()),
            TermKind::IntNeg(x) | TermKind::Not(x) | TermKind::Lam(_, x) => self.visit(x),
            TermKind::IntBin(_, a, b)
            | TermKind::IntCmp(_, a, b)
            | TermKind::BoolEq { lhs: a, rhs: b, .. }
            | TermKind::TagEq { lhs: a, rhs: b, .. }
            | TermKind::And(a, b)
            | TermKind::Or(a, b)
            | TermKind::App {
                func: a, arg: b, ..
            }
            | TermKind::Let(_, a, b) => {
                self.visit(a)?;
                self.visit(b)
            }
            TermKind::If(c, t_branch, f_branch) => {
                self.visit(c)?;
                self.visit(t_branch)?;
                self.visit(f_branch)
            }
            TermKind::Match(s, arms) => {
                self.visit(s)?;
                arms.iter().try_for_each(|a| self.visit(&a.body))
            }
            TermKind::Def(d) => self.definition(d),
        }
    }

    fn definition(&mut self, d: &Rc<Def>) -> Result<(), Refused> {
        if let Def::Value { module, name } = d.as_ref() {
            let key = (module.clone(), name.clone());
            if self.contracted_in_module.contains(&key) {
                if !self.out.contracted_deps.contains(&key) {
                    self.out.contracted_deps.push(key);
                }
                return Ok(());
            }
        }
        if self.out.body(d).is_some() {
            return Ok(());
        }
        if let Some(at) = self.path.iter().position(|p| p == d) {
            let cycle: Vec<String> = self.path[at..]
                .iter()
                .map(|p| format!("`{}`", p.shown()))
                .collect();
            return refuse(
                self.span,
                format!(
                    "{} names itself: recursion is outside the domain",
                    cycle.join(" → ")
                ),
            );
        }
        let (body, is_local) = self.body_of(d)?;
        self.path.push(d.clone());
        self.visit(&body)?;
        self.path.pop();
        self.out.defs.push((d.clone(), body));
        self.out.local.push(is_local);
        Ok(())
    }

    fn is_owner(&self, module: &str) -> bool {
        self.owner_module.is_none_or(|owner| owner == module)
    }

    fn clamp_foreign(
        &self,
        is_local: bool,
        result: Result<Term, Refused>,
    ) -> Result<(Term, bool), Refused> {
        if is_local {
            result.map(|term| (term, true))
        } else {
            match result {
                Ok(term) => Ok((term.remap_span(self.span), false)),
                Err(mut err) => {
                    err.span = self.span;
                    Err(err)
                }
            }
        }
    }

    fn body_of(&self, d: &Def) -> Result<(Term, bool), Refused> {
        let shown = d.shown();
        match d {
            Def::Value { module, name } => match self.program.item(module, name).map(|i| &i.kind) {
                Some(ItemKind::Value { body, dicts, .. }) if dicts.is_empty() => {
                    let is_local = self.is_owner(module);
                    self.clamp_foreign(is_local, extract(body, &[], self.program, self.lang))
                }
                Some(ItemKind::Value { .. }) => refuse(
                    self.span,
                    format!("`{shown}` takes dictionaries, which a contract does not pass"),
                ),
                Some(ItemKind::Foreign(_)) => refuse(
                    self.span,
                    format!("`{shown}` is foreign code, which the domain does not admit"),
                ),
                Some(_) => refuse(
                    self.span,
                    format!("`{shown}` is no definition a contract unfolds"),
                ),
                None => refuse(
                    self.span,
                    format!("the definition of `{shown}` is not available to this contract"),
                ),
            },
            Def::Method { dict, name } => {
                let (module, instance) = resolve_dict(dict, self.program, self.span)?;
                let Some((info, methods)) = self.program.instance(&module, &instance) else {
                    return refuse(
                        self.span,
                        format!("the instance `{instance}` is not available to this contract"),
                    );
                };
                if !info.context.is_empty() {
                    return refuse(
                        self.span,
                        format!("`{shown}` of `{instance}` takes dictionaries of its context"),
                    );
                }
                if info.defaults.iter().any(|(m, ..)| m == name) {
                    return refuse(
                        self.span,
                        format!("`{shown}` of `{instance}` is a default method, not admitted"),
                    );
                }
                if info.class == self.lang.eq_class {
                    return refuse(
                        self.span,
                        format!(
                            "`{shown}` of `{instance}` is an `Eq` instance, outside the domain"
                        ),
                    );
                }
                if info.class == self.lang.ord_class {
                    return refuse(
                        self.span,
                        format!(
                            "`{shown}` of `{instance}` is an `Ord` instance, outside the domain"
                        ),
                    );
                }
                let numeric_int = info.class == self.lang.numeric_class;
                if numeric_int {
                    let allowed_method = matches!(
                        name.as_str(),
                        "add" | "sub" | "negate" | "from_int" | "lt" | "le" | "gt" | "ge"
                    );
                    if !self.lang.is_numeric_int_instance(&module, &instance) || !allowed_method {
                        return refuse(
                            self.span,
                            format!("`{shown}` of `{instance}` is outside the domain"),
                        );
                    }
                }
                let is_local = self.is_owner(&module);
                match methods.iter().find(|(m, _)| m == name) {
                    Some((_, body)) if numeric_int => self.clamp_foreign(
                        is_local,
                        extract_numeric_int_method(body, self.program, self.lang),
                    ),
                    Some((_, body)) => {
                        self.clamp_foreign(is_local, extract(body, &[], self.program, self.lang))
                    }
                    None => refuse(self.span, format!("`{instance}` has no method `{shown}`")),
                }
            }
        }
    }
}

/// Resolves a context-free instance dictionary, following unambiguous
/// superclass fields.
fn resolve_dict(
    dict: &Dict,
    program: &dyn Program,
    span: Span,
) -> Result<(String, String), Refused> {
    match dict {
        Dict::Instance { module, name, args } if args.is_empty() => {
            Ok((module.clone(), name.clone()))
        }
        Dict::Super { dict, class } => {
            let (module, name) = resolve_dict(dict, program, span)?;
            let supers = program
                .instance(&module, &name)
                .map(|(info, _)| &info.supers);
            let mut found = supers.into_iter().flatten().filter(|(c, _)| c == class);
            match (found.next(), found.next()) {
                (Some((_, inner)), None) => resolve_dict(inner, program, span),
                _ => refuse(
                    span,
                    format!("the `{class}` of `{name}` is not available to this contract"),
                ),
            }
        }
        _ => refuse(
            span,
            "a dictionary that is no instance without a context is outside the domain".into(),
        ),
    }
}

/// Collects every intra-module contracted function name reachable from `roots`
/// (directly or through uncontracted helper definitions in `program`).
///
/// Walks raw IR with a visited set so it terminates even when an uncontracted
/// helper is recursive or contains constructs outside `PureTotalV2`.
pub fn collect_ir_contract_deps<S: BuildHasher>(
    roots: &[&Expr],
    module: &str,
    contracted: &HashSet<String, S>,
    program: &dyn Program,
) -> Vec<String> {
    let mut deps = Vec::new();
    let mut seen_deps = HashSet::new();
    let mut visited_values = HashSet::new();
    let mut visited_methods = HashSet::new();
    for root in roots {
        walk_ir_deps(
            root,
            module,
            contracted,
            program,
            &mut deps,
            &mut seen_deps,
            &mut visited_values,
            &mut visited_methods,
        );
    }
    deps
}

#[allow(clippy::too_many_arguments)]
fn walk_ir_deps<S: BuildHasher>(
    expr: &Expr,
    current_module: &str,
    contracted: &HashSet<String, S>,
    program: &dyn Program,
    deps: &mut Vec<String>,
    seen_deps: &mut HashSet<String>,
    visited_values: &mut HashSet<(String, String)>,
    visited_methods: &mut HashSet<(String, String, String)>,
) {
    match &expr.kind {
        ExprKind::Global { module, name, .. } => {
            if module == current_module && contracted.contains(name) {
                if seen_deps.insert(name.clone()) {
                    deps.push(name.clone());
                }
                return;
            }
            if visited_values.insert((module.clone(), name.clone()))
                && let Some(item) = program.item(module, name)
                && let ItemKind::Value { body, .. } = &item.kind
            {
                walk_ir_deps(
                    body,
                    current_module,
                    contracted,
                    program,
                    deps,
                    seen_deps,
                    visited_values,
                    visited_methods,
                );
            }
        }
        ExprKind::Method { dict, name, .. } => {
            if let Ok((inst_mod, inst_name)) = resolve_dict(dict, program, expr.span)
                && visited_methods.insert((inst_mod.clone(), inst_name.clone(), name.clone()))
                && let Some((_, methods)) = program.instance(&inst_mod, &inst_name)
                && let Some((_, body)) = methods.iter().find(|(m, _)| m == name)
            {
                walk_ir_deps(
                    body,
                    current_module,
                    contracted,
                    program,
                    deps,
                    seen_deps,
                    visited_values,
                    visited_methods,
                );
            }
        }
        _ => {}
    }
    expr.each_child(&mut |child| {
        walk_ir_deps(
            child,
            current_module,
            contracted,
            program,
            deps,
            seen_deps,
            visited_values,
            visited_methods,
        );
    });
}

/// One contracted function in the intra-module dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDepNode {
    pub name: String,
    /// Intra-module contracted callees referenced by this function's
    /// `requires`, `ensures`, `body`, or uncontracted helpers.
    pub deps: Vec<String>,
}

/// Verification schedule entry produced by [`order_contracts`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractOrder {
    /// Not on any dependency cycle; every contracted callee precedes this entry.
    Acyclic(String),
    /// Part of a direct self-loop or mutual dependency cycle; must be refused
    /// with `FLICK-T082` and never assumed by callers.
    Cyclic(String),
}

struct Tarjan<'a> {
    adj: &'a [Vec<usize>],
    nodes: &'a [ContractDepNode],
    next_index: usize,
    indices: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    out: Vec<ContractOrder>,
}

impl Tarjan<'_> {
    fn strongconnect(&mut self, v: usize) {
        let idx = self.next_index;
        self.next_index += 1;
        self.indices[v] = Some(idx);
        self.lowlink[v] = idx;
        self.stack.push(v);
        self.on_stack[v] = true;

        for &w in &self.adj[v] {
            if self.indices[w].is_none() {
                self.strongconnect(w);
                self.lowlink[v] = self.lowlink[v].min(self.lowlink[w]);
            } else if self.on_stack[w] {
                self.lowlink[v] = self.lowlink[v].min(self.indices[w].expect("visited"));
            }
        }

        if self.lowlink[v] == idx {
            let mut scc = Vec::new();
            while let Some(w) = self.stack.pop() {
                self.on_stack[w] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            let cyclic = scc.len() > 1 || self.adj[v].contains(&v);
            if cyclic {
                // Emit cycle members in declaration order for deterministic diagnostics.
                scc.sort_unstable();
                for member in scc {
                    self.out
                        .push(ContractOrder::Cyclic(self.nodes[member].name.clone()));
                }
            } else {
                self.out
                    .push(ContractOrder::Acyclic(self.nodes[v].name.clone()));
            }
        }
    }
}

/// Orders contracted functions with Tarjan's strongly connected components
/// algorithm: callees before callers, deterministic tie-breaking by declaration
/// order, and every cyclic member (self-loop or SCC size > 1) marked
/// [`ContractOrder::Cyclic`].
pub fn order_contracts(nodes: &[ContractDepNode]) -> Vec<ContractOrder> {
    let n = nodes.len();
    let index_of: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.name.as_str(), i))
        .collect();
    let mut adj: Vec<Vec<usize>> = Vec::with_capacity(n);
    for node in nodes {
        let mut edges = Vec::new();
        for dep in &node.deps {
            if let Some(&target) = index_of.get(dep.as_str())
                && !edges.contains(&target)
            {
                edges.push(target);
            }
        }
        adj.push(edges);
    }

    let mut tarjan = Tarjan {
        adj: &adj,
        nodes,
        next_index: 0,
        indices: vec![None; n],
        lowlink: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        out: Vec::with_capacity(n),
    };
    for v in 0..n {
        if tarjan.indices[v].is_none() {
            tarjan.strongconnect(v);
        }
    }
    tarjan.out
}
