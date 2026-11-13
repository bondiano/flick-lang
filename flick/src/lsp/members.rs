//! The workspace around a project, for what the language server answers
//! across its members: the members a saved library re-checks, and the
//! references, renames and symbols that reach them.

use std::collections::{HashMap, HashSet};
use std::iter;
use std::path::{Path, PathBuf};

use flick_core::ide::{Location, Symbol, Target};
use flick_core::project::{self, workspace::Workspace};
use flick_core::source::{FileId, SourceMap};
use serde_json::{Value, json};

use super::requests::{REQUEST_FAILED, Reply, ide, location};
use super::{Context, Server, Snapshot, analyze, canonical, features, overlay};

/// The workspace `dir` lies in, read through the open buffers.
fn find(docs: &HashMap<PathBuf, String>, dir: &Path) -> Option<Workspace> {
    let host = overlay(docs);
    project::workspace::find(&host, &mut SourceMap::default(), dir)
        .ok()
        .flatten()
}

/// The members of the workspace of the project at `dir` that compile it.
pub fn dependents(docs: &HashMap<PathBuf, String>, dir: &Path) -> Vec<PathBuf> {
    find(docs, dir).map_or_else(Vec::new, |w| {
        w.dependents(dir)
            .into_iter()
            .map(Path::to_path_buf)
            .collect()
    })
}

/// The project directory of the file `path`.
fn project_of(docs: &HashMap<PathBuf, String>, path: &Path) -> Option<PathBuf> {
    match Context::of(docs, path) {
        Context::Project(dir) => Some(dir),
        _ => None,
    }
}

/// An occurrence as a URI and an LSP range.
type Spot = (String, Value);

/// A symbol, its occurrences and the project it is declared in.
type Found = (Symbol, Vec<Spot>, Option<PathBuf>);

impl Server {
    /// `workspace/symbol` over the latest analysis and the other members of
    /// its workspace; none before an analysis exists.
    pub(super) fn workspace_symbols(&mut self, params: &Value) -> Value {
        let query = params["query"].as_str().unwrap_or("");
        let latest = self.latest.as_ref();
        let Some(snapshot) = latest.and_then(|context| self.snapshots.get(context)) else {
            return json!([]);
        };
        let mut found = features::symbols(snapshot, query);
        let context = snapshot.context.clone();
        let Context::Project(dir) = &context else {
            return Value::Array(found);
        };
        let members = find(&self.docs, dir).map_or_else(Vec::new, |w| w.members);
        for member in members {
            if Context::Project(canonical(&member)) == context {
                continue;
            }
            let config = member.join(project::CONFIG_FILE);
            let snapshot = analyze(&self.docs, &config, &mut self.caches);
            found.extend(features::symbols(&snapshot, query));
        }
        let mut seen = HashSet::new();
        found.retain(|s| seen.insert(s["location"].to_string()));
        Value::Array(found)
    }

    /// `textDocument/references`: in the document's analysis and in the
    /// other members that compile the project the symbol is declared in.
    pub(super) fn references(&mut self, path: &Path, offset: u32) -> Reply {
        let Some((symbol, spots, home)) = self.occurrences_here(path, offset, None)? else {
            return Ok(json!([]));
        };
        let spots = self.across(path, &symbol, home.as_deref(), None, spots)?;
        let locations = spots
            .into_iter()
            .map(|(uri, range)| json!({ "uri": uri, "range": range }));
        Ok(Value::Array(locations.collect()))
    }

    /// `textDocument/rename`: the edits in every member that compiles the
    /// project of the symbol; an occurrence there that does not spell the
    /// name refuses the rename.
    pub(super) fn rename(&mut self, path: &Path, offset: u32, new_name: &str) -> Reply {
        let Some((symbol, spots, home)) = self.occurrences_here(path, offset, Some(new_name))?
        else {
            return Ok(Value::Null);
        };
        let spots = self.across(path, &symbol, home.as_deref(), Some(new_name), spots)?;
        let mut changes = serde_json::Map::new();
        for (uri, range) in spots {
            let edit = json!({ "range": range, "newText": new_name });
            changes
                .entry(uri)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .expect("array")
                .push(edit);
        }
        Ok(json!({ "changes": changes }))
    }

    /// The symbol at `offset`, its occurrences in the document's analysis
    /// (the edits of renaming it to `rename`) and the project it is declared
    /// in.
    fn occurrences_here(
        &mut self,
        path: &Path,
        offset: u32,
        rename: Option<&str>,
    ) -> Result<Option<Found>, (i64, String)> {
        // a block, so that the analysis is no longer borrowed for `self.docs`
        let (symbol, spots, home) = {
            let snapshot = self.snapshot_for(path);
            let Some(file) = snapshot.file(path) else {
                return Ok(None);
            };
            let ide = ide(snapshot);
            let (symbol, found) = if let Some(name) = rename {
                let (symbol, ..) = ide
                    .renamable(file, offset)
                    .map_err(|e| (REQUEST_FAILED, e))?;
                let edits = ide
                    .rename(file, offset, name)
                    .map_err(|e| (REQUEST_FAILED, e))?;
                (symbol, edits)
            } else {
                let Some((symbol, _)) = ide.symbol_at(file, offset) else {
                    return Ok(None);
                };
                let found = ide.references(&symbol);
                (symbol, found)
            };
            let spots = found
                .into_iter()
                .map(|(f, (start, end))| spot(snapshot, f, start, end))
                .collect::<Result<_, _>>()?;
            let home = if let Some(Target::File(f)) = ide.definition(&symbol).map(|d| d.target) {
                Some(PathBuf::from(&snapshot.sources.get(f).path))
            } else {
                None
            };
            (symbol, spots, home)
        };
        let home = home.and_then(|p| project_of(&self.docs, &p));
        Ok(Some((symbol, spots, home)))
    }

    /// `spots` with the occurrences of `symbol` in the other members that
    /// compile the project at `home`, `home` included; with `rename`, repeat
    /// rename safety checks in each dependent before returning any edits.
    fn across(
        &mut self,
        path: &Path,
        symbol: &Symbol,
        home: Option<&Path>,
        rename: Option<&str>,
        mut spots: Vec<Spot>,
    ) -> Result<Vec<Spot>, (i64, String)> {
        let (Some(home), false) = (home, matches!(symbol, Symbol::Local { .. })) else {
            return Ok(spots);
        };
        let current = Context::of(&self.docs, path);
        let members: Vec<PathBuf> = iter::once(home.to_path_buf())
            .chain(dependents(&self.docs, home))
            .filter(|m| Context::Project(canonical(m)) != current)
            .collect();
        for member in members {
            let snapshot = analyze(
                &self.docs,
                &member.join(project::CONFIG_FILE),
                &mut self.caches,
            );
            let ide = ide(&snapshot);
            let found = if let Some(name) = rename {
                ide.rename_references(symbol, name)
                    .map_err(|e| (REQUEST_FAILED, e))?
            } else {
                ide.references(symbol)
            };
            for (f, (start, end)) in found {
                spots.push(spot(&snapshot, f, start, end)?);
            }
        }
        let mut seen = HashSet::new();
        spots.retain(|(uri, range)| seen.insert((uri.clone(), range.to_string())));
        Ok(spots)
    }
}

fn spot(snapshot: &Snapshot, file: FileId, start: u32, end: u32) -> Result<Spot, (i64, String)> {
    let at = Location {
        target: Target::File(file),
        start,
        end,
    };
    let found = location(snapshot, &at)?;
    Ok((
        found["uri"].as_str().unwrap_or_default().to_owned(),
        found["range"].clone(),
    ))
}
