/// A file checked on its own takes the path of a module of a project run,
/// while resolving only built-in imports.
pub(crate) fn check_file(
    sources: &SourceMap,
    file: FileId,
    parse: Parse,
    syntax: ModuleSyntax,
    expansion: Option<Expansion>,
    derived: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    check_file_with(sources, file, parse, syntax, expansion, derived, false)
}

/// [`check_file`], also proving any function contracts in `PureTotalV2`.
pub(crate) fn prove_file(
    sources: &SourceMap,
    file: FileId,
    parse: Parse,
    syntax: ModuleSyntax,
    expansion: Option<Expansion>,
    derived: Vec<Diagnostic>,
) -> Vec<Diagnostic> {
    check_file_with(sources, file, parse, syntax, expansion, derived, true)
}

fn check_file_with(
    sources: &SourceMap,
    file: FileId,
    parse: Parse,
    syntax: ModuleSyntax,
    expansion: Option<Expansion>,
    mut derived: Vec<Diagnostic>,
    prove_contracts: bool,
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
    let mut all_contracted = HashSet::new();
    let text = modules[0]
        .expansion
        .as_ref()
        .map_or(sources.get(file).text.as_str(), |e| e.text.as_str());
    modules[0].collect_contract_names(text, &mut all_contracted);
    let checking = Checking {
        modules: &modules,
        sources,
        by_name: &by_name,
        from_packages: &from_packages,
        project: None,
        stages_by_class: false,
        lower: false,
        lowered: &[],
        proving: prove_contracts,
        prove_contracts,
        selected: None,
        all_contracted: &all_contracted,
        verified_contracts: &[],
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
    let mut diagnostics = checking.check(&modules[0], &builtin, &builtin).diagnostics;
    diagnostics.append(&mut derived);
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
