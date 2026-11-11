use super::{
    Diagnostic, Edit, Loaded, Loader, Path, PathBuf, Root, Span, codes, expected_name, io,
    module_path,
};

impl Loader<'_> {
    pub(super) fn load(&mut self, path: &Path) -> io::Result<Option<usize>> {
        if self.modules.iter().any(|m| m.path == path) {
            return Ok(None);
        }
        let bytes = self.host.read(path)?;
        let (file, utf8) = self.sources.add_bytes(self.host.display(path), bytes);
        self.diagnostics.extend(utf8);
        let loader = self.loader_of(path);
        let mut root = self.root_of(path);
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
        let expected = (root_dir.as_ref()).map(|dir| expected_name(path, dir, loader.is_some()));
        let named = expected.as_ref().and_then(|e| e.as_deref());
        let (parse, syntax) = self.syntax_of(path, file, loader.is_some(), named);
        let header = syntax.header.as_ref();
        let name = match (header, &expected) {
            (Some(h), _) => h.name.clone(),
            (None, Some(Some(name))) => name.clone(),
            _ => String::new(),
        };
        match (&expected, header) {
            (Some(Some(expected)), Some(h)) if *expected != h.name => self.diagnostics.push(
                Diagnostic::error(
                    codes::R001,
                    format!("`{}` must declare `module {expected}`, not `{}`", self.host.display(path), h.name),
                    h.span,
                )
                .with_fix(|| (
                    format!(
                        "rename the module, or move the file to `{}`",
                        root_dir.as_ref().map_or(module_path(&h.name), |d| PathBuf::from(self.host.display(&d.join(module_path(&h.name))))).display()
                    ),
                    vec![Edit { span: h.span, replacement: expected.clone() }],
                )),
            ),
            (Some(None), _) => self.diagnostics.push(
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
            syntax,
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
}
