//! Typed derived operations (`Eq`, `Ord`, `Hash`, `Show`, and `Ord`
//! comparison operators) on by-value closed ADTs and concrete `Result`
//! instantiations, verifying every field dictionary so custom hand-written
//! instances are never bypassed.

use std::collections::HashMap;

use flick_backend_support::Known;
use flick_ir::{Dict, Expr, ExprKind, ItemKind, Lang, Module, STRUCTURAL};
use flick_pretty::docvec;

use super::Gen;
use super::adt::{AdtOps, AdtShape, AdtSlot};
use super::arith::Native;
use super::go_type::{GoType, OptPayload, OptRepr, Typed};
use super::layout::{Code, call};
use super::record::{RecordShape, RecordSlot};

/// Recorded metadata for a compiler-derived `Eq`/`Ord`/`Hash`/`Show` instance.
pub(super) struct DerivedEntry {
    pub(super) class: Lang,
    pub(super) context: Vec<String>,
    pub(super) field_dicts: Vec<Dict>,
    pub(super) constructors: Vec<(String, String, String)>,
    pub(super) supers: Vec<(String, Dict)>,
}

/// All derived `Eq`/`Ord`/`Hash`/`Show` instances across the program, keyed by
/// `(module, instance_name)`.
pub(super) type DerivedInstances = HashMap<(String, String), DerivedEntry>;

pub(super) fn derived_instances(modules: &[&Module], lang: &Known) -> DerivedInstances {
    let mut out = DerivedInstances::new();
    for m in modules {
        for item in &m.items {
            let ItemKind::Instance { info, .. } = &item.kind else {
                continue;
            };
            let Some((field_dicts, Some(ctors))) = &info.derived else {
                continue;
            };
            let Some(&class) = STRUCTURAL
                .iter()
                .find(|&&role| lang.is(role, &info.class.0, &info.class.1))
            else {
                continue;
            };
            let constructors = ctors
                .iter()
                .map(|c| (c.module.clone(), c.ty.clone(), c.name.clone()))
                .collect();
            out.insert(
                (m.name.clone(), info.name.clone()),
                DerivedEntry {
                    class,
                    context: info.context.clone(),
                    field_dicts: field_dicts.clone(),
                    constructors,
                    supers: info.supers.clone(),
                },
            );
        }
    }
    out
}

/// A verified derived/built-in operation lowered directly on a by-value `AdtShape`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AdtMethod {
    Eq(AdtShape),
    Compare(AdtShape),
    OrdCmp(AdtShape, &'static str),
    Hash(AdtShape),
    Show(AdtShape),
}

impl AdtMethod {
    pub(super) fn arity(&self) -> usize {
        match self {
            AdtMethod::Eq(_) | AdtMethod::Compare(_) | AdtMethod::OrdCmp(_, _) => 2,
            AdtMethod::Hash(_) | AdtMethod::Show(_) => 1,
        }
    }

    pub(super) fn shape(&self) -> &AdtShape {
        match self {
            AdtMethod::Eq(s)
            | AdtMethod::Compare(s)
            | AdtMethod::OrdCmp(s, _)
            | AdtMethod::Hash(s)
            | AdtMethod::Show(s) => s,
        }
    }

    pub(super) fn result(&self) -> Option<GoType> {
        match self {
            AdtMethod::Eq(_) | AdtMethod::OrdCmp(_, _) => Some(GoType::Scalar(Native::Bool)),
            AdtMethod::Compare(_) => None,
            AdtMethod::Hash(_) => Some(GoType::Scalar(Native::Int)),
            AdtMethod::Show(_) => Some(GoType::Scalar(Native::String)),
        }
    }
}

fn subst_dict(dict: &Dict, env: &[(&str, &Dict)]) -> Option<Dict> {
    if env.is_empty() {
        return Some(dict.clone());
    }
    match dict {
        Dict::Given(name) => env
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| (*v).clone()),
        Dict::Instance { module, name, args } => {
            let args: Option<Vec<Dict>> = args.iter().map(|a| subst_dict(a, env)).collect();
            Some(Dict::Instance {
                module: module.clone(),
                name: name.clone(),
                args: args?,
            })
        }
        Dict::Super { dict, class } => Some(Dict::Super {
            dict: Box::new(subst_dict(dict, env)?),
            class: class.clone(),
        }),
        Dict::Structural { class, fields } => {
            let fields: Option<Vec<Dict>> = fields.iter().map(|f| subst_dict(f, env)).collect();
            Some(Dict::Structural {
                class: class.clone(),
                fields: fields?,
            })
        }
        _ => None,
    }
}

