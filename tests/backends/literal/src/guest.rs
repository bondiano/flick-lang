//! Component guest export wiring for the `test/literal` fixture backend.

use flick_backend::bindings::flick::backend::inputs;
use flick_backend::bindings::{Guest, export_component};
use flick_backend::command::Dependency;
use flick_backend::{
    Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, InputError, Inputs, OptionValue,
    Packages, ProjectFile, TargetSupport,
};

use super::Literal;

struct Imports;
impl Inputs for Imports {
    fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
        inputs::read(id)
    }
}

struct Component;
impl Guest for Component {
    fn describe() -> Descriptor {
        Literal.describe()
    }
    fn check_target(request: BuildRequest) -> Result<TargetSupport, Vec<Diagnostic>> {
        Literal.check_target(&request, &mut Imports)
    }
    fn generate(request: BuildRequest) -> Result<BuildResult, Vec<Diagnostic>> {
        Literal.generate(&request, &mut Imports)
    }
    fn package_files() -> Vec<String> {
        Literal.package_files()
    }
    fn packages(
        files: Vec<ProjectFile>,
        dependencies: Vec<Dependency>,
        options: Vec<OptionValue>,
    ) -> Packages {
        Literal.packages(&files, &dependencies, &options)
    }
}

export_component!(Component);
flick_backend::export_commands!(Component, Literal);
