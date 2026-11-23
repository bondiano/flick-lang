//! Static declaration-level indirection analysis (transparent DAG check,
//! Phase 1 growing recursive instantiation detection, and Tarjan's SCC).

use std::collections::{BTreeMap, BTreeSet};

use flick_ir::{IrType, TypeInterner, TypeRef};

use super::LayoutError;
use super::catalog::{DeclKey, NamedKind, TypeCatalog, has_free_var, is_unboxed_decl};

/// A constructor field site on a declaration: `(decl_key, decl_ctor_index, field_index)`.
pub(crate) type FieldSite = (DeclKey, usize, usize);

/// Program-wide static analysis results computed once per [`super::LayoutEngine`].
#[derive(Debug, Clone)]
pub(crate) struct StaticIndirection {
    /// `opaque type alias` declarations that cycle without an intervening ADT.
    pub cyclic_aliases: BTreeSet<DeclKey>,
    /// `unboxed` declarations that stay transparent (not in a transparent-only cycle).
    pub transparent_unboxed: BTreeSet<DeclKey>,
    /// Constructor field sites statically forced behind pointers due to
    /// growing recursive instantiations.
    pub growing_fields: BTreeSet<FieldSite>,
}

impl StaticIndirection {
    pub(crate) fn analyze(catalog: &TypeCatalog<'_>) -> StaticIndirection {
        let (cyclic_aliases, transparent_unboxed) = analyze_transparent_dag(catalog);
        let growing_fields =
            analyze_growing_recursion(catalog, &cyclic_aliases, &transparent_unboxed);
        StaticIndirection {
            cyclic_aliases,
            transparent_unboxed,
            growing_fields,
        }
    }

    /// Expands one layer of transparent `opaque type alias` / `unboxed`
    /// declarations at the top level of `ty` until reaching a primitive,
    /// structural type, or non-transparent ADT.
    pub(crate) fn unwrap_transparent(
        &self,
        catalog: &TypeCatalog<'_>,
        ty: &TypeRef,
        interner: &mut TypeInterner,
    ) -> Result<TypeRef, LayoutError> {
        let mut cur = ty.clone();
        loop {
            let IrType::Named { module, name, args } = &*cur else {
                return Ok(cur);
            };
            let key = (module.clone(), name.clone());
            if self.cyclic_aliases.contains(&key) {
                return Err(LayoutError::CyclicAlias {
                    module: module.clone(),
                    name: name.clone(),
                });
            }
            let NamedKind::Decl(decl) = catalog.classify(module, name, args.len())? else {
                return Ok(cur);
            };
            if let Some(body) = &decl.alias {
                let subst: Vec<(&str, &TypeRef)> = decl
                    .params
                    .iter()
                    .map(String::as_str)
                    .zip(args.iter())
                    .collect();
                cur = body.substitute(&subst, interner);
                continue;
            }
            if self.transparent_unboxed.contains(&key) {
                let subst: Vec<(&str, &TypeRef)> = decl
                    .params
                    .iter()
                    .map(String::as_str)
                    .zip(args.iter())
                    .collect();
                cur = decl.constructors[0].fields[0].substitute(&subst, interner);
                continue;
            }
            return Ok(cur);
        }
    }
}

