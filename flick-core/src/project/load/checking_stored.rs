/// What a host keeps of the project's own modules between processes. The
/// core takes and gives each module it checked cleanly by a key of every
/// input its checking read, and does no I/O itself.
pub struct ModuleStore {
    /// The compiler and its std, as the host identifies them.
    pub salt: String,
    /// The module saved under a key, if any.
    pub load: Box<Load>,
    /// Saves a module under a key; a failure only means it is checked again.
    pub save: Box<Save>,
}

/// How a host reads a saved module, from any thread that checks.
pub type Load = dyn Fn(&str) -> Option<SavedModule> + Send + Sync;

/// How a host saves a module, from any thread that checks.
pub type Save = dyn Fn(&str, &SavedModule) + Send + Sync;

/// A module of the project as a run checked it: without errors or
/// warnings, so with nothing to report.
#[derive(Serialize, Deserialize)]
pub struct SavedModule {
    interface: Interface,
    declarations: Arc<Interface>,
    /// The IR with its spans in file 0, when the run lowered the module.
    ir: Option<ir::Module>,
    /// What the keys of its importers take of its interface.
    digest: String,
}

impl Checking<'_> {
    /// [`Checking::module`] with a store: the module saved under its key,
    /// else `check`, saved when it reports nothing; with the digest of what
    /// importers read of it.
    fn stored(
        &self,
        loaded: &Loaded,
        store: &ModuleStore,
        interfaces: &[Option<Interface>],
        digests: &[Option<String>],
        check: impl FnOnce() -> Output,
    ) -> (Output, Option<String>) {
        let key = self.stored_key(loaded, store, interfaces, digests);
        let wants_ir = self.lower && !self.lower_packages_only;
        if let Some(key) = &key {
            // one saved with the IR serves a run that does not lower too
            let found = (!wants_ir)
                .then(|| (store.load)(&format!("{key}-n")))
                .flatten()
                .or_else(|| (store.load)(&format!("{key}-i")));
            if let Some(saved) = found {
                debug!(module = %loaded.name, "module taken from the store");
                let digest = saved.digest.clone();
                return (self.restored(loaded, saved, wants_ir, interfaces), Some(digest));
            }
        }
        let output = check();
        let digest = exported_digest(&output.exported);
        if let (Some(key), Some(digest)) = (key, &digest)
            && output.diagnostics.is_empty()
            && let Some(saved) = saved_module(loaded.file, &output, digest.clone())
        {
            let which = if saved.ir.is_some() { "i" } else { "n" };
            (store.save)(&format!("{key}-{which}"), &saved);
        }
        (output, digest)
    }

    /// The output of `loaded` from what a run saved of it.
    fn restored(
        &self,
        loaded: &Loaded,
        saved: SavedModule,
        wants_ir: bool,
        interfaces: &[Option<Interface>],
    ) -> Output {
        let SavedModule {
            mut interface,
            declarations,
            ir,
            ..
        } = saved;
        let ir = ir.filter(|_| wants_ir).map(|mut ir| {
            ir.each_span_mut(&mut |span| span.file = loaded.file);
            ir
        });
        // re-exported names are declared where their module now says
        for import in &loaded.syntax.imports {
            if let Some(imported) = self.visible_interface(loaded, interfaces, &import.module) {
                redeclare(&mut interface, imported);
            }
        }
        Output {
            diagnostics: Vec::new(),
            #[cfg(feature = "ide")]
            typing: Arc::new(types::Typing::default()),
            #[cfg(feature = "ide")]
            inline_ide: Vec::new(),
            exported: interface.clone(),
            interface,
            ir,
            declarations,
        }
    }

    /// The key `loaded` is saved under: every input of its checking, each
    /// import by the digest of its interface; `None` for a module whose
    /// inputs a key does not cover.
    fn stored_key(
        &self,
        loaded: &Loaded,
        store: &ModuleStore,
        interfaces: &[Option<Interface>],
        digests: &[Option<String>],
    ) -> Option<String> {
        let project = self.project?;
        if !self.storable(loaded, interfaces) {
            return None;
        }
        let mut key = Hashed::default();
        key.add("salt", store.salt.as_str());
        key.add(
            "mode",
            format!("{} {}", self.stages_by_class, self.lower_packages_only),
        );
        key.add("config", format!("{:?}", project.config.checked()));
        key.add("path", loaded.path.to_string_lossy().as_bytes());
        key.add("text", self.text_of(loaded));
        // which modules are the project's decides what is local and private
        let mut modules = String::new();
        for (name, &j) in self.by_name {
            let _ = writeln!(modules, "{name} {:?}", self.modules[j].root);
        }
        key.add("modules", modules);
        for (n, import) in loaded.syntax.imports.iter().enumerate() {
            let digest = self.import_digest(&import.module, digests)?;
            key.add(format!("import {n}"), format!("{} {digest}", import.module));
        }
        Some(key.finish())
    }

    /// The digest of the interface an import reads: a module of the run by
    /// what checking it gave, a saved module of a package by the key of its
    /// package, a built-in one by the salt.
    fn import_digest(&self, name: &str, digests: &[Option<String>]) -> Option<String> {
        if let Some(&j) = self.by_name.get(name) {
            return digests[j].clone();
        }
        if let Some(package) = self.precompiled.key_of_saved(name) {
            return Some(format!("package {package}"));
        }
        let builtin = types::builtins_of(self.stages_by_class);
        builtin
            .iter()
            .any(|i| i.module == name)
            .then(|| "built-in".to_owned())
    }

    /// Whether a key covers what checking `loaded` reads: a module of the
    /// project's sources or tests outside an editor, without plugin blocks
    /// or derives but those of the compiler's own plugin, which its text
    /// holds once expanded; without applications, inline modules or
    /// lexical scopes; whose imports are all found, none an application or
    /// a scope; and which no plugin but std's watches.
    fn storable(&self, loaded: &Loaded, interfaces: &[Option<Interface>]) -> bool {
        let plain = |m: &Loaded| m.virtual_of.is_none() && m.scope_of.is_none();
        let found = |name: &str| {
            self.raw_interface(interfaces, name)
                .is_some_and(|raw| raw.member_of.is_none())
                && (self.by_name.get(name)).is_none_or(|&j| plain(&self.modules[j]))
        };
        self.cached_ir == CachedIr::Kept
            && matches!(loaded.root, Root::Source | Root::Test)
            && plain(loaded)
            && loaded.member_of.is_none()
            && loaded.derives_alone()
            && loaded.editing.is_empty()
            && loaded.syntax.inline_imports.is_empty()
            && !Self::is_module_template(loaded)
            && !loaded.parse.tree.nodes().any(|n| n.kind == K::InlineModuleDecl)
            && !(self.modules.iter())
                .any(|m| m.scope_of.as_ref().is_some_and(|s| s.module == loaded.name))
            && (self.watching.iter()).all(|w| w.package == first_party::PACKAGE)
            && loaded.syntax.imports.iter().all(|i| found(&i.module))
    }
}

/// The digest of what importers read of `exported`, for their keys.
fn exported_digest(exported: &Interface) -> Option<String> {
    let json = serde_json::to_vec(&comparable_copy(exported)).ok()?;
    let mut key = Hashed::default();
    key.add("interface", json);
    Some(key.finish())
}

/// What to save of `output`, a module of `file` checked cleanly, with its
/// IR in file 0; `None` when the IR has spans in another file.
fn saved_module(file: FileId, output: &Output, digest: String) -> Option<SavedModule> {
    let mut ir = output.ir.clone();
    let mut own = true;
    if let Some(ir) = &mut ir {
        ir.each_span_mut(&mut |span| {
            own &= span.file == file;
            span.file = FileId::default();
        });
    }
    own.then(|| SavedModule {
        interface: output.interface.clone(),
        declarations: Arc::clone(&output.declarations),
        ir,
        digest,
    })
}
