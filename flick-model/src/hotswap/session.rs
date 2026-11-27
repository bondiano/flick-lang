//! Live session semantics for hot swap and module loading.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use super::migrate::eval_pure_term;
use super::types::{
    ModuleDef, SessionError, SessionSnapshot, StateCellEntry, StepResult, SwapError, SwapRequest,
    render_term,
};
use crate::check::compat::{check_scheme_compatibility, check_swap_value, is_closed_source_term};
use crate::check::{Check, Checker, check_config, error};
use crate::decls::Decls;
use crate::eval::{Control, Machine, MachineState, Outcome, Program};
use crate::syntax::{Const, Name, Path, Term};
use crate::types::{Scheme, Stage, Type};

#[derive(Clone, Debug)]
struct ActiveEntry {
    control: Control,
    expected_ty: Type,
    initial_stage: Stage,
}

/// A live λFlick session with global cells, declared state cells, steppable
/// small-step execution, compatible hot swaps, and atomic state migrations.
pub struct LiveSession {
    pub(super) decls: Decls,
    pub(super) program: Program,
    pub(super) state: MachineState,
    active: Option<ActiveEntry>,
    pub(super) state_cells: BTreeMap<Path, StateCellEntry>,
    loaded_modules: Vec<Name>,
}

impl LiveSession {
    /// Creates a new live session initialized with base declarations (typically
    /// Prelude / fixture declarations) and execution fuel.
    #[must_use]
    pub fn new(decls: Decls, program: Program, fuel: u64) -> Self {
        let state = MachineState {
            fuel,
            ..MachineState::default()
        };
        Self {
            decls,
            program,
            state,
            active: None,
            state_cells: BTreeMap::new(),
            loaded_modules: Vec::new(),
        }
    }

    /// Returns the current declarations table (`Sig`).
    #[must_use]
    pub fn decls(&self) -> &Decls {
        &self.decls
    }

    /// Returns the current global definitions table (`Roots`).
    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Returns the current store `μ` (containing both old and migrated versions).
    #[must_use]
    pub fn mem(&self) -> &[Term] {
        &self.state.mem
    }

    /// Returns the loaded module names in load order.
    #[must_use]
    pub fn loaded_modules(&self) -> &[Name] {
        &self.loaded_modules
    }

    /// Returns metadata for all declared state cells.
    #[must_use]
    pub fn state_cells(&self) -> &BTreeMap<Path, StateCellEntry> {
        &self.state_cells
    }

    /// Returns the current store location of a declared state cell.
    #[must_use]
    pub fn state_cell_loc(&self, path: &Path) -> Option<usize> {
        self.state_cells.get(path).map(|e| e.current_loc)
    }

    /// Returns the current value held by a declared state cell.
    #[must_use]
    pub fn state_cell_value(&self, path: &Path) -> Option<&Term> {
        let loc = self.state_cell_loc(path)?;
        self.state.mem.get(loc)
    }

