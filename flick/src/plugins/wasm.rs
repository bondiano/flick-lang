//! Plugins built as WebAssembly components, run by wasmtime. Every call
//! runs in a fresh instance with a budget of fuel and memory: calls stay
//! pure, so their results can be cached, and a plugin can neither hang the
//! compiler nor take its memory. Imports other than the compiler's trap.
//! A component is compiled once per process, on its first call: a build
//! whose blocks all come from the cache never compiles it.

use std::fs;
use std::mem;
use std::path::Path;
use std::str;
use std::sync::{Arc, OnceLock};

use flick_core::plugin::source::BlockSource;
use flick_core::plugin::{Expander, Failure};
use flick_plugin::Compiler;
use sha2::{Digest, Sha256};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, StoreLimits, StoreLimitsBuilder, Trap};

use crate::fetch;

wasmtime::component::bindgen!({
    world: "plugin",
    path: "../flick-plugin/wit",
});

flick_plugin::wit_conversions!(flick::plugin::types);
flick_plugin::reach_conversions!(flick::plugin::types);

/// What one call may compute and the memory it may take.
const FUEL: u64 = 1_000_000_000;
const MEMORY: usize = 64 << 20;

/// The plugin of `package` in the component at `path`.
pub fn load(package: &str, path: &Path) -> Result<Arc<dyn Expander>, String> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read `{}`: {e}", path.display()))?;
    match interface_version(&bytes) {
        Some(version) if version == flick_plugin::VERSION => {}
        Some(version) => {
            return Err(format!(
                "`{}` was built for `flick:plugin@{version}`, and this flick speaks `{}`: \
                 rebuild it, as `flick run plugin` in its package does",
                path.display(),
                flick_plugin::VERSION
            ));
        }
        None => {
            return Err(format!(
                "`{}` is no component of `flick:plugin`",
                path.display()
            ));
        }
    }
    let id = format!("{package} {}", fetch::hex(&Sha256::digest(&bytes)));
    Ok(Arc::new(Wasm {
        id,
        bytes,
        compiled: OnceLock::new(),
    }))
}

/// The version of `flick:plugin` a component imports the compiler of, as
/// its import `flick:plugin/compiler@0.5.0` names it, read without compiling
/// it: a build from caches calls no plugin.
fn interface_version(bytes: &[u8]) -> Option<&str> {
    const NAME: &[u8] = b"flick:plugin/compiler@";
    let at = bytes.windows(NAME.len()).position(|w| w == NAME)? + NAME.len();
    let len = (bytes[at..].iter())
        .take_while(|b| b.is_ascii_digit() || **b == b'.')
        .count();
    str::from_utf8(&bytes[at..at + len])
        .ok()
        .filter(|v| !v.is_empty())
}

struct Wasm {
    id: String,
    bytes: Vec<u8>,
    compiled: OnceLock<Result<(Component, Linker<State>), String>>,
}

/// What a call of a plugin reaches: the block whose holes it asks for.
struct State {
    source: BlockSource,
    limits: StoreLimits,
}

impl flick::plugin::types::Host for State {}

impl flick::plugin::compiler::Host for State {
    fn parse_expr(
        &mut self,
        start: u32,
        end: u32,
    ) -> Result<flick::plugin::types::Hole, flick::plugin::types::Diagnostic> {
        (self.source.parse_expr(start, end))
            .map(convert::to_hole)
            .map_err(convert::to_diagnostic)
    }

    fn parse_type(
        &mut self,
        start: u32,
        end: u32,
    ) -> Result<flick::plugin::types::Hole, flick::plugin::types::Diagnostic> {
        (self.source.parse_type(start, end))
            .map(convert::to_hole)
            .map_err(convert::to_diagnostic)
    }

    fn parse_pat(
        &mut self,
        start: u32,
        end: u32,
    ) -> Result<flick::plugin::types::Hole, flick::plugin::types::Diagnostic> {
        (self.source.parse_pat(start, end))
            .map(convert::to_hole)
            .map_err(convert::to_diagnostic)
    }

    fn parse_decl(
        &mut self,
        start: u32,
        end: u32,
    ) -> Result<flick::plugin::types::Hole, flick::plugin::types::Diagnostic> {
        (self.source.parse_decl(start, end))
            .map(convert::to_hole)
            .map_err(convert::to_diagnostic)
    }

