//! The kernel's primitives, implemented independently of the JS and Go
//! runtimes from the reference (`reference/kernel.md`), as the evaluator
//! calls them: curried arguments in order, strings as UTF-16 code units.

use std::collections::BTreeMap;
use std::rc::Rc;

use flick_backend_support::Known;
use flick_backend_support::kernel::{Class, Manifest};
use flick_backend_support::opt::eval::{
    DictValue, EvalBudget, PrimitiveEval, PrimitiveOutcome, StopReason, Value,
};
use flick_backend_support::opt::{js_log, js_number};

use super::values::{
    BITS, bits, convert, equal, float, from_units, hash, order, show, text, to_int32, to_number,
    units, whole_number,
};
use flick_ir::{Binding, Constructor, ItemKind, Lang, Program};

/// The kernel primitives the evaluator cannot run: they call back into
/// Flick, need the runtime stage, read a record row by its label, or stop
/// with a defect, which the evaluator has no outcome for.
pub(super) const NOT_EVALUATED: &[(&str, &str)] = &[
    ("flick:rt", "listInitialize"),
    ("flick:rt", "defect"),
    ("flick:rt", "flickAssert"),
    ("flick:rt", "refMake"),
    ("flick:rt", "refGet"),
    ("flick:rt", "refSet"),
    ("flick:rt", "refModify"),
    ("flick:rt", "refSame"),
    ("flick:rt", "recordLabel"),
    ("flick:rt", "recordGet"),
    ("flick:rt", "recordExtend"),
    ("flick:rt", "recordRemove"),
    ("flick:rt", "recordProject"),
    ("flick:rt", "recordMergeDisjoint"),
    ("flick:rt", "recordLabels"),
    ("flick:Effect", "sleep"),
    ("flick:Effect", "flickNever"),
    ("flick:Effect", "flickYieldNow"),
];

/// The `(specifier, export)` a foreign binds, `global` for a JS global.
pub(super) fn pair(binding: &Binding) -> Option<(&str, &str)> {
    match binding {
        Binding::Module { from, name } => Some((from, name)),
        Binding::Global { name } => Some(("global", name)),
        Binding::Inline(_) => None,
    }
}

/// The evaluator's primitives: the kernel bindings of a program.
pub(super) struct Kernel {
    /// The binding and arity of each kernel foreign, by module and name.
    foreigns: BTreeMap<(String, String), (String, String, usize)>,
    some: Rc<Constructor>,
    none: Rc<Constructor>,
    ok: Rc<Constructor>,
    err: Rc<Constructor>,
    ordering: [Rc<Constructor>; 3],
}

impl Kernel {
    pub(super) fn new(program: &Program, manifest: &Manifest<'_>) -> Option<Kernel> {
        let mut foreigns = BTreeMap::new();
        let mut ctors: BTreeMap<(&str, &str), Rc<Constructor>> = BTreeMap::new();
        for module in &program.modules {
            for item in &module.items {
                match &item.kind {
                    ItemKind::Foreign(f) => {
                        let Some((from, export)) = pair(&f.binding) else {
                            continue;
                        };
                        if manifest.class(from, export) == Some(Class::Kernel) {
                            foreigns.insert(
                                (module.name.clone(), f.name.clone()),
                                (from.to_owned(), export.to_owned(), f.stages.len()),
                            );
                        }
                    }
                    ItemKind::Constructor(c) => {
                        ctors.insert((&c.module, &c.name), Rc::new(c.clone()));
                    }
                    _ => {}
                }
            }
        }
        let lang = Known::of(&program.lang);
        let by_lang = |role| lang.get(role).and_then(|key| ctors.get(&key).cloned());
        let prelude = lang.get(Lang::Prelude).map(|(m, _)| m)?;
        let order = |name| ctors.get(&(prelude, name)).cloned();
        Some(Kernel {
            foreigns,
            some: by_lang(Lang::Some)?,
            none: by_lang(Lang::None)?,
            ok: by_lang(Lang::Ok)?,
            err: by_lang(Lang::Err)?,
            ordering: [order("LT")?, order("EQ")?, order("GT")?],
        })
    }

