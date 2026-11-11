//! The places of a module that call a name a plugin watches: what each
//! watching plugin is asked about, and where its answers go. A call in the
//! code of a block stands at the block, as the diagnostics of the block do.

use flick_plugin::CallSite;

use super::expansion::Expansion;
use super::{Expander, placed_call};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};
use crate::types::Watched;

/// `sites` of the checked text at the places of the module's own text they
/// stand for.
pub fn placed_sites(
    sites: &[CallSite],
    expansion: Option<&Expansion>,
    file: FileId,
) -> Vec<CallSite> {
    let mut sites = sites.to_vec();
    let Some(expansion) = expansion else {
        return sites;
    };
    let place = |span: flick_plugin::Span| {
        let at = expansion.place(Span::new(file, span.start, span.end));
        flick_plugin::Span {
            start: at.start,
            end: at.end,
        }
    };
    for site in &mut sites {
        site.span = place(site.span);
        if let Some(binding) = &mut site.binding {
            binding.span = place(binding.span);
        }
        for argument in &mut site.arguments {
            argument.span = place(argument.span);
        }
    }
    sites
}

/// What the plugin of `package` finds wrong with the calls of `sites` it
/// watches, given `state`, what it collected from the module's blocks;
/// `P036` when the plugin itself fails.
pub fn check_calls(
    plugin: &dyn Expander,
    package: &str,
    watched: &Watched,
    sites: &[CallSite],
    state: &[u8],
    file: FileId,
) -> Vec<Diagnostic> {
    let mine: Vec<CallSite> = (sites.iter())
        .filter(|c| match &c.tagged {
            Some(tagged) => watched.tag(&tagged.tag),
            None => watched.holds(&c.module, &c.name),
        })
        .cloned()
        .collect();
    let Some(first) = mine.first() else {
        return Vec::new();
    };
    let at = Span::new(file, first.span.start, first.span.end);
    match plugin.check_calls(&mine, state) {
        Ok(found) => (found.iter())
            .map(|d| placed_call(d, package, file))
            .collect(),
        Err(why) => vec![Diagnostic::error(
            codes::P036,
            format!("plugin `{package}` failed: {why}"),
            at,
        )],
    }
}
