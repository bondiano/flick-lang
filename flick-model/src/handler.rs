//! The handler of effects of flick-core (`types/handle.rs`) in λFlick: per
//! effect the function of each operation's arms, the body, its value
//! through `ret` and its errors through `errors`; and what exists only while
//! one runs: the rest of a body a `control` arm resumes, once, and what
//! unwinds a body to its handler.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

pub use flick_core::types::OpClass;

use crate::eval::Frame;
use crate::syntax::{Arm, Name, Path, Term};

#[derive(Clone, Debug)]
pub struct Handler {
    pub effects: Vec<Handled>,
    pub body: Term,
    /// A function of the body's value; without it the body's value is the
    /// handler's.
    pub ret: Option<Term>,
    /// The arms for the body's errors, as those of a `try`.
    pub errors: Rc<[Arm]>,
}

/// An effect a handler handles.
#[derive(Clone, Debug)]
pub struct Handled {
    pub effect: Path,
    pub ops: Vec<Op>,
}

/// The arms of one operation as a function of its dictionaries and
/// arguments, and for `control` of `resume`.
#[derive(Clone, Debug)]
pub struct Op {
    pub name: Name,
    pub class: OpClass,
    pub dicts: usize,
    pub arity: usize,
    /// An arm without a guard takes every argument.
    pub covers: bool,
    pub function: Term,
}

impl Handler {
    /// The handler with `f` of each term in it but the arms for errors,
    /// and `arms` of those.
    #[must_use]
    pub fn map(
        &self,
        f: impl Fn(&Term) -> Term,
        arms: impl Fn(&Rc<[Arm]>) -> Rc<[Arm]>,
    ) -> Handler {
        Handler {
            effects: self
                .effects
                .iter()
                .map(|h| Handled {
                    effect: h.effect.clone(),
                    ops: h
                        .ops
                        .iter()
                        .map(|op| Op {
                            function: f(&op.function),
                            ..op.clone()
                        })
                        .collect(),
                })
                .collect(),
            body: f(&self.body),
            ret: self.ret.as_ref().map(&f),
            errors: arms(&self.errors),
        }
    }

    /// Each term in it but the arms for errors.
    pub fn terms(&self) -> impl Iterator<Item = &Term> {
        let ops = self
            .effects
            .iter()
            .flat_map(|h| h.ops.iter().map(|op| &op.function));
        ops.chain([&self.body]).chain(self.ret.as_ref())
    }

    pub fn op(&self, effect: &Path, name: &str) -> Option<&Op> {
        let handled = self.effects.iter().find(|h| h.effect == *effect)?;
        handled.ops.iter().find(|op| &*op.name == name)
    }
}

/// The rest of a body under its handler, the handler's frame and those
/// above it, which a `control` arm resumes once.
#[derive(Clone)]
pub struct Cont(pub Rc<RefCell<Option<Vec<Frame>>>>);

impl fmt::Debug for Cont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let used = self.0.borrow().is_none();
        write!(f, "resume{}", if used { " (used)" } else { "" })
    }
}

/// What unwinds a body to the handler `id`: an `abort` arm to run there,
/// or the end of a body an arm let go, with the value it ends with or what
/// it goes on raising.
#[derive(Clone, Debug)]
pub enum Signal {
    Abort { id: usize, arm: Term },
    Discontinue { id: usize, then: Result<Term, Term> },
}

impl Signal {
    pub fn id(&self) -> usize {
        match self {
            Signal::Abort { id, .. } | Signal::Discontinue { id, .. } => *id,
        }
    }
}
