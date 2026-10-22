//! What a build adds to a module's interface: the data and dictionary ABI
//! of the compiled code, summaries of its values and the fingerprints of
//! the interfaces it was built against.

use std::collections::BTreeMap;

use crate::codegen::{instance_export, tag};
use crate::iface::{
    Abi, ConstructorAbi, Data, Dictionary, Interface, Namespace, Summary, fingerprint,
};
use crate::ir::{BinOp, Expr, ExprKind, ItemKind, Module, Stmt};

/// The interface of a built module: `interface` with its ABI, and the
/// fingerprints of `imports` as `interface_of` finds them.
pub fn interface(
    module: &Module,
    interface: &Interface,
    package: Option<&str>,
    imports: &[String],
    interface_of: &dyn Fn(&str) -> Option<Interface>,
) -> Interface {
    let mut out = interface.clone();
    out.abi = Some(Abi {
        data: data(module, package),
        dictionaries: dictionaries(module),
        values: summaries(module, interface),
    });
    out.dependencies = imports
        .iter()
        .filter_map(|m| Some((m.clone(), fingerprint(&interface_of(m)?))))
        .collect::<BTreeMap<_, _>>();
    out
}

fn data(module: &Module, package: Option<&str>) -> Vec<Data> {
    let mut out: Vec<Data> = Vec::new();
    for item in &module.items {
        let ItemKind::Constructor(c) = &item.kind else {
            continue;
        };
        let constructor = ConstructorAbi {
            name: c.name.clone(),
            tag: tag(c, package),
            arity: c.arity,
        };
        match out.iter_mut().find(|d| d.ty == c.ty) {
            Some(d) => d.constructors.push(constructor),
            None => out.push(Data {
                ty: c.ty.clone(),
                constructors: vec![constructor],
            }),
        }
    }
    out
}

fn dictionaries(module: &Module) -> Vec<Dictionary> {
    module
        .items
        .iter()
        .filter_map(|item| {
            let ItemKind::Instance { info, methods } = &item.kind else {
                return None;
            };
            let methods = match &info.derived {
                Some(_) => vec![derived_method(&info.class.1).to_owned()],
                None => methods.iter().map(|(name, _)| name.clone()).collect(),
            };
            Some(Dictionary {
                instance: info.name.clone(),
                export: instance_export(&info.name),
                context: info.context.len(),
                methods,
                supers: info.supers.iter().map(|(class, _)| class.clone()).collect(),
            })
        })
        .collect()
}

/// The one method of a class `deriving` covers.
fn derived_method(class: &str) -> &'static str {
    match class {
        "Eq" => "eq",
        "Ord" => "compare",
        "Hash" => "hash",
        _ => "show",
    }
}

/// Summaries of the exported values.
fn summaries(module: &Module, interface: &Interface) -> Vec<Summary> {
    let exported = |name: &str| {
        interface
            .find(Namespace::Value, name)
            .is_some_and(|e| e.entity.module == module.name)
    };
    let mut out = Vec::new();
    for item in &module.items {
        let summary = match &item.kind {
            ItemKind::Value { name, dicts, body } if exported(name) => {
                let (stages, body) = match &body.kind {
                    ExprKind::Lambda { params, body } => (
                        params
                            .iter()
                            .map(|p| if p.runtime { '!' } else { '.' })
                            .collect(),
                        &**body,
                    ),
                    _ => (String::new(), body),
                };
                Summary {
                    name: name.clone(),
                    stages,
                    dicts: dicts.len(),
                    partial: partial(body),
                }
            }
            ItemKind::Foreign(f) if exported(&f.name) => Summary {
                name: f.name.clone(),
                stages: f
                    .stages
                    .iter()
                    .map(|&r| if r { '!' } else { '.' })
                    .collect(),
                dicts: f.dicts,
                // foreign code is not ours to judge
                partial: true,
            },
            _ => continue,
        };
        out.push(summary);
    }
    out
}

/// Whether evaluating `e` may throw or not end. Conservative: any call
/// other than building a value, BigInt-capable division and every runtime
/// construct count as partial.
fn partial(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(_)
        | ExprKind::Local(_)
        | ExprKind::Global { .. }
        | ExprKind::Con(_)
        | ExprKind::Label(_)
        | ExprKind::ServiceTag(_)
        | ExprKind::Method { .. }
        | ExprKind::Operation { .. }
        | ExprKind::Lambda { .. }
        | ExprKind::Task(_)
        | ExprKind::Lift(..)
        | ExprKind::Abstract(..) => false,
        ExprKind::App {
            func, arg, runtime, ..
        } => *runtime || !constructor(func) || partial(func) || partial(arg),
        ExprKind::Binary(op, l, r) => {
            matches!(op, BinOp::Div | BinOp::Rem | BinOp::Pow) || partial(l) || partial(r)
        }
        ExprKind::Field(e, _) | ExprKind::Not(e) | ExprKind::Neg(e) | ExprKind::Ignore(e) => {
            partial(e)
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => items.iter().any(partial),
        ExprKind::Record(fields) => fields.iter().any(|(_, v)| partial(v)),
        ExprKind::Update(r, fields) => partial(r) || fields.iter().any(|(_, v)| partial(v)),
        ExprKind::If(c, t, f) => partial(c) || partial(t) || partial(f),
        ExprKind::Match(s, arms) => {
            partial(s)
                || arms
                    .iter()
                    .any(|a| partial(&a.body) || a.guard.as_ref().is_some_and(partial))
        }
        ExprKind::Block(stmts, result) => {
            partial(result)
                || stmts.iter().any(|s| match s {
                    Stmt::Let(_, v) | Stmt::Expr(v) | Stmt::Fun { value: v, .. } => partial(v),
                })
        }
        ExprKind::Dicts(..)
        | ExprKind::Await(_)
        | ExprKind::Try(..)
        | ExprKind::Scope(..)
        | ExprKind::Provide(..) => true,
    }
}

/// Whether `e` is a constructor applied to fewer than all its fields.
fn constructor(e: &Expr) -> bool {
    let mut head = e;
    while let ExprKind::App { func, .. } = &head.kind {
        head = func;
    }
    matches!(head.kind, ExprKind::Con(_))
}
