//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments. A
//! plain pure binding's worker is the import itself.

use std::collections::HashMap;

use flick_ir::Shape;

use super::bridge::Toward;
use super::{Gen, names};
#[cfg(not(target_arch = "wasm32"))]
use crate::build::effects::{effect_specifier, on_kernel_with_bridge};
#[cfg(not(target_arch = "wasm32"))]
use crate::codegen::EffectBridge;
use crate::codegen::Options;
use crate::ir::Copies;
use crate::ir::{Foreign, ForeignMode, Item, ItemKind};

/// What crosses `f` as a copy: an import outside the runtime may change
/// Flick data in place (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
fn copies(f: &Foreign) -> Option<&Copies> {
    let trusted = f
        .binding
        .from
        .as_deref()
        .is_some_and(trusted_runtime_package);
    (!trusted && f.adapter.copies.any()).then_some(&f.adapter.copies)
}

fn trusted_runtime_package(spec: &str) -> bool {
    ["@flick-lang/runtime", "@flick-lang/effect-ts"]
        .iter()
        .any(|package| {
            spec.strip_prefix(package)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn contains_eff(shape: &Shape) -> bool {
    match shape {
        Shape::Eff(_) => true,
        Shape::Fun { args, result, .. } => args.iter().any(contains_eff) || contains_eff(result),
        Shape::List(item) | Shape::Option(item) => contains_eff(item),
        Shape::Result(error, value) => contains_eff(error) || contains_eff(value),
        Shape::Tuple(items) => items.iter().any(contains_eff),
        Shape::Record(fields) => fields.iter().any(|(_, field)| contains_eff(field)),
        _ => false,
    }
}

impl Gen<'_> {
    /// The import used by this binding and whether its value crosses to Effect.
    fn foreign_target(&mut self, f: &Foreign) -> (String, bool) {
        let Some(module) = &f.binding.from else {
            return (format!("globalThis.{}", f.binding.name), false);
        };
        let alias = format!("$f_{}", f.name);
        let spec = (self.options.foreign)(self.module, module);
        #[cfg(not(target_arch = "wasm32"))]
        let (spec, bridged) = {
            let mut spec = spec;
            let mut bridged = false;
            if self.kernel
                && !on_kernel_with_bridge(
                    &spec,
                    &f.binding.name,
                    self.options.effect_bridge == EffectBridge::Available,
                )
            {
                spec = effect_specifier(module);
                bridged = true;
            }
            bridged |= self.kernel
                && ((f.mode == ForeignMode::Effect && !spec.starts_with("@flick-lang/runtime"))
                    || spec == "@flick-lang/effect-ts"
                    || spec.starts_with("@flick-lang/effect-ts/"));
            bridged |= self.kernel
                && self.options.effect_bridge == EffectBridge::Available
                && !spec.starts_with("@flick-lang/runtime")
                && (f.adapter.shapes.iter().any(contains_eff)
                    || contains_eff(&f.adapter.result_shape));
            (spec, bridged)
        };
        #[cfg(target_arch = "wasm32")]
        let bridged = false;
        self.foreign
            .push((spec, f.binding.name.clone(), alias.clone()));
        (alias, bridged)
    }

    pub(super) fn foreign(&mut self, f: &Foreign) -> String {
        let (target, bridged) = self.foreign_target(f);
        let info = &f.adapter;
        let dicts: Vec<String> = (0..f.dicts).map(|i| format!("$d{i}")).collect();
        let params: Vec<String> = (0..f.taken()).map(|i| format!("$a{i}")).collect();
        let runtime = params
            .len()
            .checked_sub(1)
            .and_then(|i| f.stages.get(i))
            .copied()
            .unwrap_or(false);
        let copy_result = copies(f).is_some_and(|c| c.result);
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            let value = self.copied(copy_result, target);
            let value = if bridged {
                self.crossed(Toward::Flick, &info.result_shape, value)
            } else {
                value
            };
            return self.export(&names::ident(&f.name), &value);
        }
        let id = names::ident(&f.name);
        let plain = self.direct_foreigns.contains_key(&f.name) && !bridged;
        // a constant, not `export { … as … }`: the module names it too
        if plain && params.len() == 1 {
            return self.export(&id, &target);
        }
        let call = self.foreign_call(f, &target, &dicts, &params, bridged);
        let result = |g: &mut Self, v: &str| g.foreign_result(f, copy_result, v);
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let bridged_body = if bridged {
            self.bridged_body(f, runtime, &call, copy_result, catch.as_deref())
        } else {
            None
        };
        let mut body = match (bridged_body, f.mode, runtime) {
            (Some(body), _, _) => body,
            (None, ForeignMode::Pure, _) | (None, _, false) => result(self, &call),
            (None, ForeignMode::Sync | ForeignMode::Effect, true) if self.kernel => {
                self.kernel_foreign(f, copy_result, &call, catch)
            }
            (None, ForeignMode::Sync, true) => {
                let value = result(self, &call);
                match catch {
                    Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                    None => format!("{}(() => {value})", self.effect("sync")),
                }
            }
            (None, ForeignMode::Async, true) => {
                let value = result(self, "$v");
                let start = format!(
                    "{} Promise.resolve({call}).then(($v) => {value})",
                    self.arrow("$signal")
                );
                let catch = catch.map_or(String::new(), |c| format!(", {c}"));
                format!("{}({start}{catch})", self.rt("foreignAsync"))
            }
            (None, ForeignMode::Effect, true) => {
                let effect = match catch {
                    Some(c) => format!("{}(() => {call}, {c})", self.rt("foreignEffect")),
                    None => format!("{}(() => {call})", self.effect("suspend")),
                };
                if info.result.nullable || info.result.unit || copy_result {
                    let value = result(self, "$v");
                    format!("{}({effect}, ($v) => {value})", self.effect("map"))
                } else {
                    effect
                }
            }
        };
        if let Some(n) = (self.options.workers)(self.module, &f.name).filter(|&n| n == params.len())
        {
            let worker_id = format!("{id}${n}");
            let curried = self.curried(&worker_id, n);
            let worker = if plain {
                self.export(&worker_id, &target)
            } else {
                let worker = format!("{} {body}", self.arrow(&params.join(", ")));
                self.export(&worker_id, &worker)
            };
            return format!("{worker}\n{}", self.export(&id, &curried));
        }
        for param in params.iter().rev() {
            body = format!("{} {body}", self.arrow(param));
        }
        if !dicts.is_empty() {
            body = format!("{} {body}", self.arrow(&dicts.join(", ")));
        }
        self.export(&id, &body)
    }

    /// The call of the foreign `target` with the dictionaries and parameters
    /// of its wrapper, adapted to what JS expects.
    fn foreign_call(
        &mut self,
        f: &Foreign,
        target: &str,
        dicts: &[String],
        params: &[String],
        bridged: bool,
    ) -> String {
        let info = &f.adapter;
        let copies = copies(f);
        let mut args: Vec<String> = dicts.to_vec();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let copy = copies.is_some_and(|c| c.args.get(i) == Some(&true));
            let callback = copies.and_then(|c| c.callbacks.get(i).cloned().flatten());
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.optional.contains(&i) => {
                    let arg = format!("{}({param})", self.rt("optionalArg"));
                    self.copied(copy, arg)
                }
                n if bridged => {
                    let value = self.copied(copy, param.clone());
                    let shape = info.shapes.get(i).unwrap_or(&Shape::Data);
                    let host = self.crossed(Toward::Host, shape, value);
                    match n {
                        Some(n) => self.callback(&format!("({host})"), n, callback.as_ref()),
                        None if info.variadic && i + 1 == f.arity => format!("...{host}"),
                        None => host,
                    }
                }
                _ if info.receiver && i == 0 => self.copied(copy, param.clone()),
                Some(n) => self.callback(param, n, callback.as_ref()),
                None if info.variadic && i + 1 == f.arity => {
                    format!("...{}", self.copied(copy, param.clone()))
                }
                None => self.copied(copy, param.clone()),
            };
            args.push(arg);
        }
        if info.cancel {
            args.push("$signal".to_owned());
        }
        if info.receiver && !args.is_empty() {
            let receiver = args.remove(dicts.len());
            let rest: Vec<String> = [receiver].into_iter().chain(args).collect();
            format!("{target}.call({})", rest.join(", "))
        } else {
            format!("{target}({})", args.join(", "))
        }
    }
}

