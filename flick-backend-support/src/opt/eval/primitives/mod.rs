//! Reviewed native primitive evaluation boundary for portable release IR (#805).
//!
//! Resolves only audited pure built-in `ItemKind::Foreign` declarations by
//! declared built-in module identity, existing `@lang` roles, exact `Foreign`
//! stage/arity/adapter semantics, and target ABI approval. Unknown bindings,
//! user homonyms, alternate adapters, dynamic dictionaries, unreviewed foreign
//! functions, and target-divergent edge cases decline cleanly without executing
//! host JS/Go or loading native libraries.

mod ops;

use std::rc::Rc;

use flick_ir::{Adapter, Constructor, Foreign, ForeignMode, ItemKind, Lang, Module, Rep, Shape};

use super::value::Value;
use super::{DictValue, EvalBudget, PrimitiveEval, PrimitiveOutcome, StopReason};
use crate::{EffectBridge, Known, effects};
use ops::{StringPred, eval_int_parse, eval_string_length, eval_string_pred, eval_string_slice};

/// Reviewed pure native primitives audited for portable release folding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativePrimitive {
    /// `Int.parse : String -> Option Int` (`flick:rt` `"intParse"`).
    IntParse,
    /// `String.length : String -> Int` (`flick:String` `"length"`).
    StringLength,
    /// `String.slice : Int -> Int -> String -> String` (`flick:String` `"slice"`, `arity 2`).
    StringSlice,
    /// `String.starts_with` / `Instances.text_starts_with` (`flick:String` `"startsWith"`, `arity 1`).
    StringStartsWith,
    /// `String.contains` / `Instances.text_contains` (`flick:String` `"includes"`, `arity 1`).
    StringContains,
    /// `String.ends_with` (`flick:String` `"endsWith"`, `arity 1`).
    StringEndsWith,
}

impl NativePrimitive {
    /// Pure Flick call arity (number of curried arguments).
    pub const fn arity(self) -> usize {
        match self {
            NativePrimitive::IntParse | NativePrimitive::StringLength => 1,
            NativePrimitive::StringStartsWith
            | NativePrimitive::StringContains
            | NativePrimitive::StringEndsWith => 2,
            NativePrimitive::StringSlice => 3,
        }
    }

    /// Canonical foreign binding `(from, export)`.
    pub const fn binding(self) -> (&'static str, &'static str) {
        match self {
            NativePrimitive::IntParse => ("flick:rt", "intParse"),
            NativePrimitive::StringLength => ("flick:String", "length"),
            NativePrimitive::StringSlice => ("flick:String", "slice"),
            NativePrimitive::StringStartsWith => ("flick:String", "startsWith"),
            NativePrimitive::StringContains => ("flick:String", "includes"),
            NativePrimitive::StringEndsWith => ("flick:String", "endsWith"),
        }
    }

    const fn foreign_arity(self) -> usize {
        if matches!(self, NativePrimitive::StringSlice) {
            2
        } else {
            1
        }
    }
}

const BIT_INT_PARSE: u8 = 1 << 0;
const BIT_STR_LEN: u8 = 1 << 1;
const BIT_STR_SLICE: u8 = 1 << 2;
const BIT_STR_STARTS: u8 = 1 << 3;
const BIT_STR_CONTAINS: u8 = 1 << 4;
const BIT_STR_ENDS: u8 = 1 << 5;
const BIT_INST_STARTS: u8 = 1 << 6;
const BIT_INST_CONTAINS: u8 = 1 << 7;

fn slot_of(module: &str, name: &str, is_instances: bool) -> Option<(u8, NativePrimitive)> {
    match (module, name) {
        ("Int", "parse") => Some((BIT_INT_PARSE, NativePrimitive::IntParse)),
        ("String", "length") => Some((BIT_STR_LEN, NativePrimitive::StringLength)),
        ("String", "slice") => Some((BIT_STR_SLICE, NativePrimitive::StringSlice)),
        ("String", "starts_with") => Some((BIT_STR_STARTS, NativePrimitive::StringStartsWith)),
        ("String", "contains") => Some((BIT_STR_CONTAINS, NativePrimitive::StringContains)),
        ("String", "ends_with") => Some((BIT_STR_ENDS, NativePrimitive::StringEndsWith)),
        (_, "text_starts_with") if is_instances => {
            Some((BIT_INST_STARTS, NativePrimitive::StringStartsWith))
        }
        (_, "text_contains") if is_instances => {
            Some((BIT_INST_CONTAINS, NativePrimitive::StringContains))
        }
        _ => None,
    }
}

