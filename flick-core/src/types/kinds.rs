//! Kinds (§5 "Вычисления и kinds"): `Type`, the three effect-row kinds,
//! `Row k` for record fields, labels, regions, constraints, finite kinds a
//! `kind` declaration introduces, and arrows between them. Kinds of type
//! variables are inferred from their positions; what stays open becomes `Type`.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Type,
    ServiceRow,
    ErrorRow,
    CaptureRow,
    Symbol,
    Region,
    Constraint,
    /// A service identity: what a `ServiceRow` holds.
    Service,
    /// `Row k`; `Row Type` is the kind of record fields.
    Row(Box<Kind>),
    /// A finite kind declared with `kind K = A | B`.
    Named {
        module: String,
        name: String,
    },
    Arrow(Box<Kind>, Box<Kind>),
    /// An unknown kind while kinds are inferred.
    #[serde(skip)]
    Meta(u32),
}

impl Kind {
    pub fn arrow(param: Kind, result: Kind) -> Kind {
        Kind::Arrow(Box::new(param), Box::new(result))
    }

    /// `k1 -> … -> kn -> result`.
    pub fn arrows(params: impl DoubleEndedIterator<Item = Kind>, result: Kind) -> Kind {
        params.rev().fold(result, |acc, p| Kind::arrow(p, acc))
    }

    pub fn is_row(&self) -> bool {
        matches!(
            self,
            Kind::ServiceRow | Kind::ErrorRow | Kind::CaptureRow | Kind::Row(_)
        )
    }

    /// The built-in kind a capitalized name in kind position denotes.
    pub fn builtin(name: &str) -> Option<Kind> {
        Some(match name {
            "Type" => Kind::Type,
            "ServiceRow" => Kind::ServiceRow,
            "ErrorRow" => Kind::ErrorRow,
            "CaptureRow" => Kind::CaptureRow,
            "FieldRow" => Kind::Row(Box::new(Kind::Type)),
            "Symbol" => Kind::Symbol,
            "Region" => Kind::Region,
            "Constraint" => Kind::Constraint,
            _ => return None,
        })
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Type => f.write_str("Type"),
            Kind::ServiceRow => f.write_str("ServiceRow"),
            Kind::ErrorRow => f.write_str("ErrorRow"),
            Kind::CaptureRow => f.write_str("CaptureRow"),
            Kind::Symbol => f.write_str("Symbol"),
            Kind::Region => f.write_str("Region"),
            Kind::Constraint => f.write_str("Constraint"),
            Kind::Service => f.write_str("Service"),
            Kind::Row(k) if matches!(**k, Kind::Arrow(..)) => write!(f, "Row ({k})"),
            Kind::Row(k) => write!(f, "Row {k}"),
            Kind::Named { name, .. } => f.write_str(name),
            Kind::Arrow(a, b) if matches!(**a, Kind::Arrow(..)) => write!(f, "({a}) -> {b}"),
            Kind::Arrow(a, b) => write!(f, "{a} -> {b}"),
            Kind::Meta(_) => f.write_str("_"),
        }
    }
}

/// Unknown kinds of one inference: kinds of a module's declarations or of
/// one signature.
#[derive(Debug, Default)]
pub struct KindStore {
    metas: Vec<Option<Kind>>,
}

impl KindStore {
    pub fn fresh(&mut self) -> Kind {
        self.metas.push(None);
        Kind::Meta(u32::try_from(self.metas.len() - 1).expect("few kind variables"))
    }

    /// Follows solved unknowns at the top.
    pub fn resolve(&self, kind: &Kind) -> Kind {
        let mut kind = kind.clone();
        while let Kind::Meta(m) = kind {
            match &self.metas[m as usize] {
                Some(k) => kind = k.clone(),
                None => break,
            }
        }
        kind
    }

    /// Solves every unknown it can; with `default`, open unknowns become `Type`.
    pub fn zonk(&mut self, kind: &Kind, default: bool) -> Kind {
        match self.resolve(kind) {
            Kind::Meta(m) if default => {
                self.metas[m as usize] = Some(Kind::Type);
                Kind::Type
            }
            Kind::Row(k) => Kind::Row(Box::new(self.zonk(&k, default))),
            Kind::Arrow(a, b) => Kind::arrow(self.zonk(&a, default), self.zonk(&b, default)),
            other => other,
        }
    }

    /// `Err` with both kinds as far as they are known.
    pub fn unify(&mut self, a: &Kind, b: &Kind) -> Result<(), (Kind, Kind)> {
        let (a, b) = (self.resolve(a), self.resolve(b));
        match (&a, &b) {
            (Kind::Meta(x), Kind::Meta(y)) if x == y => Ok(()),
            (Kind::Meta(m), other) | (other, Kind::Meta(m)) => {
                if self.occurs(*m, other) {
                    return Err((self.zonk(&a, false), self.zonk(&b, false)));
                }
                self.metas[*m as usize] = Some(other.clone());
                Ok(())
            }
            (Kind::Row(x), Kind::Row(y)) => self.unify(x, y),
            (Kind::Arrow(a1, b1), Kind::Arrow(a2, b2)) => {
                self.unify(a1, a2)?;
                self.unify(b1, b2)
            }
            _ if a == b => Ok(()),
            _ => Err((self.zonk(&a, false), self.zonk(&b, false))),
        }
    }

    fn occurs(&self, m: u32, kind: &Kind) -> bool {
        match self.resolve(kind) {
            Kind::Meta(x) => x == m,
            Kind::Row(k) => self.occurs(m, &k),
            Kind::Arrow(a, b) => self.occurs(m, &a) || self.occurs(m, &b),
            _ => false,
        }
    }
}
