//! How long the e2e passes take: `times.tsv` keeps the build and run time
//! of every case in every pass, and each pass prints its wall clock and the
//! sums of its builds and runs, which CI adds to the job summary.

use std::cmp::Reverse;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{env, fs};

/// The build and run of one case in one pass.
pub struct Timing {
    pub case: PathBuf,
    pub built: Duration,
    pub ran: Duration,
}

/// One pass over the cases: an emit target in a profile.
pub struct Pass {
    pub emit: &'static str,
    pub profile: &'static str,
    pub wall: Duration,
    pub cases: Vec<Timing>,
}

impl Pass {
    /// The sum of the builds of the pass.
    pub fn built(&self) -> Duration {
        self.cases.iter().map(|t| t.built).sum()
    }

    fn ran(&self) -> Duration {
        self.cases.iter().map(|t| t.ran).sum()
    }
}

/// How many of the slowest builds and runs the report lists.
const SLOWEST: usize = 10;

/// Writes `times.tsv` under `out_root`, prints each pass and the slowest
/// cases, and adds the passes to the CI job summary.
pub fn report(out_root: &Path, passes: &[Pass], jobs: usize) {
    let mut tsv = String::from("emit\tprofile\tcase\tbuild_ms\trun_ms\n");
    for pass in passes {
        for t in &pass.cases {
            let _ = writeln!(
                tsv,
                "{}\t{}\t{}\t{}\t{}",
                pass.emit,
                pass.profile,
                t.case.display(),
                t.built.as_millis(),
                t.ran.as_millis()
            );
        }
    }
    let _ = fs::create_dir_all(out_root);
    let _ = fs::write(out_root.join("times.tsv"), tsv);

    let lines: Vec<String> = passes
        .iter()
        .map(|pass| {
            format!(
                "e2e {} {}: {} cases in {:.1}s on {jobs} jobs; builds {:.1}s, runs {:.1}s in all",
                pass.emit,
                pass.profile,
                pass.cases.len(),
                pass.wall.as_secs_f64(),
                pass.built().as_secs_f64(),
                pass.ran().as_secs_f64(),
            )
        })
        .collect();
    for line in &lines {
        println!("{line}");
    }
    let mut slowest: Vec<(&Pass, &Timing)> = passes
        .iter()
        .flat_map(|pass| pass.cases.iter().map(move |t| (pass, t)))
        .collect();
    slowest.sort_by_key(|(_, t)| Reverse(t.built + t.ran));
    for (pass, t) in slowest.iter().take(SLOWEST) {
        println!(
            "  slow: {} ({} {}): build {:.2}s, run {:.2}s",
            t.case.display(),
            pass.emit,
            pass.profile,
            t.built.as_secs_f64(),
            t.ran.as_secs_f64()
        );
    }
    if let Ok(summary) = env::var("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
    {
        for line in &lines {
            let _ = writeln!(file, "{line}");
        }
    }
}