/// Target-validated table of reviewed native primitives built from a program's
/// declared built-in IR modules.
///
/// Stored as a compact bitmask over the 8 audited built-in declarations so the
/// browser WASM compiler incurs no entry-vector or string-table overhead.
#[derive(Clone, Default)]
pub struct ReviewedPrimitives {
    bits: u8,
    instances_module: Option<Rc<str>>,
    option: Option<(Rc<Constructor>, Rc<Constructor>)>,
}

impl ReviewedPrimitives {
    /// Resolves reviewed native primitives from `modules`, accepting ONLY
    /// declarations in modules where `is_builtin(module.name)` holds, whose
    /// `ItemKind::Foreign` matches the exact reviewed built-in identity, stage,
    /// arity, and adapter semantics, whose required `@lang` roles are present in
    /// `lang`, and whose target ABI binding is approved by `target_allows`.
    pub fn build(
        modules: &[Module],
        is_builtin: &dyn Fn(&str) -> bool,
        lang: &Known<'_>,
        target_allows: &dyn Fn(&str, &Foreign, NativePrimitive) -> bool,
    ) -> Self {
        let option = resolve_option_constructors(modules, is_builtin, lang);
        let instances_mod = lang.get(Lang::Instances).map(|(m, _)| m);
        let mut bits = 0_u8;
        let mut instances_module: Option<Rc<str>> = None;

        for module in modules {
            if !is_builtin(&module.name) {
                continue;
            }
            let is_instances = instances_mod == Some(module.name.as_str());
            if !is_instances && module.name != "Int" && module.name != "String" {
                continue;
            }
            for item in &module.items {
                let ItemKind::Foreign(foreign) = &item.kind else {
                    continue;
                };
                let Some((bit, prim)) =
                    classify_builtin_foreign(&module.name, is_instances, foreign, lang)
                else {
                    continue;
                };
                if module_name_count(module, &foreign.name) != 1
                    || (prim == NativePrimitive::IntParse && option.is_none())
                    || !target_allows(&module.name, foreign, prim)
                {
                    continue;
                }
                bits |= bit;
                if is_instances && instances_module.is_none() {
                    instances_module = Some(Rc::from(module.name.as_str()));
                }
            }
        }

        let option = ((bits & BIT_INT_PARSE) != 0).then_some(option).flatten();
        ReviewedPrimitives {
            bits,
            instances_module,
            option,
        }
    }

    /// Convenience constructor validating against the official JS target ABI
    /// (both Flick's own runtime and the Effect adapter).
    pub fn for_js<'r>(
        modules: &[Module],
        is_builtin: &dyn Fn(&str) -> bool,
        lang: &Known<'_>,
        rt: bool,
        bridge: EffectBridge,
        resolved_spec: &dyn Fn(&str, &str) -> Option<&'r str>,
    ) -> Self {
        Self::build(modules, is_builtin, lang, &|module, foreign, prim| {
            let override_spec = foreign
                .binding
                .from()
                .and_then(|spec| resolved_spec(module, spec));
            js_target_allows(foreign, prim, rt, bridge, override_spec)
        })
    }

    /// Returns whether no reviewed primitives were resolved.
    pub fn is_empty(&self) -> bool {
        self.bits == 0
    }

    /// Looks up a resolved primitive by declaring module and value name.
    pub fn lookup(&self, module: &str, name: &str) -> Option<NativePrimitive> {
        let is_instances = self.instances_module.as_deref() == Some(module);
        let (bit, prim) = slot_of(module, name, is_instances)?;
        ((self.bits & bit) != 0).then_some(prim)
    }
}

