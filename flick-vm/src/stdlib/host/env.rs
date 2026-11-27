//! Host environment variables and program CLI arguments (`Host.Environment`,
//! `Host.Arguments`).

#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
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
        HostMode::LiveOs => {
            if let Some(found) = hooks.host_env_var(&key) {
                let text = heap.literal(&Lit::String(found))?;
                return heap.construct("Prelude.Option.Some", vec![text]);
            }
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            {
                match env::var(&key) {
                    Ok(found) => {
                        let text = heap.literal(&Lit::String(found))?;
                        heap.construct("Prelude.Option.Some", vec![text])
                    }
                    Err(_) => heap.construct("Prelude.Option.None", vec![]),
                }
            }
            #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
            {
                heap.construct("Prelude.Option.None", vec![])
            }
        }
    }
}

pub(super) fn list_vars(heap: &mut Heap, hooks: &mut dyn VmHooks) -> Result<Value, Error> {
    match hooks.host_mode() {
        HostMode::Deterministic => heap.construct("$list", vec![]),
        HostMode::LiveOs => {
            #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
            let entries: Vec<(String, String)> = hooks
                .host_env_vars()
                .unwrap_or_else(|| env::vars().collect());
            #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
            let entries: Vec<(String, String)> = hooks.host_env_vars().unwrap_or_default();
            let mut pairs = Vec::with_capacity(entries.len());
            for (k, v) in entries {
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