    fn con<'a>(ctor: &Rc<Constructor>, args: Vec<Value<'a>>) -> Value<'a> {
        Value::Con {
            ctor: Rc::clone(ctor),
            args: Rc::new(args),
            dicts: Rc::new(Vec::new()),
        }
    }

    fn option<'a>(&self, value: Option<Value<'a>>) -> Value<'a> {
        match value {
            Some(v) => Self::con(&self.some, vec![v]),
            None => Self::con(&self.none, vec![]),
        }
    }

    #[allow(clippy::float_cmp, reason = "a whole number is exactly its whole part")]
    fn call<'a>(&self, from: &str, export: &str, args: &[Value<'a>]) -> Option<Value<'a>> {
        Some(match (from, export, args) {
            ("global", "Number", [v]) => Value::Float(float(v)?),
            ("global", "BigInt", [Value::Int(n)]) => Value::BigInt(i128::from(*n)),
            ("global", "String", [Value::BigInt(n)]) => text(&n.to_string()),
            ("global", "Math.floor", [Value::Float(x)]) => Value::Float(x.floor()),
            ("global", "Math.sqrt", [Value::Float(x)]) => Value::Float(x.sqrt()),
            ("global", "Math.log", [Value::Float(x)]) => Value::Float(js_log(*x)),
            ("flick:rt", "equals", [a, b]) => Value::Bool(equal(a, b)?),
            ("flick:rt", "compare", [a, b]) => {
                let at = usize::try_from(order(a, b)? + 1).ok()?;
                Self::con(&self.ordering[at], vec![])
            }
            ("flick:rt", "hash", [a]) => Value::Int(hash(a)?),
            ("flick:rt", "show", [a]) => text(&show(a)?),
            ("flick:rt", "stringFromNumber", [Value::Int(n)]) => text(&n.to_string()),
            ("flick:rt", "stringFromNumber", [Value::Float(x)]) => text(&js_number(*x)),
            ("flick:rt", "stringToNumber", [s]) => {
                self.option(to_number(&units(s)?).map(Value::Float))
            }
            ("flick:rt", "showInt64", [Value::Int64(n)]) => text(&n.to_string()),
            ("flick:rt", "showUInt64", [Value::UInt64(n)]) => text(&n.to_string()),
            ("flick:rt", "intTruncate", [Value::Float(x)]) => Value::Int(to_int32(*x)),
            ("flick:rt", "bigintFromNumber", [Value::Float(x)]) => {
                let whole = x.is_finite() && x.trunc() == *x;
                if whole && x.abs() >= 2f64.powi(127) {
                    return None;
                }
                self.option(whole.then(|| Value::BigInt(whole_number(*x))))
            }
            ("flick:rt", export, [v]) if export.starts_with("to") => convert(export, v)?,
            ("flick:rt", export, [a, b]) if BITS.contains(&export) => bits(export, a, b)?,
            ("flick:rt", "bitNot", [Value::Int(a)]) => Value::Int(!a),
            ("flick:rt", "int64BitNot", [Value::Int64(a)]) => Value::Int64(!a),
            ("flick:rt", "bytesFromList", [Value::List(items)]) => {
                for (i, v) in items.iter().enumerate() {
                    let Value::Int(n) = v else { return None };
                    if !(0..=255).contains(n) {
                        let index = Value::Int(i32::try_from(i).ok()?);
                        let bad = Value::Tuple(Rc::new(vec![index, v.clone()]));
                        return Some(Self::con(&self.err, vec![bad]));
                    }
                }
                Self::con(&self.ok, vec![Value::List(Rc::clone(items))])
            }
            ("flick:rt", "bytesToList", [list @ Value::List(_)]) => list.clone(),
            ("flick:rt", "bytesEncodeUtf8", [s]) => {
                let text: String = char::decode_utf16(units(s)?)
                    .map(|c| c.unwrap_or('\u{FFFD}'))
                    .collect();
                Value::List(Rc::new(
                    text.bytes().map(|b| Value::Int(b.into())).collect(),
                ))
            }
            ("flick:rt", "bytesDecodeUtf8", [Value::List(items)]) => {
                let bytes: Option<Vec<u8>> = items
                    .iter()
                    .map(|v| match v {
                        Value::Int(n) => u8::try_from(*n).ok(),
                        _ => None,
                    })
                    .collect();
                let decoded = String::from_utf8(bytes?).ok();
                self.option(decoded.map(|s| text(&s)))
            }
            ("flick:rt", "listLength", [Value::List(items)]) => {
                Value::Int(i32::try_from(items.len()).ok()?)
            }
            ("flick:rt", "listGet", [Value::Int(i), Value::List(items)]) => {
                self.option(usize::try_from(*i).ok().and_then(|i| items.get(i)).cloned())
            }
            ("flick:String", "length", [s]) => Value::Int(i32::try_from(units(s)?.len()).ok()?),
            ("flick:String", "charCodeAt", [Value::Int(i), s]) => self.option(
                usize::try_from(*i)
                    .ok()
                    .and_then(|i| units(s)?.get(i).copied())
                    .map(|u| Value::Int(u.into())),
            ),
            ("flick:String", "slice", [Value::Int(start), Value::Int(end), s]) => {
                let units = units(s)?;
                let at = |i: i32| {
                    let n = i64::try_from(units.len()).unwrap_or(i64::MAX);
                    let i = i64::from(i);
                    usize::try_from(if i < 0 { (n + i).max(0) } else { i.min(n) }).unwrap_or(0)
                };
                let (from, to) = (at(*start), at(*end));
                from_units(if from < to { &units[from..to] } else { &[] })
            }
            ("flick:String", "indexOf" | "lastIndexOf", [part, s]) => {
                let (part, s) = (units(part)?, units(s)?);
                let mut at = (0..(s.len() + 1).saturating_sub(part.len()))
                    .filter(|&i| s[i..i + part.len()] == part[..]);
                let found = if export == "indexOf" {
                    at.next()
                } else {
                    at.next_back()
                };
                self.option(found.and_then(|i| i32::try_from(i).ok()).map(Value::Int))
            }
            _ => return None,
        })
    }
}

impl PrimitiveEval for Kernel {
    fn arity(&self, module: &str, name: &str, _lang: &Known<'_>) -> Option<usize> {
        let (from, export, arity) = self.foreigns.get(&(module.to_owned(), name.to_owned()))?;
        (!NOT_EVALUATED.contains(&(from.as_str(), export.as_str()))).then_some(*arity)
    }

    fn eval<'a>(
        &self,
        module: &str,
        name: &str,
        _dicts: &[DictValue<'a>],
        args: &[Value<'a>],
        _lang: &Known<'_>,
        budget: &mut EvalBudget,
    ) -> PrimitiveOutcome<'a> {
        if let Err(reason) = budget.charge_steps(1) {
            return PrimitiveOutcome::Stop(reason);
        }
        self.foreigns
            .get(&(module.to_owned(), name.to_owned()))
            .and_then(|(from, export, _)| self.call(from, export, args))
            .map_or(
                PrimitiveOutcome::Stop(StopReason::UnknownOp),
                PrimitiveOutcome::Known,
            )
    }
}
