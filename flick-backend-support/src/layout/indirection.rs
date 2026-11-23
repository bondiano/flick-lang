//! Static declaration-level indirection analysis (transparent DAG check,
//! Phase 1 growing recursive instantiation detection across ADTs, `unboxed`
//! wrappers, and `opaque type alias`es, and Tarjan's SCC).

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
    /// `unboxed` declarations that stay transparent (neither in a
    /// transparent-only cycle nor in a growing recursive SCC).
    pub transparent_unboxed: BTreeSet<DeclKey>,
    /// Constructor field sites statically forced behind pointers due to
    /// growing recursive instantiations.
    pub growing_fields: BTreeSet<FieldSite>,
    /// `opaque type alias` declarations in a growing recursive SCC, mapped to
    /// the set of declarations reachable from them in the declaration graph
    /// (`None` when a reachable declaration uses an HKT variable application,
    /// meaning any ADT may be reached through the type parameter).
    pub growing_aliases: BTreeMap<DeclKey, Option<BTreeSet<DeclKey>>>,
}

impl StaticIndirection {
    pub(crate) fn analyze(catalog: &TypeCatalog<'_>) -> StaticIndirection {
        let (cyclic_aliases, mut transparent_unboxed) = analyze_transparent_dag(catalog);
        let (growing_fields, growing_aliases) =
            analyze_growing_recursion(catalog, &cyclic_aliases, &mut transparent_unboxed);
        StaticIndirection {
            cyclic_aliases,
            transparent_unboxed,
            growing_fields,
            growing_aliases,
        }
    }

    /// Whether `alias_key` is a growing recursive `opaque type alias` that can
    /// reach `owner_decl` (directly, transitively, or via an HKT parameter).
    pub(crate) fn is_growing_alias_reaching(
        &self,
        alias_key: &DeclKey,
        owner_decl: &DeclKey,
    ) -> bool {
        match self.growing_aliases.get(alias_key) {
            Some(None) => true,
            Some(Some(reachable)) => reachable.contains(owner_decl),
            None => false,
        }
    }

    /// Expands transparent `opaque type alias`, `transparent_unboxed`, and
    /// empty `Forall` layers at the top level of `ty` until reaching a
    /// primitive, structural type, or non-transparent ADT.
    pub(crate) fn unwrap_transparent(
        &self,
        catalog: &TypeCatalog<'_>,
        ty: &TypeRef,
        interner: &mut TypeInterner,
    ) -> Result<TypeRef, LayoutError> {
        let mut cur = ty.clone();
        loop {
            if let IrType::Forall {
                vars,
                constraints,
                body,
            } = &*cur
                && vars.is_empty()
                && constraints.is_empty()
            {
                cur = body.clone();
                continue;
            }
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
        IrType::Forall {
            vars,
            constraints,
            body,
        } if vars.is_empty() && constraints.is_empty() => {
            collect_transparent_targets(body, idx_of, out);
        }
        IrType::Var(_) | IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. } => {}
    }
}

/// One declaration-level occurrence inside a field or alias body.
struct DeclOcc {
    target: usize,
    growing: bool,
}

/// Walks a declaration AST once without expanding aliases or `unboxed` types,
/// collecting direct references to declarations in `idx_of` and noting any
/// higher-kinded variable applications (`IrType::App`).
fn collect_decl_occs(
    catalog: &TypeCatalog<'_>,
    ty: &IrType,
    idx_of: &BTreeMap<DeclKey, usize>,
    out: &mut Vec<DeclOcc>,
    has_hkt_app: &mut bool,
) {
    match ty {
        IrType::Named { module, name, args } => {
            let Some(decl) = catalog.decl_of(module, name) else {
                // Primitive, pointer handle (`List`, `String`, `Bytes`, ...),
                // or opaque 0-constructor handle: stops by-value traversal.
                return;
            };
            let key = (module.clone(), name.clone());
            if let Some(&target) = idx_of.get(&key) {
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
                collect_decl_occs(catalog, arg, idx_of, out, has_hkt_app);
            }
        }
        IrType::Tuple(items) => {
            for item in items {
                collect_decl_occs(catalog, item, idx_of, out, has_hkt_app);
            }
        }
        IrType::Record { fields, .. } => {
            for (_, field_ty) in fields {
                collect_decl_occs(catalog, field_ty, idx_of, out, has_hkt_app);
            }
        }
        IrType::App { head, args } => {
            if !matches!(&**head, IrType::Named { .. }) {
                *has_hkt_app = true;
            }
            collect_decl_occs(catalog, head, idx_of, out, has_hkt_app);
            for arg in args {
                collect_decl_occs(catalog, arg, idx_of, out, has_hkt_app);
            }
        }
        IrType::Forall {
            vars,
            constraints,
            body,
        } if vars.is_empty() && constraints.is_empty() => {
            collect_decl_occs(catalog, body, idx_of, out, has_hkt_app);
        }
        IrType::Var(_) | IrType::Fun { .. } | IrType::Forall { .. } | IrType::Exists { .. } => {}
    }
}