    /// Returns `true` if an entry expression is currently paused mid-execution.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.active.is_some()
    }

    /// Reconstructs the current plugged configuration term `E[e]` when running.
    #[must_use]
    pub fn active_configuration(&self) -> Option<Term> {
        self.active
            .as_ref()
            .map(|a| Machine::plug_control(&self.state.frames, &a.control))
    }

    /// Captures a comparable snapshot of the entire session state.
    #[must_use]
    pub fn snapshot(&self) -> SessionSnapshot {
        let mem_rendered = self.state.mem.iter().map(render_term).collect();
        let region_live = self.state.lambda.regions.iter().map(|r| r.live).collect();
        let active_config_rendered = self.active_configuration().map(|t| render_term(&t));
        let global_schemes = self
            .decls
            .values
            .iter()
            .map(|(p, s)| (p.clone(), s.clone()))
            .collect();
        let global_values_rendered = self
            .program
            .globals
            .iter()
            .map(|(p, v)| (p.clone(), render_term(v)))
            .collect();
        SessionSnapshot {
            mem_rendered,
            region_live,
            active_config_rendered,
            state_cells: self.state_cells.clone(),
            global_schemes,
            global_values_rendered,
        }
    }

    /// Loads a module into the live session atomically: checks all globals
    /// (enforcing swap compatibility for any already-declared path) and pure
    /// state-cell initializers before committing any change.
    ///
    /// # Errors
    /// Returns [`SessionError`] if any global or state cell fails validation or
    /// initialization, leaving the session unchanged.
    pub fn load_module(&mut self, module: ModuleDef) -> Result<(), SessionError> {
        let mut candidate_decls = self.decls.clone();
        for g in &module.globals {
            if let Some(old_scheme) = self.decls.values.get(&g.path) {
                check_scheme_compatibility(&self.decls, &g.scheme, old_scheme)
                    .map_err(|err| SessionError::Swap(SwapError::Incompatible(err)))?;
            }
            candidate_decls
                .values
                .insert(g.path.clone(), g.scheme.clone());
        }
        for g in &module.globals {
            check_swap_value(&candidate_decls, &g.path, &g.scheme, &g.value)
                .map_err(|err| SessionError::Swap(SwapError::IllTypedReplacement(err)))?;
        }
        let mut candidate_program = self.program.clone();
        for g in &module.globals {
            candidate_program
                .globals
                .insert(g.path.clone(), g.value.clone());
        }

        let mut initialized = Vec::with_capacity(module.state_cells.len());
        let mut seen_cells = BTreeSet::new();
        for cell in &module.state_cells {
            if self.state_cells.contains_key(&cell.path) || !seen_cells.insert(cell.path.clone()) {
                return Err(SessionError::DuplicateStateCell(cell.path.clone()));
            }
            if !is_closed_source_term(&cell.init) {
                return Err(SessionError::StateInitFailed {
                    cell: cell.path.clone(),
                    reason: "initializer must be a closed pure source term".into(),
                });
            }
            let expected = Scheme::new(Vec::new(), cell.ty.clone());
            Checker::new(&candidate_decls)
                .check_top(&cell.init, &expected)
                .map_err(SessionError::Type)?;
            let val = eval_pure_term(
                &candidate_program,
                &self.state.mem,
                cell.init.clone(),
                self.state.fuel.min(10_000),
            )
            .map_err(|reason| SessionError::StateInitFailed {
                cell: cell.path.clone(),
                reason: reason.to_string(),
            })?;
            initialized.push((cell.path.clone(), cell.ty.clone(), val));
        }

        self.decls = candidate_decls;
        self.program = candidate_program;
        for (path, ty, val) in initialized {
            let loc = self.state.mem.len();
            self.state.mem.push(val);
            self.state_cells.insert(
                path,
                StateCellEntry {
                    current_loc: loc,
                    current_ty: ty.clone(),
                    history: vec![(loc, ty)],
                },
            );
        }
        self.loaded_modules.push(module.name);
        Ok(())
    }

    /// Starts executing `entry` (checked against `expected_ty` in the current
    /// store and declarations) so it can be stepped and interleaved with swaps
    /// and migrations.
    ///
    /// # Errors
    /// Returns [`SessionError`] if another entry is already active or if
    /// `entry` fails configuration typing.
    pub fn start_entry(&mut self, entry: Term, expected_ty: Type) -> Result<Stage, SessionError> {
        if self.active.is_some() {
            return Err(SessionError::ActiveExecutionInProgress);
        }
        let initial_stage = check_config(
            &self.decls,
            &self.state.mem,
            &self.state.lambda,
            &entry,
            &expected_ty,
        )
        .map_err(SessionError::Type)?;
        self.state.frames.clear();
        self.active = Some(ActiveEntry {
            control: Control::Eval(entry),
            expected_ty,
            initial_stage: initial_stage.clone(),
        });
        Ok(initial_stage)
    }

    /// Advances the active execution by up to `max_steps` small steps, checking
    /// configuration typing (`Preservation`) after every step when `verify` is
    /// `true`.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if configuration typing fails at any intermediate
    /// state.
    pub fn step_n(&mut self, max_steps: usize, verify_each_step: bool) -> Check<StepResult> {
        if self.active.is_none() {
            return Ok(StepResult::Idle);
        }
        let mut steps_taken = 0;
        for _ in 0..max_steps {
            let Some(mut active) = self.active.take() else {
                break;
            };
            let state = mem::take(&mut self.state);
            let mut machine = Machine::from_state(&self.program, state);
            let step = machine.step_once(active.control);
            self.state = machine.into_state();
            steps_taken += 1;
            match step {
                Ok(next_control) => {
                    active.control = next_control;
                    if verify_each_step {
                        let cfg_term = Machine::plug_control(&self.state.frames, &active.control);
                        let step_stage = check_config(
                            &self.decls,
                            &self.state.mem,
                            &self.state.lambda,
                            &cfg_term,
                            &active.expected_ty,
                        )?;
                        if !step_stage.within(&active.initial_stage) {
                            let initial_stage = active.initial_stage.clone();
                            self.active = Some(active);
                            return error(
                                "Preservation",
                                format!(
                                    "stage `{step_stage}` exceeds initial stage `{initial_stage}`"
                                ),
                            );
                        }
                    }
                    self.active = Some(active);
                }
                Err(outcome) => {
                    self.state.frames.clear();
                    return Ok(StepResult::Finished {
                        steps_taken,
                        outcome,
                    });
                }
            }
        }
        Ok(StepResult::Running { steps_taken })
    }

    /// Runs the active entry to completion, checking configuration typing on
    /// every step when `verify_each_step` is `true`.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if configuration typing fails at any step.
    pub fn run_to_end(&mut self, verify_each_step: bool) -> Check<Option<Outcome>> {
        while self.active.is_some() {
            let budget = usize::try_from(self.state.fuel.saturating_add(1)).unwrap_or(usize::MAX);
            match self.step_n(budget.min(64), verify_each_step)? {
                StepResult::Running { .. } => {}
                StepResult::Finished { outcome, .. } => return Ok(Some(outcome)),
                StepResult::Idle => return Ok(None),
            }
        }
        Ok(None)
    }

    /// Checks configuration typing of the active execution (if any) and every
    /// declared state cell version in the store.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if the active configuration or any declared
    /// state cell fails typing against the current declarations and store.
    pub fn check_configuration(&self) -> Check<Option<Stage>> {
        for (path, entry) in &self.state_cells {
            for (loc, ty) in &entry.history {
                let Some(val) = self.state.mem.get(*loc) else {
                    return error(
                        "Store",
                        format!("state cell `{path}` location ℓ{loc} out of bounds"),
                    );
                };
                if !val.is_value() {
                    return error(
                        "Store",
                        format!("state cell `{path}` location ℓ{loc} does not hold a value"),
                    );
                }
                check_config(
                    &self.decls,
                    &self.state.mem,
                    &self.state.lambda,
                    &Term::app(Term::konst(Const::RefGet), Term::Loc(*loc)),
                    ty,
                )?;
            }
        }
        let Some(active) = &self.active else {
            return Ok(None);
        };
        let cfg_term = Machine::plug_control(&self.state.frames, &active.control);
        let stage = check_config(
            &self.decls,
            &self.state.mem,
            &self.state.lambda,
            &cfg_term,
            &active.expected_ty,
        )?;
        if !stage.within(&active.initial_stage) {
            return error(
                "Preservation",
                format!(
                    "stage `{stage}` exceeds initial stage `{}`",
                    active.initial_stage
                ),
            );
        }
        Ok(Some(stage))
    }

    /// Attempts a hot swap replacing global cell `request.target` with a closed
    /// value of a compatible scheme.
    ///
    /// Old frames and closures retain their already-captured code; subsequent
    /// evaluations of `Term::Global(target)` enter `request.new_value`.
    /// On error, the session is completely unchanged.
    ///
    /// # Errors
    /// Returns [`SwapError`] if `target` does not exist, `new_scheme` does not
    /// subsume the old scheme, or `new_value` fails closed-value typing.
    pub fn swap(&mut self, request: SwapRequest) -> Result<(), SwapError> {
        let Some(old_scheme) = self.decls.values.get(&request.target) else {
            return Err(SwapError::UnknownGlobal(request.target));
        };
        check_scheme_compatibility(&self.decls, &request.new_scheme, old_scheme)
            .map_err(SwapError::Incompatible)?;

        let mut candidate_decls = self.decls.clone();
        candidate_decls
            .values
            .insert(request.target.clone(), request.new_scheme.clone());
        check_swap_value(
            &candidate_decls,
            &request.target,
            &request.new_scheme,
            &request.new_value,
        )
        .map_err(SwapError::IllTypedReplacement)?;

        self.decls = candidate_decls;
        self.program
            .globals
            .insert(request.target, request.new_value);
        Ok(())
    }

    /// Evaluates a fresh expression against the current globals and store
    /// without disturbing any paused active entry.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if `expr` fails configuration typing at `expected_ty`.
    pub fn eval_call(&self, expr: Term, expected_ty: &Type, fuel: u64) -> Check<Outcome> {
        check_config(
            &self.decls,
            &self.state.mem,
            &self.state.lambda,
            &expr,
            expected_ty,
        )?;
        let scratch = MachineState {
            mem: self.state.mem.clone(),
            lambda: self.state.lambda.clone(),
            frames: Vec::new(),
            roots: self.state.roots.clone(),
            stdout: String::new(),
            fuel,
            handlers: self.state.handlers,
        };
        let mut machine = Machine::from_state(&self.program, scratch);
        Ok(machine.run(expr))
    }

    /// Evaluates a `Ref.get` on the current location of declared state cell
    /// `cell`, verifying it against the cell's current type.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if `cell` is unknown or fails typing.
    pub fn read_state(&self, cell: &Path, fuel: u64) -> Check<Outcome> {
        let Some(entry) = self.state_cells.get(cell) else {
            return error("State", format!("unknown declared state cell `{cell}`"));
        };
        let expr = Term::app(Term::konst(Const::RefGet), Term::Loc(entry.current_loc));
        self.eval_call(expr, &entry.current_ty, fuel)
    }
}
