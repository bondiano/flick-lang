//! Registry of diagnostic codes. A code is `FLICK-<area letter><3 digits>`; the
//! area letter is the code's range. Codes are never reused: a code that stops
//! being emitted stays registered and is marked retired in its explanation.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Area {
    Syntax,
    Resolve,
    Types,
    Effects,
    Classes,
    Regions,
    Ffi,
    Schema,
    Sql,
    Ui,
    Project,
}

impl Area {
    pub const ALL: [Area; 11] = [
        Area::Syntax,
        Area::Resolve,
        Area::Types,
        Area::Effects,
        Area::Classes,
        Area::Regions,
        Area::Ffi,
        Area::Schema,
        Area::Sql,
        Area::Ui,
        Area::Project,
    ];

    pub const fn letter(self) -> char {
        match self {
            Area::Syntax => 'S',
            Area::Resolve => 'R',
            Area::Types => 'T',
            Area::Effects => 'E',
            Area::Classes => 'C',
            Area::Regions => 'G',
            Area::Ffi => 'F',
            Area::Schema => 'D',
            Area::Sql => 'Q',
            Area::Ui => 'U',
            Area::Project => 'P',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Code {
    pub area: Area,
    pub number: u16,
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FLICK-{}{:03}", self.area.letter(), self.number)
    }
}

impl std::str::FromStr for Code {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        lookup(s).map(|info| info.code).ok_or(())
    }
}

#[derive(Debug)]
pub struct CodeInfo {
    pub code: Code,
    pub title: &'static str,
    /// Text for `flick explain`: short explanation plus an example fix.
    pub explain: &'static str,
}

/// Finds a registered code by its `FLICK-…` spelling.
pub fn lookup(code: &str) -> Option<&'static CodeInfo> {
    REGISTRY.iter().find(|info| info.code.to_string() == code)
}

macro_rules! codes {
    ($($name:ident = $area:ident $number:literal, $title:literal;)*) => {
        $(pub const $name: Code = Code { area: Area::$area, number: $number };)*

        pub static REGISTRY: &[CodeInfo] = &[$(CodeInfo {
            code: $name,
            title: $title,
            explain: include_str!(concat!("explain/", stringify!($name), ".md")),
        },)*];
    };
}

codes! {
    S001 = Syntax 1, "source file is not valid UTF-8";
    S002 = Syntax 2, "construct not supported by the bootstrap front-end";
    P001 = Project 1, "cannot read source file";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_codes_are_unique_well_formed_and_explained() {
        let mut seen = HashSet::new();
        for info in REGISTRY {
            let code = info.code.to_string();
            assert!(seen.insert(info.code), "{code} registered twice");
            assert!((1..=999).contains(&info.code.number), "{code} out of range");
            assert!(!info.title.is_empty(), "{code} has no title");
            assert!(
                info.explain.trim().len() > 20 && info.explain.contains("```"),
                "{code} explain text needs an explanation and an example"
            );
            assert_eq!(code.parse::<Code>(), Ok(info.code));
        }
    }

    #[test]
    fn area_letters_are_distinct() {
        let letters: HashSet<_> = Area::ALL.iter().map(|a| a.letter()).collect();
        assert_eq!(letters.len(), Area::ALL.len());
    }

    #[test]
    fn display_pads_to_three_digits() {
        assert_eq!(S001.to_string(), "FLICK-S001");
        assert_eq!(lookup("FLICK-X001").map(|i| i.code), None);
    }
}
