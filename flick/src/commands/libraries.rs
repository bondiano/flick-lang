//! The Flick libraries of `[dependencies]`: `flick add` with a source,
//! `flick remove` of one, and their pins in `flick deps`.

use std::fs;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::project::config::{self, Config};
use flick_core::project::lock::{LOCK_FILE, Lock};
use flick_core::project::{CONFIG_FILE, Project};
use flick_core::source::SourceMap;

use super::manifest;
use super::run::find_project;
use crate::args::{AddArguments, GlobalArguments};
use crate::command::Outcome;
use crate::fetch;

fn failure(message: String) -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: vec![Diagnostic::new(codes::P032, Severity::Error, message)],
    }
}

fn done() -> Outcome {
    Outcome {
        sources: SourceMap::default(),
        diagnostics: Vec::new(),
    }
}

/// `text` as the project's manifest, when it is a valid one.
fn parse(text: &str) -> Result<Config, Outcome> {
    let mut sources = SourceMap::default();
    let file = sources.add(CONFIG_FILE.to_owned(), text);
    config::parse(text, file).map_err(|d| Outcome {
        sources,
        diagnostics: vec![d],
    })
}

/// `flick add <name> --git/--url/--path`: the library in `[dependencies]`,
/// fetched and pinned; the manifest stays as it was when fetching fails.
pub fn add_library(options: &AddArguments, global: &GlobalArguments) -> Outcome {
    let [name] = &options.packages[..] else {
        return failure("a source option adds one Flick library: give one name".to_owned());
    };
    let Some(source) = options.source() else {
        return failure("name the library's source".to_owned());
    };
    let project = match find_project(global) {
        Ok(p) => p,
        Err((sources, diagnostics)) => {
            return Outcome {
                sources,
                diagnostics,
            };
        }
    };
    let path = project.root.join(CONFIG_FILE);
    let Ok(old) = fs::read_to_string(&path) else {
        return failure(format!("cannot read `{}`", path.display()));
    };
    let text = manifest::add(&old, name, &manifest::entry(name, &source));
    let config = match parse(&text) {
        Ok(config) => config,
        Err(outcome) => return outcome,
    };
    if let Err(e) = fs::write(&path, &text) {
        return failure(format!("cannot write `{}`: {e}", path.display()));
    }
    let update = [name.clone()];
    if let Err(message) = fetch::install(
        &project.root,
        &config.dependencies,
        &update,
        fetch::Options::default(),
    ) {
        let _ = fs::write(&path, &old);
        return Outcome {
            sources: SourceMap::default(),
            diagnostics: vec![Diagnostic::new(codes::P034, Severity::Error, message)],
        };
    }
    done()
}

/// Removes the libraries among `names` from `[dependencies]` and the lock;
/// the names that are not libraries, for pnpm.
pub fn remove(project: &Project, names: &[String]) -> Result<Vec<String>, Outcome> {
    let path = project.root.join(CONFIG_FILE);
    let Ok(mut text) = fs::read_to_string(&path) else {
        return Ok(names.to_vec());
    };
    let mut rest = Vec::new();
    for name in names {
        match manifest::remove(&text, name) {
            Some(without) => text = without,
            None => rest.push(name.clone()),
        }
    }
    if rest.len() == names.len() {
        return Ok(rest);
    }
    let config = parse(&text)?;
    fs::write(&path, &text)
        .map_err(|e| failure(format!("cannot write `{}`: {e}", path.display())))?;
    // what is left is pinned and stored already: this only drops the removed pins
    fetch::install(
        &project.root,
        &config.dependencies,
        &[],
        fetch::Options::default(),
    )
    .map_err(failure)?;
    Ok(rest)
}

/// The packages `flick.lock` pins, one per line.
pub fn print_locked(project: &Project) {
    let Ok(text) = fs::read_to_string(project.root.join(LOCK_FILE)) else {
        return;
    };
    let mut sources = SourceMap::default();
    let file = sources.add(LOCK_FILE.to_owned(), &text);
    let Ok(lock) = Lock::parse(&text, file) else {
        return;
    };
    println!("Flick libraries ({LOCK_FILE}):");
    for p in &lock.packages {
        println!("  {} {}  {}", p.name, p.version, p.source);
    }
}
