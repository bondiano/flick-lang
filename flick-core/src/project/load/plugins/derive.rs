//! The code classes marked `@derive` give the types that name them: each
//! type declaration naming such a class in `deriving` is a block of the
//! class's plugin, its code going after the declaration, and so is each
//! `deriving instance`, its code in its place.

use std::sync::Arc;

use flick_plugin::{Block, DeriveTarget, Place};

use super::super::{Loaded, Loader, Root};
use super::{Found, Plugins, Results, failure, refusal};
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::derive::{Derived, Named, builtin_derived, derived_classes, named, target};
use crate::plugin::reach::Reach;
use crate::plugin::render::{Rendered, render};
use crate::plugin::source::{BlockSource, token_kind};
use crate::plugin::{Expander, Failure, first_party};
use crate::source::{FileId, Span};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse;
use crate::types::{self, is_type};

impl Loader<'_> {
    /// The derive blocks of every loaded module, with the code their plugins
    /// gave.
    pub(super) fn derive_blocks(
        &self,
        plugins: &mut Plugins,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<(Found, Option<Rendered>)> {
        let mut out = Vec::new();
        for (i, m) in self.modules.iter().enumerate() {
            let tree = &m.parse.tree;
            let wanted = tree.nodes().any(|n| {
                n.kind == K::DerivingDecl
                    || matches!(n.kind, K::TypeDecl | K::ErrorDecl)
                        && n.nodes().any(|c| c.kind == K::DerivingClause)
            });
            if !wanted {
                continue;
            }
            let text = &self.sources.get(m.file).text;
            let shared: Arc<str> = Arc::from(text.as_str());
            let heads = m.syntax.heads();
            let declared: Vec<&Node> = (tree.nodes())
                .filter(|n| matches!(n.kind, K::TypeDecl | K::ErrorDecl))
                .collect();
            let summary: Vec<DeriveTarget> = (declared.iter())
                .filter_map(|d| {
                    let source = BlockSource::declaration(
                        Arc::clone(&shared),
                        m.file,
                        d,
                        Arc::clone(&heads),
                        true,
                    );
                    target(d, text, source.block(), None)
                })
                .collect();
            let call = Call {
                loader: self,
                module: i,
                summary: &summary,
                shared: &shared,
                heads: &heads,
            };
            for decl in tree.nodes() {
                let (classes, keep) = match decl.kind {
                    K::TypeDecl | K::ErrorDecl => {
                        match decl.nodes().find(|c| c.kind == K::DerivingClause) {
                            Some(clause) => (named(clause, text), true),
                            None => continue,
                        }
                    }
                    K::DerivingDecl => (named(decl, text), false),
                    _ => continue,
                };
                for class in classes {
                    let made = call.class(decl, &class, keep, plugins, results, diagnostics);
                    out.extend(made);
                }
            }
        }
        out
    }

    /// The package whose plugin derives the class `qualifier.name` of `m`,
    /// and the class's full name; `None` for a class without `@derive`.
    fn derive_of(&self, m: &Loaded, qualifier: &str, name: &str) -> Option<(String, String)> {
        let marked = |module: &str| {
            (self.derived_of(module).into_iter())
                .find(|d| d.class == name)
                .map(|d| (module.to_owned(), d))
        };
        let (module, derived) = if qualifier.is_empty() {
            marked(&m.name).or_else(|| {
                (m.syntax.imports.iter())
                    .filter(|i| {
                        i.exposing
                            .iter()
                            .any(|e| e.name == name || e.name.is_empty())
                    })
                    .find_map(|i| marked(&i.module))
            })
            // the Prelude's, as `Schema`, which every module sees
            .or_else(|| marked("Prelude"))?
        } else {
            match (m.syntax.imports.iter()).find(|i| i.qualifier() == qualifier) {
                Some(import) => marked(&import.module)?,
                // a built-in module is seen qualified without an import; another is not
                None => (builtin_derived(qualifier).iter())
                    .find(|d| d.class == name)
                    .map(|d| (qualifier.to_owned(), d.clone()))?,
            }
        };
        let package = derived.package.or_else(|| self.package_of(&module))?;
        Some((package, format!("{module}.{name}")))
    }

    /// The classes `module` marks `@derive`, from its source.
    // shortcut: a module only the package cache holds has no source here, so its classes derive nothing; read `@derive` from its interface when a cached package needs it
    fn derived_of(&self, module: &str) -> Vec<Derived> {
        if let Some(&j) = self.by_name.get(module) {
            let m = &self.modules[j];
            return derived_classes(&m.parse.tree, &self.sources.get(m.file).text);
        }
        builtin_derived(module).to_vec()
    }

    /// The package `module` belongs to, when it is one's.
    fn package_of(&self, module: &str) -> Option<String> {
        match self.by_name.get(module).map(|&j| &self.modules[j].root) {
            Some(Root::Package(name)) => Some(name.clone()),
            Some(_) => None,
            None => types::implicit_package_of(module)
                .or_else(|| self.precompiled.package_of(module))
                .map(str::to_owned),
        }
    }

    /// The plugin of `package`, loaded once.
    pub(super) fn load_plugin(
        &self,
        plugins: &mut Plugins,
        package: &str,
    ) -> Result<Arc<dyn Expander>, String> {
        plugins
            .entry(package.to_owned())
            .or_insert_with(|| match first_party::plugin(package) {
                Some(plugin) => Ok(plugin),
                None => self.host.plugin(package, self.component(package)),
            })
            .clone()
    }

    /// The type of `deriving instance C (M.T a)`, declared by the module
    /// `M` imports: what it declares, without holes in this module.
    fn foreign(
        &self,
        m: &Loaded,
        decl: &Node,
        heads: &Arc<[String]>,
    ) -> Option<(DeriveTarget, BlockSource)> {
        let text = &self.sources.get(m.file).text;
        let ty = decl.nodes().find(|n| is_type(n.kind))?;
        let head = first_name(ty)?;
        let parts: Vec<&str> = (head.child_tokens())
            .filter(|t| matches!(t.kind, K::Con | K::Ident))
            .map(|t| t.text(text))
            .collect();
        let (name, qualifier) = parts.split_last()?;
        let qualifier = qualifier.join(".");
        let import = m
            .syntax
            .imports
            .iter()
            .find(|i| i.qualifier() == qualifier)?;
        let (other, other_text) = self.source_of(&import.module)?;
        let shared: Arc<str> = Arc::from(other_text.as_str());
        let tree = parse(&shared, m.file).tree;
        let declared = (tree.nodes()).find(|n| {
            n.kind == K::TypeDecl
                && n.child_tokens()
                    .find(|t| t.kind == K::Con)
                    .map(|t| t.text(&shared))
                    == Some(*name)
        })?;
        let source = BlockSource::declaration(
            Arc::clone(&shared),
            other,
            declared,
            Arc::clone(heads),
            false,
        );
        let target = target(declared, &shared, source.block(), Some(&qualifier))?;
        Some((target, source))
    }

    /// The text of `module` and its file, from the loaded modules or the
    /// built-in ones.
    fn source_of(&self, module: &str) -> Option<(FileId, String)> {
        if let Some(&j) = self.by_name.get(module) {
            let m = &self.modules[j];
            return Some((m.file, self.sources.get(m.file).text.clone()));
        }
        let (_, text) = types::builtin_sources()
            .iter()
            .find(|(n, _)| *n == module)?;
        Some((FileId::default(), (*text).to_owned()))
    }
}

