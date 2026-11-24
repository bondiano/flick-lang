/// Checks `roots` (absolute `.fl` paths) and their transitive imports.
pub fn check(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Vec<Diagnostic> {
    run(host, sources, project, roots, Mode::Check, None, None).diagnostics
}

/// [`check`], taking the modules of source packages `packages` saved and
/// giving those to save; a command that checks again and again keeps what
/// it checked in `cache`.
pub fn check_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Check, cache, packages);
    (output.diagnostics, output.fresh)
}

/// Checks `roots` (absolute `.fl` paths) and their transitive imports, also
/// verifying proofs and every contract declared in `roots`.
#[cfg(feature = "contracts")]
pub fn prove(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> Vec<Diagnostic> {
    run(host, sources, project, roots, Mode::Prove, None, None).diagnostics
}

/// [`prove`], reusing saved package modules and `cache` for non-proved modules
/// while always re-proving proof and contracted modules.
#[cfg(feature = "contracts")]
pub fn prove_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Vec<Fresh>) {
    let output = run(host, sources, project, roots, Mode::Prove, cache, packages);
    (output.diagnostics, output.fresh)
}

/// [`check`], also lowering every module to the IR when nothing has errors.
pub fn compile(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
) -> (Vec<Diagnostic>, Compilation) {
    let Output {
        diagnostics,
        compilation,
        ..
    } = run(host, sources, project, roots, Mode::Compile, None, None);
    (diagnostics, compilation)
}

/// [`compile`], taking the modules of source packages `packages` saved and
/// giving those to save; `rt` overrides whether stages follow class derivation
/// (`Some(true)` for RT, `Some(false)` for Effect/Go, `None` for the project
/// default); a command that builds again and again keeps what it compiled in
/// `cache`.
pub fn compile_with(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    rt: Option<bool>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let target = rt.map(|rt| TargetOverride {
        rt: Some(rt),
        ..TargetOverride::default()
    });
    compile_for(
        host,
        sources,
        project,
        roots,
        packages,
        target.as_ref(),
        cache,
    )
}

/// [`compile_with`], parameterized by a target/option override (from CLI
/// `--target`, resolved backend options, loaded descriptor tags, or test mode).
pub fn compile_for(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    packages: Option<&PackageCache>,
    target: Option<&TargetOverride>,
    cache: Option<&mut ModuleCache>,
) -> (Vec<Diagnostic>, Compilation, Vec<Fresh>) {
    let mode = Mode::Compile;
    let output = run_in(host, sources, project, roots, mode, cache, packages, target);
    (output.diagnostics, output.compilation, output.fresh)
}

fn run(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[PathBuf],
    mode: Mode,
    cache: Option<&mut ModuleCache>,
    saved: Option<&PackageCache>,
) -> Output {
    run_in(host, sources, project, roots, mode, cache, saved, None)
}
