//! What a build adds to a module's interface: the data and dictionary ABI
//! of the compiled code, summaries of its values and the fingerprints of
//! the interfaces it was built against.

use std::collections::BTreeMap;

use crate::codegen::{instance_export, qualified_id, tag};
use crate::iface::{
    Abi, ConstructorAbi, Data, Dictionary, Interface, Namespace, Summary, fingerprint,
};
use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Module, Stmt};
use crate::types::Rep;

/// The interface with its ABI and dependency fingerprints. When `imports`
/// is nonempty, `interface_of` must return the finalized published interfaces.
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
        values: summaries(module, interface, interface_of),
        core: None,
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
            id: qualified_id(c, package),
            tag: (c.rep == Rep::Tagged).then(|| tag(c, package)),
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
        "Value" => "parse",
        _ => "show",
    }
}

/// Summaries of the exported values. `partial` is the least fixpoint over
/// the module: a value is total once its body calls only what is known to
/// be total, so a recursive value stays partial.
fn summaries(
    module: &Module,
    interface: &Interface,
    interface_of: &dyn Fn(&str) -> Option<Interface>,
) -> Vec<Summary> {
    let exported = |name: &str| {
        interface
            .find(Namespace::Value, name)
            .is_some_and(|e| e.entity.module == module.name)
    };
    let mut all: Vec<(Summary, Option<&Expr>)> = Vec::new();
    for item in &module.items {
        match &item.kind {
            ItemKind::Value { name, dicts, body } => {
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
                let summary = Summary {
                    name: name.clone(),
                    stages,
                    dicts: dicts.len(),
                    partial: true,
                };
                all.push((summary, Some(body)));
            }
            ItemKind::Foreign(f) => {
                let summary = Summary {
                    name: f.name.clone(),
                    stages: f
                        .stages
                        .iter()
                        .map(|&r| if r { '!' } else { '.' })
                        .collect(),
                    dicts: f.dicts,
                    // foreign code is not ours to judge
                    partial: true,
                };
                all.push((summary, None));
            }
            _ => {}
        }
    }
    let imported: BTreeMap<String, Vec<Summary>> = module
        .items
        .iter()
        .flat_map(|item| globals(item))
        .filter(|m| *m != module.name)
        .filter_map(|m| Some((m.to_owned(), interface_of(m)?.abi?.values)))
        .collect();
    loop {
        let mut changed = false;
        for i in 0..all.len() {
            let Some(body) = all[i].1 else {
                continue;
            };
            if !all[i].0.partial {
                continue;
            }
            let total = {
                let known = |m: &str, name: &str| -> Option<(usize, bool)> {
                    let summary = if m == module.name {
                        all.iter().map(|(s, _)| s).find(|s| s.name == name)
                    } else {
                        imported.get(m)?.iter().find(|s| s.name == name)
                    }?;
                    (!summary.stages.contains('!'))
                        .then_some((summary.stages.len(), summary.partial))
                };
                !Totality { known: &known }.partial(body)
            };
            if total {
                all[i].0.partial = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    all.into_iter()
        .map(|(s, _)| s)
        .filter(|s| exported(&s.name))
        .collect()
}

/// The modules of the top-level values `item` refers to.
fn globals(item: &Item) -> Vec<&str> {
    fn walk<'a>(e: &'a Expr, out: &mut Vec<&'a str>) {
        if let ExprKind::Global { module, .. } = &e.kind {
            out.push(module);
        }
        e.each_child(&mut |c| walk(c, out));
    }
    let mut out = Vec::new();
    if let ItemKind::Value { body, .. } = &item.kind {
        walk(body, &mut out);
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Of a pure top-level value by module and name: how many parameters it
/// has, and whether applying it to all of them may throw or not end.
type Known<'a> = &'a dyn Fn(&str, &str) -> Option<(usize, bool)>;

struct Totality<'a> {
    known: Known<'a>,
}

impl Totality<'_> {
    /// Whether evaluating `e` may throw or not end. Calls of top-level
    /// values known to be total, building values, and partial applications
    /// of known functions are total; every other call, BigInt-capable
    /// division and every runtime construct count as partial.
    fn partial(&self, e: &Expr) -> bool {
        let partial = |e: &Expr| self.partial(e);
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
            } => *runtime || !self.total_call(e) || partial(func) || partial(arg),
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
            | ExprKind::Scope { .. }
            | ExprKind::Within(..)
            | ExprKind::CodecBlock(..)
            | ExprKind::Provide(..) => true,
        }
    }

    /// Whether the call `e` itself (its arguments aside) is total: a
    /// constructor applied to fields, or a known top-level value applied
    /// to at most its parameters, all of them only when it is total.
    fn total_call(&self, e: &Expr) -> bool {
        let mut head = e;
        let mut args = 0;
        while let ExprKind::App { func, .. } = &head.kind {
            head = func;
            args += 1;
        }
        if let ExprKind::Dicts(inner, _) = &head.kind {
            head = inner;
        }
        match &head.kind {
            ExprKind::Con(_) => true,
            ExprKind::Global { module, name } => match (self.known)(module, name) {
                Some((params, partial)) => args < params || (args == params && !partial),
                None => false,
            },
            _ => false,
        }
    }
}
