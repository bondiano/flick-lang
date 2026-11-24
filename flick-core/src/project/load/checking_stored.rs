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
    /// The inputs of the module last saved from a path: what a later miss
    /// of the same path names as changed. Without a record it cannot.
    pub inputs: Box<LoadInputs>,
    /// Records the inputs of a module just saved.
    pub keep_inputs: Box<SaveInputs>,
}

/// How a host reads a saved module, from any thread that checks.
pub type Load = dyn Fn(&str) -> Option<SavedModule> + Send + Sync;

/// How a host saves a module, from any thread that checks.
pub type Save = dyn Fn(&str, &SavedModule) + Send + Sync;

/// How a host reads the digests of the inputs of the module last saved
/// from a path, by a digest of the path.
pub type LoadInputs = dyn Fn(&str) -> Option<Vec<(String, String)>> + Send + Sync;

/// How a host records them for a module it saved.
pub type SaveInputs = dyn Fn(&str, &[(String, String)]) + Send + Sync;

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
    /// importers read of it, and why it was checked or taken.
    fn stored(
        &self,
        loaded: &Loaded,
        store: &ModuleStore,
        interfaces: &[Option<Interface>],
        digests: &[Option<String>],
        check: impl FnOnce() -> Output,
    ) -> (Output, Option<String>, Reason) {
        let signed = self.stored_key(loaded, store, interfaces, digests);
        let wants_ir = self.lower && !self.lower_packages_only;
        // the inputs of a key are recorded by the digest of its path
        let path = signed.as_ref().and_then(|(_, parts)| {
            (parts.iter()).find(|(label, _)| label == "path").map(|(_, digest)| digest.clone())
        });
        if let Some((key, _)) = &signed {
            // one saved with the IR serves a run that does not lower too
            let found = (!wants_ir)
                .then(|| (store.load)(&format!("{key}-n")))
                .flatten()
                .or_else(|| (store.load)(&format!("{key}-i")));
            if let Some(saved) = found {
                debug!(module = %loaded.name, "module taken from the store");
                let digest = saved.digest.clone();
                let restored = self.restored(loaded, saved, wants_ir, interfaces);
                return (restored, Some(digest), Reason::Restored);
            }
        }
        let reason = match (&signed, &path) {
            _ if !self.cache.is_some_and(|c| c.explain) => Reason::Changed(Vec::new()),
            (Some((_, parts)), Some(path)) => Reason::Changed(
                (store.inputs)(path).map_or_else(Vec::new, |saved| {
                    let imports: Vec<&str> =
                        (loaded.syntax.imports.iter()).map(|i| i.module.as_str()).collect();
                    changed_inputs(&saved, parts, &imports)
                }),
            ),
            _ => Reason::NotStorable,
        };
        let output = check();
        let digest = exported_digest(&output.exported);
        if let (Some((key, parts)), Some(digest)) = (&signed, &digest)
            && output.diagnostics.is_empty()
            && let Some(saved) = saved_module(loaded.file, &output, digest.clone())
        {
            let which = if saved.ir.is_some() { "i" } else { "n" };
            (store.save)(&format!("{key}-{which}"), &saved);
            if let Some(path) = &path {
                (store.keep_inputs)(path, parts);
            }
        }
        (output, digest, reason)
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
            ir: ir.map(Arc::new),
            declarations,
            proofs: Vec::new(),
            contracts: Vec::new(),
        }
    }

    /// The key `loaded` is saved under, with the digest of each labeled
    /// part: every input of its checking, each import by the digest of its
    /// interface; `None` for a module whose inputs a key does not cover.
    fn stored_key(
        &self,
        loaded: &Loaded,
        store: &ModuleStore,
        interfaces: &[Option<Interface>],
        digests: &[Option<String>],
    ) -> Option<(String, Vec<(String, String)>)> {
        let project = self.project?;
        if !self.storable(loaded, interfaces) {
            return None;
        }
        let mut key = Hashed::default();
        key.add("iface-version", IFACE_VERSION.to_le_bytes());
        key.add("ir-version", flick_ir::VERSION);
        key.add("salt", store.salt.as_str());
        key.add(
            "mode",
            format!("{} {}", self.stages_by_class, self.lower_packages_only),
        );
        key.add("config", format!("{:?}", project.config.checked()));
        key.add(
            "foreign-backend",
            serde_json::to_vec(&(
                self.foreign_backend_for(loaded),
                self.env_ctx.validate_foreign_metadata,
                &self.env_ctx.native_overrides,
            ))
            .unwrap_or_default(),
        );
        key.add("path", loaded.path.to_string_lossy().as_bytes());
        key.add("source", self.sources.get(loaded.file).text.as_bytes());
        key.add("text", self.text_of(loaded));
        if let Some(expansion) = &loaded.expansion {
            for (n, plugin) in expansion.plugins.iter().enumerate() {
                key.add(format!("plugin {n}"), plugin.as_bytes());
            }
            for (n, used) in expansion.used.iter().enumerate() {
                key.add(
                    format!("used {n}"),
                    serde_json::to_vec(used).unwrap_or_default(),
                );
            }
        }
        let mut editing: Vec<&_> = loaded.editing.iter().collect();
        editing.sort_by_key(|e| (e.start, e.plugin.id(), e.state.as_ref()));
        for (n, editor) in editing.into_iter().enumerate() {
            key.add(format!("editing {n} start"), editor.start.to_le_bytes());
            key.add(format!("editing {n} plugin"), editor.plugin.id().as_bytes());
            key.add(format!("editing {n} state"), editor.state.as_ref());
        }
        let mut watching: Vec<&Watching> = self.watching.iter().collect();
        watching.sort_by(|a, b| {
            a.package
                .cmp(&b.package)
                .then_with(|| a.plugin.id().cmp(b.plugin.id()))
                .then_with(|| a.watched.names().cmp(b.watched.names()))
        });
        for (n, w) in watching.into_iter().enumerate() {
            key.add(format!("watching {n} package"), w.package.as_bytes());
            key.add(format!("watching {n} plugin"), w.plugin.id().as_bytes());
            key.add(
                format!("watching {n} watched"),
                serde_json::to_vec(w.watched.names()).unwrap_or_default(),
            );
        }
        // which modules are the project's decides what is local and private
        let mut modules = String::new();
        for (name, &j) in self.by_name {
            let _ = writeln!(modules, "{name} {:?}", self.modules[j].root);
        }
        key.add("modules", modules);
        for (n, import) in loaded.syntax.imports.iter().enumerate() {
            // a test companion reads all its module declares
            let digest = if companion_of(&loaded.name) == Some(import.module.as_str()) {
                exported_digest(self.privates.get(&import.module)?)?
            } else {
                self.import_digest(&import.module, digests)?
            };
            key.add(format!("import {n}"), format!("{} {digest}", import.module));
        }
        Some(key.signed())
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
    /// project's sources or tests outside an editor, including modules with
    /// plugin blocks, custom derives or watching plugins (whose ids, reached
    /// inputs, block states and watched names are in the key); without
    /// applications, inline modules, module templates or lexical scopes; and
    /// whose imports are all found, none an application or a scope.
    fn storable(&self, loaded: &Loaded, interfaces: &[Option<Interface>]) -> bool {
        let plain = |m: &Loaded| m.virtual_of.is_none() && m.scope_of.is_none();
        let found = |name: &str| {
            self.raw_interface(interfaces, name)
                .is_some_and(|raw| raw.member_of.is_none())
                && (self.by_name.get(name)).is_none_or(|&j| plain(&self.modules[j]))
        };
        let tree = loaded
            .expansion
            .as_ref()
            .map_or(&loaded.parse.tree, |e| &e.tree);
        self.cached_ir == CachedIr::Kept
            && matches!(loaded.root, Root::Source | Root::Test | Root::Proof)
            && plain(loaded)
            && loaded.member_of.is_none()
            && loaded.syntax.inline_imports.is_empty()
            && !Self::is_module_template(loaded)
            && !loaded.parse.tree.nodes().any(|n| n.kind == K::InlineModuleDecl)
            && !tree.nodes().any(|n| n.kind == K::InlineModuleDecl)
            && !(self.modules.iter())
                .any(|m| m.scope_of.as_ref().is_some_and(|s| s.module == loaded.name))
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
    let mut ir = output.ir.as_deref().cloned();
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
