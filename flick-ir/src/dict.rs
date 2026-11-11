//! The dictionaries class constraints resolve to.

use serde::{Deserialize, Serialize};

/// The dictionary a class constraint resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dict {
    /// An instance applied to the dictionaries of its context.
    Instance {
        module: String,
        name: String,
        args: Vec<Dict>,
    },
    /// A constraint of the enclosing signature or instance.
    Given(String),
    /// A named dictionary exported by a static module argument.
    Value { module: String, name: String },
    /// A dictionary field of a lexically opened `Module Sig` value.
    Opened { binder: String, field: String },
    /// A dictionary a matched value holds, bound to this local by the
    /// pattern of its constructor.
    Field(String),
    /// The superclass `class` of another dictionary.
    Super { dict: Box<Dict>, class: String },
    /// The compiler-owned instance of a record or tuple, by field.
    Structural { class: String, fields: Vec<Dict> },
    /// A row primitive, solved by the compiler: the constraint as shown, and
    /// the labels its operation needs at run time (those `Project` keeps).
    Primitive {
        constraint: String,
        labels: Vec<String>,
    },
    /// `CallSite` of a use the checker made: its place and definition.
    Site { place: String, definition: String },
    /// A constraint on a type that already produced a diagnostic.
    Error,
}

impl Dict {
    /// Each top-level name the dictionary stands on, as its module and
    /// name: an instance (`true`) or a named dictionary value.
    pub fn each_global(&self, f: &mut impl FnMut(bool, &str, &str)) {
        match self {
            Dict::Instance { module, name, args } => {
                f(true, module, name);
                for a in args {
                    a.each_global(f);
                }
            }
            Dict::Value { module, name } => f(false, module, name),
            Dict::Super { dict, .. } => dict.each_global(f),
            Dict::Structural { fields, .. } => {
                for d in fields {
                    d.each_global(f);
                }
            }
            Dict::Opened { .. }
            | Dict::Given(_)
            | Dict::Field(_)
            | Dict::Primitive { .. }
            | Dict::Site { .. }
            | Dict::Error => {}
        }
    }
}
