//! A `foreign` binding: a wrapper per stage around the JS import, and in
//! release the worker `name$n` that importers call with all arguments.

use super::{Gen, names};
use crate::ir::{Foreign, ForeignMode};

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
        if f.arity == 0 && params.is_empty() && f.dicts == 0 && f.mode == ForeignMode::Pure {
            return self.export(&names::ident(&f.name), &target);
        }
        let id = names::ident(&f.name);
        let call = self.foreign_call(f, &target, &dicts, &params);
        let result = |g: &mut Self, v: &str| {
            let v = if info.result.nullable {
                format!("{}({v})", g.rt("fromNullable"))
            } else {
                v.to_owned()
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
                if info.result.nullable || info.result.unit {
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
            let worker = format!("{} {body}", self.arrow(&params.join(", ")));
            let curried = self.curried(&worker_id, n);
            return format!(
                "{}\n{}",
                self.export(&worker_id, &worker),
                self.export(&id, &curried)
            );
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
        let mut args: Vec<String> = dicts.to_vec();
        for (i, param) in params.iter().take(f.arity).enumerate() {
            let arg = match info.callbacks.get(i).copied().flatten() {
                _ if info.this && i == 0 => param.clone(),
                Some(1) => param.clone(),
                Some(n) => {
                    let xs: Vec<String> = (0..n).map(|k| format!("$x{k}")).collect();
                    let applied = xs.iter().fold(String::new(), |acc, x| acc + "(" + x + ")");
                    format!("{} {param}{applied}", self.arrow(&xs.join(", ")))
                }
                None if info.optional.contains(&i) => {
                    format!("{}({param})", self.rt("optionalArg"))
                }
                None if info.variadic && i + 1 == f.arity => format!("...{param}"),
                None => param.clone(),
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
