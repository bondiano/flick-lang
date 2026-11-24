//! How a top-level name is held in Go, and which instances build the same
//! dictionary whoever builds it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::Input;
use super::arith::Native;
use super::go_type::{GoType, TypeDecls};
use flick_backend_support::reach::Key;
use flick_ir::{Dict, InstanceInfo, default_method};
use flick_ir::{Expr, ExprKind, Item, ItemKind, Param, Pat, Stage};

/// How a top-level name is held in Go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Shape {
    /// A variable assigned at initialization.
    Var,
    /// A Go function of the fiber, dictionaries, then parameters; `effects`
    /// has bit `i` set when parameter `i`'s language stage runs effects
    /// (Sync or Runtime), independently of the hidden execution context.
    Func {
        dicts: usize,
        params: usize,
        effects: u64,
        types: Types,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TypedSig {
    result: Option<GoType>,
    params: Vec<Option<GoType>>,
}

/// The parameters and the result of a function that are of Go's own typed
/// representations (scalars or allocation-free `Option` layouts): a value
/// passed to or returned by the function is not boxed into `flickrt.Value`
/// for the call.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Types(Option<Arc<TypedSig>>);

impl Types {
    pub(super) fn of(result: Option<GoType>, params: &[Option<GoType>]) -> Types {
        if result.is_none() && params.iter().all(Option::is_none) {
            return Types(None);
        }
        Types(Some(Arc::new(TypedSig {
            result,
            params: params.to_vec(),
        })))
    }

    pub(super) fn of_natives(result: Option<Native>, params: &[Option<Native>]) -> Types {
        let params: Vec<Option<GoType>> = params.iter().map(|p| p.map(GoType::Scalar)).collect();
        Types::of(result.map(GoType::Scalar), &params)
    }

    pub(super) fn result(&self) -> Option<&GoType> {
        self.0.as_ref()?.result.as_ref()
    }

    pub(super) fn param(&self, i: usize) -> Option<&GoType> {
        self.0.as_ref()?.params.get(i)?.as_ref()
    }

    pub(super) fn param_len(&self) -> usize {
        self.0.as_ref().map_or(0, |s| s.params.len())
    }

    /// Whether the function has a parameter or a result of a typed Go
    /// representation, so that it is not itself a function of values.
    pub(super) fn any(&self) -> bool {
        self.0.is_some()
    }
}

impl Shape {
    /// The same function taking and giving values only.
    pub(super) fn of_values(self) -> Shape {
        match self {
            Shape::Func {
                dicts,
                params,
                effects,
                ..
            } => Shape::Func {
                dicts,
                params,
                effects,
                types: Types::default(),
            },
            Shape::Var => Shape::Var,
        }
    }

    /// Whether the function's last language stage runs effects.
    pub(super) fn effectful(&self) -> bool {
        matches!(self, Shape::Func { params, effects, .. } if *params > 0 && effects >> (*params - 1) & 1 == 1)
    }
}

/// The effects mask of stages, by whether each runs effects.
fn effects(stages: impl IntoIterator<Item = bool>) -> u64 {
    stages
        .into_iter()
        .enumerate()
        .fold(0, |mask, (i, effectful)| mask | u64::from(effectful) << i)
}

/// The shape of a Go function taking `dicts` dictionaries, then `params`,
/// and computing `body`.
pub(super) fn function(
    input: &Input,
    decls: &TypeDecls,
    dicts: usize,
    params: &[Param],
    body: &Expr,
) -> Shape {
    let param_types: Vec<Option<GoType>> = params
        .iter()
        .map(|p| match p.pat {
            Pat::Bind(_) | Pat::Record(_) | Pat::Con(_, _) => {
                GoType::of(input.lang, decls, p.ty.as_deref())
            }
            _ => None,
        })
        .collect();
    Shape::Func {
        dicts,
        params: params.len(),
        effects: effects(params.iter().map(|p| p.stage != Stage::Pure)),
        types: Types::of(
            GoType::of(input.lang, decls, body.ty.as_deref()),
            &param_types,
        ),
    }
}

pub(super) fn shape(input: &Input, decls: &TypeDecls, module: &str, kind: &ItemKind) -> Shape {
    match kind {
        ItemKind::Value { dicts, body, .. } => match &body.kind {
            ExprKind::Lambda { params, body } => function(input, decls, dicts.len(), params, body),
            _ if dicts.is_empty() => Shape::Var,
            _ => Shape::Func {
                dicts: dicts.len(),
                params: 0,
                effects: 0,
                types: Types::default(),
            },
        },
        ItemKind::Foreign(f) => {
            let params = input.bindings.parameters(module, f);
            if f.dicts == 0 && params == 0 {
                Shape::Var
            } else {
                Shape::Func {
                    dicts: f.dicts,
                    params,
                    effects: effects(
                        (0..params).map(|i| f.stages.get(i).copied().unwrap_or(false)),
                    ),
                    // those the table of bindings gives its Go function, or inferred
                    // native scalar types for qualified Go package globals
                    types: {
                        let from_table = input.bindings.types(module, f);
                        if from_table.any() {
                            from_table
                        } else {
                            super::foreign_pkg::direct_go_types(input.lang, decls, f, params)
                        }
                    },
                }
            }
        }
        ItemKind::Instance { info, .. } if !info.context.is_empty() => Shape::Func {
            dicts: info.context.len(),
            params: 0,
            effects: 0,
            types: Types::default(),
        },
        _ => Shape::Var,
    }
}

/// Whether naming the top-level value `name` of `module` runs none of its
/// code: a variable is read and a function is made a value, but a value
/// that takes dictionaries is computed where it is named.
fn named_quietly(shapes: &HashMap<Key, Shape>, module: &str, name: &str) -> bool {
    let shape = shapes.get(&(module.to_owned(), name.to_owned()));
    !matches!(shape, Some(Shape::Func { params: 0, .. }))
}

/// Whether every dictionary `dict` builds on the way is one of `settled`.
pub(super) fn builds_settled(
    dict: &Dict,
    settled: &HashSet<Key>,
    shapes: &HashMap<Key, Shape>,
) -> bool {
    let all = |dicts: &[Dict]| dicts.iter().all(|d| builds_settled(d, settled, shapes));
    match dict {
        Dict::Instance { module, name, args } => {
            (args.is_empty() || settled.contains(&(module.clone(), name.clone()))) && all(args)
        }
        Dict::Structural { fields, .. } => all(fields),
        Dict::Super { dict, .. } => builds_settled(dict, settled, shapes),
        Dict::Value { module, name } => named_quietly(shapes, module, name),
        _ => true,
    }
}

/// The instances with a context that build one dictionary whatever fiber
/// builds it and whenever: building it, and what it computes when first
/// read, runs no code of the program, it only makes functions and other
/// such dictionaries. Code the language calls pure may read what a fiber
/// carries, so an instance whose method is computed, not written as a
/// function, is not one of these; neither is one that builds one that is
/// not. A dictionary made of these alone can be built once.
pub(super) fn settled<'m>(
    items: impl Iterator<Item = (&'m str, &'m Item)>,
    shapes: &HashMap<Key, Shape>,
) -> HashSet<Key> {
    let quiet = |info: &InstanceInfo, methods: &[(String, Expr)]| {
        let (module, class) = &info.class;
        methods.iter().all(|(_, method)| match &method.kind {
            ExprKind::Lambda { .. } => true,
            ExprKind::Global { module, name, .. } => named_quietly(shapes, module, name),
            _ => false,
        }) && info.defaults.iter().all(|(name, own, _)| {
            *own > 0 || named_quietly(shapes, module, &default_method(class, name))
        })
    };
    let instances: Vec<(Key, &InstanceInfo)> = items
        .filter_map(|(module, item)| match &item.kind {
            ItemKind::Instance { info, methods }
                if !info.context.is_empty() && quiet(info, methods) =>
            {
                Some(((module.to_owned(), info.name.clone()), &**info))
            }
            _ => None,
        })
        .collect();
    let mut settled: HashSet<Key> = instances.iter().map(|(key, _)| key.clone()).collect();
    // one that builds an instance that is not settled is not either
    loop {
        let builds = |info: &InstanceInfo| {
            let supers = info.supers.iter().map(|(_, d)| d);
            let fields = info.derived.iter().flat_map(|(fields, _)| fields);
            let defaults = info.defaults.iter().flat_map(|(_, _, extra)| extra);
            supers
                .chain(fields)
                .chain(defaults)
                .all(|d| builds_settled(d, &settled, shapes))
        };
        let kept: HashSet<Key> = instances
            .iter()
            .filter(|(key, info)| settled.contains(key) && builds(info))
            .map(|(key, _)| key.clone())
            .collect();
        if kept.len() == settled.len() {
            return settled;
        }
        settled = kept;
    }
}