fn class_short_name<'a>(lang: &Known<'a>, class: Lang) -> Option<&'a str> {
    lang.get(class).map(|(_, n)| n)
}

/// Unwraps `Dict::Super` chains when the superclass is a known derived or built-in
/// `Eq` instance.
fn resolve_dict(
    dict: &Dict,
    expected_class: Lang,
    lang: &Known,
    derived: &DerivedInstances,
) -> Option<Dict> {
    match dict {
        Dict::Super { dict: inner, class } => {
            let short = class.rsplit('.').next().unwrap_or(class);
            if class_short_name(lang, expected_class) != Some(short) {
                return None;
            }
            match inner.as_ref() {
                Dict::Instance { module, name, args } => {
                    if let Some(entry) = derived.get(&(module.clone(), name.clone())) {
                        if entry.context.len() != args.len() {
                            return None;
                        }
                        let (_, super_dict) = entry
                            .supers
                            .iter()
                            .find(|(c, _)| c.rsplit('.').next().unwrap_or(c) == short)?;
                        let env: Vec<(&str, &Dict)> = entry
                            .context
                            .iter()
                            .map(String::as_str)
                            .zip(args.iter())
                            .collect();
                        let resolved = subst_dict(super_dict, &env)?;
                        return resolve_dict(&resolved, expected_class, lang, derived);
                    }
                    // Built-in scalar/Unit/Option/Result superclasses live in `Instances`.
                    let instances_mod = lang.get(Lang::Instances)?.0;
                    if module == instances_mod
                        && let Some((_, tail)) = name.split_once(' ')
                    {
                        let target_name = format!("{short} {tail}");
                        let new_args: Option<Vec<Dict>> = args
                            .iter()
                            .map(|a| {
                                resolve_dict(
                                    &Dict::Super {
                                        dict: Box::new(a.clone()),
                                        class: class.clone(),
                                    },
                                    expected_class,
                                    lang,
                                    derived,
                                )
                            })
                            .collect();
                        return Some(Dict::Instance {
                            module: module.clone(),
                            name: target_name,
                            args: new_args?,
                        });
                    }
                    None
                }
                Dict::Structural { fields, .. } => {
                    let new_fields: Option<Vec<Dict>> = fields
                        .iter()
                        .map(|f| {
                            resolve_dict(
                                &Dict::Super {
                                    dict: Box::new(f.clone()),
                                    class: class.clone(),
                                },
                                expected_class,
                                lang,
                                derived,
                            )
                        })
                        .collect();
                    Some(Dict::Structural {
                        class: short.to_owned(),
                        fields: new_fields?,
                    })
                }
                _ => None,
            }
        }
        other => Some(other.clone()),
    }
}

fn scalar_lang_name<'a>(lang: &Known<'a>, n: Native) -> Option<&'a str> {
    let role = match n {
        Native::Int => Lang::Int,
        Native::Float => Lang::Float,
        Native::Bool => Lang::Bool,
        Native::String => Lang::String,
        Native::Int8 => Lang::Int8,
        Native::Int16 => Lang::Int16,
        Native::Int64 => Lang::Int64,
        Native::UInt8 => Lang::UInt8,
        Native::UInt16 => Lang::UInt16,
        Native::UInt32 => Lang::UInt32,
        Native::UInt64 => Lang::UInt64,
        Native::Float32 => Lang::Float32,
    };
    lang.get(role).map(|(_, name)| name)
}

fn dict_matches_adt(
    dict: &Dict,
    class: Lang,
    shape: &AdtShape,
    lang: &Known,
    derived: &DerivedInstances,
) -> bool {
    let Some(resolved) = resolve_dict(dict, class, lang, derived) else {
        return false;
    };
    let Dict::Instance { module, name, args } = &resolved else {
        return false;
    };
    if shape.is_result {
        let Some((instances_mod, _)) = lang.get(Lang::Instances) else {
            return false;
        };
        let Some(cls_name) = class_short_name(lang, class) else {
            return false;
        };
        let Some((_, result_name)) = lang.get(Lang::Result) else {
            return false;
        };
        if module != instances_mod
            || args.len() != 2
            || !name.starts_with(&format!("{cls_name} ({result_name} "))
        {
            return false;
        }
        return dict_matches_slot(
            &args[0],
            class,
            &shape.variants[0].fields[0].slot,
            lang,
            derived,
        ) && dict_matches_slot(
            &args[1],
            class,
            &shape.variants[1].fields[0].slot,
            lang,
            derived,
        );
    }

    let Some(entry) = derived.get(&(module.clone(), name.clone())) else {
        return false;
    };
    if entry.class != class
        || entry.context.len() != args.len()
        || entry.constructors.len() != shape.variants.len()
    {
        return false;
    }
    for ((cm, ct, cn), v) in entry.constructors.iter().zip(&shape.variants) {
        if cm != &shape.module || ct != &shape.name || cn != &v.name {
            return false;
        }
    }
    let env: Vec<(&str, &Dict)> = entry
        .context
        .iter()
        .map(String::as_str)
        .zip(args.iter())
        .collect();
    let mut offset = 0usize;
    for v in &shape.variants {
        for f in &v.fields {
            let Some(raw) = entry.field_dicts.get(offset) else {
                return false;
            };
            offset += 1;
            let Some(sub) = subst_dict(raw, &env) else {
                return false;
            };
            if !dict_matches_slot(&sub, class, &f.slot, lang, derived) {
                return false;
            }
        }
    }
    offset == entry.field_dicts.len()
}

