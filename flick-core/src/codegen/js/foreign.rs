//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments. A
//! plain pure binding's worker is the import itself.

use std::collections::HashMap;

use super::{Gen, names};
use crate::ir::Copies;
use crate::ir::{Foreign, ForeignMode, Item, ItemKind};

/// What crosses `f` as a copy: an import outside the runtime may change
/// Flick data in place (`docs/formal/ts-holes.md#31-flick-data-is-copied-at-the-foreign-boundary`).
fn copies(f: &Foreign) -> Option<&Copies> {
    let trusted = f
        .binding
        .from
        .as_deref()
        .is_some_and(|m| m.starts_with("@flick-lang/runtime"));
    (!trusted && f.adapter.copies.any()).then_some(&f.adapter.copies)
}

impl Gen<'_> {
    pub(super) fn foreign(&mut self, f: &Foreign) -> String {
        let alias = format!("$f_{}", f.name);
        let mut bridged = false;
        let target = match &f.binding.from {
            Some(module) => {
                let mut spec = (self.options.foreign)(self.module, module);
                // spike: what the kernel has no export for comes from Effect, over the bridge
                if self.kernel && !crate::build::effects::on_kernel(&spec, &f.binding.name) {
                    spec = module.clone();
                    bridged = true;
                }
                // spike: a foreign `effect` of the program's own gives an Effect
                bridged |= self.kernel && f.mode == ForeignMode::Effect && !module.starts_with("@flick-lang/runtime");
                self.runtime |= self.kernel;
                self.foreign
                    .push((spec, f.binding.name.clone(), alias.clone()));
                alias
            }
            None => format!("globalThis.{}", f.binding.name),
        };
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
            let value = if bridged { self.from_host(&info.result_shape, value) } else { value };
            return self.export(&names::ident(&f.name), &value);
        }
        let id = names::ident(&f.name);
        // spike: a shape to convert means a wrapper
        let plain = self.direct_foreigns.contains_key(&f.name)
            && !(bridged && (!info.result_shape.is_data() || info.shapes.iter().any(|s| !s.is_data())));
        // a constant, not `export { … as … }`: the module names it too
        if plain && params.len() == 1 {
            return self.export(&id, &target);
        }
        let call = self.foreign_call(f, &target, &dicts, &params, bridged);
        let result = |g: &mut Self, v: &str| g.foreign_result(f, copy_result, v);
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let mut body = match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) if bridged => {
                let value = result(self, &call);
                self.from_host(&info.result_shape, value)
            }
            (ForeignMode::Pure, _) | (_, false) => result(self, &call),
            (ForeignMode::Effect, true) if bridged => {
                let made = match catch {
                    Some(c) => format!("{}(() => {}({call}), {c})", self.rt("foreignEffect"), self.rt("fromEffect")),
                    None => format!("{}({call})", self.rt("fromEffect")),
                };
                let value = result(self, "$v");
                let value = self.from_host(&info.result_shape, value);
                if value == "$v" { made } else { format!("{}({made}, ($v) => {value})", self.rt("mapped")) }
            }
            (ForeignMode::Sync, true) if bridged => {
                let value = result(self, &call);
                let value = self.from_host(&info.result_shape, value);
                match catch {
                    Some(c) => format!("{}(() => {value}, {c})", self.rt("foreignSync")),
                    None => value,
                }
            }
            (ForeignMode::Sync | ForeignMode::Effect, true) if self.kernel => {
                self.kernel_foreign(f, copy_result, &call, catch)
            }
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
        bridged: bool,
    ) -> String {
        let info = &f.adapter;
        let copies = copies(f);
        let mut args: Vec<String> = dicts.to_vec();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let copy = copies.is_some_and(|c| c.args.get(i) == Some(&true));
            let callback = copies.and_then(|c| c.callbacks.get(i).cloned().flatten());
            let arg = match info.callbacks.get(i).copied().flatten() {
                n if bridged && !info.shapes.get(i).is_none_or(flick_ir::Shape::is_data) => {
                    let host = self.to_host(&info.shapes[i], param.clone());
                    match n {
                        Some(n) => self.callback(&format!("({host})"), n, callback.as_ref()),
                        None => host,
                    }
                }
                _ if info.receiver && i == 0 => self.copied(copy, param.clone()),
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

impl Gen<'_> {
    /// spike: the Flick value `v` as Effect's side takes it.
    pub(super) fn to_host(&mut self, s: &flick_ir::Shape, v: String) -> String {
        use flick_ir::Shape;
        match s {
            Shape::Data => v,
            Shape::Fun { args, result, runtime, sync } => {
                let d = self.bridge_depth();
                let xs: Vec<String> = (0..args.len()).map(|k| format!("$h{d}_{k}")).collect();
                let mut call = v;
                for (x, a) in xs.iter().zip(args) {
                    let arg = self.from_host(a, x.clone());
                    call = format!("{call}({arg})");
                }
                let body = if *sync {
                    let at = format!("$r{d}");
                    let map = self.to_host(result, at.clone());
                    let ran = format!("{}(() => {call})", self.rt("callSync"));
                    if map == at { ran } else { format!("(({at}) => {map})({ran})") }
                } else if *runtime {
                    let at = format!("$r{d}");
                    let map = self.to_host(result, at.clone());
                    if map == at {
                        format!("{}(() => {call})", self.rt("callEffect"))
                    } else {
                        format!("{}(() => {}({call}, ({at}) => {map}))", self.rt("callEffect"), self.rt("mapped"))
                    }
                } else {
                    self.to_host(result, call)
                };
                // curried, as Flick calls it: a callback the binding's host calls with several
                // arguments at once is adapted by `callback`, as when nothing is bridged
                xs.iter()
                    .rev()
                    .fold(body, |body, x| format!("({x}) => {body}"))
            }
            Shape::Eff(inner) => {
                let d = self.bridge_depth();
                let at = format!("$r{d}");
                let map = self.to_host(inner, at.clone());
                if map == at {
                    format!("{}(() => {v}())", self.rt("callEffect"))
                } else {
                    format!("{}(() => {}({v}(), ({at}) => {map}))", self.rt("callEffect"), self.rt("mapped"))
                }
            }
            Shape::List(inner) => {
                let d = self.bridge_depth();
                let map = self.to_host(inner, format!("$e{d}"));
                format!("({v}).map(($e{d}) => {map})")
            }
            Shape::Option(inner) => {
                let d = self.bridge_depth();
                let map = self.to_host(inner, format!("$e{d}"));
                format!("{}({v}, ($e{d}) => {map})", self.rt("optionMapped"))
            }
            Shape::Result(e, a) => {
                let d = self.bridge_depth();
                let (fe, fa) = (self.to_host(e, format!("$e{d}")), self.to_host(a, format!("$a{d}")));
                format!("{}({v}, ($a{d}) => {fa}, ($e{d}) => {fe})", self.rt("resultMapped"))
            }
            Shape::Tuple(items) => {
                let d = self.bridge_depth();
                let parts: Vec<String> = items.iter().enumerate().map(|(i, s)| self.to_host(s, format!("$t{d}[{i}]"))).collect();
                format!("(($t{d}) => [{}])({v})", parts.join(", "))
            }
            Shape::Record(fields) => {
                let d = self.bridge_depth();
                let parts: Vec<String> = fields.iter().filter(|(_, s)| !s.is_data()).map(|(n, s)| {
                    let name = names::string(n);
                    format!("[{name}]: {}", self.to_host(s, format!("$t{d}[{name}]")))
                }).collect();
                format!("(($t{d}) => ({{ ...$t{d}, {} }}))({v})", parts.join(", "))
            }
            Shape::Layer => format!("{}({v})", self.rt("layerToEffect")),
            Shape::Handlers => format!("{}({v})", self.rt("handlersToEffect")),
            Shape::Exit => format!("{}({v})", self.rt("exitToEffect")),
            Shape::Failure => format!("{}({v})", self.rt("failureToEffect")),
        }
    }

    /// spike: the value `v` of Effect's side as Flick takes it.
    pub(super) fn from_host(&mut self, s: &flick_ir::Shape, v: String) -> String {
        use flick_ir::Shape;
        match s {
            Shape::Data | Shape::Layer | Shape::Handlers => v,
            Shape::Fun { args, result, runtime, sync } => {
                let d = self.bridge_depth();
                let xs: Vec<String> = (0..args.len()).map(|k| format!("$g{d}_{k}")).collect();
                let mut call = v;
                for (x, a) in xs.iter().zip(args) {
                    let arg = self.to_host(a, x.clone());
                    call = format!("{call}({arg})");
                }
                let mut body = if *sync {
                    let value = format!("{}(() => {call})", self.rt("inSyncContext"));
                    self.from_host(result, value)
                } else if *runtime {
                    let at = format!("$r{d}");
                    let map = self.from_host(result, at.clone());
                    let ran = format!("{}({call})", self.rt("fromEffect"));
                    if map == at { ran } else { format!("{}({ran}, ({at}) => {map})", self.rt("mapped")) }
                } else {
                    self.from_host(result, call)
                };
                for x in xs.iter().rev() {
                    body = format!("({x}) => {body}");
                }
                body
            }
            Shape::Eff(inner) => {
                let d = self.bridge_depth();
                let at = format!("$r{d}");
                let map = self.from_host(inner, at.clone());
                let ran = format!("{}({v})", self.rt("fromEffect"));
                if map == at {
                    format!("() => {ran}")
                } else {
                    format!("() => {}({ran}, ({at}) => {map})", self.rt("mapped"))
                }
            }
            Shape::List(inner) => {
                let d = self.bridge_depth();
                let map = self.from_host(inner, format!("$e{d}"));
                format!("({v}).map(($e{d}) => {map})")
            }
            Shape::Option(inner) => {
                let d = self.bridge_depth();
                let map = self.from_host(inner, format!("$e{d}"));
                format!("{}({v}, ($e{d}) => {map})", self.rt("optionMapped"))
            }
            Shape::Result(e, a) => {
                let d = self.bridge_depth();
                let (fe, fa) = (self.from_host(e, format!("$e{d}")), self.from_host(a, format!("$a{d}")));
                format!("{}({v}, ($a{d}) => {fa}, ($e{d}) => {fe})", self.rt("resultMapped"))
            }
            Shape::Tuple(items) => {
                let d = self.bridge_depth();
                let parts: Vec<String> = items.iter().enumerate().map(|(i, s)| self.from_host(s, format!("$t{d}[{i}]"))).collect();
                format!("(($t{d}) => [{}])({v})", parts.join(", "))
            }
            Shape::Record(fields) => {
                let d = self.bridge_depth();
                let parts: Vec<String> = fields.iter().filter(|(_, s)| !s.is_data()).map(|(n, s)| {
                    let name = names::string(n);
                    format!("[{name}]: {}", self.from_host(s, format!("$t{d}[{name}]")))
                }).collect();
                format!("(($t{d}) => ({{ ...$t{d}, {} }}))({v})", parts.join(", "))
            }
            Shape::Exit => format!("{}({v})", self.rt("exitFromEffect")),
            Shape::Failure => format!("{}({v})", self.rt("failureFromEffect")),
        }
    }

    fn bridge_depth(&mut self) -> String {
        let t = self.temp();
        t.trim_start_matches('$').to_owned()
    }
}

/// Whether the JS function itself can stand for `f` in release: a pure
/// import called with its arguments as they are and returning its result.
pub(super) fn plain(f: &Foreign) -> bool {
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
