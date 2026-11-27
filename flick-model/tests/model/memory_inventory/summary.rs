use std::collections::BTreeMap;
use std::env;

use flick_vm::tier0::Stats;

#[derive(Default)]
pub(super) struct CorpusSummary {
    pub passed: usize,
    pub expected_failure: usize,
    pub failed_outcome: Vec<String>,
    pub out_of_fuel: Vec<String>,
    pub leaked: Vec<String>,
    pub unsupported: BTreeMap<String, usize>,
    pub compile_error: usize,
    pub no_main: usize,
    pub total_profiles: usize,
    pub tier1_checked: usize,
    pub total_allocations: usize,
    pub total_reuses: usize,
    pub total_reuse_fallbacks: usize,
    pub acyclic_reclaimed_profiles: usize,
    pub cell_teardown_profiles: usize,
}

impl CorpusSummary {
    pub fn record_stats(&mut self, stats: &Stats) {
        self.total_allocations += stats.allocations;
        self.total_reuses += stats.reuses;
        self.total_reuse_fallbacks += stats.reuse_fallbacks;
        if stats.cells_detached_on_teardown == 0 {
            self.acyclic_reclaimed_profiles += 1;
        } else {
            self.cell_teardown_profiles += 1;
        }
    }

    pub fn record_unsupported(&mut self, error: &str) {
        // Normalize span suffix so identical lowering gaps aggregate cleanly.
        let key = error
            .split(" at Span")
            .next()
            .unwrap_or(error)
            .lines()
            .next()
            .unwrap_or(error)
            .to_owned();
        *self.unsupported.entry(key).or_default() += 1;
    }

    pub fn print(&self, title: &str) {
        let unsupported_total: usize = self.unsupported.values().sum();
        println!(
            "=== {title} (profiles={}, tier1_checked={}) ===",
            self.total_profiles, self.tier1_checked
        );
        println!(
            "  passed={} expected_failure={} out_of_fuel={} failed_outcome={} leaked={} unsupported={} compile_error={} no_main={}",
            self.passed,
            self.expected_failure,
            self.out_of_fuel.len(),
            self.failed_outcome.len(),
            self.leaked.len(),
            unsupported_total,
            self.compile_error,
            self.no_main,
        );
        println!(
            "  allocations={} reuses={} reuse_fallbacks={} acyclic_reclaimed={} cell_teardown={}",
            self.total_allocations,
            self.total_reuses,
            self.total_reuse_fallbacks,
            self.acyclic_reclaimed_profiles,
            self.cell_teardown_profiles,
        );
        if !self.out_of_fuel.is_empty() {
            println!("  [out_of_fuel]: {}", self.out_of_fuel.join(", "));
        }
        if !self.failed_outcome.is_empty() {
            if env::var("FLICK_INVENTORY_DIFF").is_ok() {
                for item in &self.failed_outcome {
                    println!("  [FAIL]: {item}");
                }
            }
            let mut by_reason: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for item in &self.failed_outcome {
                let (name, rest) = item.split_once(": ").unwrap_or((item, ""));
                let key = rest.split(" expected=").next().unwrap_or(rest).to_owned();
                by_reason.entry(key).or_default().push(name.to_owned());
            }
            let mut ranked_fail: Vec<_> = by_reason.into_iter().collect();
            ranked_fail.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
            println!(
                "  [failed_outcome grouped ({} total)]:",
                self.failed_outcome.len()
            );
            for (reason, names) in ranked_fail {
                let sample: Vec<_> = names.iter().take(6).cloned().collect();
                let more = if names.len() > 6 {
                    format!(" ... +{}", names.len() - 6)
                } else {
                    String::new()
                };
                println!(
                    "    {:4}  {reason}  [{}]{more}",
                    names.len(),
                    sample.join(", ")
                );
            }
        }
        if !self.leaked.is_empty() {
            for item in &self.leaked {
                println!("  [LEAK]: {item}");
            }
        }
        let mut ranked: Vec<_> = self.unsupported.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        if !ranked.is_empty() {
            println!("  [top unsupported reasons]:");
            for (reason, count) in ranked.into_iter().take(25) {
                println!("    {count:4}  {reason}");
            }
        }
    }
}