/// Step 1: analyzes ALL non-cyclic declarations (regular ADTs, `unboxed` ADTs,
/// and `opaque type alias`es) as vertices in a single finite declaration graph
/// without eager expansion. Identifies growing recursive fields, demotes
/// growing-recursive `unboxed` wrappers so their growing field is boxed, and
/// records growing-recursive `opaque type alias`es with their reachable ADTs.
fn analyze_growing_recursion(
    catalog: &TypeCatalog<'_>,
    cyclic_aliases: &BTreeSet<DeclKey>,
    transparent_unboxed: &mut BTreeSet<DeclKey>,
) -> (
    BTreeSet<FieldSite>,
    BTreeMap<DeclKey, Option<BTreeSet<DeclKey>>>,
) {
    let decls: Vec<DeclKey> = catalog
        .decls
        .iter()
        .filter(|(key, decl)| {
            !cyclic_aliases.contains(*key)
                && (decl.alias.is_some() || !decl.constructors.is_empty())
        })
        .map(|(key, _)| key.clone())
        .collect();
    let idx_of: BTreeMap<DeclKey, usize> = decls
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), i))
        .collect();

    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); decls.len()];
    let mut has_hkt_app = vec![false; decls.len()];
    let mut site_occs: Vec<(usize, FieldSite, Vec<DeclOcc>)> = Vec::new();
    let mut alias_occs: Vec<(usize, Vec<DeclOcc>)> = Vec::new();

    for (src_idx, key) in decls.iter().enumerate() {
        let decl = catalog.decls[key];
        let mut target_set = BTreeSet::new();
        if let Some(body) = &decl.alias {
            let mut occs = Vec::new();
            collect_decl_occs(catalog, body, &idx_of, &mut occs, &mut has_hkt_app[src_idx]);
            for occ in &occs {
                target_set.insert(occ.target);
            }
            alias_occs.push((src_idx, occs));
        } else {
            for (ctor_idx, ctor) in decl.constructors.iter().enumerate() {
                for (field_idx, field_ty) in ctor.fields.iter().enumerate() {
                    let mut occs = Vec::new();
                    collect_decl_occs(
                        catalog,
                        field_ty,
                        &idx_of,
                        &mut occs,
                        &mut has_hkt_app[src_idx],
                    );
                    for occ in &occs {
                        target_set.insert(occ.target);
                    }
                    site_occs.push((src_idx, (key.clone(), ctor_idx, field_idx), occs));
                }
            }
        }
        edges[src_idx] = target_set.into_iter().collect();
    }

    let sccs = tarjan_scc(decls.len(), &edges);
    let mut growing_sccs = BTreeSet::new();
    let mut growing_fields = BTreeSet::new();

    for &(src_idx, ref site, ref occs) in &site_occs {
        if occs
            .iter()
            .any(|occ| occ.growing && sccs[src_idx] == sccs[occ.target])
        {
            growing_sccs.insert(sccs[src_idx]);
            growing_fields.insert(site.clone());
        }
    }
    for (src_idx, occs) in &alias_occs {
        if occs
            .iter()
            .any(|occ| occ.growing && sccs[*src_idx] == sccs[occ.target])
        {
            growing_sccs.insert(sccs[*src_idx]);
        }
    }

    // Demote any `unboxed` declaration participating in a growing SCC so
    // Phase 1 / Phase 2 can box its field instead of expanding it forever.
    for (idx, key) in decls.iter().enumerate() {
        if growing_sccs.contains(&sccs[idx]) {
            transparent_unboxed.remove(key);
        }
    }

    // Record every growing `opaque type alias` together with all declarations
    // reachable from it in the declaration graph (or `None` if an HKT `App`
    // can reach arbitrary ADTs).
    let mut growing_aliases = BTreeMap::new();
    for (idx, key) in decls.iter().enumerate() {
        let decl = catalog.decls[key];
        if decl.alias.is_some() && growing_sccs.contains(&sccs[idx]) {
            let reach = reachable_from(idx, &edges, &has_hkt_app, &decls);
            growing_aliases.insert(key.clone(), reach);
        }
    }

    (growing_fields, growing_aliases)
}

/// Computes all declarations reachable from `start` in `edges`, returning
/// `None` if any reachable vertex contains a higher-kinded variable `App`.
fn reachable_from(
    start: usize,
    edges: &[Vec<usize>],
    has_hkt_app: &[bool],
    decls: &[DeclKey],
) -> Option<BTreeSet<DeclKey>> {
    let mut seen = vec![false; decls.len()];
    let mut stack = vec![start];
    let mut out = BTreeSet::new();
    while let Some(v) = stack.pop() {
        if seen[v] {
            continue;
        }
        seen[v] = true;
        if has_hkt_app[v] {
            return None;
        }
        out.insert(decls[v].clone());
        for &next in &edges[v] {
            if !seen[next] {
                stack.push(next);
            }
        }
    }
    Some(out)
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
