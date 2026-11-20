//! Finite, declaration-only epsilon productivity certificate.
//!
//! Exact types never form table keys. Only productive-entry buckets, raw-head
//! observations and the finite primitive constructor state cross alias calls.
use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use super::owner_templates::{OwnerTypeKind, OwnerTypeTemplates};
use crate::types::Elem;
use crate::types::lang::{self, Lang};

pub const BOTTOM: u8 = 0;
pub const OPAQUE: u8 = 1;
pub const EFF0: u8 = 2;
pub const EFF1: u8 = 3;
pub const EFF2: u8 = 4;
pub const ERRORS: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerRawHead {
    /// Other, constructor, variable, rigid/meta atom, or Symbol.
    pub kind: u8,
    pub name: usize,
    pub arguments: u8,
    pub first_symbol: bool,
    pub held: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerAbstract {
    pub productive: bool,
    pub head: OwnerRawHead,
    pub row: Vec<bool>,
    pub constructor: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerSummary {
    pub alias: usize,
    pub arguments: Vec<usize>,
    pub value: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerProductivity {
    pub root: usize,
    pub names: Vec<String>,
    pub labels: Vec<String>,
    pub values: Vec<OwnerAbstract>,
    pub aliases: Vec<OwnerSummary>,
}

#[derive(Clone)]
struct Environment(HashMap<String, usize>);

struct Certificate<'a> {
    syntax: &'a OwnerTypeTemplates,
    result: OwnerProductivity,
    values: HashMap<OwnerAbstract, usize>,
    aliases: HashMap<(usize, Vec<usize>), usize>,
}

/// The finite coordinates are P, raw kind/name/arity/first-symbol/held,
/// primitive-constructor state, and L+1 productive-entry buckets. With N
/// distinguished names their domain is bounded by
/// `2 * 5 * (N+1) * 4 * 2 * 2 * 6 * 2^(L+1)`; instantiated types are not keys.
/// Alias equations only grow positive witnesses; fixed-label subtraction
/// deletes buckets independently of those witnesses. Demand discovery must
/// stabilize as well as outputs. Exact Errors head normalization still checks
/// closed singleton equality: a failed projection terminates, while a
/// successful projection selects structurally equal entries with the same P.
/// Thus relaxed row witnesses cannot invent an infinite productive epsilon
/// path; a true least-fixed-point witness has finite derivation rank.
pub(super) fn certificate(syntax: &OwnerTypeTemplates) -> Option<OwnerProductivity> {
    if syntax.has_meta_views() {
        return None;
    }
    let mut labels = BTreeSet::new();
    let mut names = BTreeSet::new();
    for expression in &syntax.exprs {
        if let OwnerTypeKind::Without(gone) = &expression.kind {
            labels.extend(gone.iter().map(ToString::to_string));
        }
    }
    for item in [
        Lang::Outlives,
        Lang::Provided,
        Lang::Detached,
        Lang::NoControl,
        Lang::TypeEq,
    ] {
        if let Some(item) = lang::table().find(item) {
            names.insert(format!("{}.{}", item.module, item.name));
        }
    }
    for label in &labels {
        names.insert(label.clone());
        if let Some(base) = label.strip_prefix(['~', '@', '^']) {
            names.insert(base.to_owned());
        }
    }
    let original: Vec<_> = names.iter().cloned().collect();
    for mut name in original {
        while let Some(base) = name.strip_suffix("$Dict") {
            name = base.to_owned();
            names.insert(name.clone());
        }
    }
    let result = OwnerProductivity {
        root: 0,
        names: names.into_iter().collect(),
        labels: labels.into_iter().collect(),
        values: Vec::new(),
        aliases: Vec::new(),
    };
    let mut state = Certificate {
        syntax,
        result,
        values: HashMap::new(),
        aliases: HashMap::new(),
    };
    loop {
        let previous = state.result.aliases.len();
        state.evaluate(syntax.root, &Environment(HashMap::new()));
        let mut changed = false;
        let count = state.result.aliases.len();
        for index in 0..count {
            let row = state.result.aliases[index].clone();
            let alias = &syntax.aliases[row.alias];
            let env = Environment(
                alias
                    .params
                    .iter()
                    .cloned()
                    .zip(row.arguments.iter().copied())
                    .collect(),
            );
            let body = state.evaluate(alias.body, &env);
            let mut value = state.result.values[body].clone();
            value.head = state.alias_head(row.alias, &row.arguments);
            value.row.fill(false);
            let value = state.intern(value);
            changed |= value != state.result.aliases[index].value;
            state.result.aliases[index].value = value;
        }
        if !changed && state.result.aliases.len() == previous {
            break;
        }
    }
    state.result.root = state.evaluate(syntax.root, &Environment(HashMap::new()));
    state.complete_observations();
    Some(state.result)
}

impl Certificate<'_> {
    fn blank(&self) -> OwnerAbstract {
        OwnerAbstract {
            productive: false,
            head: OwnerRawHead {
                kind: 0,
                name: self.result.names.len(),
                arguments: 0,
                first_symbol: false,
                held: false,
            },
            row: vec![false; self.result.labels.len() + 1],
            constructor: OPAQUE,
        }
    }
    fn name(&self, name: &str) -> usize {
        self.result
            .names
            .binary_search_by(|known| known.as_str().cmp(name))
            .unwrap_or(self.result.names.len())
    }
    fn label(&self, name: &str) -> usize {
        self.result
            .labels
            .binary_search_by(|known| known.as_str().cmp(name))
            .unwrap_or(self.result.labels.len())
    }
    fn intern(&mut self, value: OwnerAbstract) -> usize {
        if let Some(&id) = self.values.get(&value) {
            return id;
        }
        let id = self.result.values.len();
        self.result.values.push(value.clone());
        self.values.insert(value, id);
        id
    }
    fn named(&self, kind: u8, name: &str) -> OwnerAbstract {
        let mut value = self.blank();
        value.head.kind = kind;
        value.head.name = self.name(name);
        value
    }
    fn alias_head(&self, id: usize, arguments: &[usize]) -> OwnerRawHead {
        let alias = &self.syntax.aliases[id];
        OwnerRawHead {
            kind: 1,
            name: self.name(&format!("{}.{}", alias.module, alias.name)),
            arguments: u8::try_from(alias.params.len().min(3)).expect("raw head arity"),
            first_symbol: alias.module == "$held"
                && arguments
                    .first()
                    .is_some_and(|&id| self.result.values[id].head.kind == 4),
            held: alias.module == "$held",
        }
    }
    fn alias(&mut self, id: usize, arguments: Vec<usize>) -> usize {
        if let Some(&row) = self.aliases.get(&(id, arguments.clone())) {
            return self.result.aliases[row].value;
        }
        let mut initial = self.blank();
        initial.head = self.alias_head(id, &arguments);
        initial.constructor = BOTTOM;
        let value = self.intern(initial);
        let row = self.result.aliases.len();
        self.aliases.insert((id, arguments.clone()), row);
        self.result.aliases.push(OwnerSummary {
            alias: id,
            arguments,
            value,
        });
        value
    }
    fn variable(&mut self, name: &str, env: &Environment) -> usize {
        if let Some(&value) = env.0.get(name) {
            return value;
        }
        if let Some(base) = name.strip_suffix("$Dict")
            && let Some(&id) = env.0.get(base)
        {
            let identity = &self.result.values[id];
            if identity.head.arguments == 0 && matches!(identity.head.kind, 1 | 2) {
                let mut value = self.blank();
                value.head = identity.head.clone();
                value.head.name = self
                    .result
                    .names
                    .get(identity.head.name)
                    .map_or(self.result.names.len(), |name| {
                        self.name(&format!("{name}$Dict"))
                    });
                return self.intern(value);
            }
        }
        self.intern(self.named(2, name))
    }
    fn dictionary(&self, value: &OwnerAbstract) -> bool {
        if value.head.kind != 1 {
            return true;
        }
        if value.head.held && value.head.arguments == 2 && value.head.first_symbol {
            return false;
        }
        ![
            Lang::Outlives,
            Lang::Provided,
            Lang::Detached,
            Lang::NoControl,
            Lang::TypeEq,
        ]
        .into_iter()
        .any(|item| {
            lang::table().find(item).is_some_and(|item| {
                value.head.name == self.name(&format!("{}.{}", item.module, item.name))
            })
        })
    }
    fn relabel(&self, label: &Elem, value: &OwnerAbstract) -> usize {
        let prefix = match label {
            Elem::Error(_) => "~",
            Elem::Service(_) => "@",
            Elem::Capture(_) => "^",
            _ => return self.label(&label.to_string()),
        };
        let head = &value.head;
        if matches!(label, Elem::Capture(_)) && head.arguments != 0 {
            return self.label(&label.to_string());
        }
        if head.kind == 1 && !matches!(label, Elem::Capture(_)) {
            return self
                .result
                .names
                .get(head.name)
                .map_or(self.result.labels.len(), |name| self.label(name));
        }
        if matches!(head.kind, 2 | 3) {
            return self
                .result
                .names
                .get(head.name)
                .map_or(self.result.labels.len(), |name| {
                    self.label(&format!("{prefix}{name}"))
                });
        }
        self.label(&label.to_string())
    }
    fn apply(&mut self, function: usize, argument: usize) -> usize {
        let mut value = self.blank();
        let f = &self.result.values[function];
        let a = &self.result.values[argument];
        value.head = f.head.clone();
        if value.head.held && value.head.arguments == 0 {
            value.head.first_symbol = a.head.kind == 4 && a.head.arguments == 0;
        }
        value.head.arguments = (value.head.arguments + 1).min(3);
        match f.constructor {
            BOTTOM => value.constructor = BOTTOM,
            EFF0 => value.constructor = EFF1,
            EFF1 => value.constructor = EFF2,
            EFF2 => value.productive = true,
            ERRORS => value.productive = a.row.iter().any(|bit| *bit),
            _ => {}
        }
        self.intern(value)
    }

    fn complete_observations(&mut self) {
        // These finite raw atoms are also produced by lazy $Dict substitution.
        for name in 0..=self.result.names.len() {
            for kind in 1..=3 {
                for held in [false, true] {
                    let mut value = self.blank();
                    value.head.kind = kind;
                    value.head.name = name;
                    value.head.held = kind == 1 && held;
                    self.intern(value);
                }
            }
        }
        for productive in [false, true] {
            for constructor in BOTTOM..=ERRORS {
                let mut value = self.blank();
                value.productive = productive;
                value.constructor = constructor;
                self.intern(value);
            }
        }
        loop {
            let count = self.result.values.len();
            for id in 0..count {
                let source = self.result.values[id].clone();
                for productive in [false, true] {
                    for symbol in [false, true] {
                        let mut value = self.blank();
                        value.head = source.head.clone();
                        if value.head.held && value.head.arguments == 0 {
                            value.head.first_symbol = symbol;
                        }
                        value.head.arguments = (value.head.arguments + 1).min(3);
                        match source.constructor {
                            BOTTOM => value.constructor = BOTTOM,
                            EFF0 => value.constructor = EFF1,
                            EFF1 => value.constructor = EFF2,
                            EFF2 => value.productive = true,
                            ERRORS => value.productive = productive,
                            _ => {}
                        }
                        self.intern(value);
                    }
                }
            }
            if self.result.values.len() == count {
                break;
            }
        }
    }

    fn evaluate(&mut self, id: usize, environment: &Environment) -> usize {
        let expression = self.syntax.exprs[id].clone();
        if matches!(expression.kind, OwnerTypeKind::App) {
            return self.application(id, environment);
        }
        if let OwnerTypeKind::Var(name) = &expression.kind {
            return self.variable(name, environment);
        }
        let mut masked = environment.clone();
        if let OwnerTypeKind::Forall { vars, .. } = &expression.kind {
            for var in vars {
                masked.0.remove(&var.name);
            }
        }
        let children: Vec<_> = expression
            .children
            .iter()
            .map(|&child| self.evaluate(child, &masked))
            .collect();
        let mut value = self.blank();
        match &expression.kind {
            OwnerTypeKind::Con { module, name } => {
                if let Some(alias) = self
                    .syntax
                    .aliases
                    .iter()
                    .position(|a| a.module == *module && a.name == *name && a.params.is_empty())
                {
                    return self.alias(alias, Vec::new());
                }
                value = self.named(1, &format!("{module}.{name}"));
                value.head.held = module == "$held";
                if lang::is(Lang::Eff, module, name) {
                    value.constructor = EFF0;
                }
                if lang::is(Lang::Errors, module, name) {
                    value.constructor = ERRORS;
                }
            }
            OwnerTypeKind::Symbol(_) => value.head.kind = 4,
            OwnerTypeKind::Fun { .. } => value.productive = true,
            OwnerTypeKind::Captured { .. } | OwnerTypeKind::Bounded => {
                value.productive = self.result.values[children[0]].productive;
                value.constructor = self.result.values[children[0]].constructor;
            }
            OwnerTypeKind::Forall { constraints, .. } => {
                value.productive = children[..*constraints]
                    .iter()
                    .any(|&id| self.dictionary(&self.result.values[id]))
                    || self.result.values[children[*constraints]].productive;
            }
            OwnerTypeKind::Row { labels, tail } => {
                for (index, label) in labels.iter().enumerate() {
                    let element = &self.result.values[children[index]];
                    let bucket = self.relabel(label, element);
                    value.row[bucket] |= element.productive;
                }
                if *tail {
                    merge(
                        &mut value.row,
                        &self.result.values[*children.last().expect("row tail")].row,
                    );
                }
            }
            OwnerTypeKind::Union => {
                merge(&mut value.row, &self.result.values[children[0]].row);
                merge(&mut value.row, &self.result.values[children[1]].row);
            }
            OwnerTypeKind::Without(gone) => {
                value.row.clone_from(&self.result.values[children[0]].row);
                for label in gone {
                    value.row[self.label(&label.to_string())] = false;
                }
            }
            OwnerTypeKind::Atom {
                atom: Some(name), ..
            } => value = self.named(3, name),
            _ => {}
        }
        self.intern(value)
    }

    fn application(&mut self, id: usize, env: &Environment) -> usize {
        let (head, args) = spine(self.syntax, id);
        let values: Vec<_> = args.iter().map(|&arg| self.evaluate(arg, env)).collect();
        let alias = match &self.syntax.exprs[head].kind {
            OwnerTypeKind::Con { module, name } => self
                .syntax
                .aliases
                .iter()
                .position(|a| a.module == *module && a.name == *name),
            _ => None,
        };
        let (mut function, rest) = if let Some(alias) = alias {
            let count = self.syntax.aliases[alias].params.len();
            if values.len() >= count {
                (
                    self.alias(alias, values[..count].to_vec()),
                    &values[count..],
                )
            } else {
                (self.evaluate(head, env), &values[..])
            }
        } else {
            (self.evaluate(head, env), &values[..])
        };
        for &argument in rest {
            function = self.apply(function, argument);
        }
        function
    }
}

fn merge(target: &mut [bool], from: &[bool]) {
    for (a, b) in target.iter_mut().zip(from) {
        *a |= b;
    }
}

fn spine(syntax: &OwnerTypeTemplates, mut id: usize) -> (usize, Vec<usize>) {
    let mut arguments = Vec::new();
    while matches!(syntax.exprs[id].kind, OwnerTypeKind::App) {
        arguments.push(syntax.exprs[id].children[1]);
        id = syntax.exprs[id].children[0];
    }
    arguments.reverse();
    (id, arguments)
}
