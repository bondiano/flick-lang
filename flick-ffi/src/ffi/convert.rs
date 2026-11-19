//! TypeScript shapes to Flick types, or the reason one has none.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::enums::{self, Literals};
use super::import::{Status, flick_name};
use super::{ExportPolicy, Param, Shape, Signature};

pub(super) struct Blocked {
    pub status: Status,
    pub reason: String,
}

impl Blocked {
    pub(super) fn unsupported(reason: String) -> Self {
        Blocked {
            status: Status::Unsupported,
            reason,
        }
    }

    pub(super) fn policy(reason: String) -> Self {
        Blocked {
            status: Status::NeedsPolicy,
            reason,
        }
    }
}

/// Where a type sits: what JS gives Flick, or what Flick gives JS.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pos {
    Result,
    Param,
}

/// Parameter types, their metadata, and notes on what is left out.
pub(super) type Params = (Vec<String>, Vec<String>, Vec<String>);

#[derive(Clone)]
pub(super) struct Convert {
    pub strict: bool,
    /// Opaque types the converted types name.
    pub handles: BTreeSet<String>,
    /// Enums the converted types name, with their literals.
    pub enums: BTreeMap<String, Literals>,
    /// Type variables the converted types use.
    pub vars: BTreeSet<String>,
    /// A parameter is a callback.
    pub callback: bool,
}

impl Blocked {
    pub(super) fn at(self, param: &str) -> Blocked {
        Blocked {
            reason: format!("parameter `{param}`: {}", self.reason),
            ..self
        }
    }
}

impl Convert {
    /// The Flick types of the parameters, their metadata and notes on what
    /// is left out: trailing optional parameters Flick cannot pass.
    pub(super) fn params(&mut self, all: &[Param]) -> Result<Params, Blocked> {
        let mut metadata = Vec::new();
        let mut notes = Vec::new();
        let mut params = all;
        while let Some((last, rest)) = params.split_last()
            && last.optional
            && self.clone().param(last).is_err()
        {
            notes.push(format!(
                "optional parameter `{}: {}` is left out",
                last.name,
                last.ty.text()
            ));
            params = rest;
        }
        let mut types = Vec::new();
        for (i, p) in params.iter().enumerate() {
            if p.rest {
                if i + 1 != params.len() {
                    return Err(Blocked::unsupported(
                        "a rest parameter before the last".into(),
                    ));
                }
                metadata.push("variadic".to_owned());
            }
            let (ty, optional) = self.param(p).map_err(|e| e.at(&p.name))?;
            if optional {
                metadata.push(format!("optional {i}"));
            }
            types.push(ty);
        }
        Ok((types, metadata, notes))
    }

    /// The Flick result, whether JS returns a Promise of it, and whether an
    /// empty JS value is `None`.
    pub(super) fn result(
        &mut self,
        shape: &Shape,
        policy: &ExportPolicy,
    ) -> Result<(String, bool, bool), Blocked> {
        let (inner, promise) = match shape {
            Shape::Promise { ty } => (ty.as_ref(), true),
            other => (other, false),
        };
        let Some(value) = nullable(inner) else {
            return Ok((self.ty(inner, Pos::Result)?, promise, false));
        };
        if policy.undefined.as_deref() == Some("none") {
            return Ok((
                format!("Option {}", atom(&self.ty(value, Pos::Result)?)),
                promise,
                true,
            ));
        }
        let empty: Vec<String> = match inner {
            Shape::Union { types, .. } => types
                .iter()
                .filter(|t| matches!(t, Shape::Undefined | Shape::Null | Shape::Void))
                .map(|t| format!("`{}`", t.text()))
                .collect(),
            _ => Vec::new(),
        };
        Err(Blocked::policy(format!(
            "it may return {}: say what that means (`undefined = \"none\"`)",
            empty.join(" or ")
        )))
    }

    /// The Flick type of a parameter, and whether `None` passes `undefined`.
    fn param(&mut self, p: &Param) -> Result<(String, bool), Blocked> {
        let mut optional = p.optional;
        let ty = match (&p.ty, p.rest) {
            (Shape::Array { element }, true) => {
                format!("List {}", atom(&self.ty(element, Pos::Param)?))
            }
            (_, true) => {
                return Err(Blocked::unsupported(
                    "a rest parameter that is not an array".into(),
                ));
            }
            (Shape::Function { signatures }, _) => self.callback(signatures)?,
            (ty, _) => match nullable(ty) {
                Some(inner) if matches!(ty, Shape::Union { types, .. } if !types.contains(&Shape::Null)) =>
                {
                    optional = true;
                    self.ty(inner, Pos::Param)?
                }
                Some(_) => {
                    return Err(Blocked::unsupported(
                        "it takes `null`, which Flick passes only through a reviewed adapter"
                            .into(),
                    ));
                }
                None => self.ty(ty, Pos::Param)?,
            },
        };
        Ok(if optional {
            (format!("Option {}", atom(&ty)), true)
        } else {
            (ty, false)
        })
    }