impl Gen<'_> {
    /// What a foreign call's value `v` is to the program: a copy when asked,
    /// its `null` a `None`, nothing for `Unit`.
    pub(super) fn foreign_result(&mut self, f: &Foreign, copy: bool, v: &str) -> String {
        let info = &f.adapter;
        let v = self.copied(copy, v.to_owned());
        let v = if info.result.nullable {
            format!("{}({v})", self.rt("fromNullable"))
        } else {
            v
        };
        if info.result.unit {
            format!("(void {v})")
        } else {
            v
        }
    }

    /// `value`, as a copy when `copy`.
    fn copied(&mut self, copy: bool, value: String) -> String {
        if copy {
            format!("{}({value})", self.rt("copyData"))
        } else {
            value
        }
    }

    /// The Flick function `param` as the JS callback of `n` arguments, with
    /// `copies` of what crosses: each argument JS gives, and the result.
    fn callback(&mut self, param: &str, n: usize, copies: Option<&(Vec<bool>, bool)>) -> String {
        let (args, result) = copies.map_or((&[][..], false), |(a, r)| (a.as_slice(), *r));
        // a callback of one argument keeps its identity unless it copies
        if n == 1 && !result && !args.contains(&true) {
            return param.to_owned();
        }
        let xs: Vec<String> = (0..n).map(|k| format!("$x{k}")).collect();
        let mut call = param.to_owned();
        for (k, x) in xs.iter().enumerate() {
            let x = self.copied(args.get(k) == Some(&true), x.clone());
            call = format!("{call}({x})");
        }
        let call = self.copied(result, call);
        format!("{} {call}", self.arrow(&xs.join(", ")))
    }
}