    fn parse_body(
        &mut self,
        start: u32,
        end: u32,
    ) -> Result<flick::plugin::types::Hole, flick::plugin::types::Diagnostic> {
        (self.source.parse_body(start, end))
            .map(convert::to_hole)
            .map_err(convert::to_diagnostic)
    }

    fn read_file(&mut self, path: String) -> Result<Vec<u8>, flick::plugin::types::ReachError> {
        (self.source.read_file(&path)).map_err(reach_convert::to_error)
    }

    fn env(&mut self, name: String) -> Result<Option<String>, flick::plugin::types::ReachError> {
        (self.source.env(&name)).map_err(reach_convert::to_error)
    }

    fn literal_of(&mut self, name: String) -> Option<String> {
        self.source.literal_of(&name)
    }

    fn published(&mut self, name: String) -> Option<Vec<u8>> {
        self.source.published(&name)
    }

    fn published_names(&mut self) -> Vec<String> {
        self.source.published_names()
    }

    fn fetch(
        &mut self,
        request: flick::plugin::types::Request,
    ) -> Result<flick::plugin::types::Response, flick::plugin::types::ReachError> {
        (self.source.fetch(&reach_convert::from_request(request)))
            .map(reach_convert::to_response)
            .map_err(reach_convert::to_error)
    }
}

fn engine() -> Result<&'static Engine, String> {
    static ENGINE: OnceLock<Result<Engine, String>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let mut config = Config::new();
            config.consume_fuel(true);
            // the same results on every machine, which caches rely on
            config.cranelift_nan_canonicalization(true);
            Engine::new(&config).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

impl Wasm {
    fn compiled(&self) -> Result<&(Component, Linker<State>), String> {
        self.compiled
            .get_or_init(|| {
                let engine = engine()?;
                let component = Component::new(engine, &self.bytes)
                    .map_err(|e| format!("is not a component: {e}"))?;
                let mut linker = Linker::new(engine);
                // every import traps, then the compiler's own replace theirs
                linker.allow_shadowing(true);
                linker
                    .define_unknown_imports_as_traps(&component)
                    .and_then(|()| {
                        Plugin::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
                    })
                    .map_err(|e| format!("cannot be linked: {e}"))?;
                Ok((component, linker))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Runs `call` on a fresh instance with `source`, which comes back.
    fn run<R>(
        &self,
        source: BlockSource,
        call: impl FnOnce(&Plugin, &mut Store<State>) -> wasmtime::Result<R>,
    ) -> (Result<R, String>, BlockSource) {
        let state = State {
            source,
            limits: StoreLimitsBuilder::new()
                .memory_size(MEMORY)
                .trap_on_grow_failure(true)
                .build(),
        };
        let Ok(engine) = engine() else {
            return (Err("no engine".into()), state.source);
        };
        let mut store = Store::new(engine, state);
        store.limiter(|s| &mut s.limits);
        let result = self.compiled().and_then(|(component, linker)| {
            let plugin = store
                .set_fuel(FUEL)
                .and_then(|()| Plugin::instantiate(&mut store, component, linker))
                .map_err(|e| {
                    format!(
                        "cannot be instantiated, maybe built for another version of \
                         `flick:plugin` than {}: {e}",
                        flick_plugin::VERSION
                    )
                })?;
            call(&plugin, &mut store).map_err(|e| why(&e))
        });
        (result, store.into_data().source)
    }
}

/// Why a call failed, in words.
fn why(error: &wasmtime::Error) -> String {
    match error.downcast_ref::<Trap>() {
        Some(Trap::OutOfFuel) => "ran out of its fuel".into(),
        Some(Trap::AllocationTooLarge) => "ran out of its memory".into(),
        _ if format!("{error:?}").contains("forcing trap when growing memory") => {
            "ran out of its memory".into()
        }
        _ => format!("trapped: {error}"),
    }
}

impl Expander for Wasm {
    fn id(&self) -> &str {
        &self.id
    }

    fn collect(
        &self,
        blocks: &[flick_plugin::Block],
        source: &mut BlockSource,
    ) -> Result<Vec<u8>, String> {
        let blocks: Vec<_> = blocks.iter().map(convert::to_block).collect();
        let (result, back) = self.run(mem::take(source), |plugin, store| {
            plugin.call_collect(store, &blocks)
        });
        *source = back;
        result
    }

    fn publish(&self, state: &[u8]) -> Result<Vec<flick_plugin::Published>, String> {
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.call_publish(store, state)
        });
        Ok((result?.into_iter())
            .map(|p| flick_plugin::Published {
                module: p.module,
                name: p.name,
                data: p.data,
            })
            .collect())
    }

    fn expand(
        &self,
        block: &flick_plugin::Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<flick_plugin::Arena, Failure> {
        let block = convert::to_block(block);
        let (result, back) = self.run(mem::take(source), |plugin, store| {
            plugin.call_expand(store, &block, state)
        });
        *source = back;
        match result {
            Ok(Ok(arena)) => Ok(convert::from_arena(arena)),
            Ok(Err(found)) => Err(Failure::Refused(
                found.into_iter().map(convert::from_diagnostic).collect(),
            )),
            Err(why) => Err(Failure::Broken(why)),
        }
    }

    fn complete(
        &self,
        block: &flick_plugin::Block,
        state: &[u8],
        offset: u32,
        member: bool,
        source: &mut BlockSource,
    ) -> Vec<flick_plugin::Completion> {
        let block = convert::to_block(block);
        let (result, back) = self.run(mem::take(source), |plugin, store| {
            plugin.call_complete(store, &block, state, offset, member)
        });
        *source = back;
        result.map_or_else(
            |_| Vec::new(),
            |found| found.into_iter().map(convert::from_completion).collect(),
        )
    }

    fn hover(
        &self,
        block: &flick_plugin::Block,
        state: &[u8],
        offset: u32,
        facts: &[flick_plugin::Fact],
    ) -> Option<flick_plugin::Hover> {
        let block = convert::to_block(block);
        let facts: Vec<_> = facts.iter().map(convert::to_fact).collect();
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.call_hover(store, &block, state, offset, &facts)
        });
        result.ok().flatten().map(convert::from_hover)
    }