/// Validates that `foreign` resolves to the canonical JS runtime/adapter target
/// for `prim` (declining alternate or overridden adapters and unavailable stubs).
pub fn js_target_allows(
    foreign: &Foreign,
    prim: NativePrimitive,
    rt: bool,
    bridge: EffectBridge,
    override_spec: Option<&str>,
) -> bool {
    let Some((from, export)) = foreign.binding.module_export() else {
        return false;
    };
    let (expected_from, expected_export) = prim.binding();
    if from != expected_from || export != expected_export {
        return false;
    }
    if rt {
        let Some(expected_rt) = effects::rt_specifier(from) else {
            return false;
        };
        let actual = override_spec.unwrap_or(expected_rt.as_str());
        actual == expected_rt
            && !effects::unavailable_on_kernel(actual, expected_export)
            && effects::selected_on_kernel(actual, expected_export, bridge)
    } else {
        let expected_effect = effects::effect_specifier(from);
        override_spec.unwrap_or(expected_effect.as_str()) == expected_effect
    }
}

impl PrimitiveEval for ReviewedPrimitives {
    fn arity(&self, module: &str, name: &str, lang: &Known<'_>) -> Option<usize> {
        let prim = self.lookup(module, name)?;
        if !has_required_roles(prim, module, lang)
            || (prim == NativePrimitive::IntParse && self.option.is_none())
        {
            return None;
        }
        Some(prim.arity())
    }

    fn eval<'a>(
        &self,
        module: &str,
        name: &str,
        dicts: &[DictValue<'a>],
        args: &[Value<'a>],
        lang: &Known<'_>,
        budget: &mut EvalBudget,
    ) -> PrimitiveOutcome<'a> {
        if !dicts.is_empty() {
            return PrimitiveOutcome::Stop(StopReason::UnknownOp);
        }
        let Some(prim) = self.lookup(module, name) else {
            return PrimitiveOutcome::Stop(StopReason::UnknownOp);
        };
        if args.len() != prim.arity() || !has_required_roles(prim, module, lang) {
            return PrimitiveOutcome::Stop(StopReason::UnknownOp);
        }
        if let Err(reason) = budget.charge_steps(1) {
            return PrimitiveOutcome::Stop(reason);
        }
        match prim {
            NativePrimitive::IntParse => {
                let Some((some_ctor, none_ctor)) = self.option.as_ref() else {
                    return PrimitiveOutcome::Stop(StopReason::UnknownOp);
                };
                eval_int_parse(&args[0], some_ctor, none_ctor, budget)
            }
            NativePrimitive::StringLength => eval_string_length(&args[0], budget),
            NativePrimitive::StringSlice => eval_string_slice(&args[0], &args[1], &args[2], budget),
            NativePrimitive::StringStartsWith => {
                eval_string_pred(StringPred::StartsWith, &args[0], &args[1], budget)
            }
            NativePrimitive::StringContains => {
                eval_string_pred(StringPred::Contains, &args[0], &args[1], budget)
            }
            NativePrimitive::StringEndsWith => {
                eval_string_pred(StringPred::EndsWith, &args[0], &args[1], budget)
            }
        }
    }
}

fn module_name_count(module: &Module, name: &str) -> usize {
    let mut count = 0_usize;
    for item in &module.items {
        if match &item.kind {
            ItemKind::Foreign(f) => f.name == name,
            ItemKind::Value { name: n, .. } => n == name,
            _ => false,
        } {
            count += 1;
        }
    }
    count
}

fn has_required_roles(prim: NativePrimitive, module: &str, lang: &Known<'_>) -> bool {
    if lang.get(Lang::String).is_none() {
        return false;
    }
    let extra_ok = match prim {
        NativePrimitive::IntParse => {
            lang.get(Lang::Int).is_some()
                && lang.get(Lang::Option).is_some()
                && lang.get(Lang::Some).is_some()
                && lang.get(Lang::None).is_some()
        }
        NativePrimitive::StringLength | NativePrimitive::StringSlice => {
            lang.get(Lang::Int).is_some()
        }
        NativePrimitive::StringStartsWith
        | NativePrimitive::StringContains
        | NativePrimitive::StringEndsWith => lang.get(Lang::Bool).is_some(),
    };
    extra_ok
        && (module == "Int"
            || module == "String"
            || lang.get(Lang::Instances).is_some_and(|(m, _)| m == module))
}

