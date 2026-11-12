//! The typed IR of the Flick compiler: a checked module with everything a
//! backend must not guess made explicit. The compiler builds it; backends,
//! the reference model and tools read it.

mod data;
mod dict;
mod expr;
mod float;
mod foreign;
mod handler;
mod instance;
mod lang;
mod num;
mod op;
mod program;
mod span;
mod stage;
mod validate;
mod visit;

pub use data::{Constructor, EnumLit, JsClass, Rep, TypeName};
pub use dict::Dict;
pub use expr::{
    Arm, BinOp, Expr, ExprKind, Initialization, Item, ItemKind, Label, Lit, Module, Param, Pat,
    Prefix, Stmt, operation_arity,
};
pub use foreign::{Adapter, Binding, Copies, Foreign, ForeignMode, ResultAdapter, Shape};
pub use handler::{Handled, Handler, OpArm};
pub use instance::{GenericInfo, InstanceInfo};
pub use lang::{LangItem, Namespace};
pub use num::{Arith, Num, NumKind};
pub use op::OpClass;
pub use program::{Error, Program, VERSION};
pub use span::{FileId, Span};
pub use stage::{Lift, Stage};
pub use validate::validate;