/// Step 0: separates cyclic `opaque type alias`es and demotes self-cyclic
/// `unboxed` wrappers so remaining transparent declarations form a strict DAG.
fn analyze_transparent_dag(catalog: &TypeCatalog<'_>) -> (BTreeSet<DeclKey>, BTreeSet<DeclKey>) {
    let mut transparent: Vec<DeclKey> = Vec::new();
    let mut alias_keys: BTreeSet<DeclKey> = BTreeSet::new();
    let mut unboxed_keys: BTreeSet<DeclKey> = BTreeSet::new();
    for (key, decl) in &catalog.decls {
        if decl.alias.is_some() {
            transparent.push(key.clone());
            alias_keys.insert(key.clone());
        } else if is_unboxed_decl(decl) {
            transparent.push(key.clone());
            unboxed_keys.insert(key.clone());
        }
    }
    let idx_of: BTreeMap<DeclKey, usize> = transparent
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();

    let mut alias_edges: Vec<Vec<usize>> = vec![Vec::new(); transparent.len()];
    let mut trans_edges: Vec<Vec<usize>> = vec![Vec::new(); transparent.len()];
    for (i, key) in transparent.iter().enumerate() {
        let decl = catalog.decls[key];
        let body = decl
            .alias
            .as_ref()
            .unwrap_or_else(|| &decl.constructors[0].fields[0]);
        let mut targets = BTreeSet::new();
        collect_transparent_targets(body, &idx_of, &mut targets);
        for &t in &targets {
            trans_edges[i].push(t);
            if alias_keys.contains(key) && alias_keys.contains(&transparent[t]) {
                alias_edges[i].push(t);
            }
        }
    }

    let alias_sccs = tarjan_scc(transparent.len(), &alias_edges);
    let mut cyclic_aliases = BTreeSet::new();
    for (i, key) in transparent.iter().enumerate() {
        if alias_keys.contains(key)
            && alias_edges[i]
                .iter()
                .any(|&t| alias_sccs[i] == alias_sccs[t])
        {
            cyclic_aliases.insert(key.clone());
        }
    }

    let trans_sccs = tarjan_scc(transparent.len(), &trans_edges);
    let mut transparent_unboxed = BTreeSet::new();
    for (i, key) in transparent.iter().enumerate() {
        if unboxed_keys.contains(key)
            && !trans_edges[i]
                .iter()
                .any(|&t| trans_sccs[i] == trans_sccs[t])
        {
            transparent_unboxed.insert(key.clone());
        }
    }

    (cyclic_aliases, transparent_unboxed)
}

/// Collects transparent declarations referenced by value in `ty` without
/// crossing any nominal ADT or pointer/closure boundary.
fn collect_transparent_targets(
    ty: &IrType,
    idx_of: &BTreeMap<DeclKey, usize>,
    out: &mut BTreeSet<usize>,
) {
    match ty {
        IrType::Named { module, name, args } => {
            let key = (module.clone(), name.clone());
            if let Some(&idx) = idx_of.get(&key) {
                out.insert(idx);
                for arg in args {
                    collect_transparent_targets(arg, idx_of, out);
                }
            }
        }
        IrType::Tuple(items) => {
            for item in items {
                collect_transparent_targets(item, idx_of, out);
            }
        }
        IrType::Record { fields, .. } => {
            for (_, field_ty) in fields {
                collect_transparent_targets(field_ty, idx_of, out);
            }
        }
        IrType::App { head, args } => {
            collect_transparent_targets(head, idx_of, out);
            for arg in args {
                collect_transparent_targets(arg, idx_of, out);
            }
        }
        IrType::Var(_) | IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. } => {}
    }
}

/// One declaration-level occurrence of a non-transparent ADT inside a field.
struct DeclOcc {
    target: usize,
    growing: bool,
}

struct OccCollector<'a, 'p> {
    catalog: &'a TypeCatalog<'p>,
    cyclic_aliases: &'a BTreeSet<DeclKey>,
    transparent_unboxed: &'a BTreeSet<DeclKey>,
    idx_of: &'a BTreeMap<DeclKey, usize>,
    interner: TypeInterner,
}

