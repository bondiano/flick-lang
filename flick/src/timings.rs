//! `--timings`: how long the phases of a command and its slowest modules
//! took and why the build cache missed, collected from the `tracing` events
//! the compiler and the commands emit. Without the flag nothing subscribes,
//! so each of those events costs a disabled callsite.

use std::fmt::{Debug, Write as _};
use std::io::{self, Write as _};
use std::mem;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use flick_core::diagnostics::render::SCHEMA_VERSION;
use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::subscriber::set_global_default;
use tracing::{Event, Subscriber, debug};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry;

use crate::args::{DiagnosticsFormat, GlobalArguments};

/// How many of the slowest modules a person reads.
const SLOWEST: usize = 10;

/// What the events said since the last report.
struct Records {
    /// Each phase a command announced, with when it began.
    stages: Vec<(String, Instant)>,
    /// Work measured inside the stages, in microseconds.
    phases: Vec<(String, u64)>,
    /// Modules checked, in microseconds.
    modules: Vec<(String, u64)>,
    misses: Vec<String>,
}

static RECORDS: Mutex<Records> = Mutex::new(Records {
    stages: Vec::new(),
    phases: Vec::new(),
    modules: Vec::new(),
    misses: Vec::new(),
});

fn records() -> MutexGuard<'static, Records> {
    RECORDS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts collecting: the events below are recorded from now on.
pub fn install() {
    records()
        .stages
        .push(("Starting".to_owned(), Instant::now()));
    // a subscriber already set keeps its events: there is nothing to time
    let _ = set_global_default(registry().with(Collect));
}

/// A phase of the command begins; it lasts until the next one or the report.
pub fn stage(label: &'static str) {
    debug!(stage = label);
}

/// Work inside a stage that took `elapsed`.
pub fn phase(label: &str, elapsed: Duration) {
    debug!(micros = micros(elapsed), "{label}");
}

/// `duration` in whole microseconds, the unit of a timing event.
fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

struct Collect;

impl<S: Subscriber> Layer<S> for Collect {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut records = records();
        match fields {
            Fields {
                stage: Some(stage), ..
            } => records.stages.push((stage, Instant::now())),
            Fields {
                miss: Some(miss), ..
            } => records.misses.push(miss),
            Fields {
                module: Some(module),
                micros: Some(micros),
                ..
            } => records.modules.push((module, micros)),
            Fields {
                message: Some(message),
                micros: Some(micros),
                ..
            } => records.phases.push((message, micros)),
            Fields { .. } => {}
        }
    }
}

/// The fields of an event that say what was timed.
#[derive(Default)]
struct Fields {
    stage: Option<String>,
    miss: Option<String>,
    module: Option<String>,
    message: Option<String>,
    micros: Option<u64>,
}

impl Visit for Fields {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "micros" {
            self.micros = Some(value);
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_debug(field, &format_args!("{value}"));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        let slot = match field.name() {
            "stage" => &mut self.stage,
            "miss" => &mut self.miss,
            "module" => &mut self.module,
            "message" => &mut self.message,
            _ => return,
        };
        *slot = Some(format!("{value:?}"));
    }
}

/// What a command took, each time in microseconds.
#[derive(Debug, PartialEq)]
struct Report {
    stages: Vec<(String, u64)>,
    phases: Vec<(String, u64)>,
    /// The slowest first.
    modules: Vec<(String, u64)>,
    misses: Vec<String>,
}

impl Report {
    /// The records of a command that ended at `end`.
    fn of(records: Records, end: Instant) -> Self {
        let ends = (records.stages.iter().skip(1))
            .map(|(_, began)| *began)
            .chain([end]);
        let stages = (records.stages.iter().zip(ends))
            .map(|((label, began), end)| {
                (label.clone(), micros(end.saturating_duration_since(*began)))
            })
            .collect();
        let mut modules = records.modules;
        modules.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Self {
            stages,
            phases: records.phases,
            modules,
            misses: records.misses,
        }
    }

    fn human(&self) -> String {
        let mut out = String::from("Timings\n");
        let line = |out: &mut String, (label, micros): &(String, u64)| {
            let millis = Duration::from_micros(*micros).as_secs_f64() * 1000.0;
            let _ = writeln!(out, "{millis:>10.1} ms  {label}");
        };
        self.stages.iter().for_each(|stage| line(&mut out, stage));
        if !self.phases.is_empty() {
            out.push_str("Within them\n");
            self.phases.iter().for_each(|phase| line(&mut out, phase));
        }
        if !self.modules.is_empty() {
            let _ = writeln!(out, "Slowest of {} modules checked", self.modules.len());
            (self.modules.iter().take(SLOWEST)).for_each(|module| line(&mut out, module));
        }
        for miss in &self.misses {
            let _ = writeln!(out, "Build cache missed: {miss}");
        }
        out
    }

    /// One event of the `--diagnostics json` stream.
    fn json(&self) -> String {
        let timed = |key: &'static str, list: &[(String, u64)]| -> Vec<serde_json::Value> {
            (list.iter())
                .map(|(name, micros)| json!({ key: name, "micros": micros }))
                .collect()
        };
        let event = json!({
            "schema-version": SCHEMA_VERSION,
            "type": "timings",
            "stages": timed("label", &self.stages),
            "phases": timed("label", &self.phases),
            "modules": timed("module", &self.modules),
            "cache-misses": self.misses,
        });
        format!("{event}\n")
    }
}

/// With `--timings`, writes what was collected since the last report on
/// stderr and starts anew: a watching command reports each of its runs,
/// and keeps the records of none.
pub fn report(global: &GlobalArguments) {
    if !global.timings {
        return;
    }
    let collected = {
        let mut records = records();
        let empty = Records {
            stages: Vec::new(),
            phases: Vec::new(),
            modules: Vec::new(),
            misses: Vec::new(),
        };
        mem::replace(&mut *records, empty)
    };
    let report = Report::of(collected, Instant::now());
    let out = match global.diagnostics {
        DiagnosticsFormat::Human => report.human(),
        DiagnosticsFormat::Json => report.json(),
    };
    // stderr closed: nothing useful left to do with the error
    let _ = io::stderr().lock().write_all(out.as_bytes());
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Records, Report};

    #[test]
    fn a_stage_lasts_until_the_next_and_the_slowest_module_leads() {
        let began = Instant::now();
        let at = |millis| began + Duration::from_millis(millis);
        let records = Records {
            stages: vec![
                ("Starting".to_owned(), at(0)),
                ("Checking".to_owned(), at(2)),
            ],
            phases: vec![("built-in modules checked".to_owned(), 1500)],
            modules: vec![("A".to_owned(), 10), ("B".to_owned(), 4000)],
            misses: vec!["`config` differs".to_owned()],
        };
        let report = Report::of(records, at(7));
        assert_eq!(
            report.stages,
            [("Starting".to_owned(), 2000), ("Checking".to_owned(), 5000)]
        );
        assert_eq!(
            report.human(),
            "Timings
       2.0 ms  Starting
       5.0 ms  Checking
Within them
       1.5 ms  built-in modules checked
Slowest of 2 modules checked
       4.0 ms  B
       0.0 ms  A
Build cache missed: `config` differs
"
        );
        let json: serde_json::Value = serde_json::from_str(&report.json()).expect("one object");
        assert_eq!(json["type"], "timings");
        assert_eq!(json["modules"][0]["module"], "B");
        assert_eq!(json["stages"][1]["micros"], 5000);
    }
}
