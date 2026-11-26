//! Per-module derive invocation runner (`Call`, `Job`, and deriving argument
//! block packing).

use std::collections::BTreeMap;
use std::sync::Arc;

use flick_plugin::{Block, DeriveTarget, Place};

use super::super::super::Loader;
use super::super::{Found, GroupStates, Plugins, Results, failure, group, refusal};
use super::first_name;
use crate::diagnostics::{Diagnostic, codes};
use crate::plugin::derive::{Declared, Named, target_in};
use crate::plugin::reach::Reach;
use crate::plugin::render::{Rendered, render};
use crate::plugin::source::{BlockSource, token_kind};
use crate::plugin::{Expander, Failure};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::is_type;

/// One call of a derive.
struct Job<'j> {
    plugin: &'j dyn Expander,
    target: &'j DeriveTarget,
    class: &'j str,
    args: Option<&'j Block>,
}

/// What every derive of a module shares.
pub(super) struct Call<'c, 'l> {
    pub(super) loader: &'c Loader<'l>,
    pub(super) module: usize,
    pub(super) summary: &'c [DeriveTarget],
    pub(super) group_states: &'c GroupStates,
    pub(super) shared: &'c Arc<str>,
    pub(super) heads: &'c Arc<[String]>,
    pub(super) enums: &'c Arc<BTreeMap<String, String>>,
}

impl Call<'_, '_> {
    /// The block of `class` in the `deriving` of `decl`, kept before its code
    /// when `keep`, and the code its plugin gives; `None` for a class no
    /// plugin derives.
    pub(super) fn class(
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
        let (package, qualified, plugin) = self.deriver(class, keep, plugins, diagnostics)?;
        let (target, source) = if keep {
            let source = BlockSource::declaration(
                Arc::clone(self.shared),
                m.file,
                decl,
                Arc::clone(self.heads),
                true,
            )
            .with_module(&m.name)
            .with_catalog(Arc::clone(&loader.head_catalog))
            .with_enums(Arc::clone(self.enums));
            (
                target_in(
                    decl,
                    Some(&m.parse.tree),
                    Some(&m.syntax),
                    text,
                    source.block(),
                    None,
                )?,
                source,
            )
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
            plugin_id: plugin.id().to_owned(),
            keep,
            whole: at,
            declared: Vec::new(),
        };
        let job = Job {
            plugin: plugin.as_ref(),
            target: &target,
            class: &qualified,
            args: args.as_ref(),
        };
        Some(self.run(found, &job, results, diagnostics))
    }

    /// The package whose plugin derives `class`, the class's full name and
    /// the plugin; `None` for a class no plugin derives, which only a
    /// `deriving instance` (not `keep`) reports.
    fn deriver(
        &self,
        class: &Named,
        keep: bool,
        plugins: &mut Plugins,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(String, String, Arc<dyn Expander>)> {
        let loader = self.loader;
        let m = &loader.modules[self.module];
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
        if let Some(off) = loader.disabled(&package, &qualified, at) {
            diagnostics.push(off);
            return None;
        }
        match loader.load_plugin(plugins, &package) {
            Ok(plugin) => Some((package, qualified, plugin)),
            Err(why) => {
                let message = format!("the plugin of package `{package}` cannot be loaded: {why}");
                diagnostics.push(Diagnostic::error(codes::P035, message, at));
                None
            }
        }
    }

    /// The code the plugin of `class` gives for `declared`, a type the code
    /// of the block `block` declares: the block is its declaration, and the
    /// code goes with the block's own.
    pub(super) fn declared(
        &self,
        block: &Found,
        declared: &Declared,
        class: &str,
        plugins: &mut Plugins,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(Found, Option<Rendered>)> {
        let (module, name) = class.rsplit_once('.').unwrap_or(("", class));
        let written = Named {
            qualifier: module.to_owned(),
            name: name.to_owned(),
            args: None,
            span: (block.whole.start, block.whole.end),
        };
        let (package, qualified, plugin) = self.deriver(&written, true, plugins, diagnostics)?;
        let target = match &declared.target {
            Ok(target) => target,
            Err(why) => {
                diagnostics.push(failure(block, &package, why));
                return None;
            }
        };
        let m = &self.loader.modules[self.module];
        let source = self.loader.block_sources(m)(&block.node)?;
        let found = Found {
            module: self.module,
            node: block.node.clone(),
            source,
            package,
            plugin_id: plugin.id().to_owned(),
            keep: false,
            whole: block.whole,
            declared: Vec::new(),
        };
        let job = Job {
            plugin: plugin.as_ref(),
            target,
            class: &qualified,
            args: None,
        };
        Some(self.run(found, &job, results, diagnostics))
    }

    /// Runs `job`, keeping the code it gave or reporting why it gave none.
    fn run(
        &self,
        mut found: Found,
        job: &Job,
        results: &mut Results,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> (Found, Option<Rendered>) {
        let m = &self.loader.modules[self.module];
        let package = found.package.clone();
        let gate = self.loader.gate(&package, &m.root);
        found.source.reach = Reach::new(gate.clone());
        let state_key = (group(&m.root), package.clone());
        let (state, collect_used) = if let Some((state, used)) = self.group_states.get(&state_key) {
            (state.clone(), used.clone())
        } else {
            let mut collecting = BlockSource::default();
            collecting.reach = Reach::new(gate);
            let collected = results.collect(job.plugin, &[], &mut collecting);
            // what the host refused, whatever the plugin made of it
            for (code, message) in collecting.reach.problems {
                diagnostics.push(Diagnostic::error(code, message, found.whole));
            }
            match collected {
                Ok(state) => (state, collecting.reach.used),
                Err(why) => {
                    diagnostics.push(failure(&found, &package, &format!("failed: {why}")));
                    return (found, None);
                }
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
        found.source.reach.used.extend(collect_used);
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
