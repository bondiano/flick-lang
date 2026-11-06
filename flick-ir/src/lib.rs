//! The typed IR of the Flick compiler: a checked module with everything a
//! backend must not guess made explicit. The compiler builds it; backends,
//! the reference model and tools read it.

mod data;
mod dict;
mod instance;
mod num;
mod op;
mod span;
mod stage;

pub use data::{Constructor, EnumLit, JsClass, Rep, TypeName};
pub use dict::Dict;
pub use instance::{GenericInfo, InstanceInfo};
pub use num::{Arith, Num, NumKind};
pub use op::OpClass;
pub use span::{FileId, Span};
pub use stage::{Lift, Stage};
