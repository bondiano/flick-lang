//! Projection summaries: an exported value that is a projection, a pure
//! function of one parameter reading a path of its fields (`.address.city`
//! and `fun r -> r.address.city` lower to the same lambda), says so in its
//! module's exports. Another module of the build reads that path in place
//! of a call from the summary alone, never from a body guessed by name.
//!
//! A summary carries the [`PROJECTION_VERSION`] it was written with; a
//! reader that knows another version ignores it, and the use stays a call.

use serde::{Deserialize, Serialize};

use crate::{Expr, ExprKind, Param, Pat, Stage, TypeRef};

/// The version of [`Projection`] this crate writes and reads.
pub const PROJECTION_VERSION: u32 = 1;

/// The longest path of fields a projection reads. Each step is one field
/// read, about the size of the call it replaces, so a use grows by at most
/// this many reads however many uses there are.
pub const PROJECTION_DEPTH: usize = 8;

/// The fields an exported projection reads, first one first, with the
/// types code generation needs where it reads them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    /// The [`PROJECTION_VERSION`] the summary was written with.
    pub version: u32,
    pub path: Vec<String>,
    /// The type parameters of the value, which `param` and `result` name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_params: Vec<String>,
    /// The type of the parameter, the record read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub param: Option<TypeRef>,
    /// The type of the last field read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<TypeRef>,
}

/// The labels a projection of `params` and `body` reads, the first one
/// read first: one pure parameter that binds a name, and a body of 1 to
/// [`PROJECTION_DEPTH`] field reads over it, with nothing else (no
/// dictionaries, `o?`, calls, branches or captures).
pub fn projection_path(params: &[Param], body: &Expr) -> Option<Vec<String>> {
    let [param] = params else {
        return None;
    };
    let Pat::Bind(x) = &param.pat else {
        return None;
    };
    if param.stage != Stage::Pure {
        return None;
    }
    let mut labels = Vec::new();
    let mut at = body;
    while let ExprKind::Field(inner, label) = &at.kind {
        if labels.len() == PROJECTION_DEPTH {
            return None;
        }
        labels.push(label.clone());
        at = inner;
    }
    let reads_param = matches!(&at.kind, ExprKind::Local(l) if l == x);
    (reads_param && !labels.is_empty()).then(|| {
        labels.reverse();
        labels
    })
}

impl Projection {
    /// The summary of a value with `type_params` and no dictionaries whose
    /// `body` is a projection.
    pub fn of(type_params: &[String], body: &Expr) -> Option<Projection> {
        let ExprKind::Lambda { params, body } = &body.kind else {
            return None;
        };
        Some(Projection {
            version: PROJECTION_VERSION,
            path: projection_path(params, body)?,
            type_params: type_params.to_vec(),
            param: params[0].ty.clone(),
            result: body.ty.clone(),
        })
    }

    /// Whether this crate reads the summary: it was written with
    /// [`PROJECTION_VERSION`] and its path is within the bound.
    pub fn readable(&self) -> bool {
        self.version == PROJECTION_VERSION
            && !self.path.is_empty()
            && self.path.len() <= PROJECTION_DEPTH
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod tests;
