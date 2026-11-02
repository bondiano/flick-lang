//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments. A
//! plain pure binding's worker is the import itself.

use std::collections::HashMap;

use super::{Gen, names};
use crate::ir::{Foreign, ForeignMode, Item, ItemKind};
use crate::types::Copies;

/// What crosses `f` as a copy: an import outside the runtime may change
/// Flick data in place (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
fn copies(f: &Foreign) -> Option<&Copies> {
    let trusted = f
        .module
        .as_deref()
        .is_some_and(|m| m.starts_with("@flick-lang/runtime"));
    (!trusted && f.info.copies.any()).then_some(&f.info.copies)
}

impl Gen<'_> {
    pub(super) fn foreign(&mut self, f: &Foreign) -> String {
        let alias = format!("$f_{}", f.name);
        let target = match &f.module {
            Some(module) => {
                let spec = (self.options.foreign)(self.module, module);
                self.foreign.push((spec, f.export.clone(), alias.clone()));
                alias
            }
            None => format!("globalThis.{}", f.export),
        };
        let info = &f.info;
        let dicts: Vec<String> = (0..f.dicts).map(|i| format!("$d{i}")).collect();
        let params: Vec<String> = (0..info.stages_taken()).map(|i| format!("$a{i}")).collect();
        let runtime = params
            .len()
            .checked_sub(1)
            .and_then(|i| f.stages.get(i))
            .copied()
            .unwrap_or(false);
        let copy_result = copies(f).is_some_and(|c| c.result);
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            let value = self.copied(copy_result, target);
            return self.export(&names::ident(&f.name), &value);
        }
        let id = names::ident(&f.name);
        let plain = self.direct_foreigns.contains_key(&f.name);
        // a constant, not `export { … as … }`: the module names it too
        if plain && params.len() == 1 {
            return self.export(&id, &target);
        }
        let call = self.foreign_call(f, &target, &dicts, &params);
        let result = |g: &mut Self, v: &str| {
            let v = g.copied(copy_result, v.to_owned());
            let v = if info.result.nullable {
                format!("{}({v})", g.rt("fromNullable"))
            } else {
                v
            };
            if info.result.unit {
                format!("(void {v})")
            } else {
                v
            }
        };
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let mut body = match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) => result(self, &call),
            (ForeignMode::Sync, true) => {
                let value = result(self, &call);
                match catch {
                    Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                    None => format!("{}(() => {value})", self.effect("sync")),
                }
            }
            (ForeignMode::Async, true) => {
                let value = result(self, "$v");
                let start = format!(
                    "{} Promise.resolve({call}).then(($v) => {value})",
                    self.arrow("$signal")
                );
                let catch = catch.map_or(String::new(), |c| format!(", {c}"));
                format!("{}({start}{catch})", self.rt("foreignAsync"))
            }
            (ForeignMode::Effect, true) => {
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
    ) -> String {
        let info = &f.info;
        let copies = copies(f);
        let mut args: Vec<String> = dicts.to_vec();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let copy = copies.is_some_and(|c| c.args.get(i) == Some(&true));
            let callback = copies.and_then(|c| c.callbacks.get(i).cloned().flatten());
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.this && i == 0 => self.copied(copy, param.clone()),
                Some(n) => self.callback(param, n, callback.as_ref()),
                None if info.optional.contains(&i) => {
                    let arg = format!("{}({param})", self.rt("optionalArg"));
                    self.copied(copy, arg)
                }
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
        if info.this && !args.is_empty() {
            let receiver = args.remove(dicts.len());
            let rest: Vec<String> = [receiver].into_iter().chain(args).collect();
            format!("{target}.call({})", rest.join(", "))
        } else {
            format!("{target}({})", args.join(", "))
        }
    }
}

impl Gen<'_> {
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
pub(super) fn plain(f: &Foreign) -> bool {
    let info = &f.info;
    f.mode == ForeignMode::Pure
        && f.module.is_some()
        && f.dicts == 0
        && f.catch.is_none()
        && !f.stages.is_empty()
        && f.stages.iter().all(|&runtime| !runtime)
        && f.arity == f.stages.len()
        && info.stages_taken() == f.arity
        && !(info.this || info.cancel || info.variadic)
        && info.optional.is_empty()
        && info.callbacks.iter().all(|c| matches!(c, None | Some(1)))
        && !(info.result.nullable || info.result.unit)
        && copies(f).is_none()
}

/// The plain foreigns of a module (see [`plain`]), by name, with their arity
/// and import alias: a full application calls the import.
pub(super) fn plain_foreigns(items: &[Item]) -> HashMap<String, (usize, String)> {
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Foreign(f) if plain(f) => {
                Some((f.name.clone(), (f.stages.len(), format!("$f_{}", f.name))))
            }
            _ => None,
        })
        .collect()
}
