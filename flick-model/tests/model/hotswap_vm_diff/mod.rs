//! Differential tests comparing `#989`'s `flick-model` hot-swap oracle against
//! `flick-vm` live sessions running on BOTH Tier 0 and Tier 1 (with peephole
//! fusion, bytecode verification, and `RegionInvalidator` hooks).

mod compile;
mod types;

use std::sync::{Arc, Mutex};

use flick_core::ir::memory::Name;
use flick_model::hotswap::{
    ActionObservation, SessionAction, SessionOracleReport, SessionOutcomeSummary, SessionScenario,
    generate_session, run_session_oracle,
};
use flick_vm::session::{
    EvalOutcome, ExecutionTier, RegionInvalidator, StateCellSpec, StepEvalOutcome, SwapBinding,
    SwapDefinitionsRequest, VmSession,
};
use flick_vm::tier0::HostMode;
use proptest::collection::vec;
use proptest::prelude::*;

use crate::props::GEN;
use compile::{compile_entry, compile_module, compile_single_global, convert_migration_batch};
use types::{convert_scheme, convert_type, path_to_name};

struct CountingInvalidator {
    invocations: Arc<Mutex<Vec<Vec<Name>>>>,
}

impl RegionInvalidator for CountingInvalidator {
    fn invalidate(&mut self, bindings: &[Name]) {
        self.invocations
            .lock()
            .expect("invalidator lock")
            .push(bindings.to_vec());
    }
}

#[allow(clippy::too_many_lines)]
fn replay_on_vm(scenario: &SessionScenario, oracle: &SessionOracleReport, tier: ExecutionTier) {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    session.set_tier(tier);
    let inv_log = Arc::new(Mutex::new(Vec::new()));
    session.register_region_invalidator(Box::new(CountingInvalidator {
        invocations: Arc::clone(&inv_log),
    }));

    for module in &scenario.initial_modules {
        let prog = compile_module(module);
        let init_entries: Vec<Name> = module
            .globals
            .iter()
            .map(|g| path_to_name(&g.path))
            .chain(module.state_cells.iter().map(|c| path_to_name(&c.path)))
            .collect();
        assert_eq!(
            session.eval_declarations(&prog, &module.name, &init_entries, None, None),
            EvalOutcome::Declaration {
                stdout: String::new()
            }
        );
        for g in &module.globals {
            session.register_binding_scheme(path_to_name(&g.path), convert_scheme(&g.scheme));
        }
        for c in &module.state_cells {
            session
                .declare_state_cell(StateCellSpec {
                    name: path_to_name(&c.path),
                    ty: convert_type(&c.ty),
                    is_ref: true,
                })
                .expect("declare state cell");
        }
    }

    let entry_prog = compile_entry(&scenario.entry);
    let task_id = session
        .start_step_eval_on_tier(&entry_prog, &Name::new("Entry", "main"), tier)
        .expect("start entry task");
    let mut entry_finished: Option<EvalOutcome> = None;
    let initial_pause_steps = match tier {
        ExecutionTier::Tier0 => 9,
        ExecutionTier::Tier1 => 8,
    };
    let paused = session
        .step_eval(task_id, initial_pause_steps)
        .expect("initial pause");
    assert!(
        matches!(paused, StepEvalOutcome::Running { .. }),
        "entry must pause mid-driver on {tier:?}"
    );

    let mut call_counter = 0usize;
    let mut expected_invalidations = 0usize;
    for (idx, (action, expected_obs)) in scenario
        .actions
        .iter()
        .zip(&oracle.observations)
        .enumerate()
    {
        match (action, expected_obs) {
            (SessionAction::Step { .. }, ActionObservation::Stepped { finished, .. }) => {
                if *finished && entry_finished.is_none() {
                    entry_finished = Some(
                        session
                            .finish_step_eval(task_id)
                            .expect("finish stepped entry"),
                    );
                }
            }
            (SessionAction::Swap { request, kind }, ActionObservation::SwapAccepted { .. }) => {
                let target = path_to_name(&request.target);
                let incoming = compile_single_global(target.clone(), &request.new_value);
                let report = session
                    .swap_definitions(SwapDefinitionsRequest::new(
                        incoming,
                        vec![SwapBinding {
                            name: target,
                            new_scheme: Some(convert_scheme(&request.new_scheme)),
                        }],
                    ))
                    .unwrap_or_else(|err| {
                        panic!(
                            "action #{idx} ({tier:?}, {kind:?}): VM rejected swap accepted by oracle: {err}"
                        )
                    });
                assert!(report.bytecode_verified);
                expected_invalidations += 1;
            }
            (SessionAction::Swap { request, kind }, ActionObservation::SwapRejected { .. }) => {
                let slots_before = session.heap_inventory().total;
                let target = path_to_name(&request.target);
                let incoming = compile_single_global(target.clone(), &request.new_value);
                let res = session.swap_definitions(SwapDefinitionsRequest::new(
                    incoming,
                    vec![SwapBinding {
                        name: target,
                        new_scheme: Some(convert_scheme(&request.new_scheme)),
                    }],
                ));
                assert!(
                    res.is_err(),
                    "action #{idx} ({tier:?}, {kind:?}): VM accepted swap rejected by oracle"
                );
                assert_eq!(session.heap_inventory().total, slots_before);
            }
            (
                SessionAction::Migrate { batch, kind },
                ActionObservation::MigrationCommitted { .. },
            ) => {
                let req = convert_migration_batch(batch);
                let report = session.migrate_state_cells(&req).unwrap_or_else(|err| {
                    panic!(
                        "action #{idx} ({tier:?}, {kind:?}): VM rolled back migration committed by oracle: {err}"
                    )
                });
                assert_eq!(report.migrated_cells.len(), batch.migrations.len());
                expected_invalidations += 1;
            }
            (
                SessionAction::Migrate { batch, kind },
                ActionObservation::MigrationRolledBack { .. },
            ) => {
                let slots_before = session.heap_inventory().total;
                let bytes_before = session.heap_live_bytes();
                let req = convert_migration_batch(batch);
                let res = session.migrate_state_cells(&req);
                assert!(
                    res.is_err(),
                    "action #{idx} ({tier:?}, {kind:?}): VM committed migration rolled back by oracle"
                );
                assert_eq!(session.heap_inventory().total, slots_before);
                assert_eq!(session.heap_live_bytes(), bytes_before);
            }
            (SessionAction::Call { expr, .. }, ActionObservation::CallResult { outcome }) => {
                call_counter += 1;
                let call_name = Name::new(&format!("Call.C{call_counter}"), "it");
                let call_prog = compile_single_global(call_name.clone(), expr);
                let vm_out = session.eval_phrase_on_tier(tier, &call_prog, &call_name);
                assert_outcome_matches_oracle(&vm_out, outcome, idx, tier);
            }
            (SessionAction::ReadState { cell }, ActionObservation::StateRead { outcome, .. }) => {
                let vm_out = session
                    .read_state_cell(&path_to_name(cell))
                    .expect("read state cell");
                assert_outcome_matches_oracle(&vm_out, outcome, idx, tier);
            }
            (action, obs) => panic!("unexpected action/observation pair: {action:?} / {obs:?}"),
        }
    }

    let final_vm = match entry_finished {
        Some(out) => out,
        None => session
            .finish_step_eval(task_id)
            .expect("finish entry at session end"),
    };
    assert_outcome_matches_oracle(
        &final_vm,
        &oracle.final_outcome,
        scenario.actions.len(),
        tier,
    );
    assert_eq!(
        inv_log.lock().expect("inv log").len(),
        expected_invalidations,
        "RegionInvalidator call count mismatch on {tier:?}"
    );
    assert_eq!(
        session.shutdown(),
        Ok(()),
        "zero heap leaks required on {tier:?}"
    );
}

