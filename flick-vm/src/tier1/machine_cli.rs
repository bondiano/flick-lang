//! Thin tier-1 dispatch adapter for [`CliIntrinsic`], delegating all CLI
//! parsing, help, completions, and runner transitions to [`crate::stdlib::cli`].

use flick_core::ir::memory::CliIntrinsic;

use crate::stdlib::cli::{self, CliRunState, CliStep};
use crate::stdlib::{FuelCounter, HostContext};
use crate::tier0::Error;

use super::machine::{End, Machine, RegVal};

impl Machine<'_, '_> {
    pub(super) fn exec_cli(
        &mut self,
        dst: usize,
        op: CliIntrinsic,
        args: &[usize],
    ) -> Result<(), Error> {
        self.stage_slots(args)?;
        if op == CliIntrinsic::RunVersion {
            let mut hooks = HostContext::new(&mut self.fuel, self.host_mode, &mut self.stdout)
                .with_stderr(&mut self.stderr)
                .with_program_args(&self.program_args)
                .with_host_services(&mut self.host_services);
            let step = cli::eval_run_version(self.heap, &self.scratch, &mut hooks)?;
            self.release_operands()?;
            return self.drive_cli_step(step, dst);
        }
        let value = cli::eval_builder(
            self.heap,
            op,
            &self.scratch,
            &mut FuelCounter(&mut self.fuel),
        )?;
        self.finish_primitive(dst, value)
    }

    pub(super) fn complete_cli_step(
        &mut self,
        state: CliRunState,
        dst: usize,
    ) -> Result<(), Error> {
        let reg = self.pending.take().expect("cli callback return");
        let returned = self.materialize_reg(reg);
        let mut hooks = HostContext::new(&mut self.fuel, self.host_mode, &mut self.stdout)
            .with_stderr(&mut self.stderr)
            .with_program_args(&self.program_args)
            .with_host_services(&mut self.host_services);
        let step = cli::resume_run(state, self.heap, returned, &mut hooks)?;
        self.drive_cli_step(step, dst)
    }

    fn drive_cli_step(&mut self, step: CliStep, dst: usize) -> Result<(), Error> {
        match step {
            CliStep::Return(value) => self.store_reg(dst, RegVal::Value(value)),
            CliStep::Raise(payload) => {
                self.scratch.push(payload);
                self.raise_ordinary()
            }
            CliStep::Call { callee, arg, state } => self.apply_reg(
                RegVal::Value(callee),
                RegVal::Value(arg),
                End::CliStep { state, dst },
                false,
            ),
            CliStep::InvokeHandler { handler, options } => self.apply_reg(
                RegVal::Value(handler),
                RegVal::Value(options),
                End::Store(dst),
                false,
            ),
        }
    }
}
