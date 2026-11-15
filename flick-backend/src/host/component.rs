use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Error, Store};

use super::runtime::{Cancellation, MemoryLimits, memory_limits, prepare, why};
use crate::request::validate_size;
use crate::{
    Backend, BackendPin, BuildRequest, BuildResult, DeclaredInputs, Descriptor, Diagnostic, Inputs,
    Limits, Severity, TargetSupport, sha256,
};

#[allow(clippy::all, clippy::pedantic)]
mod bindings {
    wasmtime::component::bindgen!({ world: "backend", path: "wit", imports: { default: trappable } });
}

crate::wit_conversions!(bindings::flick::backend::types);

/// Limits are supplied by the host/project, never by the component.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub fuel: u64,
    pub memory: usize,
    pub output: usize,
    pub inputs: usize,
    pub input_calls: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            fuel: 1_000_000_000,
            memory: 64 << 20,
            output: 128 << 20,
            inputs: 256 << 20,
            input_calls: 100_000,
        }
    }
}

struct State {
    inputs: DeclaredInputs,
    sizes: BTreeMap<String, u64>,
    remaining: u64,
    calls: usize,
    violation: Option<String>,
    limits: MemoryLimits,
}

impl State {
    fn new(
        request: Option<&BuildRequest>,
        inputs: &mut dyn Inputs,
        budget: Budget,
        cancellation: &Cancellation,
    ) -> Result<Self, String> {
        cancellation.check()?;
        let manifest = request.map_or(&[][..], |r| r.inputs.as_slice());
        let limit = u64::try_from(budget.inputs).unwrap_or(u64::MAX);
        if manifest
            .iter()
            .try_fold(0_u64, |n, input| n.checked_add(input.size))
            .is_none_or(|n| n > limit)
            || request.is_some_and(|r| r.program.len() > budget.inputs)
        {
            return Err("component input exceeds its host limit".into());
        }
        if let Some(request) = request {
            let sized = validate_size(
                request,
                Limits {
                    request_bytes: budget.inputs,
                    ..Limits::default()
                },
            );
            cancellation.check()?;
            sized?;
        }
        let contents = DeclaredInputs::new(manifest, |id| {
            if cancellation.is_cancelled() {
                return None;
            }
            let bytes = inputs.read(id).ok();
            if cancellation.is_cancelled() {
                None
            } else {
                bytes
            }
        });
        cancellation.check()?;
        let contents = contents?;
        let sizes = manifest.iter().map(|i| (i.id.clone(), i.size)).collect();
        cancellation.check()?;
        Ok(Self {
            inputs: contents,
            sizes,
            remaining: limit,
            calls: budget.input_calls,
            violation: None,
            limits: memory_limits(budget.memory),
        })
    }
}

impl bindings::flick::backend::types::Host for State {}

impl bindings::flick::backend::inputs::Host for State {
    fn read(
        &mut self,
        id: String,
    ) -> wasmtime::Result<Result<Vec<u8>, bindings::flick::backend::types::InputError>> {
        let Some(calls) = self.calls.checked_sub(1) else {
            self.violation = Some("exceeded its side-input call limit".into());
            return Err(Error::msg("side-input call limit"));
        };
        self.calls = calls;
        let Some(&size) = self.sizes.get(&id) else {
            self.violation = Some(format!("read an undeclared side input `{id}`"));
            return Err(Error::msg("undeclared side input"));
        };
        let Some(remaining) = self.remaining.checked_sub(size) else {
            self.violation = Some("exceeded its side-input transfer limit".into());
            return Err(Error::msg("side-input transfer limit"));
        };
        self.remaining = remaining;
        Ok(self.inputs.read(&id).map_err(convert::input_error))
    }
}

struct Image {
    sha256: String,
    component: Component,
    linker: Linker<State>,
}

impl Image {
    fn load(engine: &Engine, digest: &str, bytes: &[u8]) -> Result<Arc<Self>, String> {
        static KEPT: Mutex<Vec<Arc<Image>>> = Mutex::new(Vec::new());
        let find = |images: &mut Vec<Arc<Image>>| {
            images
                .iter()
                .position(|image| {
                    image.sha256 == digest && Engine::same(image.component.engine(), engine)
                })
                .map(|at| images.remove(at))
        };
        {
            let mut images = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(image) = find(&mut images) {
                images.insert(0, image.clone());
                return Ok(image);
            }
        }
        let component =
            Component::new(engine, bytes).map_err(|e| format!("not a backend component: {e}"))?;
        let inputs_abi = format!("flick:backend/inputs@{}", crate::VERSION);
        let types_abi = format!("flick:backend/types@{}", crate::VERSION);
        for (name, _) in component.component_type().imports(engine) {
            if name.starts_with("flick:backend/") && name != inputs_abi && name != types_abi {
                return Err(format!("incompatible backend import `{name}`"));
            }
        }
        let mut linker = Linker::new(engine);
        linker.allow_shadowing(true);
        linker
            .define_unknown_imports_as_traps(&component)
            .map_err(|e| e.to_string())?;
        bindings::Backend::add_to_linker::<State, HasSelf<State>>(&mut linker, |s| s)
            .map_err(|e| e.to_string())?;
        let image = Arc::new(Self {
            sha256: digest.into(),
            component,
            linker,
        });
        let mut images = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
        let image = find(&mut images).unwrap_or(image);
        images.insert(0, image.clone());
        images.truncate(8);
        Ok(image)
    }

