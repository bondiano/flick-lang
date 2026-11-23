/// A file checked on its own takes the path of a module of a project run,
/// while resolving only built-in imports.
pub(crate) fn check_file(
    sources: &SourceMap,
    file: FileId,
    parse: Parse,
    syntax: ModuleSyntax,
    expansion: Option<Expansion>,
    mut derived: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    let name = syntax
        .header
        .as_ref()
        .map_or(String::new(), |h| h.name.clone());
    let loaded = Loaded {
        path: PathBuf::from(&sources.get(file).path),
        file,
        root: Root::Other(None),
        parse: Arc::new(parse),
        syntax: Arc::new(syntax),
        name: name.clone(),
        expansion: expansion.map(Arc::new),
        editing: Vec::new(),
        loader: None,
        virtual_of: None,
        member_of: None,
        scope_of: None,
    };
    let modules = [loaded];
    let mut by_name = BTreeMap::new();
    by_name.insert(name, 0);
    let from_packages = BTreeMap::new();
    let precompiled = Precompiled::default();
    let watched = types::Watched::new(first_party::WATCH);
    let watching = first_party::plugin(first_party::PACKAGE).map(|plugin| Watching {
        package: first_party::PACKAGE.to_owned(),
        plugin,
        watched: types::Watched::new(first_party::WATCH),
    });
    let watching: Vec<_> = watching.into_iter().collect();
    let env_ctx = super::environment::EnvironmentContext::standalone_js();
    let checking = Checking {
        modules: &modules,
        sources,
        by_name: &by_name,
        from_packages: &from_packages,
        project: None,
        stages_by_class: false,
        env_ctx: &env_ctx,
        #[cfg(feature = "native-project")]
        packages: None,
        lower: false,
        lowered: &[],
        proving: false,
        cached_ir: CachedIr::Dropped,
        lower_packages_only: false,
        precompiled: &precompiled,
        cache: None,
        watching: &watching,
        watched: &watched,
        privates: &BTreeMap::new(),
    };
    let builtin = |name: &str| {
        types::builtins()
            .iter()
            .find(|i| i.module == name && !i.private)
            .cloned()
    };
    let output = checking.check(&modules[0], &builtin, &builtin);
    let mut diagnostics = output.diagnostics;
    diagnostics.extend(output.adapter_diagnostics);
    diagnostics.append(&mut derived);
    super::environment::validate_standalone_when(
        &modules[0].syntax,
        sources.get(file),
        &mut diagnostics,
    );
    // a lone file has no package, so no private module of one is its own
    for import in &modules[0].syntax.imports {
        let private = |i: &Interface| i.module == import.module && i.private;
        if types::builtins().iter().any(private) {
            diagnostics.push(Diagnostic::error(
                codes::R017,
                format!(
                    "module `{}` is private to its package; import its public facade instead",
                    import.module
                ),
                import.span,
            ));
        }
    }
    Diagnostic::sort_by_span(&mut diagnostics);
    diagnostics
}

/// What importers see of a module: all of a clean one; of one with errors,
/// the exports and instances checking finished, typed, and the others by
/// name alone. One error then costs its importers what it broke, not every
/// type of the module.
fn exported(interface: &Interface, clean: bool) -> Interface {
    // a type left open holds variables of the module's own store, which no
    // interface writes and no importer may read with its own
    let mut out = interface.clone();
    if clean {
        return out;
    }
    for export in &mut out.exports {
        if !closed::export_is_closed(export) {
            (export.scheme, export.definition) = (None, None);
        }
    }
    out.instances.retain(closed::instance_is_closed);
    out
}

/// `interface` as a cache key compares it: without where its names are
/// declared and their docs, which checking does not read.
fn comparable_copy(interface: &Interface) -> Arc<Interface> {
    let mut copy = interface.clone();
    for export in &mut copy.exports {
        export.at = None;
    }
    copy.docs.clear();
    Arc::new(copy)
}

/// The names editors find and rename: those of the module's own text, and
/// those a plugin's code writes where the user wrote them, as a table
/// `Users` of a query that stands for `Users.table`.
#[cfg(feature = "ide")]
fn editor_names(
    mut typing: types::Typing,
    expansion: &Expansion,
    file: FileId,
    own: &str,
) -> types::Typing {
    let written: Vec<_> = (typing.names.iter())
        .filter(|((start, _), _)| *start >= expansion.len)
        .filter_map(|(&(start, end), r)| {
            let at = expansion.place(Span::new(file, start, end));
            let word = own.get(at.start as usize..at.end as usize)?;
            let types::Ref::Value { name, .. } = r else {
                return None;
            };
            let named = name
                .split_once('$')
                .map_or(name.as_str(), |(owner, _)| owner);
            let last = word.rsplit('.').next().unwrap_or(word);
            (at.end <= expansion.len && last == named).then(|| ((at.start, at.end), r.clone()))
        })
        .collect();
    typing.names.retain(|&(start, _), _| start < expansion.len);
    typing.names.extend(written);
    typing
        .type_names
        .retain(|&(start, _), _| start < expansion.len);
    typing
}
