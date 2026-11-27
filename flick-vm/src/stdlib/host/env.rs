//! Host environment variables and program CLI arguments (`Host.Environment`,
//! `Host.Arguments`).

use std::env;

use flick_core::ir::Lit;

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

use super::HostMode;

pub(super) fn get_env(
    heap: &mut Heap,
    key_val: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let key = heap.string(key_val)?.to_owned();
    match hooks.host_mode() {
        HostMode::Deterministic => heap.construct("Prelude.Option.None", vec![]),
        HostMode::LiveOs => match env::var(&key) {
            Ok(found) => {
                let text = heap.literal(&Lit::String(found))?;
                heap.construct("Prelude.Option.Some", vec![text])
            }
            Err(_) => heap.construct("Prelude.Option.None", vec![]),
        },
    }
}

pub(super) fn list_vars(heap: &mut Heap, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    match hooks.host_mode() {
        HostMode::Deterministic => heap.construct("$list", vec![]),
        HostMode::LiveOs => {
            let mut pairs = Vec::new();
            for (k, v) in env::vars() {
                let key = match heap.literal(&Lit::String(k)) {
                    Ok(val) => val,
                    Err(err) => {
                        heap.drop_values(pairs)?;
                        return Err(err);
                    }
                };
                let val = match heap.literal(&Lit::String(v)) {
                    Ok(val) => val,
                    Err(err) => {
                        heap.drop_value(key)?;
                        heap.drop_values(pairs)?;
                        return Err(err);
                    }
                };
                match heap.construct("$tuple", vec![key, val]) {
                    Ok(pair) => pairs.push(pair),
                    Err(err) => {
                        heap.drop_values(pairs)?;
                        return Err(err);
                    }
                }
            }
            heap.construct("$list", pairs)
        }
    }
}

pub(super) fn list_args(heap: &mut Heap, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    match hooks.host_mode() {
        HostMode::Deterministic => heap.construct("$list", vec![]),
        HostMode::LiveOs => {
            let args: Vec<String> = hooks.program_args().to_vec();
            let mut values = Vec::with_capacity(args.len());
            for arg in args {
                match heap.literal(&Lit::String(arg)) {
                    Ok(val) => values.push(val),
                    Err(err) => {
                        heap.drop_values(values)?;
                        return Err(err);
                    }
                }
            }
            heap.construct("$list", values)
        }
    }
}
