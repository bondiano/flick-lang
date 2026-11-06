//! The groups of a round: each plugin collects from its blocks of a group,
//! the project's sources, its tests or a package, and publishes what it
//! learned, before any block of the round expands.

use std::collections::BTreeMap;
use std::sync::Arc;

use flick_plugin::Block;

use super::published::{self, Publications};
use super::results::Results;
use super::{Collected, Found, Plugins, failure, group};
use crate::diagnostics::Diagnostic;
use crate::plugin::reach::{Gate, Reach};
use crate::plugin::source::BlockSource;
use crate::project::load::{Loader, Root};

impl Loader<'_> {
    /// Each group of `found` collected and published, with its plugin's
    /// package, its members in order, their blocks, its state and its gate;
    /// a group whose plugin fails either is left out.
    pub(super) fn collect_groups(
        &self,
        plugins: &Plugins,
        results: &mut Results,
        publications: &mut Publications,
        found: &[Found],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Collected> {
        let mut groups: BTreeMap<(String, String), Vec<(String, usize)>> = BTreeMap::new();
        for (j, f) in found.iter().enumerate() {
            let m = &self.modules[f.module];
            let entry = groups
                .entry((group(&m.root), f.package.clone()))
                .or_default();
            entry.push((m.name.clone(), j));
        }
        let mut states = Vec::new();
        for ((_, package), mut members) in groups {
            let Some(Ok(plugin)) = plugins.get(&package) else {
                continue;
            };
            members.sort_by_key(|(name, j)| (name.clone(), found[*j].node.range()));
            let members: Vec<usize> = members.into_iter().map(|(_, j)| j).collect();
            let gate = members
                .first()
                .and_then(|&j| self.gate(&package, &self.modules[found[j].module].root));
            let blocks: Vec<Block> = members.iter().map(|&j| found[j].source.block()).collect();
            let mut collecting = BlockSource::default();
            collecting.reach = Reach::new(gate.clone());
            let collected = results.collect(plugin.as_ref(), &blocks, &mut collecting);
            let first = &found[members[0]];
            for (code, message) in collecting.reach.problems {
                diagnostics.push(Diagnostic::error(code, message, first.whole));
            }
            let state = match collected {
                Ok(state) => state,
                Err(why) => {
                    for &j in &members {
                        diagnostics.push(failure(&found[j], &package, &format!("failed: {why}")));
                    }
                    continue;
                }
            };
            // a group whose plugin fails to publish is not expanded, as one
            // that fails to collect: its blocks would miss what it learned
            match results.publish(plugin.as_ref(), &state) {
                Ok(published) => published::add(publications, &package, published),
                Err(why) => {
                    for &j in &members {
                        diagnostics.push(failure(&found[j], &package, &format!("failed: {why}")));
                    }
                    continue;
                }
            }
            states.push((package, members, blocks, state, gate));
        }
        states
    }

    /// What the plugin of `package` may reach for the blocks of modules of
    /// `root`, as the project grants it: only the project's own modules
    /// reach anything, and only with a host that reaches outside.
    pub(super) fn gate(&self, package: &str, root: &Root) -> Option<Arc<Gate>> {
        if matches!(root, Root::Package(_)) {
            return None;
        }
        let project = self.project?;
        Some(Arc::new(Gate {
            package: package.to_owned(),
            root: project.root.clone(),
            grants: (project.config.plugins.get(package).cloned()).unwrap_or_default(),
            outside: self.host.outside()?,
        }))
    }
}
