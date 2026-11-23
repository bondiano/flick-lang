//! Delegated backend `pack` execution with host enforcement of the
//! `.flick/pack` write boundary.

use std::env;

use flick_backend::command::{self, Argument, Invocation};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::Project;
use flick_core::source::SourceMap;

use super::delegate::Delegated;
use super::plan::{self, InPlaceMode};
use super::registry::Contributor;
use super::{inputs, invocation};
use crate::args::GlobalArguments;
use crate::backends::{self, Package};

/// The only directory under the project root that a backend's `pack` command
/// is allowed to declare in `descriptor.writes`.
const PACK_STAGE: &str = ".flick/pack";

/// Runs the already-loaded backend `package`'s `pack` plan command with
/// host-supplied context `arguments`, filtering them to the options declared by
/// `descriptor` and enforcing that every declared write stays under `.flick/pack`.
pub(crate) fn run_pack_command(
    project: &Project,
    package: &Package,
    descriptor: &command::Descriptor,
    arguments: Vec<Argument>,
    global: &GlobalArguments,
) -> Delegated {
    let id = package.pin().identity.id;
    run_pack_with(project, &id, package, descriptor, arguments, global)
}

fn validate_pack_writes(descriptor: &command::Descriptor, inv: &Invocation) -> Result<(), String> {
    let writes = inputs::expand(&descriptor.writes, inv)?;
    let allowed = [PACK_STAGE.to_owned()];
    if let Some(path) = writes.iter().find(|p| !inputs::under(p, &allowed)) {
        return Err(format!(
            "the `pack` command may only declare writes under `{PACK_STAGE}`, not `{path}`"
        ));
    }
    Ok(())
}

