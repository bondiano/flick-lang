//! Whole-program indexing and static type-driven fallback classification.
//!
//! Runs before any specialization instantiation begins. Uses Tarjan's SCC over
//! the top-level call graph to detect polymorphic recursion statically (without
//! waiting for a budget limit), alongside static detection of higher-rank
//! callbacks, packed modules / existentials, GADT constructors holding
//! dictionaries or existentials, and polymorphic `foreign` bindings.

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{
    Constructor, ConstructorDecl, Expr, ExprKind, Foreign, InstanceInfo, IrType, Item, ItemKind,
    Program, Span, TypeDecl, TypeRef,
};

use super::type_util::{
    contains_exists_or_module, contains_open_record, has_free_type_vars, has_higher_rank_param,
    is_growing_type_arg,
};
use super::types::{DefKey, FallbackReason};

/// Indexed reference to one top-level item in a [`Program`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum ItemEntry<'p> {
    Value {
        type_params: &'p [String],
        dicts: &'p [String],
        ty: Option<&'p TypeRef>,
        body: &'p Expr,
        span: Span,
    },
    Foreign {
        foreign: &'p Foreign,
    },
    Instance {
        info: &'p InstanceInfo,
        methods: &'p [(String, Expr)],
        span: Span,
    },
    Constructor {
        ctor: &'p Constructor,
    },
    Destructure,
    Service,
}

/// Deterministic whole-program lookup index over all definitions, instances,
/// constructors, and type declarations.
pub(crate) struct ProgramIndex<'p> {
    pub items: BTreeMap<DefKey, ItemEntry<'p>>,
    pub type_decls: BTreeMap<DefKey, &'p TypeDecl>,
    pub ctor_decls: BTreeMap<(String, String, String), (&'p TypeDecl, &'p ConstructorDecl)>,
    pub module_type_key: Option<DefKey>,
}