fn dict_matches_opt(
    dict: &Dict,
    class: Lang,
    repr: &OptRepr,
    lang: &Known,
    derived: &DerivedInstances,
) -> bool {
    let Some(resolved) = resolve_dict(dict, class, lang, derived) else {
        return false;
    };
    let Dict::Instance { module, name, args } = &resolved else {
        return false;
    };
    let Some((instances_mod, _)) = lang.get(Lang::Instances) else {
        return false;
    };
    let Some(cls_name) = class_short_name(lang, class) else {
        return false;
    };
    let Some((_, opt_name)) = lang.get(Lang::Option) else {
        return false;
    };
    if module != instances_mod
        || args.len() != 1
        || !name.starts_with(&format!("{cls_name} ({opt_name} "))
    {
        return false;
    }
    let inner_slot = match repr {
        OptRepr::BoolSentinel => AdtSlot::Scalar(Native::Bool),
        OptRepr::Flag(OptPayload::Int) => AdtSlot::Scalar(Native::Int),
        OptRepr::Flag(OptPayload::Float) => AdtSlot::Scalar(Native::Float),
        OptRepr::Flag(OptPayload::String) => AdtSlot::Scalar(Native::String),
        OptRepr::Flag(OptPayload::Nested(inner)) => AdtSlot::Opt((**inner).clone()),
        OptRepr::RefSentinel | OptRepr::Flag(OptPayload::Value) => return false,
    };
    dict_matches_slot(&args[0], class, &inner_slot, lang, derived)
}

fn dict_matches_record(
    dict: &Dict,
    class: Lang,
    rec: &RecordShape,
    lang: &Known,
    derived: &DerivedInstances,
) -> bool {
    let Some(resolved) = resolve_dict(dict, class, lang, derived) else {
        return false;
    };
    let Dict::Structural {
        class: dcls,
        fields,
    } = &resolved
    else {
        return false;
    };
    let Some(cls_name) = class_short_name(lang, class) else {
        return false;
    };
    if dcls != cls_name || fields.len() != rec.fields.len() {
        return false;
    }
    // Structural record field dictionaries are ordered by UTF-16 label order.
    let mut order: Vec<usize> = (0..rec.fields.len()).collect();
    order.sort_by(|&a, &b| {
        rec.fields[a]
            .0
            .encode_utf16()
            .cmp(rec.fields[b].0.encode_utf16())
    });
    for (dict_field, idx) in fields.iter().zip(order) {
        let slot = match &rec.fields[idx].1 {
            RecordSlot::Scalar(n) => AdtSlot::Scalar(*n),
            RecordSlot::Adt(a) => AdtSlot::Adt(Box::new(a.clone())),
            RecordSlot::Value => return false,
        };
        if !dict_matches_slot(dict_field, class, &slot, lang, derived) {
            return false;
        }
    }
    true
}

