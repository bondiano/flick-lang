//! `flick expand`: the code each plugin block of the selected files stands
//! for, as its plugin gives it, the user's code in place.

use flick_core::project::{self, Host};
use flick_core::source::SourceMap;

use super::check::select;
use crate::args::ExpandArguments;
use crate::command::Outcome;
use crate::sources::FsHost;

pub fn expand(options: &ExpandArguments, explicit: Option<&str>) -> Outcome {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (project, roots) = match select(&host, &mut sources, explicit, &options.paths) {
        Ok(selected) => selected,
        Err(diagnostic) => {
            return Outcome {
                sources,
                diagnostics: vec![diagnostic],
            };
        }
    };
    let analysis = project::analyze(&host, &mut sources, project.as_ref(), &roots);
    let mut modules: Vec<_> = (analysis.modules.iter())
        .filter(|m| roots.contains(&m.path))
        .collect();
    modules.sort_by(|a, b| a.path.cmp(&b.path));
    for m in modules {
        let file = sources.get(m.file);
        for shown in m.expansion.iter().flat_map(|e| e.shown()) {
            let (line, column) = file.line_col(shown.range.0);
            println!(
                "{}:{}:{}: plugin `{}`",
                host.display(&m.path),
                line + 1,
                column + 1,
                shown.plugin
            );
            for code in shown.code.lines() {
                println!("  {code}");
            }
            println!();
        }
    }
    Outcome {
        sources,
        diagnostics: analysis.diagnostics,
    }
}
