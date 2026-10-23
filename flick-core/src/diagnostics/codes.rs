//! Registry of diagnostic codes. A code is `FLICK-<area letter><3 digits>`; the
//! area letter is the code's range. Codes are never reused: a code that stops
//! being emitted stays registered and is marked retired in its explanation.

use std::{fmt, str};

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

impl str::FromStr for Code {
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
    S003 = Syntax 3, "unexpected character";
    S004 = Syntax 4, "unterminated literal or comment";
    S005 = Syntax 5, "invalid string literal";
    S006 = Syntax 6, "invalid numeric literal";
    S007 = Syntax 7, "syntax error";
    S008 = Syntax 8, "unclosed delimiter";
    S009 = Syntax 9, "indentation error";
    S010 = Syntax 10, "construct from another language that Flick does not have";
    S011 = Syntax 11, "unknown foreign metadata";
    S012 = Syntax 12, "inconsistent pattern bindings";
    R001 = Resolve 1, "module name does not match the file path";
    R002 = Resolve 2, "two files declare the same module";
    R003 = Resolve 3, "unknown module";
    R004 = Resolve 4, "name is not exported";
    R005 = Resolve 5, "constructor is not exported";
    R006 = Resolve 6, "import cycle";
    R007 = Resolve 7, "cyclic type alias";
    R008 = Resolve 8, "name defined more than once";
    R009 = Resolve 9, "definition shadows an imported name";
    R010 = Resolve 10, "ambiguous name";
    R011 = Resolve 11, "invalid export";
    T001 = Types 1, "unknown name";
    T002 = Types 2, "kind mismatch";
    T003 = Types 3, "type mismatch";
    T004 = Types 4, "function without a signature";
    T005 = Types 5, "types written in the `let` header";
    T006 = Types 6, "signature without a definition below it";
    T007 = Types 7, "type variable escapes its scope";
    T008 = Types 8, "implementation less polymorphic than its signature";
    T009 = Types 9, "patterns do not cover every value";
    T010 = Types 10, "unreachable match arm";
    T011 = Types 11, "discarded non-Unit result";
    T012 = Types 12, "recursive value";
    T013 = Types 13, "type cannot be generalized";
    T014 = Types 14, "duplicate record label";
    T015 = Types 15, "polymorphic recursion without an explicit forall";
    T016 = Types 16, "infinite type";
    T017 = Types 17, "recursion without rec";
    T018 = Types 18, "wrong number of constructor arguments in a pattern";
    T019 = Types 19, "block without a result expression";
    T020 = Types 20, "representation of a type that does not fit it";
    E001 = Effects 1, "runtime call in a pure position";
    E002 = Effects 2, "services or errors not allowed by the bound";
    E003 = Effects 3, "argument runs before the function that takes an Eff";
    E004 = Effects 4, "two layers build the same service";
    E005 = Effects 5, "error payload holds a live value";
    E006 = Effects 6, "service operation is not a runtime function";
    E007 = Effects 7, "handler pattern is not an error";
    C001 = Classes 1, "no instance for a constraint";
    C002 = Classes 2, "ambiguous class constraint";
    C003 = Classes 3, "overlapping instances";
    C004 = Classes 4, "instances break a functional dependency";
    C005 = Classes 5, "instance does not cover a functional dependency";
    C006 = Classes 6, "orphan instance";
    C007 = Classes 7, "instance context might not terminate";
    C008 = Classes 8, "cyclic superclasses";
    C009 = Classes 9, "instance resolution exceeds its budget";
    C010 = Classes 10, "instance method does not match the class";
    C011 = Classes 11, "missing or unknown instance method";
    C012 = Classes 12, "instance of a compiler-owned constraint";
    C013 = Classes 13, "cannot derive an instance";
    G001 = Regions 1, "value holds more than its capture qualifier allows";
    G002 = Regions 2, "scoped value leaves its scope";
    G003 = Regions 3, "child may outlive what it holds";
    G004 = Regions 4, "resource acquired outside a scope";
    F001 = Ffi 1, "invalid foreign metadata";
    F002 = Ffi 2, "foreign parameter may hold a scope without a retention contract";
    F003 = Ffi 3, "catch mapping does not make an error of the signature";
    F004 = Ffi 4, "export has no TypeScript form";
    F005 = Ffi 5, "use of a blocked foreign import";
    F006 = Ffi 6, "generated bindings are out of date";
    F007 = Ffi 7, "reviewed binding does not match its declaration";
    F008 = Ffi 8, "cannot import the declarations of a package";
    D001 = Schema 1, "type has no Schema witness";
    D002 = Schema 2, "codec misses a field";
    D003 = Schema 3, "codec gives a field twice";
    D004 = Schema 4, "codec gives a field the record does not have";
    D005 = Schema 5, "two codec fields share a wire name";
    D006 = Schema 6, "constant fails the check of its refined type";
    D007 = Schema 7, "codec block over a schema that is not a record";
    D008 = Schema 8, "codec has no strict JSON Schema";
    Q001 = Sql 1, "unknown column";
    Q002 = Sql 2, "unknown table alias";
    Q003 = Sql 3, "ambiguous column";
    Q004 = Sql 4, "duplicate name in a query";
    Q005 = Sql 5, "column neither grouped nor aggregated";
    Q006 = Sql 6, "aggregate where it is not allowed";
    Q007 = Sql 7, "INSERT misses a required column";
    Q008 = Sql 8, "write to a generated column";
    Q009 = Sql 9, "unknown query parameter";
    Q010 = Sql 10, "outside the supported SQL subset";
    Q011 = Sql 11, "column type does not match its SQL type";
    Q012 = Sql 12, "value without a SQL form";
    U001 = Ui 1, "event policy needs a positive integer";
    P001 = Project 1, "cannot read source file";
    P002 = Project 2, "file is not formatted";
    P003 = Project 3, "internal formatter error";
    P004 = Project 4, "cannot write source file";
    P005 = Project 5, "invalid project configuration";
    P006 = Project 6, "unsupported configuration schema version";
    P007 = Project 7, "application without an entry module";
    P008 = Project 8, "no project found";
    P009 = Project 9, "dependency is not installed";
    P010 = Project 10, "invalid package interface";
    P011 = Project 11, "invalid entry module";
    P012 = Project 12, "construct is not compiled yet";
    P013 = Project 13, "cannot write build output";
    P014 = Project 14, "package built against another interface of a dependency";
    P015 = Project 15, "incompatible toolchain or runtime";
    P016 = Project 16, "package manager unavailable or of another version";
    P017 = Project 17, "package manager failed";
    P018 = Project 18, "unknown module or value for a tool";
    P019 = Project 19, "file exists";
    P020 = Project 20, "incomplete package";
    P021 = Project 21, "application cleanup failed";
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