fn assert_outcome_matches_oracle(
    vm_outcome: &EvalOutcome,
    oracle_outcome: &SessionOutcomeSummary,
    idx: usize,
    tier: ExecutionTier,
) {
    match (vm_outcome, oracle_outcome) {
        (EvalOutcome::Value { rendered, .. }, SessionOutcomeSummary::Value(expected)) => {
            assert_eq!(
                rendered, expected,
                "action #{idx} ({tier:?}): VM value `{rendered}` != oracle `{expected}`"
            );
        }
        other => panic!("action #{idx} ({tier:?}): outcome mismatch: {other:?}"),
    }
}

#[test]
fn hot_swap_vm_tier0_and_tier1_match_model_oracle_across_deterministic_seeds() {
    GEN.with(|(decls, program)| {
        for seed in 0u32..48 {
            let choices: Vec<u32> = (0u32..40)
                .map(|i| seed.wrapping_mul(31).wrapping_add(i.wrapping_mul(7)))
                .collect();
            let scenario = generate_session(&choices);
            let oracle = run_session_oracle(decls, program, &scenario)
                .unwrap_or_else(|e| panic!("seed {seed} failed oracle: {e}"));
            replay_on_vm(&scenario, &oracle, ExecutionTier::Tier0);
            replay_on_vm(&scenario, &oracle, ExecutionTier::Tier1);
        }
    });
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "a model differential property; run `just model`"]
    fn hot_swap_vm_tier0_and_tier1_agree_with_oracle_on_generated_sessions(
        choices in vec(any::<u32>(), 0..96)
    ) {
        let scenario = generate_session(&choices);
        let oracle = GEN
            .with(|(decls, program)| run_session_oracle(decls, program, &scenario))
            .expect("oracle succeeds on generated scenario");
        replay_on_vm(&scenario, &oracle, ExecutionTier::Tier0);
        replay_on_vm(&scenario, &oracle, ExecutionTier::Tier1);
    }
}
