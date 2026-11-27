//! Host console output primitives (`stdout_line`, `stderr_line`, `stdout_write`).

use crate::stdlib::hooks::VmHooks;
use crate::tier0::{Error, Heap, Value};

pub(super) fn stderr_line(
    heap: &Heap,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let text = heap.string_lossy(value)?;
    hooks.write_stderr(&text);
    hooks.write_stderr("\n");
    Ok(Value::Unit)
}

pub(super) fn stdout_write(
    heap: &Heap,
    value: &Value,
    hooks: &mut dyn VmHooks,
) -> Result<Value, Error> {
    let text = heap.string_lossy(value)?;
    hooks.write_stdout(&text);
    Ok(Value::Unit)
}