fn dict_matches_slot(
    dict: &Dict,
    class: Lang,
    slot: &AdtSlot,
    lang: &Known,
    derived: &DerivedInstances,
) -> bool {
    match slot {
        AdtSlot::Scalar(n) => {
            let Some(resolved) = resolve_dict(dict, class, lang, derived) else {
                return false;
            };
            let Dict::Instance { module, name, args } = &resolved else {
                return false;
            };
            let Some((instances_mod, _)) = lang.get(Lang::Instances) else {
                return false;
            };
            let Some(cls_name) = class_short_name(lang, class) else {
                return false;
            };
            let Some(ty_name) = scalar_lang_name(lang, *n) else {
                return false;
            };
            module == instances_mod && args.is_empty() && name == &format!("{cls_name} {ty_name}")
        }
        AdtSlot::Unit => {
            let Some(resolved) = resolve_dict(dict, class, lang, derived) else {
                return false;
            };
            let Dict::Instance { module, name, args } = &resolved else {
                return false;
            };
            let Some((instances_mod, _)) = lang.get(Lang::Instances) else {
                return false;
            };
            let Some(cls_name) = class_short_name(lang, class) else {
                return false;
            };
            let Some((_, unit_name)) = lang.get(Lang::Unit) else {
                return false;
            };
            module == instances_mod && args.is_empty() && name == &format!("{cls_name} {unit_name}")
        }
        AdtSlot::Opt(r) => dict_matches_opt(dict, class, r, lang, derived),
        AdtSlot::Record(r) => dict_matches_record(dict, class, r, lang, derived),
        AdtSlot::Adt(a) => dict_matches_adt(dict, class, a, lang, derived),
    }
}

impl<'a> Gen<'a> {
    /// Recognizes a derived `Eq`/`Ord`/`Hash`/`Show` method or `Ord` comparison
    /// operator on a by-value closed ADT operand.
    pub(super) fn adt_method_head(
        &self,
        head: &Expr,
        operand: Option<&Expr>,
        operand_ty: Option<&GoType>,
    ) -> Option<AdtMethod> {
        let derived = self.derived?;
        let lang = self.input.lang;
        let shape = if let Some(GoType::Adt(s)) = operand_ty {
            s.clone()
        } else {
            let op = operand?;
            match self.predict_go_type(op, &[]) {
                Some(GoType::Adt(s)) => s,
                _ => self.adt_shape_of(op.ty.as_deref())?,
            }
        };
        match &head.kind {
            ExprKind::Method { dict, name, .. } => {
                let (class, method) = if name == "eq" {
                    (Lang::Eq, AdtMethod::Eq(shape.clone()))
                } else if name == "compare" {
                    (Lang::Ord, AdtMethod::Compare(shape.clone()))
                } else if name == "hash" {
                    (Lang::Hash, AdtMethod::Hash(shape.clone()))
                } else if lang.get(Lang::ShowMethod).is_some_and(|(_, m)| m == name) {
                    (Lang::Show, AdtMethod::Show(shape.clone()))
                } else {
                    return None;
                };
                dict_matches_adt(dict, class, &shape, lang, derived).then_some(method)
            }
            ExprKind::Dicts(inner, dicts) if dicts.len() == 1 => {
                let ExprKind::Global { module, name, .. } = &inner.kind else {
                    return None;
                };
                let op = if lang.is(Lang::OrdLt, module, name) {
                    "< 0"
                } else if lang.is(Lang::OrdLe, module, name) {
                    "<= 0"
                } else if lang.is(Lang::OrdGt, module, name) {
                    "> 0"
                } else if lang.is(Lang::OrdGe, module, name) {
                    ">= 0"
                } else {
                    return None;
                };
                dict_matches_adt(&dicts[0], Lang::Ord, &shape, lang, derived)
                    .then_some(AdtMethod::OrdCmp(shape, op))
            }
            _ => None,
        }
    }

    pub(super) fn adt_method_call(&mut self, method: &AdtMethod, all: &[Code<'a>]) -> Typed<'a> {
        let a = self.a;
        let code = match method {
            AdtMethod::Eq(s) => {
                self.register_adt(s);
                s.eq_doc(a, all[0].doc, all[1].doc).into()
            }
            AdtMethod::Compare(s) => {
                self.register_adt_ops(s, AdtOps::ORD);
                call(a, a.alloc_str(&s.ord_fn()), [all[0].doc, all[1].doc]).into()
            }
            AdtMethod::OrdCmp(s, op) => {
                self.register_adt_ops(s, AdtOps::ORD);
                let c = call(a, a.alloc_str(&s.cmp_fn()), [all[0].doc, all[1].doc]);
                docvec![a, "(", c, " ", *op, ")"].into()
            }
            AdtMethod::Hash(s) => {
                self.register_adt_ops(s, AdtOps::HASH);
                call(a, a.alloc_str(&s.hash_fn()), [all[0].doc]).into()
            }
            AdtMethod::Show(s) => {
                self.register_adt_ops(s, AdtOps::SHOW);
                call(a, a.alloc_str(&s.show_fn()), [all[0].doc]).into()
            }
        };
        Typed {
            code,
            ty: method.result(),
        }
    }
}