    fn highlight(&self, block: &flick_plugin::Block, state: &[u8]) -> Vec<flick_plugin::Highlight> {
        let block = convert::to_block(block);
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.call_highlight(store, &block, state)
        });
        result.map_or_else(
            |_| Vec::new(),
            |found| found.into_iter().map(convert::from_highlight).collect(),
        )
    }

    fn derive(
        &self,
        target: &flick_plugin::DeriveTarget,
        class: &str,
        args: Option<&flick_plugin::Block>,
        module: &[flick_plugin::DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<flick_plugin::Arena, Failure> {
        let target = convert::to_target(target);
        let args = args.map(convert::to_block);
        let module: Vec<_> = module.iter().map(convert::to_target).collect();
        let (result, back) = self.run(mem::take(source), |plugin, store| {
            plugin.call_derive(store, &target, class, args.as_ref(), &module, state)
        });
        *source = back;
        match result {
            Ok(Ok(arena)) => Ok(convert::from_arena(arena)),
            Ok(Err(found)) => Err(Failure::Refused(
                found.into_iter().map(convert::from_diagnostic).collect(),
            )),
            Err(why) => Err(Failure::Broken(why)),
        }
    }

    fn format(
        &self,
        block: &flick_plugin::Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Option<Vec<flick_plugin::Piece>> {
        let block = convert::to_block(block);
        let (result, back) = self.run(mem::take(source), |plugin, store| {
            plugin.call_format(store, &block, state)
        });
        *source = back;
        Some(result.ok()??.into_iter().map(convert::from_piece).collect())
    }

    fn check(
        &self,
        block: &flick_plugin::Block,
        state: &[u8],
        facts: &[flick_plugin::Fact],
    ) -> Result<Vec<flick_plugin::Diagnostic>, String> {
        let block = convert::to_block(block);
        let facts: Vec<_> = facts.iter().map(convert::to_fact).collect();
        let (result, _) = self.run(BlockSource::default(), |plugin, store| {
            plugin.call_check(store, &block, state, &facts)
        });
        Ok(result?.into_iter().map(convert::from_diagnostic).collect())
    }
}
