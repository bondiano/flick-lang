/// The module `m` of the build, lowered.
fn compiled(m: &Loaded, (ir, declarations, _): Lowered, interface: Interface) -> Compiled {
    Compiled {
        name: m.name.clone(),
        path: m.path.clone(),
        file: m.file,
        test: m.root == Root::Test || companion_of(&m.name).is_some(),
        package: match &m.root {
            Root::Package(name) => Some(name.clone()),
            _ => None,
        },
        imports: m
            .syntax
            .imports
            .iter()
            .chain(&m.syntax.inline_imports)
            .filter(|i| !i.module.starts_with("$scope:"))
            .map(|i| i.module.clone())
            .chain(
                m.virtual_of
                    .iter()
                    .flat_map(|v| v.capture.scopes.iter().map(|s| s.module.clone())),
            )
            .collect(),
        ir,
        declarations,
        interface,
        expansion: m.expansion.clone(),
    }
}

/// The modules of a build: the project's and those of its libraries.
#[derive(Default)]
pub struct Compilation {
    pub modules: Vec<Compiled>,
    /// The libraries of `[dependencies]`, each with its source directory.
    pub libraries: Vec<(String, PathBuf)>,
    /// The npm packages its dependency closure asks for, which a js build
    /// needs installed.
    #[cfg(feature = "native-project")]
    pub js_dependencies: Vec<super::js::Wanted>,
}

/// A module that checked cleanly, lowered to the IR.
pub struct Compiled {
    pub name: String,
    pub path: PathBuf,
    pub file: FileId,
    /// Whether it is in the test root.
    pub test: bool,
    /// The source package it belongs to; `None` for the project's own.
    pub package: Option<String>,
    /// The modules it imports.
    pub imports: Vec<String>,
    pub ir: ir::Module,
    /// Every top-level entity with its type, for the reference model.
    pub declarations: Arc<Interface>,
    pub interface: Interface,
    /// The module with its plugin blocks expanded, when it has any.
    pub expansion: Option<Arc<Expansion>>,
}

impl Compilation {
    /// The library of `[dependencies]` that `module` belongs to, with its
    /// source directory.
    pub fn library_of(&self, module: &str) -> Option<(&str, &Path)> {
        let module = self.modules.iter().find(|m| m.name == module)?;
        let package = module.package.as_deref()?;
        (self.libraries.iter())
            .find(|(name, _)| name == package)
            .map(|(name, dir)| (name.as_str(), dir.as_path()))
    }

    /// The file of the foreign module `spec` when the build ships it beside
    /// `module`: an `.mjs` or `.cjs` of a library or the declarations of
    /// one, inside its source directory, which a tarball or a fetched copy
    /// of the library has. Any other relative file stays where it is, and
    /// the output refers to it.
    pub fn shipped_foreign(&self, module: &str, spec: &str) -> Option<PathBuf> {
        if !spec.starts_with("./") && !spec.starts_with("../") {
            return None;
        }
        let (_, dir) = self.library_of(module).filter(|_| ships(spec))?;
        let source = self.modules.iter().find(|m| m.name == module)?;
        let path = normalize(&source.path.parent()?.join(spec));
        path.starts_with(dir).then_some(path)
    }
}

/// Whether a build ships the file `path` of a library into its output: a
/// foreign module, or the TypeScript declarations of one.
pub fn ships(path: &str) -> bool {
    [".mjs", ".cjs", ".d.mts", ".d.cts", ".d.ts"]
        .iter()
        .any(|end| path.ends_with(end))
}
