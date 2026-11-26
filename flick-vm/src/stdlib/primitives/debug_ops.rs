//! Counted `Flick.Debug` (`todo`, `echo`, `let assert`) implementations.

use flick_core::ir::memory::DebugIntrinsic;

use super::{Error, Fuel, Heap, Value, charge};

pub(super) fn run(
    heap: &mut Heap,
    op: DebugIntrinsic,
    args: &[Value],
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        DebugIntrinsic::Todo => {
            let msg = heap.string(&args[0])?.to_owned();
            Err(Error::Todo(msg))
        }
        DebugIntrinsic::Echo => {
            // Counted model observes stdout only (matching `HostPrimitive::StderrLine`);
            // validate place string and return an owned duplicate of the echoed value.
            let _ = heap.string(&args[0])?;
            heap.dup(&args[1])
        }
        DebugIntrinsic::Assert => {
            let msg = heap.string(&args[0])?.to_owned();
            let rendered = super::json_ops::stringify_value(heap, &args[1], fuel)?;
            Err(Error::Assert(format!("{msg}: {rendered}")))
        }
    }
}
