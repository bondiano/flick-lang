//! Finite original type syntax for lazy foreign owner adapters.
//!
//! Alias bodies are retained once, before following their references. Runtime
//! descriptors substitute parameters lazily; construction never expands an
//! increasingly instantiated recursive alias.
use crate::types::lang::Lang;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::OwnerProductivity;
use crate::types::env::Form;
use crate::types::ty::Elided;
use crate::types::{Binder, Checker, Elem, Kind, MetaOccurrence, Stage, Type, stage_of};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerTypeExpr {
    pub kind: OwnerTypeKind,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OwnerTypeKind {
    Con {
        module: String,
        name: String,
    },
    Var(String),
    Symbol(String),
    App,
    DynamicCon,
    DynamicVar,
    Fun {
        runtime: bool,
        sync: bool,
        stage: Stage,
    },
    Tuple,
    Record {
        labels: Vec<Elem>,
        tail: bool,
    },
    Row {
        labels: Vec<Elem>,
        tail: bool,
    },
    Union,
    Without(Vec<Elem>),
    Captured {
        value: bool,
        elided: Elided,
    },
    Bounded,
    Forall {
        vars: Vec<Binder>,
        constraints: usize,
    },
    Exists(String),
    Atom {
        identity: String,
        atom: Option<String>,
    },
    /// Inference-only, retaining the complete occurrence until finalization.
    #[serde(skip)]
    MetaView(MetaOccurrence),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerTypeAlias {
    pub module: String,
    pub name: String,
    pub params: Vec<String>,
    pub body: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerTypeTemplates {
    pub root: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub productivity: Option<OwnerProductivity>,
    pub recursive: bool,
    pub cyclic_aliases: Vec<usize>,
    pub dynamic_con: usize,
    pub dynamic_var: usize,
    pub never: usize,
    pub exprs: Vec<OwnerTypeExpr>,
    pub aliases: Vec<OwnerTypeAlias>,
}

impl OwnerTypeTemplates {
    pub fn has_meta_views(&self) -> bool {
        self.exprs.iter().any(|expression| match &expression.kind {
            OwnerTypeKind::MetaView(_) => true,
            OwnerTypeKind::Forall { vars, .. } => vars.iter().any(|var| var.kind.has_meta_view()),
            OwnerTypeKind::Record { labels, .. }
            | OwnerTypeKind::Row { labels, .. }
            | OwnerTypeKind::Without(labels) => labels.iter().any(Elem::has_meta_view),
            _ => false,
        })
    }
}

struct Extract<'a, 'c> {
    checker: &'a Checker<'c>,
    exprs: Vec<OwnerTypeExpr>,
    aliases: Vec<OwnerTypeAlias>,
    alias_seen: HashMap<(String, String), usize>,
    active: HashSet<(String, String)>,
    recursive: bool,
    pending: bool,
}

impl Checker<'_> {
    pub(in crate::types) fn owner_type_templates(&self, ty: &Type) -> OwnerTypeTemplates {
        self.owner_type_extraction(ty).0
    }

    pub(in crate::types) fn owner_type_is_pending(&self, ty: &Type) -> bool {
        self.owner_type_extraction(ty).1
    }

    fn owner_type_extraction(&self, ty: &Type) -> (OwnerTypeTemplates, bool) {
        let mut extract = Extract {
            checker: self,
            exprs: Vec::new(),
            aliases: Vec::new(),
            alias_seen: HashMap::new(),
            active: HashSet::new(),
            recursive: false,
            pending: false,
        };
        let root = extract.expr(ty);
        let never = extract.expr(&Type::lang(Lang::Never));
        let dynamic_con = extract.exprs.len();
        extract.exprs.push(OwnerTypeExpr {
            kind: OwnerTypeKind::DynamicCon,
            children: Vec::new(),
        });
        let dynamic_var = extract.exprs.len();
        extract.exprs.push(OwnerTypeExpr {
            kind: OwnerTypeKind::DynamicVar,
            children: Vec::new(),
        });
        let pending = extract.pending;
        let mut templates = OwnerTypeTemplates {
            root,
            productivity: None,
            recursive: extract.recursive,
            cyclic_aliases: Vec::new(),
            dynamic_con,
            dynamic_var,
            never,
            exprs: extract.exprs,
            aliases: extract.aliases,
        };
        templates.cyclic_aliases = cyclic_aliases(&templates);
        (templates, pending)
    }
}

impl Extract<'_, '_> {
    fn expr(&mut self, ty: &Type) -> usize {
        let mut ty = self.checker.store.resolve(ty);
        // A solved `^t` denotes t's captures, not a permanently unknown
        // metadata atom. Reuse the checker's existing capture-row semantics.
        if matches!(ty, Type::Row(_)) {
            ty = self.checker.expanded(&ty);
        }
        self.pending |= matches!(ty, Type::Meta(_));
        if let Type::Con { module, name } = &ty
            && let Some(definition) = self.checker.env.type_def(module, name)
        {
            self.pending |= pending_kind(&self.checker.kinds.resolve_deep(&definition.kind));
        }
        match &mut ty {
            Type::Forall { vars, .. } => {
                for binder in vars {
                    binder.kind = self.checker.kinds.resolve_deep(&binder.kind);
                    self.pending |= pending_kind(&binder.kind);
                }
            }
            Type::Row(row) | Type::Record(row) => {
                for (label, _) in &mut row.entries {
                    *label = self.checker.store.zonk_label(label);
                    self.pending |= pending_label(label);
                }
            }
            Type::Without { gone, .. } => {
                for label in gone {
                    *label = self.checker.store.zonk_label(label);
                    self.pending |= pending_label(label);
                }
            }
            _ => {}
        }
        let id = self.exprs.len();
        self.exprs.push(OwnerTypeExpr {
            kind: OwnerTypeKind::Tuple,
            children: Vec::new(),
        });
        let kind = kind(&ty);
        let mut children = Vec::new();
        ty.for_each_child(&mut |child| children.push(self.expr(child)));
        if let Type::Con { module, name } = &ty {
            self.alias(module, name);
        }
        self.exprs[id] = OwnerTypeExpr { kind, children };
        id
    }

    fn alias(&mut self, module: &str, name: &str) {
        let key = (module.to_owned(), name.to_owned());
        if self.alias_seen.contains_key(&key) {
            self.recursive |= self.active.contains(&key);
            return;
        }
        let Some(Form::Alias { params, body, .. }) =
            self.checker.env.type_def(module, name).map(|def| &def.form)
        else {
            return;
        };
        let (params, body) = (params.clone(), body.clone());
        let id = self.aliases.len();
        self.alias_seen.insert(key.clone(), id);
        self.active.insert(key.clone());
        self.aliases.push(OwnerTypeAlias {
            module: module.to_owned(),
            name: name.to_owned(),
            params,
            body: 0,
        });
        self.aliases[id].body = self.expr(&body);
        self.active.remove(&key);
    }
}

fn kind(ty: &Type) -> OwnerTypeKind {
    match ty {
        Type::Con { module, name } => OwnerTypeKind::Con {
            module: module.clone(),
            name: name.clone(),
        },
        Type::Var(name) => OwnerTypeKind::Var(name.clone()),
        Type::Symbol(name) => OwnerTypeKind::Symbol(name.clone()),
        Type::App { .. } => OwnerTypeKind::App,
        Type::Fun { effects, .. } => OwnerTypeKind::Fun {
            runtime: effects.is_some(),
            sync: effects.as_ref().is_some_and(|e| e.sync),
            stage: stage_of(effects.as_deref()),
        },
        Type::Tuple(_) => OwnerTypeKind::Tuple,
        Type::Record(row) => OwnerTypeKind::Record {
            labels: row.entries.iter().map(|(label, _)| label.clone()).collect(),
            tail: row.tail.is_some(),
        },
        Type::Row(row) => OwnerTypeKind::Row {
            labels: row.entries.iter().map(|(label, _)| label.clone()).collect(),
            tail: row.tail.is_some(),
        },
        Type::Union(_, _) => OwnerTypeKind::Union,
        Type::Without { gone, .. } => OwnerTypeKind::Without(gone.clone()),
        Type::Captured { value, elided, .. } => OwnerTypeKind::Captured {
            value: *value,
            elided: *elided,
        },
        Type::Bounded { .. } => OwnerTypeKind::Bounded,
        Type::Forall {
            vars, constraints, ..
        } => OwnerTypeKind::Forall {
            vars: vars.clone(),
            constraints: constraints.len(),
        },
        Type::Exists { binder, .. } => OwnerTypeKind::Exists(binder.clone()),
        Type::Meta(meta) => match meta.legacy_root() {
            Some(id) => OwnerTypeKind::Atom {
                identity: format!("meta:{id}"),
                atom: Some(format!("?{id}")),
            },
            None => OwnerTypeKind::MetaView(meta.clone()),
        },
        Type::Skolem(id) => OwnerTypeKind::Atom {
            identity: format!("skolem:{id}"),
            atom: Some(format!("'{id}")),
        },
        Type::Error => OwnerTypeKind::Atom {
            identity: "error".to_owned(),
            atom: None,
        },
    }
}

fn cyclic_aliases(syntax: &OwnerTypeTemplates) -> Vec<usize> {
    let aliases: HashMap<_, _> = syntax
        .aliases
        .iter()
        .enumerate()
        .map(|(id, alias)| ((alias.module.as_str(), alias.name.as_str()), id))
        .collect();
    let mut edges = Vec::new();
    for alias in &syntax.aliases {
        let mut nodes = vec![alias.body];
        let mut adjacent = HashSet::new();
        while let Some(node) = nodes.pop() {
            let expression = &syntax.exprs[node];
            nodes.extend(&expression.children);
            if let OwnerTypeKind::Con { module, name } = &expression.kind
                && let Some(&id) = aliases.get(&(module.as_str(), name.as_str()))
            {
                adjacent.insert(id);
            }
        }
        edges.push(adjacent);
    }
    (0..syntax.aliases.len())
        .filter(|&root| {
            let mut pending: Vec<_> = edges[root].iter().copied().collect();
            let mut visited = HashSet::new();
            while let Some(alias) = pending.pop() {
                if alias == root {
                    return true;
                }
                if visited.insert(alias) {
                    pending.extend(&edges[alias]);
                }
            }
            false
        })
        .collect()
}

fn pending_kind(kind: &Kind) -> bool {
    match kind {
        Kind::Meta(_) => true,
        Kind::Arrow(a, b) => pending_kind(a) || pending_kind(b),
        Kind::Row(inner) => pending_kind(inner),
        _ => false,
    }
}

fn pending_label(label: &Elem) -> bool {
    use crate::types::Atom;
    match label {
        Elem::Capture(Atom::Meta(_))
        | Elem::Error(Atom::Meta(_))
        | Elem::Service(Atom::Meta(_)) => true,
        Elem::Provided(inner) => pending_label(inner),
        _ => false,
    }
}