fn run_pack_with(
    project: &Project,
    id: &str,
    contributor: &dyn Contributor,
    descriptor: &command::Descriptor,
    arguments: Vec<Argument>,
    global: &GlobalArguments,
) -> Delegated {
    let failed = |code, message: String| Delegated {
        stdout: Vec::new(),
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(code, Severity::Error, message)],
        executed: false,
    };
    let declared = |name: &str| {
        (descriptor.positionals.iter())
            .chain(&descriptor.options)
            .any(|p| p.name == name)
    };
    let filtered: Vec<Argument> = arguments
        .into_iter()
        .filter(|a| declared(&a.name))
        .collect();
    let tools = backends::granted_tools(project, id);
    let cwd = env::current_dir().unwrap_or_else(|_| project.root.clone());
    let inv = match invocation(project, id, descriptor, filtered, &tools, &cwd) {
        Ok(inv) => inv,
        Err(why) => return failed(codes::P055, format!("`pack` cannot run: {why}")),
    };
    if let Err(why) = validate_pack_writes(descriptor, &inv) {
        return failed(codes::P055, format!("the plan of `pack` is refused: {why}"));
    }
    let (stdout, sources, diagnostics) = plan::outcome(
        project,
        contributor,
        descriptor,
        &inv,
        &tools,
        global,
        InPlaceMode::PackageManager,
    );
    Delegated {
        stdout,
        sources,
        diagnostics,
        executed: true,
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use flick_backend::command::{self, Argument, Invocation, Parameter, Style, ValueKind};
    use flick_backend::host::runtime::Cancellation;
    use flick_core::diagnostics::codes;
    use flick_core::project;
    use flick_core::source::SourceMap;

    use super::{Contributor, run_pack_with, validate_pack_writes};
    use crate::args::{Color, DiagnosticsFormat, GlobalArguments};
    use crate::sources::FsHost;

    struct PanicContributor;

    impl Contributor for PanicContributor {
        fn describe(&self) -> Result<Vec<command::Descriptor>, String> {
            unreachable!("describe should not be called")
        }

        fn run(&self, _invocation: &Invocation) -> Result<command::Plan, Vec<command::Diagnostic>> {
            unreachable!("run must not be called when descriptor.writes is refused")
        }

        fn resume(
            &self,
            _state: Vec<u8>,
            _outputs: Vec<command::File>,
            _answers: Vec<command::Answer>,
        ) -> Result<command::Plan, Vec<command::Diagnostic>> {
            unreachable!("resume should not be called")
        }

        fn open(
            &self,
            _invocation: &Invocation,
        ) -> Result<command::Opened, Vec<command::Diagnostic>> {
            unreachable!("open should not be called")
        }

        fn handle(
            &self,
            _state: Vec<u8>,
            _request: Vec<u8>,
            _answers: &[command::Answer],
            _cancel: &Cancellation,
        ) -> Result<command::Handled, String> {
            unreachable!("handle should not be called")
        }

        fn close(&self, _state: Vec<u8>) -> command::Plan {
            unreachable!("close should not be called")
        }

        fn templates(&self) -> Result<Vec<command::TemplateDescriptor>, String> {
            unreachable!("templates should not be called")
        }

        fn scaffold(
            &self,
            _request: &command::TemplateRequest,
        ) -> Result<command::Plan, Vec<command::Diagnostic>> {
            unreachable!("scaffold should not be called")
        }
    }

    fn pack_descriptor(writes: &[&str], options: Vec<Parameter>) -> command::Descriptor {
        command::Descriptor {
            name: "pack".into(),
            summary: "test pack".into(),
            style: Style::Plan,
            positionals: Vec::new(),
            options,
            inputs: Vec::new(),
            writes: writes.iter().map(|s| (*s).to_owned()).collect(),
            reads: Vec::new(),
        }
    }

    fn empty_invocation(arguments: Vec<Argument>) -> Invocation {
        Invocation {
            name: "pack".into(),
            arguments,
            sources: "src".into(),
            inputs: Vec::new(),
            tools: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    fn test_global() -> GlobalArguments {
        GlobalArguments {
            diagnostics: DiagnosticsFormat::Human,
            color: Color::Never,
            quiet: true,
            verbose: false,
            timings: false,
            project: None,
            package: None,
            warnings_as_errors: false,
            offline: false,
            allow_tool: Vec::new(),
        }
    }

    #[test]
    fn validate_pack_writes_enforces_stage_boundary() {
        let inv = empty_invocation(vec![Argument {
            name: "file".into(),
            values: vec!["dist/App.js".into()],
        }]);
        for allowed in [&[][..], &[".flick/pack"][..], &[".flick/pack/out"][..]] {
            let desc = pack_descriptor(allowed, Vec::new());
            assert!(validate_pack_writes(&desc, &inv).is_ok(), "{allowed:?}");
        }
        for forbidden in [
            &["src", "flick.toml"][..],
            &["flick.toml"][..],
            &[""][..],
            &["."][..],
            &[".flick/pack2"][..],
            &["../.flick/pack"][..],
            &["$file"][..],
            &["$sources"][..],
        ] {
            let desc = pack_descriptor(forbidden, Vec::new());
            assert!(
                validate_pack_writes(&desc, &inv).is_err(),
                "expected refusal for {forbidden:?}"
            );
        }
    }

    #[test]
    fn run_pack_refuses_writes_outside_pack_stage_before_running_contributor() {
        let root = env::temp_dir().join(format!("flick-pack-writes-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).expect("src");
        let manifest = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
        fs::write(root.join("flick.toml"), manifest).expect("manifest");
        let mut sources = SourceMap::default();
        let project = project::discover(&FsHost::new(), &mut sources, &root, Some(&root))
            .expect("discover")
            .expect("project");

        let desc = pack_descriptor(
            &["src", "flick.toml"],
            vec![Parameter {
                name: "stage".into(),
                help: "stage".into(),
                kind: ValueKind::Text,
                required: false,
                multiple: false,
            }],
        );
        let ran = run_pack_with(
            &project,
            "third/party",
            &PanicContributor,
            &desc,
            vec![Argument {
                name: "stage".into(),
                values: vec![".flick/pack".into()],
            }],
            &test_global(),
        );
        assert!(!ran.executed, "refused before running contributor");
        assert!(
            ran.diagnostics
                .iter()
                .any(|d| d.code == codes::P055 && d.message.contains(".flick/pack")),
            "{:?}",
            ran.diagnostics
        );
        assert_eq!(
            fs::read_to_string(root.join("flick.toml")).expect("manifest intact"),
            manifest
        );

        fs::remove_dir_all(&root).expect("cleanup");
    }
}