/// Whether the JS function itself can stand for `f` in release: a pure
/// import called with its arguments as they are and returning its result.
pub(super) fn plain(f: &Foreign, kernel: bool) -> bool {
    let info = &f.adapter;
    f.mode == ForeignMode::Pure
        && f.binding.from.is_some()
        && f.dicts == 0
        && f.catch.is_none()
        && !f.stages.is_empty()
        && f.stages.iter().all(|&runtime| !runtime)
        && f.arity == f.stages.len()
        && f.taken() == f.arity
        && !(info.receiver || info.cancel || info.variadic)
        && info.optional.is_empty()
        && info.callbacks.iter().all(|c| matches!(c, None | Some(1)))
        && (!kernel || (info.shapes.iter().all(Shape::is_data) && info.result_shape.is_data()))
        && !(info.result.nullable || info.result.unit)
        && copies(f).is_none()
}

/// The plain foreigns of a module (see [`plain`]), by name, with their arity
/// and import alias: a full application calls the import.
pub(super) fn plain_foreigns(
    items: &[Item],
    module: &str,
    options: &Options<'_>,
) -> HashMap<String, (usize, String)> {
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Foreign(f) if plain(f, options.rt) => {
                #[cfg(not(target_arch = "wasm32"))]
                if options.rt {
                    let source = f.binding.from.as_deref()?;
                    let spec = (options.foreign)(module, source);
                    if spec == "@flick-lang/effect-ts"
                        || spec.starts_with("@flick-lang/effect-ts/")
                        || !on_kernel_with_bridge(
                            &spec,
                            &f.binding.name,
                            options.effect_bridge == EffectBridge::Available,
                        )
                    {
                        return None;
                    }
                }
                #[cfg(target_arch = "wasm32")]
                let _ = module;
                Some((f.name.clone(), (f.stages.len(), format!("$f_{}", f.name))))
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::trusted_runtime_package;

    #[test]
    fn only_runtime_and_adapter_package_paths_are_trusted() {
        assert!(trusted_runtime_package("@flick-lang/runtime/rt"));
        assert!(trusted_runtime_package(
            "@flick-lang/effect-ts/effect/Effect"
        ));
        assert!(!trusted_runtime_package("@flick-lang/runtime-extra"));
        assert!(!trusted_runtime_package("@flick-lang/effect-ts-extra"));
    }
}
