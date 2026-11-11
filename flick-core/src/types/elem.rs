//! What labels an entry of a row: the name of a record's field, of a
//! service or of an error, or one of the elements that stand for something
//! still to be known. `formal/Formal/TyBase.lean` has the same cases as
//! `Elem`; here an interface writes each as the text it always had.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ty::Type;

/// A variable an element follows until it stands for something named.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Atom {
    Var(String),
    Skolem(u32),
    Meta(u32),
}

impl Atom {
    pub fn of(t: &Type) -> Option<Atom> {
        match t {
            Type::Var(name) => Some(Atom::Var(name.clone())),
            Type::Skolem(s) => Some(Atom::Skolem(*s)),
            Type::Meta(m) => Some(Atom::Meta(*m)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Elem {
    /// A field by its name; a service or an error by its qualified name.
    Named(String),
    /// A region, or a type variable whose captures are meant, in a capture
    /// row.
    Capture(Atom),
    /// A type variable as an element of a row of errors, `{t | e}`.
    Error(Atom),
    /// A service a static module supplies while it is still a variable.
    Service(Atom),
    /// `Live C` in a row of services: what the stage needs alive.
    Live,
    /// The obligation to provide the service the element names.
    Provided(Box<Elem>),
}

const LIVE: &str = "Fiber.Live ";
const PROVIDED: &str = "Fiber.Provided ";

impl Elem {
    pub fn named(name: impl Into<String>) -> Elem {
        Elem::Named(name.into())
    }

    /// The name of a field, a service or an error.
    pub fn name(&self) -> Option<&str> {
        match self {
            Elem::Named(name) => Some(name),
            _ => None,
        }
    }

    /// The element an interface wrote as `text`.
    fn read(text: &str) -> Elem {
        let atom = |rest: &str| match rest.strip_prefix('\'').map(str::parse) {
            Some(Ok(s)) => Atom::Skolem(s),
            _ => match rest.strip_prefix('?').map(str::parse) {
                Some(Ok(m)) => Atom::Meta(m),
                _ => Atom::Var(rest.to_owned()),
            },
        };
        if text == LIVE {
            Elem::Live
        } else if let Some(rest) = text.strip_prefix(PROVIDED) {
            Elem::Provided(Box::new(Elem::read(rest)))
        } else if let Some(rest) = text.strip_prefix('^') {
            Elem::Capture(atom(rest))
        } else if let Some(rest) = text.strip_prefix('~') {
            Elem::Error(atom(rest))
        } else if let Some(rest) = text.strip_prefix('@') {
            Elem::Service(atom(rest))
        } else {
            Elem::Named(text.to_owned())
        }
    }
}

/// A field, a service or an error from its name.
impl From<&str> for Elem {
    fn from(name: &str) -> Elem {
        Elem::Named(name.to_owned())
    }
}

impl From<String> for Elem {
    fn from(name: String) -> Elem {
        Elem::Named(name)
    }
}

/// Equal to a name when it is the field, service or error of that name.
impl PartialEq<str> for Elem {
    fn eq(&self, name: &str) -> bool {
        self.name() == Some(name)
    }
}

impl PartialEq<&str> for Elem {
    fn eq(&self, name: &&str) -> bool {
        self.name() == Some(*name)
    }
}

impl PartialEq<String> for Elem {
    fn eq(&self, name: &String) -> bool {
        self.name() == Some(name)
    }
}

/// What finds an entry of a row: an element, or the name of a field, a
/// service or an error.
pub trait Label {
    /// How `elem` stands to this label in the order of a row.
    fn against(&self, elem: &Elem) -> Ordering;
}

impl Label for Elem {
    fn against(&self, elem: &Elem) -> Ordering {
        elem.cmp(self)
    }
}

impl Label for str {
    fn against(&self, elem: &Elem) -> Ordering {
        match elem {
            Elem::Named(own) => own.as_str().cmp(self),
            _ => elem.to_string().as_str().cmp(self),
        }
    }
}

impl Label for String {
    fn against(&self, elem: &Elem) -> Ordering {
        self.as_str().against(elem)
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Atom::Var(name) => f.write_str(name),
            Atom::Skolem(s) => write!(f, "'{s}"),
            Atom::Meta(m) => write!(f, "?{m}"),
        }
    }
}

/// The text of the element in an interface, which also orders a row.
impl fmt::Display for Elem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Elem::Named(name) => f.write_str(name),
            Elem::Capture(atom) => write!(f, "^{atom}"),
            Elem::Error(atom) => write!(f, "~{atom}"),
            Elem::Service(atom) => write!(f, "@{atom}"),
            Elem::Live => f.write_str(LIVE),
            Elem::Provided(of) => write!(f, "{PROVIDED}{of}"),
        }
    }
}

impl Ord for Elem {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Elem::Named(a), Elem::Named(b)) => a.cmp(b),
            _ => self.to_string().cmp(&other.to_string()),
        }
    }
}

impl PartialOrd for Elem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for Elem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Elem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(|text| Elem::read(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::{Atom, Elem};

    #[test]
    fn an_element_reads_back_from_the_text_an_interface_writes() {
        let elems = [
            Elem::named("App.Main.Db"),
            Elem::named("x"),
            Elem::Capture(Atom::Var("s".into())),
            Elem::Capture(Atom::Skolem(3)),
            Elem::Error(Atom::Meta(7)),
            Elem::Service(Atom::Var("S".into())),
            Elem::Live,
            Elem::Provided(Box::new(Elem::named("App.Main.Db"))),
            Elem::Provided(Box::new(Elem::Service(Atom::Var("r".into())))),
        ];
        for elem in elems {
            assert_eq!(Elem::read(&elem.to_string()), elem);
        }
    }

    #[test]
    fn elements_order_as_their_texts_do() {
        let mut elems = [
            Elem::Live,
            Elem::named("Zeta"),
            Elem::Capture(Atom::Var("s".into())),
            Elem::named("Alpha.B"),
            Elem::Error(Atom::Var("e".into())),
        ];
        let mut texts: Vec<String> = elems.iter().map(ToString::to_string).collect();
        elems.sort();
        texts.sort();
        assert_eq!(
            elems.iter().map(ToString::to_string).collect::<Vec<_>>(),
            texts
        );
    }
}