impl OccCollector<'_, '_> {
    fn collect(&mut self, ty: &TypeRef, out: &mut Vec<DeclOcc>) {
        match &**ty {
            IrType::Named { module, name, args } => self.collect_named(module, name, args, out),
            IrType::Tuple(items) => {
                for item in items {
                    self.collect(item, out);
                }
            }
            IrType::Record { fields, .. } => {
                for (_, field_ty) in fields {
                    self.collect(field_ty, out);
                }
            }
            IrType::App { head, args } => {
                self.collect(head, out);
                for arg in args {
                    self.collect(arg, out);
                }
            }
            IrType::Var(_) | IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. } => {
            }
        }
    }

    fn collect_named(
        &mut self,
        module: &str,
        name: &str,
        args: &[TypeRef],
        out: &mut Vec<DeclOcc>,
    ) {
        let key = (module.to_owned(), name.to_owned());
        if self.cyclic_aliases.contains(&key) {
            return;
        }
        let Some(decl) = self.catalog.decl_of(module, name) else {
            return;
        };
        if let Some(body) = &decl.alias {
            if args.len() == decl.params.len() {
                let subst: Vec<(&str, &TypeRef)> = decl
                    .params
                    .iter()
                    .map(String::as_str)
                    .zip(args.iter())
                    .collect();
                let expanded = body.substitute(&subst, &mut self.interner);
                self.collect(&expanded, out);
            }
            return;
        }
        if self.transparent_unboxed.contains(&key) {
            if args.len() == decl.params.len() {
                let subst: Vec<(&str, &TypeRef)> = decl
                    .params
                    .iter()
                    .map(String::as_str)
                    .zip(args.iter())
                    .collect();
                let expanded =
                    decl.constructors[0].fields[0].substitute(&subst, &mut self.interner);
                self.collect(&expanded, out);
            }
            return;
        }
        if let Some(&target) = self.idx_of.get(&key) {
            let partial = args.len() < decl.params.len();
            let growing_arg = args
                .iter()
                .any(|a| !matches!(&**a, IrType::Var(_)) && has_free_var(a));
            out.push(DeclOcc {
                target,
                growing: partial || growing_arg,
            });
        }
        for arg in args {
            self.collect(arg, out);
        }
    }
}

/// Step 1: identifies constructor fields that contain growing recursive
/// declaration occurrences (such as `Nested (List a)` or partial applications
/// of an SCC member) so they are put behind pointers before any instantiation.
fn analyze_growing_recursion(
    catalog: &TypeCatalog<'_>,
    cyclic_aliases: &BTreeSet<DeclKey>,
    transparent_unboxed: &BTreeSet<DeclKey>,
) -> BTreeSet<FieldSite> {
    let mut adts: Vec<DeclKey> = Vec::new();
    for (key, decl) in &catalog.decls {
        if decl.alias.is_none()
            && !decl.constructors.is_empty()
            && !transparent_unboxed.contains(key)
        {
            adts.push(key.clone());
        }
    }
    let idx_of: BTreeMap<DeclKey, usize> = adts
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();

    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); adts.len()];
    let mut site_occs: Vec<(usize, FieldSite, Vec<DeclOcc>)> = Vec::new();
    let mut collector = OccCollector {
        catalog,
        cyclic_aliases,
        transparent_unboxed,
        idx_of: &idx_of,
        interner: TypeInterner::new(),
    };

    for (src_idx, key) in adts.iter().enumerate() {
        let decl = catalog.decls[key];
        let mut target_set = BTreeSet::new();
        for (ctor_idx, ctor) in decl.constructors.iter().enumerate() {
            for (field_idx, field_ty) in ctor.fields.iter().enumerate() {
                let mut occs = Vec::new();
                collector.collect(field_ty, &mut occs);
                for occ in &occs {
                    target_set.insert(occ.target);
                }
                site_occs.push((src_idx, (key.clone(), ctor_idx, field_idx), occs));
            }
        }
        edges[src_idx] = target_set.into_iter().collect();
    }

    let sccs = tarjan_scc(adts.len(), &edges);
    let mut growing = BTreeSet::new();
    for (src_idx, site, occs) in site_occs {
        if occs
            .iter()
            .any(|occ| occ.growing && sccs[src_idx] == sccs[occ.target])
        {
            growing.insert(site);
        }
    }
    growing
}

/// Deterministic Tarjan strongly connected components algorithm over `0..n`.
/// Returns `scc_id[v]` for each vertex `v` in `0..n`.
pub(crate) fn tarjan_scc(n: usize, adj: &[Vec<usize>]) -> Vec<usize> {
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
