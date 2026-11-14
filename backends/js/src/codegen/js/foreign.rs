//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments. A
//! plain pure binding's worker is the import itself.

use std::collections::HashMap;

use flick_ir::Shape;

use super::bridge::Toward;
use super::doc::{After, Before, Head, Js, Stmt, call};
use super::{Gen, names};
use crate::codegen::Options;
use crate::effects::owned;
use crate::effects::{belongs_to, bindings};
use crate::effects::{effect_specifier, requires_adapter, selected_on_kernel};
use crate::ir::Copies;
use crate::ir::{Foreign, ForeignMode, Item, ItemKind};
use flick_pretty::docvec;

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
    owned(spec)
}

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

impl<'d> Gen<'_, 'd> {
    /// The import used by this binding and whether its value crosses to Effect.
    fn foreign_target(&mut self, f: &Foreign) -> (String, bool) {
        let Some(module) = &f.binding.from else {
            return (format!("globalThis.{}", f.binding.name), false);
        };
        let alias = format!("$f_{}", f.name);
        let spec = self.options.foreign(self.module, module);
        let (spec, bridged) = {
            let mut spec = spec;
            let mut bridged = false;
            if self.kernel
                && !self.options.effect_bridge.available()
                && requires_adapter(&spec, &f.binding.name, f.mode == ForeignMode::Effect)
            {
                // Build validation rejects reachable uses. Unused members of a
                // mixed built-in module must not import an unavailable adapter.
                let message = serde_json::to_string(&format!(
                    "`{spec}` `{}` requires the optional `{}` adapter",
                    f.binding.name,
                    bindings::ADAPTER_PREFIX
                ))
                .expect("a diagnostic string serializes");
                let missing = format!("(() => {{ throw new Error({message}); }})");
                // An unreachable foreign value can also be a Schema or other
                // opaque constant. Keep its emitted TS declaration assignable.
                let missing = if self.options.typescript() {
                    format!("({missing} as any)")
                } else {
                    missing
                };
                return (missing, false);
            }
            if self.kernel
                && !selected_on_kernel(&spec, &f.binding.name, self.options.effect_bridge)
            {
                spec = effect_specifier(module);
                bridged = true;
            }
            bridged |= self.kernel
                && ((f.mode == ForeignMode::Effect && !belongs_to(&spec, bindings::OWNED_PREFIX))
                    || belongs_to(&spec, bindings::ADAPTER_PREFIX));
            bridged |= self.kernel
                && self.options.effect_bridge.available()
                && !belongs_to(&spec, bindings::OWNED_PREFIX)
                && (f.adapter.shapes.iter().any(contains_eff)
                    || contains_eff(&f.adapter.result_shape));
            (spec, bridged)
        };
        self.foreign
            .push((spec, f.binding.name.clone(), alias.clone()));
        (alias, bridged)
    }

    pub(super) fn foreign(&mut self, f: &Foreign) -> Vec<Stmt<'d>> {
        let a = self.a;
        let (target, bridged) = self.foreign_target(f);
        let info = &f.adapter;
        let copy_result = copies(f).is_some_and(|c| c.result);
        let dicts: Vec<String> = (0..f.dicts).map(|i| format!("$d{i}")).collect();
        let params: Vec<String> = (0..f.taken()).map(|i| format!("$a{i}")).collect();
        let runtime = params
            .len()
            .checked_sub(1)
            .and_then(|i| f.stages.get(i))
            .copied()
            .unwrap_or(false);
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            let value = self.copied(copy_result, self.leaf(&target));
            let value = if bridged {
                self.crossed(Toward::Flick, &info.result_shape, value)
            } else {
                value
            };
            return vec![self.export(&names::ident(&f.name), value)];
        }
        let id = names::ident(&f.name);
        let plain = self.direct_foreigns.contains_key(&f.name) && !bridged;
        // a constant, not `export { … as … }`: the module names it too
        if plain && params.len() == 1 {
            return vec![self.export(&id, self.leaf(&target))];
        }
        let applied = self.foreign_call(f, &target, &dicts, &params, bridged);
        let result = |g: &mut Self, v: Js<'d>| g.foreign_result(f, copy_result, v);
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let resolved = |g: &mut Self| {
            let value = result(g, g.leaf("$v"));
            g.arrow_to(a.text("($v) =>"), value)
        };
        let bridged_body = if bridged {
            self.bridged_body(f, runtime, applied, copy_result, catch)
        } else {
            None
        };
        let mut body = match (bridged_body, f.mode, runtime) {
            (Some(body), _, _) => body,
            (None, ForeignMode::Pure, _) | (None, _, false) => result(self, applied),
            (None, ForeignMode::Sync | ForeignMode::Effect, true) if self.kernel => {
                self.kernel_foreign(f, copy_result, applied, catch)
            }
            (None, ForeignMode::Sync, true) => {
                let value = result(self, applied);
                let thunk = self.thunk(value);
                self.guarded(("foreignSync", "sync"), thunk, catch)
            }
            (None, ForeignMode::Async, true) => {
                let then = resolved(self);
                let promise = call(a, self.leaf("Promise.resolve"), &[applied]);
                let started = call(a, self.member(promise, "then"), &[then]);
                let start = self.lambda_of("$signal", started);
                let foreign_async = self.rt("foreignAsync");
                let args: Vec<Js<'d>> = [start].into_iter().chain(catch).collect();
                call(a, foreign_async, &args)
            }
            (None, ForeignMode::Effect, true) => {
                let thunk = self.thunk(applied);
                let effect = self.guarded(("foreignEffect", "suspend"), thunk, catch);
                if info.result.nullable || info.result.unit || copy_result {
                    let then = resolved(self);
                    let map = self.effect("map");
                    call(a, map, &[effect, then])
                } else {
                    effect
                }
            }
        };
        if let Some(n) = self
            .options
            .worker(self.module, &f.name)
            .filter(|&n| n == params.len())
        {
            let worker_id = format!("{id}${n}");
            let curried = self.curried(&worker_id, n);
            let worker = if plain {
                self.export(&worker_id, self.leaf(&target))
            } else {
                let worker = self.lambda_of(&params.join(", "), body);
                self.export(&worker_id, worker)
            };
            return vec![worker, self.export(&id, curried)];
        }
        for param in params.iter().rev() {
            body = self.lambda_of(param, body);
        }
        if !dicts.is_empty() {
            body = self.lambda_of(&dicts.join(", "), body);
        }
        vec![self.export(&id, body)]
    }

    /// `$rt.<caught>(thunk, catch)` with a `catch`, `Effect.<plain>(thunk)`
    /// without.
    fn guarded(
        &mut self,
        (caught, plain): (&str, &str),
        thunk: Js<'d>,
        catch: Option<Js<'d>>,
    ) -> Js<'d> {
        if let Some(catch) = catch {
            let f = self.rt(caught);
            call(self.a, f, &[thunk, catch])
        } else {
            let f = self.effect(plain);
            call(self.a, f, &[thunk])
        }
    }

    fn foreign_call(
        &mut self,
        f: &Foreign,
        target: &str,
        dicts: &[String],
        params: &[String],
        bridged: bool,
    ) -> Js<'d> {
        let a = self.a;
        let info = &f.adapter;
        let copies = copies(f);
        let mut args: Vec<Js<'d>> = dicts.iter().map(|d| self.leaf(d)).collect();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let copy = copies.is_some_and(|c| c.args.get(i) == Some(&true));
            let callback = copies.and_then(|c| c.callbacks.get(i).cloned().flatten());
            let value = self.leaf(param);
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.optional.contains(&i) => {
                    if bridged {
                        let value = self.copied(copy, value);
                        let host = self.crossed(
                            Toward::Host,
                            info.shapes.get(i).unwrap_or(&Shape::Data),
                            value,
                        );
                        let optional = self.rt("optionalArg");
                        call(a, optional, &[host])
                    } else {
                        let optional = self.rt("optionalArg");
                        let arg = call(a, optional, &[value]);
                        self.copied(copy, arg)
                    }
                }
                n if bridged => {
                    let value = self.copied(copy, value);
                    let host = self.crossed(
                        Toward::Host,
                        info.shapes.get(i).unwrap_or(&Shape::Data),
                        value,
                    );
                    match n {
                        Some(n) => {
                            let host = Js::code(
                                docvec![
                                    a,
                                    "(",
                                    host.at(a, Before::Delimiter, After::Delimiter),
                                    ")"
                                ],
                                Head::Paren,
                                &[host],
                            );
                            self.callback(host, n, callback.as_ref())
                        }
                        None if info.variadic && i + 1 == f.arity => self.spread(host),
                        None => host,
                    }
                }
                _ if info.receiver && i == 0 => self.copied(copy, value),
                Some(n) => self.callback(value, n, callback.as_ref()),
                None if info.variadic && i + 1 == f.arity => {
                    let value = self.copied(copy, value);
                    self.spread(value)
                }
                None => self.copied(copy, value),
            };
            args.push(arg);
        }
        if info.cancel {
            args.push(self.leaf("$signal"));
        }
        if info.receiver && !args.is_empty() {
            let receiver = args.remove(dicts.len());
            let rest: Vec<Js<'d>> = [receiver].into_iter().chain(args).collect();
            call(a, self.leaf(&format!("{target}.call")), &rest)
        } else {
            call(a, self.leaf(target), &args)
        }
    }

    fn spread(&self, value: Js<'d>) -> Js<'d> {
        Js::code(
            docvec![
                self.a,
                "...",
                value.at(self.a, Before::Other, After::Delimiter)
            ],
            Head::Other,
            &[value],
        )
    }

    pub(super) fn foreign_result(&mut self, f: &Foreign, copy: bool, v: Js<'d>) -> Js<'d> {
        let a = self.a;
        let info = &f.adapter;
        let v = self.copied(copy, v);
        let v = if info.result.nullable {
            let from = self.rt("fromNullable");
            call(a, from, &[v])
        } else {
            v
        };
        if info.result.unit {
            Js::code(
                docvec![a, "void ", v.at(a, Before::Other, After::Delimiter)],
                Head::Ident,
                &[v],
            )
            .in_parens()
        } else {
            v
        }
    }

    fn copied(&mut self, copy: bool, value: Js<'d>) -> Js<'d> {
        if copy {
            let copier = self.rt("copyData");
            call(self.a, copier, &[value])
        } else {
            value
        }
    }

    fn callback(&mut self, param: Js<'d>, n: usize, copies: Option<&(Vec<bool>, bool)>) -> Js<'d> {
        let (args, result) = copies.map_or((&[][..], false), |(a, r)| (a.as_slice(), *r));
        if n == 1 && !result && !args.contains(&true) {
            return param;
        }
        let xs: Vec<String> = (0..n).map(|k| format!("$x{k}")).collect();
        let mut applied = param;
        for (k, x) in xs.iter().enumerate() {
            let x = self.copied(args.get(k) == Some(&true), self.leaf(x));
            applied = call(self.a, applied, &[x]);
        }
        let applied = self.copied(result, applied);
        self.lambda_of(&xs.join(", "), applied)
    }
}

/// Whether the JS function itself can stand for `f` in release: a pure
/// import called with its arguments as they are and returning its result.
/// On Flick's runtime, only data crosses an import the bridge may convert.
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
pub(super) fn plain_foreigns(
    items: &[Item],
    module: &str,
    options: &Options<'_>,
) -> HashMap<String, (usize, String)> {
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Foreign(f) if plain(f) => {
                if options.rt {
                    let source = f.binding.from.as_deref()?;
                    let spec = options.foreign(module, source);
                    let data = f.adapter.shapes.iter().all(Shape::is_data)
                        && f.adapter.result_shape.is_data();
                    // the runtime's own exports take every value as it is
                    if !(data || belongs_to(&spec, bindings::OWNED_PREFIX))
                        || belongs_to(&spec, bindings::ADAPTER_PREFIX)
                        || (!options.effect_bridge.available()
                            && requires_adapter(&spec, &f.binding.name, false))
                        || !selected_on_kernel(&spec, &f.binding.name, options.effect_bridge)
                    {
                        return None;
                    }
                }
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
        assert!(trusted_runtime_package("flick:Effect"));
        assert!(!trusted_runtime_package("@flick-lang/runtime-extra"));
        assert!(!trusted_runtime_package("@flick-lang/effect-ts-extra"));
    }
}
