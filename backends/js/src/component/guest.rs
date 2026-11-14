//! The production component exports exactly the public WIT operations.

use super::Javascript;
use flick_backend::bindings::{Guest, export_component, flick::backend::inputs};
use flick_backend::{
    Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, InputError, Inputs, TargetSupport,
};

struct Imports;
impl Inputs for Imports {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        inputs::read(id)
    }
}
struct Component;
impl Guest for Component {
    fn describe() -> Descriptor {
        Javascript.describe()
    }
    fn check_target(request: BuildRequest) -> Result<TargetSupport, Vec<Diagnostic>> {
        Javascript.check_target(&request, &mut Imports)
    }
    fn generate(request: BuildRequest) -> Result<BuildResult, Vec<Diagnostic>> {
        Javascript.generate(&request, &mut Imports)
    }
}
export_component!(Component);
