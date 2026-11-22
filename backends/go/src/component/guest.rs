//! The production component exports exactly the public WIT operations.

use super::Go;
use flick_backend::bindings::{Guest, export_component, flick::backend::inputs};
use flick_backend::command::Dependency;
use flick_backend::{
    Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, InputError, Inputs, OptionValue,
    Packages, ProjectFile, TargetSupport,
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
        Go.describe()
    }
    fn check_target(request: BuildRequest) -> Result<TargetSupport, Vec<Diagnostic>> {
        Go.check_target(&request, &mut Imports)
    }
    fn generate(request: BuildRequest) -> Result<BuildResult, Vec<Diagnostic>> {
        Go.generate(&request, &mut Imports)
    }
    fn package_files() -> Vec<String> {
        Go.package_files()
    }
    fn packages(
        files: Vec<ProjectFile>,
        dependencies: Vec<Dependency>,
        options: Vec<OptionValue>,
    ) -> Packages {
        Go.packages(&files, &dependencies, &options)
    }
}
export_component!(Component);
flick_backend::export_commands!(Component, Go);