impl<'p> ProgramIndex<'p> {
    pub(crate) fn build(program: &'p Program) -> ProgramIndex<'p> {
        let mut items = BTreeMap::new();
        let mut type_decls = BTreeMap::new();
        let mut ctor_decls = BTreeMap::new();
        let mut module_type_key = None;

        for module in &program.modules {
            for decl in &module.types {
                let key = (module.name.clone(), decl.name.clone());
                if module.name == "Prelude" && decl.name == "Module" {
                    module_type_key = Some(key.clone());
                }
                for c in &decl.constructors {
                    ctor_decls.insert(
                        (module.name.clone(), decl.name.clone(), c.name.clone()),
                        (decl, c),
                    );
                }
                type_decls.insert(key, decl);
            }
            for item in &module.items {
                match &item.kind {
                    ItemKind::Value {
                        name,
                        type_params,
                        dicts,
                        ty,
                        body,
                        ..
                    } => {
                        items.insert(
                            (module.name.clone(), name.clone()),
                            ItemEntry::Value {
                                type_params,
                                dicts,
                                ty: ty.as_ref(),
                                body,
                                span: item.span,
                            },
                        );
                    }
                    ItemKind::Foreign(foreign) => {
                        items.insert(
                            (module.name.clone(), foreign.name.clone()),
                            ItemEntry::Foreign { foreign },
                        );
                    }
                    ItemKind::Instance { info, methods } => {
                        items.insert(
                            (module.name.clone(), info.name.clone()),
                            ItemEntry::Instance {
                                info,
                                methods,
                                span: item.span,
                            },
                        );
                    }
                    ItemKind::Constructor(ctor) => {
                        items.insert(
                            (module.name.clone(), ctor.name.clone()),
                            ItemEntry::Constructor { ctor },
                        );
                    }
                    ItemKind::Destructure { pattern, .. } => {
                        let mut binders = Vec::new();
                        pattern.binders(&mut binders);
                        for b in binders {
                            items.insert(
                                (module.name.clone(), b.to_owned()),
                                ItemEntry::Destructure,
                            );
                        }
                    }
                    ItemKind::Service { name, .. } => {
                        items.insert((module.name.clone(), name.clone()), ItemEntry::Service);
                    }
                }
            }
        }

        if module_type_key.is_none() {
            module_type_key = Some(("Prelude".to_owned(), "Module".to_owned()));
        }

        ProgramIndex {
            items,
            type_decls,
            ctor_decls,
            module_type_key,
        }
    }

    /// Whether `ty` is `Prelude.Module <sig>` (a packed module).
    pub(crate) fn is_packed_module_type(&self, ty: &IrType) -> bool {
        self.module_type_key
            .as_ref()
            .is_some_and(|(m, n)| ty.is_named(m, n))
    }

    /// Looks up the declaration of constructor `c`.
    pub(crate) fn ctor_decl(&self, c: &Constructor) -> Option<(&'p TypeDecl, &'p ConstructorDecl)> {
        self.ctor_decls
            .get(&(c.module.clone(), c.ty.clone(), c.name.clone()))
            .copied()
    }

    /// Whether constructor `c` holds GADT dictionaries or existential type
    /// parameters.
    pub(crate) fn is_gadt_dict_or_existential(&self, c: &Constructor) -> bool {
        if c.dicts > 0 {
            return true;
        }
        self.ctor_decl(c)
            .is_some_and(|(_, decl)| !decl.constraints.is_empty() || !decl.existentials.is_empty())
    }
}

/// Static whole-program classification computed before specialization starts.
#[derive(Debug, Clone)]
pub(crate) struct Classification {
    /// Definitions statically forced to compile once over uniform boxed cells,
    /// with the primary reason.
    pub fallbacks: BTreeMap<DefKey, FallbackReason>,
}

impl Classification {
    pub(crate) fn fallback_reason(&self, key: &DefKey) -> Option<FallbackReason> {
        self.fallbacks.get(key).copied()
    }
}

/// Runs the static whole-program classification pass over `index`.
pub(crate) fn classify_program(index: &ProgramIndex<'_>) -> Classification {
    let mut fallbacks: BTreeMap<DefKey, FallbackReason> = BTreeMap::new();

    for (key, entry) in &index.items {
        match *entry {
            ItemEntry::Foreign { foreign } if is_polymorphic_foreign(foreign, index) => {
                fallbacks.insert(key.clone(), FallbackReason::PolymorphicForeign);
            }
            ItemEntry::Value { ty: Some(t), .. } => {
                if has_higher_rank_param(t) {
                    fallbacks.insert(key.clone(), FallbackReason::HigherRankCallback);
                } else if contains_exists_or_module(t, index) {
                    fallbacks.insert(key.clone(), FallbackReason::PackedOrExistential);
                }
            }
            _ => {}
        }
    }

    detect_polymorphic_recursion(index, &mut fallbacks);
    Classification { fallbacks }
}

fn is_polymorphic_foreign(foreign: &Foreign, index: &ProgramIndex<'_>) -> bool {
    if !foreign.type_params.is_empty() {
        return true;
    }
    foreign.ty.as_ref().is_some_and(|t| {
        has_free_type_vars(t)
            || has_higher_rank_param(t)
            || contains_exists_or_module(t, index)
            || contains_open_record(t)
    })
}

struct CallEdge {
    dst: usize,
    type_args: Vec<TypeRef>,
}

fn detect_polymorphic_recursion(
    index: &ProgramIndex<'_>,
    fallbacks: &mut BTreeMap<DefKey, FallbackReason>,
) {
    let values: Vec<(DefKey, &[String], &Expr)> = index
        .items
        .iter()
        .filter_map(|(key, entry)| match *entry {
            ItemEntry::Value {
                type_params, body, ..
            } => Some((key.clone(), type_params, body)),
            _ => None,
        })
        .collect();

    let id_of: BTreeMap<DefKey, usize> = values
        .iter()
        .enumerate()
        .map(|(i, (k, _, _))| (k.clone(), i))
        .collect();

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); values.len()];
    let mut call_edges: Vec<Vec<CallEdge>> = (0..values.len()).map(|_| Vec::new()).collect();

    for (src, (_, _, body)) in values.iter().enumerate() {
        let mut raw_calls = Vec::new();
        collect_global_calls(body, &mut raw_calls);
        let mut targets = BTreeSet::new();
        for (callee, type_args) in raw_calls {
            if let Some(&dst) = id_of.get(&callee) {
                targets.insert(dst);
                call_edges[src].push(CallEdge { dst, type_args });
            }
        }
        adj[src] = targets.into_iter().collect();
    }

    let sccs = tarjan_scc(values.len(), &adj);
    let mut scc_members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (v, &scc) in sccs.iter().enumerate() {
        scc_members.entry(scc).or_default().push(v);
    }

    for members in scc_members.values() {
        let is_cyclic =
            members.len() > 1 || (members.len() == 1 && adj[members[0]].contains(&members[0]));
        if !is_cyclic || !members.iter().any(|&v| !values[v].1.is_empty()) {
            continue;
        }
        let scc_id = sccs[members[0]];
        let mut poly_rec = false;
        'outer: for &src in members {
            let src_params: BTreeSet<&str> = values[src].1.iter().map(String::as_str).collect();
            for edge in &call_edges[src] {
                if sccs[edge.dst] != scc_id {
                    continue;
                }
                let dst_params = values[edge.dst].1;
                if dst_params.is_empty() {
                    continue;
                }
                if edge.type_args.len() != dst_params.len()
                    || edge
                        .type_args
                        .iter()
                        .any(|arg| is_growing_type_arg(arg, &src_params))
                {
                    poly_rec = true;
                    break 'outer;
                }
            }
        }
        if poly_rec {
            for &v in members {
                if !values[v].1.is_empty() {
                    fallbacks
                        .entry(values[v].0.clone())
                        .or_insert(FallbackReason::PolymorphicRecursion);
                }
            }
        }
    }
}

