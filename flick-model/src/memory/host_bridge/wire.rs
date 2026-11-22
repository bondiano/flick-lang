//! Iterative flat-node wire codec matching `host_bridge/wire.mjs`.
//!
//! Both encoding and decoding use explicit heap work stacks so 20,000+ deep
//! Flick ADTs/lists never overflow the Rust call stack or `serde_json`'s
//! recursion limit, and every partial decode failure drops all allocated slots.

use crate::memory::{CellHandle, Error, Heap, ObjectId};

mod decode;
mod decode_nodes;
mod encode;
mod encode_obj;

pub(in crate::memory) use decode::{decode_cause_wire, decode_value};
pub(in crate::memory) use encode::encode_value;

/// Callback/task/owner/cell registration hooks provided by `Machine` during encoding.
pub(in crate::memory) trait WireRegistrar {
    fn register_closure(
        &mut self,
        heap: &mut Heap,
        id: ObjectId,
        arity: usize,
    ) -> Result<u64, Error>;
    fn register_task(&mut self, heap: &mut Heap, id: ObjectId) -> Result<u64, Error>;
    fn register_dict_getter(
        &mut self,
        heap: &mut Heap,
        dict_id: ObjectId,
        getter_id: ObjectId,
    ) -> Result<u64, Error>;
    fn register_cell(&mut self, cell: &CellHandle) -> u64;
    fn record_owner(&mut self, owner_index: usize);
}