    /// A function JS calls: a pure Flick function with the same parameters.
    fn callback(&mut self, signatures: &[Signature]) -> Result<String, Blocked> {
        let [sig] = signatures else {
            return Err(Blocked::unsupported("an overloaded callback".into()));
        };
        if sig.this
            || !sig.type_params.is_empty()
            || sig.params.iter().any(|p| p.rest || p.optional)
        {
            return Err(Blocked::unsupported(
                "a callback with `this`, type parameters, rest or optional parameters".into(),
            ));
        }
        self.callback = true;
        let mut out = String::new();
        for p in &sig.params {
            let _ = write!(out, "{} -> ", arrow_operand(&self.ty(&p.ty, Pos::Result)?));
        }
        if sig.params.is_empty() {
            out.push_str("Unit -> ");
        }
        let result = match &sig.result {
            Shape::Void | Shape::Undefined => "Unit".to_owned(),
            Shape::Promise { .. } => {
                return Err(Blocked::unsupported(
                    "a callback that returns a Promise needs `Js.promise_callback` in a reviewed binding".into(),
                ));
            }
            other => self.ty(other, Pos::Param)?,
        };
        Ok(format!("({out}{result})"))
    }

    pub(super) fn ty(&mut self, shape: &Shape, pos: Pos) -> Result<String, Blocked> {
        let not_here = |what: &str| Err(Blocked::unsupported(what.to_owned()));
        Ok(match shape {
            Shape::String => "String".into(),
            Shape::Number => "Float".into(),
            Shape::Boolean => "Bool".into(),
            Shape::Bigint => "BigInt".into(),
            Shape::Any | Shape::Unknown if self.strict => {
                return not_here(&format!("`{}` is refused by `strict = true`", shape.text()));
            }
            Shape::Any | Shape::Unknown => "JsValue".into(),
            Shape::Void | Shape::Undefined if pos == Pos::Result => "Unit".into(),
            Shape::Never if pos == Pos::Result => "Never".into(),
            Shape::Void | Shape::Undefined | Shape::Never | Shape::Null => {
                return not_here(&format!("`{}` has no Flick value here", shape.text()));
            }
            Shape::Literal { text } => {
                return not_here(&format!("the literal type `{text}` needs an adapter"));
            }
            Shape::Param { name, bound: None } => {
                let var = var_name(name);
                self.vars.insert(var.clone());
                var
            }
            Shape::Param {
                name,
                bound: Some(bound),
            } => {
                return not_here(&format!(
                    "type parameter `{name}` is bounded by `{bound}`, which a Flick type variable cannot say"
                ));
            }
            Shape::Union { types, alias } => {
                let shown = shape.text();
                let Some(texts) = enums::literals(types) else {
                    return not_here(&format!(
                        "the union `{shown}` has no single Flick type; pick one member in a reviewed binding"
                    ));
                };
                if let Some(reason) = enums::refused(&shown, &texts) {
                    return not_here(&reason);
                }
                let Some(name) = alias.as_ref().filter(|a| is_type_name(a)) else {
                    return not_here(&format!(
                        "the union `{shown}` has no name: a `type` alias of literals is an enum"
                    ));
                };
                self.enums.insert(name.clone(), texts);
                name.clone()
            }
            Shape::Tuple { elements } => {
                let parts = elements
                    .iter()
                    .map(|e| self.ty(e, pos))
                    .collect::<Result<Vec<_>, _>>()?;
                match parts.len() {
                    0 => "Unit".into(),
                    1 => return not_here("a one-element tuple"),
                    _ => format!("({})", parts.join(", ")),
                }
            }
            Shape::Array { element } => format!("List {}", atom(&self.ty(element, pos)?)),
            Shape::Promise { .. } => {
                return not_here(&format!(
                    "`{}` inside another type; only a returned Promise is awaited",
                    shape.text()
                ));
            }
            Shape::Function { .. } if pos == Pos::Param => {
                return not_here("a callback inside another type");
            }
            Shape::Function { .. } => return not_here("it returns a function"),
            Shape::Handle { name }
            | Shape::Object {
                name: Some(name), ..
            } if is_type_name(name) => {
                self.handles.insert(name.clone());
                name.clone()
            }
            Shape::Object { name: None, text } if pos == Pos::Result => {
                let _ = text;
                "JsValue".into()
            }
            Shape::Object { text, .. } => {
                return not_here(&format!("the object type `{text}` needs a checked adapter"));
            }
            Shape::Handle { name } => {
                return not_here(&format!("`{name}` is not a Flick type name"));
            }
            Shape::Other { text } => {
                return not_here(&format!("`{text}` is outside the supported subset"));
            }
        })
    }
}

/// `T | undefined`, `T | null`, `T | null | undefined` → `T`.
pub(super) fn nullable(shape: &Shape) -> Option<&Shape> {
    let Shape::Union { types, .. } = shape else {
        return None;
    };
    let rest: Vec<&Shape> = types
        .iter()
        .filter(|t| !matches!(t, Shape::Undefined | Shape::Null | Shape::Void))
        .collect();
    match rest[..] {
        [one] if rest.len() < types.len() => Some(one),
        _ => None,
    }
}

/// Wraps a type applied to arguments, or an arrow, for an argument position.
pub(super) fn atom(ty: &str) -> String {
    if ty.contains(' ') && !ty.starts_with('(') {
        format!("({ty})")
    } else {
        ty.to_owned()
    }
}

pub(super) fn arrow_operand(ty: &str) -> String {
    if ty.starts_with("forall") {
        format!("({ty})")
    } else {
        ty.to_owned()
    }
}

pub(super) fn mentions(ty: &str, var: &str) -> bool {
    ty.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|w| w == var)
}

pub(super) fn is_type_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A TypeScript type parameter as a Flick type variable: `T` → `t`.
pub(super) fn var_name(name: &str) -> String {
    flick_name(name)
}