    fn call<T>(
        &self,
        state: State,
        budget: Budget,
        cancellation: &Cancellation,
        call: impl FnOnce(&bindings::Backend, &mut Store<State>) -> wasmtime::Result<T>,
    ) -> Result<T, String> {
        let mut store = Store::new(self.component.engine(), state);
        store.limiter(|state| &mut state.limits);
        prepare(&mut store, budget.fuel, budget.output).map_err(|e| why(&e))?;
        let _active = cancellation.enter(&mut store)?;
        let instance = bindings::Backend::instantiate(&mut store, &self.component, &self.linker)
            .map_err(|e| why(&e))?;
        let result = call(&instance, &mut store)
            .map_err(|e| store.data().violation.clone().unwrap_or_else(|| why(&e)))?;
        if let Some(violation) = store.data().violation.as_ref() {
            return Err(violation.clone());
        }
        Ok(result)
    }
}

/// A pinned component, compiled by content hash. Calls always instantiate
/// a fresh store and instance; only immutable compiled code is retained.
pub struct WasmBackend {
    pin: BackendPin,
    descriptor: Descriptor,
    image: Arc<Image>,
    budget: Budget,
    cancellation: Cancellation,
}

impl WasmBackend {
    pub fn load(
        engine: &Engine,
        pin: BackendPin,
        bytes: &[u8],
        budget: Budget,
    ) -> Result<Self, String> {
        Self::load_with_cancellation(engine, pin, bytes, budget, Cancellation::new(engine))
    }

    pub fn load_with_cancellation(
        engine: &Engine,
        pin: BackendPin,
        bytes: &[u8],
        budget: Budget,
        cancellation: Cancellation,
    ) -> Result<Self, String> {
        if cancellation.is_cancelled() {
            return Err("component call cancelled".into());
        }
        if sha256(bytes) != pin.sha256 {
            return Err("backend component digest does not match its project pin".into());
        }
        let image = Image::load(engine, &pin.sha256, bytes)?;
        let mut no_inputs = DeclaredInputs::new(&[], |_| None)?;
        let state = State::new(None, &mut no_inputs, budget, &cancellation)?;
        let descriptor =
            convert::descriptor(image.call(state, budget, &cancellation, |i, store| {
                i.call_describe(store)
            })?);
        if descriptor.identity != pin.identity {
            return Err("backend descriptor does not match its pinned id and version".into());
        }
        Ok(Self {
            pin,
            descriptor,
            image,
            budget,
            cancellation,
        })
    }

    pub fn cancellation(&self) -> Cancellation {
        self.cancellation.clone()
    }

    fn failure(&self, phase: &str, error: &str) -> Vec<Diagnostic> {
        vec![Diagnostic {
            code: "P013".into(),
            severity: Severity::Error,
            message: format!(
                "backend {}@{}#{} {phase}: {error}",
                self.pin.identity.id, self.pin.identity.version, self.pin.sha256
            ),
            span: None,
            notes: vec![],
        }]
    }
}

impl Backend for WasmBackend {
    fn loaded_pin(&self) -> Option<&BackendPin> {
        Some(&self.pin)
    }

    fn check_cancelled(&self) -> Result<(), Vec<Diagnostic>> {
        self.cancellation
            .check()
            .map_err(|error| self.failure("preflight", &error))
    }

    fn describe(&self) -> Descriptor {
        self.descriptor.clone()
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        if request.backend != self.pin {
            return Err(self.failure(
                "preflight",
                "request backend pin differs from the loaded component",
            ));
        }
        let state = State::new(Some(request), inputs, self.budget, &self.cancellation)
            .map_err(|e| self.failure("preflight", &e))?;
        self.cancellation
            .check()
            .map_err(|e| self.failure("preflight", &e))?;
        let request = convert::request(request);
        match self
            .image
            .call(state, self.budget, &self.cancellation, |i, store| {
                i.call_check_target(store, &request)
            }) {
            Ok(Ok(support)) => Ok(convert::support(support)),
            Ok(Err(diagnostics)) => Err(convert::diagnostics(diagnostics)),
            Err(error) => Err(self.failure("preflight", &error)),
        }
    }

    fn generate(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        if request.backend != self.pin {
            return Err(self.failure(
                "generation",
                "request backend pin differs from the loaded component",
            ));
        }
        let state = State::new(Some(request), inputs, self.budget, &self.cancellation)
            .map_err(|e| self.failure("generation", &e))?;
        self.cancellation
            .check()
            .map_err(|e| self.failure("generation", &e))?;
        let request = convert::request(request);
        match self
            .image
            .call(state, self.budget, &self.cancellation, |i, store| {
                i.call_generate(store, &request)
            }) {
            Ok(Ok(result)) => Ok(convert::result(result)),
            Ok(Err(diagnostics)) => Err(convert::diagnostics(diagnostics)),
            Err(error) => Err(self.failure("generation", &error)),
        }
    }
}
