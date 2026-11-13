//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments. A
//! plain pure binding's worker is the import itself.

use std::collections::HashMap;

use flick_pretty::docvec;

use super::doc::{After, Before, Head, Js, Stmt, call};
use super::{Gen, names};
use crate::ir::{Foreign, ForeignMode, Item, ItemKind};

impl<'d> Gen<'_, 'd> {
    pub(super) fn foreign(&mut self, f: &Foreign) -> Vec<Stmt<'d>> {
        let a = self.a;
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
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            return vec![self.export(&names::ident(&f.name), self.leaf(&target))];
        }
        let id = names::ident(&f.name);
        let plain = self.direct_foreigns.contains_key(&f.name);
        // a constant, not `export { … as … }`: the module names it too
        if plain && params.len() == 1 {
            return vec![self.export(&id, self.leaf(&target))];
        }
        let applied = self.foreign_call(f, &target, &dicts, &params);
        let result = |g: &mut Self, v: Js<'d>| {
            let v = if info.result.nullable {
                let from_nullable = g.rt("fromNullable");
                call(a, from_nullable, &[v])
            } else {
                v
            };
            if info.result.unit {
                let doc = docvec![a, "void ", v.at(a, Before::Other, After::Delimiter)];
                Js::code(doc, Head::Ident, &[v]).in_parens()
            } else {
                v
            }
        };
        let catch = f.catch.as_ref().map(|c| self.expr(c));
        let resolved = |g: &mut Self| {
            let value = result(g, g.leaf("$v"));
            g.arrow_to(a.text("($v) =>"), value)
        };
        let mut body = match (f.mode, runtime) {
            (ForeignMode::Pure, _) | (_, false) => result(self, applied),
            (ForeignMode::Sync, true) => {
                let value = result(self, applied);
                let thunk = self.thunk(value);
                self.guarded(("foreignSync", "sync"), thunk, catch)
            }
            (ForeignMode::Async, true) => {
                let then = resolved(self);
                let promise = call(a, self.leaf("Promise.resolve"), &[applied]);
                let started = call(a, self.member(promise, "then"), &[then]);
                let start = self.lambda_of("$signal", started);
                let foreign_async = self.rt("foreignAsync");
                let args: Vec<Js<'d>> = [start].into_iter().chain(catch).collect();
                call(a, foreign_async, &args)
            }
            (ForeignMode::Effect, true) => {
                let thunk = self.thunk(applied);
                let effect = self.guarded(("foreignEffect", "suspend"), thunk, catch);
                if info.result.nullable || info.result.unit {
                    let then = resolved(self);
                    let map = self.effect("map");
                    call(a, map, &[effect, then])
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

    /// The call of the foreign `target` with the dictionaries and parameters
    /// of its wrapper, adapted to what JS expects.
    fn foreign_call(
        &mut self,
        f: &Foreign,
        target: &str,
        dicts: &[String],
        params: &[String],
    ) -> Js<'d> {
        let a = self.a;
        let info = &f.info;
        let mut args: Vec<Js<'d>> = dicts.iter().map(|d| self.leaf(d)).collect();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.this && i == 0 => self.leaf(param),
                Some(1) => self.leaf(param),
                Some(n) => {
                    let xs: Vec<String> = (0..n).map(|k| format!("$x{k}")).collect();
                    let applied = xs
                        .iter()
                        .fold(self.leaf(param), |f, x| call(a, f, &[self.leaf(x)]));
                    self.lambda_of(&xs.join(", "), applied)
                }
                None if info.optional.contains(&i) => {
                    let optional = self.rt("optionalArg");
                    call(a, optional, &[self.leaf(param)])
                }
                None if info.variadic && i + 1 == f.arity => {
                    let spread = a.text(a.alloc_str(&format!("...{param}")));
                    Js::code(spread, Head::Other, &[])
                }
                None => self.leaf(param),
            };
            args.push(arg);
        }
        if info.cancel {
            args.push(self.leaf("$signal"));
        }
        if info.this && !args.is_empty() {
            let receiver = args.remove(dicts.len());
            let rest: Vec<Js<'d>> = [receiver].into_iter().chain(args).collect();
            call(a, self.leaf(&format!("{target}.call")), &rest)
        } else {
            call(a, self.leaf(target), &args)
        }
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
