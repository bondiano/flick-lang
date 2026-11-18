use std::sync::Arc;

use super::{
    Diagnostic, Edit, Loaded, Loader, ModuleSyntax, Path, PathBuf, Root, Span, codes,
    expected_name, io, module_path,
};
use crate::project::{COMPANION, COMPANIONS, companion_module, companion_of};
use crate::resolve::{Header, Import, Item, ItemKind, TESTS};
use crate::source::FileId;

impl Loader<'_> {
    /// Loads and registers a module file once; `Ok(None)` if it is already loaded.
    pub(super) fn load(&mut self, path: &Path) -> io::Result<Option<usize>> {
        if self.modules.iter().any(|m| m.path == path) {
            return Ok(None);
        }
        let bytes = self.host.read(path)?;
        let (file, utf8) = self.sources.add_bytes(self.host.display(path), bytes);
        self.diagnostics.extend(utf8);
        let loader = self.loader_of(path);
        let mut root = self.root_of(path);
        // a test companion is named after the module whose file it stands beside
        let companion = (COMPANIONS && root == Root::Source)
            .then(|| companion_module(path))
            .flatten();
        let root_dir = match (&root, self.project) {
            (Root::Source, Some(p)) => Some(p.source_root()),
            (Root::Test, Some(p)) => Some(p.test_root()),
            (Root::Package(name), _) => self
                .package_sources()
                .into_iter()
                .find(|(n, _)| n == name)
                .map(|(_, dir)| dir),
            _ => None,
        };
        let at = companion.as_deref().unwrap_or(path);
        let expected = (root_dir.as_ref()).map(|dir| expected_name(at, dir, loader.is_some()));
        let named = expected.as_ref().and_then(|e| e.as_deref());
        let (parse, syntax) = self.syntax_of(path, file, loader.is_some(), named);
        let header = syntax.header.as_ref();
        let mut name = match (header, &expected) {
            (Some(h), _) => h.name.clone(),
            (None, Some(Some(name))) => name.clone(),
            _ => String::new(),
        };
        if let Some(expected) = &expected {
            self.check_header(path, file, root_dir.as_deref(), expected.as_deref(), header);
        }
        if companion.is_some() && !name.is_empty() {
            name.push_str(COMPANION);
        }
        if let (Root::Other(dir), Some(h)) = (&mut root, header) {
            // `x/App/Users.fl` declaring `App.Users` imports siblings from `x/`
            let tail = module_path(&h.name);
            *dir = path
                .to_str()
                .and_then(|p| p.strip_suffix(tail.to_str()?))
                .map(PathBuf::from);
        }
        let index = self.modules.len();
        if !name.is_empty() {
            match self.by_name.get(&name) {
                Some(&first) => {
                    let first = &self.modules[first];
                    let mut diagnostic = Diagnostic::error(
                        codes::R002,
                        format!(
                            "module `{name}` is also declared by `{}`",
                            self.host.display(&first.path)
                        ),
                        header.map_or(Span::new(file, 0, 0), |h| h.span),
                    );
                    if let Some(h) = &first.syntax.header {
                        diagnostic = diagnostic.with_related(h.span, "first declared here");
                    }
                    self.diagnostics.push(diagnostic);
                }
                None => {
                    self.by_name.insert(name.clone(), index);
                }
            }
        }
        self.modules.push(Loaded {
            path: path.to_path_buf(),
            file,
            root,
            parse,
            syntax: companion_syntax(syntax, &name, file),
            name,
            expansion: None,
            editing: Vec::new(),
            loader,
            virtual_of: None,
            member_of: None,
            scope_of: None,
        });
        Ok(Some(index))
    }

    pub(super) fn root_of(&self, path: &Path) -> Root {
        match self.project {
            Some(p) if path.starts_with(p.source_root()) => Root::Source,
            Some(p) if path.starts_with(p.test_root()) => Root::Test,
            _ => self
                .package_sources()
                .into_iter()
                .find(|(_, dir)| path.starts_with(dir))
                .map_or(Root::Other(None), |(name, _)| Root::Package(name)),
        }
    }

    /// The source directory of each source package the project depends on.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    pub(super) fn package_sources(&self) -> Vec<(String, PathBuf)> {
        #[cfg(feature = "native-project")]
        return self
            .packages
            .sources()
            .map(|(name, dir)| (name.to_owned(), dir.to_path_buf()))
            .collect();
        #[cfg(not(feature = "native-project"))]
        Vec::new()
    }

    /// What is wrong with the header of the module at `path` in `root_dir`:
    /// another name than `expected`, the one its path gives, or no module
    /// path at all.
    fn check_header(
        &mut self,
        path: &Path,
        file: FileId,
        root_dir: Option<&Path>,
        expected: Option<&str>,
        header: Option<&Header>,
    ) {
        match (expected, header) {
            (Some(expected), Some(h)) if expected != h.name => self.diagnostics.push(
                Diagnostic::error(
                    codes::R001,
                    format!("`{}` must declare `module {expected}`, not `{}`", self.host.display(path), h.name),
                    h.span,
                )
                .with_fix(|| (
                    format!(
                        "rename the module, or move the file to `{}`",
                        root_dir.map_or(module_path(&h.name), |d| PathBuf::from(self.host.display(&d.join(module_path(&h.name))))).display()
                    ),
                    vec![Edit { span: h.span, replacement: expected.to_owned() }],
                )),
            ),
            (None, _) => self.diagnostics.push(
                Diagnostic::error(
                    codes::R001,
                    format!(
                        "`{}` is not at a module path: each directory and the file name must be a capitalized name",
                        self.host.display(path)
                    ),
                    header.map_or(Span::new(file, 0, 0), |h| h.span),
                )
                .with_fix(|| ("rename the file after its module, e.g. `src/App/Users.fl` for `App.Users`", Vec::new())),
            ),
            _ => {}
        }
    }
}

/// The syntax of a module named `name`, its own; of a test companion, with
/// an import of everything its module declares, private names included,
/// and an export of its `tests`, which `flick test` runs, whether blocks
/// declare them or the companion does by hand.
fn companion_syntax(syntax: Arc<ModuleSyntax>, name: &str, file: FileId) -> Arc<ModuleSyntax> {
    let Some(module) = companion_of(name).filter(|_| COMPANIONS) else {
        return syntax;
    };
    let mut syntax = (*syntax).clone();
    let span = (syntax.header.as_ref()).map_or(Span::new(file, 0, 0), |h| h.span);
    syntax.imports.push(Import {
        module: module.to_owned(),
        span,
        alias: None,
        exposing: vec![Item {
            kind: ItemKind::All,
            name: String::new(),
            span,
        }],
        plugins: Vec::new(),
    });
    syntax.tests.get_or_insert(Item {
        kind: ItemKind::Value,
        name: TESTS.to_owned(),
        span,
    });
    Arc::new(syntax)
}