fn has_expected_adapter(a: &Adapter, foreign_arity: usize, prim: NativePrimitive) -> bool {
    let is_parse = prim == NativePrimitive::IntParse;
    let result_shape_ok = if is_parse {
        matches!(&a.result_shape, Shape::Option(inner) if matches!(**inner, Shape::Data))
    } else {
        matches!(a.result_shape, Shape::Data)
    };
    a.words.is_empty()
        && a.optional.is_empty()
        && a.owned.is_empty()
        && !a.result.nullable
        && !a.result.unit
        && a.callbacks.len() == foreign_arity
        && a.callbacks.iter().all(Option::is_none)
        && a.copies.args.len() == foreign_arity
        && a.copies.args.iter().all(|&c| !c)
        && a.copies.result == is_parse
        && a.copies.callbacks.len() == foreign_arity
        && a.copies.callbacks.iter().all(Option::is_none)
        && a.shapes.len() == foreign_arity
        && a.shapes.iter().all(|s| matches!(s, Shape::Data))
        && result_shape_ok
}

fn classify_builtin_foreign(
    module: &str,
    is_instances: bool,
    foreign: &Foreign,
    lang: &Known<'_>,
) -> Option<(u8, NativePrimitive)> {
    if foreign.source_name.is_some()
        || foreign.mode != ForeignMode::Pure
        || foreign.dicts != 0
        || foreign.catch.is_some()
    {
        return None;
    }
    let (bit, prim) = slot_of(module, &foreign.name, is_instances)?;
    let (expected_from, expected_export) = prim.binding();
    if foreign.binding.module_export() != Some((expected_from, expected_export))
        || !has_required_roles(prim, module, lang)
    {
        return None;
    }
    let foreign_arity = prim.foreign_arity();
    if foreign.arity != foreign_arity
        || foreign.stages.len() != prim.arity()
        || foreign.stages.iter().any(|&runtime| runtime)
    {
        return None;
    }
    has_expected_adapter(&foreign.adapter, foreign_arity, prim).then_some((bit, prim))
}

fn resolve_option_constructors(
    modules: &[Module],
    is_builtin: &dyn Fn(&str) -> bool,
    lang: &Known<'_>,
) -> Option<(Rc<Constructor>, Rc<Constructor>)> {
    let (opt_mod, opt_name) = lang.get(Lang::Option)?;
    let (some_mod, some_name) = lang.get(Lang::Some)?;
    let (none_mod, none_name) = lang.get(Lang::None)?;
    if some_mod != opt_mod || none_mod != opt_mod || some_name == none_name || !is_builtin(opt_mod)
    {
        return None;
    }
    let owner = modules.iter().find(|m| m.name == opt_mod)?;
    let mut some = None;
    let mut none = None;
    for item in &owner.items {
        let ItemKind::Constructor(c) = &item.kind else {
            continue;
        };
        let (slot, expected_arity) = if c.name == some_name {
            (&mut some, 1)
        } else if c.name == none_name {
            (&mut none, 0)
        } else {
            continue;
        };
        if slot.is_some()
            || c.module != opt_mod
            || c.ty != opt_name
            || c.arity != expected_arity
            || c.rep != Rep::Tagged
            || c.dicts != 0
        {
            return None;
        }
        *slot = Some(Rc::new(c.clone()));
    }
    #[cfg_attr(not(feature = "typed-ir"), allow(unused_mut))]
    let mut some = some?;
    #[cfg_attr(not(feature = "typed-ir"), allow(unused_mut))]
    let mut none = none?;
    #[cfg(feature = "typed-ir")]
    if !some.fields.is_empty()
        && let Some((int_mod, int_name)) = lang.get(Lang::Int)
    {
        let int_ty = flick_ir::TypeRef::new(flick_ir::IrType::Named {
            module: int_mod.to_owned(),
            name: int_name.to_owned(),
            args: Vec::new(),
        });
        let s = Rc::make_mut(&mut some);
        s.fields = vec![int_ty.clone()];
        s.type_args = vec![int_ty.clone()];
        let n = Rc::make_mut(&mut none);
        n.fields.clear();
        n.type_args = vec![int_ty];
    }
    Some((some, none))
}