fn collect_global_calls(expr: &Expr, out: &mut Vec<(DefKey, Vec<TypeRef>)>) {
    if let ExprKind::Global {
        module,
        name,
        type_args,
    } = &expr.kind
    {
        out.push(((module.clone(), name.clone()), type_args.clone()));
    }
    expr.each_child(&mut |child| collect_global_calls(child, out));
}

/// Deterministic Tarjan strongly connected components algorithm over `0..n`.
fn tarjan_scc(n: usize, adj: &[Vec<usize>]) -> Vec<usize> {
    struct State<'a> {
        adj: &'a [Vec<usize>],
        next_index: usize,
        index: Vec<Option<usize>>,
        lowlink: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        scc_id: Vec<usize>,
        next_scc: usize,
    }

    fn strongconnect(v: usize, st: &mut State<'_>) {
        let idx = st.next_index;
        st.next_index += 1;
        st.index[v] = Some(idx);
        st.lowlink[v] = idx;
        st.stack.push(v);
        st.on_stack[v] = true;

        for &w in &st.adj[v] {
            if st.index[w].is_none() {
                strongconnect(w, st);
                st.lowlink[v] = st.lowlink[v].min(st.lowlink[w]);
            } else if st.on_stack[w]
                && let Some(w_idx) = st.index[w]
            {
                st.lowlink[v] = st.lowlink[v].min(w_idx);
            }
        }

        if Some(st.lowlink[v]) == st.index[v] {
            let id = st.next_scc;
            st.next_scc += 1;
            while let Some(w) = st.stack.pop() {
                st.on_stack[w] = false;
                st.scc_id[w] = id;
                if w == v {
                    break;
                }
            }
        }
    }

    let mut st = State {
        adj,
        next_index: 0,
        index: vec![None; n],
        lowlink: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        scc_id: vec![0; n],
        next_scc: 0,
    };
    for v in 0..n {
        if st.index[v].is_none() {
            strongconnect(v, &mut st);
        }
    }
    st.scc_id
}

#[allow(dead_code)]
fn _keep(_: Option<&Item>) {}
