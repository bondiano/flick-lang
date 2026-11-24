//! The typed IR of the Flick compiler: a checked module with everything a
//! backend must not guess made explicit. The compiler builds it; backends,
//! the reference model and tools read it.

pub mod bridge_plan;
mod data;
mod dict;
mod expr;
mod float;
mod foreign;
mod handler;
mod instance;
mod lang;
pub mod memory;
mod num;
mod op;
mod order;
mod program;
mod projection;
mod span;
mod stage;
mod ty;
mod validate;
mod visit;

pub use data::{Constructor, ConstructorDecl, EnumLit, JsClass, Rep, TypeDecl, TypeName};
pub use dict::Dict;
pub use expr::{
    Arm, BinOp, Exported, Exports, Expr, ExprKind, Initialization, Item, ItemKind, Label, Lit,
    Module, Param, Pat, Prefix, Reexport, Stmt, operation_arity,
};
pub use foreign::{
    Adapter, AdapterArg, BackendAdapter, Binding, Copies, Foreign, ForeignMode, InlineCode,
    ResultAdapter, Retention, Shape,
};
pub use handler::{Handled, Handler, OpArm};
pub use instance::{GenericInfo, InstanceInfo, default_method};
pub use lang::{Lang, LangItem, Namespace, STRUCTURAL};
pub use num::{Arith, Num, NumKind};
pub use op::OpClass;
pub use order::{
    apply_order, cmp_utf16, sort_by_str, sort_by_utf16, sort_indices, sort_strings_utf16,
    utf16_sorted_indices,
};
pub use program::{Error, Program, VERSION};
pub use projection::{PROJECTION_DEPTH, PROJECTION_VERSION, Projection, projection_path};
pub use span::{FileId, Span};
pub use stage::{Lift, Stage};
pub use ty::{Constraint, IrType, TypeInterner, TypeRef};
pub use validate::validate;
