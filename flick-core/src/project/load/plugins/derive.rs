//! The code classes marked `@derive` give the types that name them: each
//! type declaration naming such a class in `deriving` is a block of the
//! class's plugin, its code going after the declaration, and so is each
//! `deriving instance`, its code in its place. A type the code of a block
//! declares names its classes there, and the block is its declaration.

mod call;

use std::sync::Arc;

use flick_plugin::DeriveTarget;

use super::super::{Loaded, Loader, Root};
use super::{Found, GroupStates, Plugins, Results};
use crate::diagnostics::Diagnostic;
#[cfg(feature = "native-project")]
use crate::plugin::blocks_under;
use crate::plugin::derive::{Derived, builtin_derived, derived_classes, named, target_in};
use crate::plugin::render::Rendered;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, first_party};
use crate::resolve;
use crate::source::FileId;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::parse_with_catalog;
use crate::types::{self, is_type};
use call::Call;

impl Loaded {
    /// Whether the code of the module is its own text and, at most, what
    /// the compiler's own plugin derives from its declarations: a function
    /// of the sources and the compiler, which the key of a package holds.
    #[cfg(feature = "native-project")]
    pub(in crate::project::load) fn derives_alone(&self) -> bool {
        let Some(expansion) = &self.expansion else {
            return true;
        };
        let mut written = Vec::new();
        blocks_under(&self.parse.tree, &mut written);
        written.is_empty() && self.loader.is_none() && expansion.only_of(first_party::PACKAGE)
    }
}

impl Loader<'_> {
    /// The derive blocks of every loaded module, with the code their plugins
    /// gave.
    pub(super) fn derive_blocks(
        &self,
        expanded: &[(Found, Option<Rendered>)],
        group_states: &GroupStates,
        plugins: &mut Plugins,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<(Found, Option<Rendered>)> {
        let mut out = Vec::new();
        for (i, m) in self.modules.iter().enumerate() {
            let tree = &m.parse.tree;
            // the blocks whose code declares types
            let blocks: Vec<&Found> = (expanded.iter())
                .filter(|(f, code)| f.module == i && code.is_some() && !f.declared.is_empty())
                .map(|(f, _)| f)
                .collect();
            let wanted = tree.nodes().any(|n| {
                n.kind == K::DerivingDecl
                    || matches!(n.kind, K::TypeDecl | K::ErrorDecl)
                        && n.nodes().any(|c| c.kind == K::DerivingClause)
            });
            if !wanted && blocks.is_empty() {
                continue;
            }
            let text = &self.sources.get(m.file).text;
            let shared: Arc<str> = Arc::from(text.as_str());
            let heads = m.syntax.heads();
            let enums = Arc::new(self.enums_seen(m));
            let declared: Vec<&Node> = (tree.nodes())
                .filter(|n| matches!(n.kind, K::TypeDecl | K::ErrorDecl))
                .collect();
            let mut summary: Vec<DeriveTarget> = (declared.iter())
                .filter_map(|d| {
                    let source = BlockSource::declaration(
                        Arc::clone(&shared),
                        m.file,
                        d,
                        Arc::clone(&heads),
                        true,
                    )
                    .with_module(&m.name)
                    .with_catalog(Arc::clone(&self.head_catalog))
                    .with_enums(Arc::clone(&enums));
                    target_in(d, Some(tree), Some(&m.syntax), text, source.block(), None)
                })
                .collect();
            // the types the code of its blocks declares are the module's too
            for declared in blocks.iter().flat_map(|f| &f.declared) {
                if let Ok(target) = &declared.target {
                    summary.push(target.clone());
                }
            }
            let call = Call {
                loader: self,
                module: i,
                summary: &summary,
                group_states,
                shared: &shared,
                heads: &heads,
                enums: &enums,
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
            for f in blocks {
                for (d, class) in
                    (f.declared.iter()).flat_map(|d| d.classes.iter().map(move |c| (d, c)))
                {
                    out.extend(call.declared(f, d, class, plugins, results, diagnostics));
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

    /// The classes `module` marks `@derive`, from its source, validated plugin
    /// metadata, or built-in definitions.
    fn derived_of(&self, module: &str) -> Vec<Derived> {
        if let Some(&j) = self.by_name.get(module) {
            let m = &self.modules[j];
            return derived_classes(&m.parse.tree, &self.sources.get(m.file).text);
        }
        let mut from_meta = Vec::new();
        for (package, meta) in &self.plugin_metadata {
            for entry in &meta.derives {
                if let Some((owner_mod, class)) = entry.rsplit_once('.')
                    && owner_mod == module
                {
                    from_meta.push(Derived {
                        class: class.to_owned(),
                        package: Some(package.clone()),
                    });
                }
            }
        }
        if !from_meta.is_empty() {
            return from_meta;
        }
        if let Some((_, text)) = self.source_of(module)
            && text.contains("@derive")
        {
            let tree = parse_with_catalog(&text, FileId::default(), &self.head_catalog).tree;
            return derived_classes(&tree, &text);
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
    pub(in crate::project::load) fn load_plugin(
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
        let tree = parse_with_catalog(&shared, other, &self.head_catalog).tree;
        let other_syntax = resolve::module_syntax(&tree, &shared, other);
        let declared = (tree.nodes()).find(|n| {
            matches!(n.kind, K::TypeDecl | K::ErrorDecl)
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
        )
        .with_module(&import.module)
        .with_catalog(Arc::clone(&self.head_catalog));
        let target = target_in(
            declared,
            Some(&tree),
            Some(&other_syntax),
            &shared,
            source.block(),
            Some(&qualifier),
        )?;
        Some((target, source))
    }

    /// The text of `module` and its file, from the loaded modules or the
    /// built-in ones; a file of another language has no Flick to read.
    fn source_of(&self, module: &str) -> Option<(FileId, String)> {
        if let Some(&j) = self.by_name.get(module) {
            let m = &self.modules[j];
            if m.loader.is_some() {
                return None;
            }
            return Some((m.file, self.sources.get(m.file).text.clone()));
        }
        let (_, text) = types::builtin_sources()
            .iter()
            .find(|(n, _)| *n == module)?;
        Some((FileId::default(), (*text).to_owned()))
    }
}

/// The first name a type starts with: the head of an application.
fn first_name(ty: &Node) -> Option<&Node> {
    if ty.kind == K::NameType {
        return Some(ty);
    }
    ty.nodes().find_map(first_name)
}
