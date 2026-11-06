//! The typed IR of the Flick compiler: a checked module with everything a
//! backend must not guess made explicit. The compiler builds it; backends,
//! the reference model and tools read it.

mod num;
mod op;
mod span;
mod stage;

pub use num::{Arith, Num, NumKind};
pub use op::OpClass;
pub use span::{FileId, Span};
pub use stage::Stage;