/// One call of a derive.
struct Job<'j> {
    plugin: &'j dyn Expander,
    target: &'j DeriveTarget,
    class: &'j str,
    args: Option<&'j Block>,
}

/// What every derive of a module shares.
struct Call<'c, 'l> {
    loader: &'c Loader<'l>,
    module: usize,
    summary: &'c [DeriveTarget],
    shared: &'c Arc<str>,
    heads: &'c Arc<[String]>,
}

impl Call<'_, '_> {
    /// The block of `class` in the `deriving` of `decl`, kept before its code
    /// when `keep`, and the code its plugin gives; `None` for a class no
    /// plugin derives.
    fn class(
        &self,
        decl: &Node,
        class: &Named,
        keep: bool,
        plugins: &mut Plugins,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(Found, Option<Rendered>)> {
        let loader = self.loader;
        let m = &loader.modules[self.module];
        let text = &loader.sources.get(m.file).text;
        let at = Span::new(m.file, class.span.0, class.span.1);
        let Some((package, qualified)) = loader.derive_of(m, &class.qualifier, &class.name) else {
            if !keep {
                let message = format!(
                    "the class `{}` has no derive: `deriving instance` names a class marked `@derive`",
                    class.name
                );
                diagnostics.push(Diagnostic::error(codes::C013, message, at));
            }
            return None;
        };
        let plugin = match loader.load_plugin(plugins, &package) {
            Ok(plugin) => plugin,
            Err(why) => {
                let message = format!("the plugin of package `{package}` cannot be loaded: {why}");
                diagnostics.push(Diagnostic::error(codes::P035, message, at));
                return None;
            }
        };
        let (target, source) = if keep {
            let source = BlockSource::declaration(
                Arc::clone(self.shared),
                m.file,
                decl,
                Arc::clone(self.heads),
                true,
            );
            (target(decl, text, source.block(), None)?, source)
        } else {
            let Some(found) = loader.foreign(m, decl, self.heads) else {
                let ty = decl.nodes().find(|n| is_type(n.kind)).and_then(first_name);
                let (start, end) = ty.unwrap_or(decl).range();
                let message = "`deriving instance` derives for a type declared by a module this one imports, as `Shapes.Shape`";
                diagnostics.push(Diagnostic::error(
                    codes::C013,
                    message,
                    Span::new(m.file, start, end),
                ));
                return None;
            };
            found
        };
        let args = class.args.map(|a| arguments(a, text, &class.name));
        let found = Found {
            module: self.module,
            node: decl.clone(),
            source,
            package,
            keep,
            whole: at,
        };
        let job = Job {
            plugin: plugin.as_ref(),
            target: &target,
            class: &qualified,
            args: args.as_ref(),
        };
        Some(self.run(found, &job, results, diagnostics))
    }

    /// `job` for the block `found`, and the code it gave.
    fn run(
        &self,
        mut found: Found,
        job: &Job,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> (Found, Option<Rendered>) {
        let package = found.package.clone();
        let m = &self.loader.modules[self.module];
        let gate = self.loader.gate(&package, &m.root);
        let mut collecting = BlockSource::default();
        collecting.reach = Reach::new(gate.clone());
        found.source.reach = Reach::new(gate);
        let collected = results.collect(job.plugin, &[], &mut collecting);
        // what the host refused, whatever the plugin made of it
        for (code, message) in collecting.reach.problems {
            diagnostics.push(Diagnostic::error(code, message, found.whole));
        }
        let state = match collected {
            Ok(state) => state,
            Err(why) => {
                diagnostics.push(failure(&found, &package, &format!("failed: {why}")));
                return (found, None);
            }
        };
        let made = results.derive(
            job.plugin,
            job.target,
            job.class,
            job.args,
            self.summary,
            &state,
            &mut found.source,
        );
        for (code, message) in found.source.reach.problems.drain(..) {
            diagnostics.push(Diagnostic::error(code, message, found.whole));
        }
        let rendered = match made {
            Ok(arena) => {
                let len = found.source.end() - found.source.start();
                match render(&arena, len, Place::Declaration, &m.syntax.aliases()) {
                    Ok(code) => Some(code),
                    Err(why) => {
                        let what = format!("gave code that is not valid: {why}");
                        diagnostics.push(failure(&found, &package, &what));
                        None
                    }
                }
            }
            Err(Failure::Refused(refused)) => {
                if refused.is_empty() && found.source.diagnostics.is_empty() {
                    diagnostics.push(failure(&found, &package, "derived nothing"));
                }
                diagnostics.extend(refused.iter().map(|d| refusal(&found, &package, d)));
                None
            }
            Err(Failure::Broken(why)) => {
                diagnostics.push(failure(&found, &package, &format!("failed: {why}")));
                None
            }
        };
        diagnostics.append(&mut found.source.diagnostics);
        (found, rendered)
    }
}

/// The `{ … }` of a class in `deriving`, as a block whose body is its text.
fn arguments(node: &Node, text: &str, class: &str) -> Block {
    let (start, end) = node.range();
    let tokens = (node.tokens().into_iter())
        .filter(|t| !t.kind.is_trivia())
        .map(|t| flick_plugin::Token {
            kind: token_kind(t.kind),
            span: flick_plugin::Span {
                start: t.start - start,
                end: t.end - start,
            },
        })
        .collect();
    Block {
        head: class.to_owned(),
        text: text[start as usize..end as usize].to_owned(),
        body: 0,
        place: Place::Expression,
        tokens,
        docs: String::new(),
        signature: None,
        module: String::new(),
    }
}

/// The first name a type starts with: the head of an application.
fn first_name(ty: &Node) -> Option<&Node> {
    if ty.kind == K::NameType {
        return Some(ty);
    }
    ty.nodes().find_map(first_name)
}
