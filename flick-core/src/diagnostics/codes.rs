//! Registry of diagnostic codes. A code is `FLICK-<area letter><3 digits>`; the
//! area letter is the code's range. Codes are never reused: a code that stops
//! being emitted stays registered and is marked retired in its explanation.
//! A plugin's codes are its own, shown under its package: `std/L001`.

#[path = "codes/code.rs"]
mod code;
pub use code::{Area, Code};

/// Interns a plugin code, keeping its package namespace.
pub fn plugin(package: &str, code: &str) -> Option<Code> {
    self::code::plugin(package, code)
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
    S013 = Syntax 13, "`<-` outside a `do` block";
    S014 = Syntax 14, "unknown attribute";
    S015 = Syntax 15, "a constructor result where none can be written";
    S016 = Syntax 16, "`mut` where only a local variable of one name can be";
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
    R012 = Resolve 12, "unused import";
    R013 = Resolve 13, "unused variable";
    R014 = Resolve 14, "unused private definition";
    R015 = Resolve 15, "local shadows another local";
    R016 = Resolve 16, "lang item outside the built-in modules";
    R017 = Resolve 17, "private module belongs to another package";
    R018 = Resolve 18, "inline module is private to its source file";
    R019 = Resolve 19, "static module application inside an inline module";
    R020 = Resolve 20, "`let mut` variable never assigned";
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
    T021 = Types 21, "annotation of a type the value already has";
    T022 = Types 22, "`todo` left in the code";
    T023 = Types 23, "`echo` left in the code";
    T024 = Types 24, "type hole";
    T025 = Types 25, "`let assert` with a pattern that matches every value";
    T026 = Types 26, "`let assert` outside a block";
    T027 = Types 27, "`let assert` in a project with `[lint] no-let-assert`";
    T028 = Types 28, "`else ()` an `if` does not need";
    T029 = Types 29, "`do` over `Eff`, which a `task` with direct calls says";
    T030 = Types 30, "a binding of a `do` block only returned";
    T031 = Types 31, "a whole infinite sequence";
    T032 = Types 32, "`Seq.to_list` of a `for`, which `[for …]` builds";
    T033 = Types 33, "`List.filter` then `List.map`, which `[for …]` says";
    T034 = Types 34, "a `match` that fails with the error of a `Result`, which `?` says";
    T035 = Types 35, "a chain of `flat_map`, which `do` says";
    T036 = Types 36, "function longer than `[lint] max-function-lines`";
    T037 = Types 37, "nesting deeper than `[lint] max-nesting`";
    T038 = Types 38, "function more complex than `[lint] max-complexity`";
    T039 = Types 39, "function with more parameters than `[lint] max-params`";
    T040 = Types 40, "an `if` that returns its condition";
    T041 = Types 41, "an `if not` with an `else`";
    T042 = Types 42, "an `if` alone in an `else`";
    T043 = Types 43, "a literal on the left of a comparison";
    T044 = Types 44, "a value compared with itself";
    T045 = Types 45, "a condition known before the program runs";
    T046 = Types 46, "a `match` on a `Bool`, which `if` says";
    T047 = Types 47, "`List.head` of `List.filter`, which `List.find` says";
    T048 = Types 48, "the length of a `List.filter` compared with zero, which `List.any` says";
    T049 = Types 49, "a length compared with zero, which `List.is_empty` says";
    T050 = Types 50, "`List.concat` of `List.map`, which `List.flat_map` says";
    T051 = Types 51, "a `match` that unwraps an `Option`, which `Option.with_default` says";
    T052 = Types 52, "a `fun` that only passes its parameter on";
    T053 = Types 53, "two string literals joined with `++`";
    T054 = Types 54, "a local function that uses no local around it";
    T055 = Types 55, "an `_` arm that hides constructors of a type of this module";
    T056 = Types 56, "a constructor builds a type it cannot";
    T057 = Types 57, "a refining match needs known types";
    T058 = Types 58, "a refinement Flick does not make";
    T059 = Types 59, "a part of a signature it gets anyway";
    T060 = Types 60, "module lacks a required contract member";
    T061 = Types 61, "module member does not satisfy its contract";
    T062 = Types 62, "module member has a different nominal identity";
    T063 = Types 63, "abstract class or service cannot be packed";
    T064 = Types 64, "an unpacked module's abstract type escapes";
    T065 = Types 65, "module contract dictionary source has the wrong type";
    T066 = Types 66, "ambiguous local module dictionaries";
    T067 = Types 67, "public parameterized module requires an explicit result signature";
    T068 = Types 68, "static parameterized module or nested module cannot be packed";
    T069 = Types 69, "assignment to a name that is not a `let mut` variable";
    T070 = Types 70, "assignment inside a restricted construct";
    T071 = Types 71, "invalid loop control";
    T072 = Types 72, "one cell of a type for every type";
    T073 = Types 73, "children of a call written as a list";
    T074 = Types 74, "a record in the braces of an `if` branch";
    E001 = Effects 1, "runtime call in a pure position";
    E002 = Effects 2, "services or errors not allowed by the bound";
    E003 = Effects 3, "argument runs before the function that takes an Eff";
    E004 = Effects 4, "two layers build the same service";
    E005 = Effects 5, "error payload holds a live value";
    E006 = Effects 6, "service operation is not a runtime function";
    E007 = Effects 7, "handler pattern is not an error";
    E008 = Effects 8, "effect in a project built without effects";
    E009 = Effects 9, "client entry reaches `Server`";
    E010 = Effects 10, "`server name` of a value that is no `server fn`";
    E011 = Effects 11, "guarded `server fn` served the wrong way";
    E012 = Effects 12, "server function needs server stdlib";
    E013 = Effects 13, "service operation that is not `tail`";
    E014 = Effects 14, "`abort` operation that may return";
    E015 = Effects 15, "handler `return` arm that may not match";
    E016 = Effects 16, "`provide` of an effect that is not a service";
    E017 = Effects 17, "task in a fiber of its own needs a `control` effect";
    E018 = Effects 18, "`tail` arm that may suspend";
    E019 = Effects 19, "row variable that may hold another instance of a provided service";
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
    C014 = Classes 14, "invalid implicit conversion instance";
    G001 = Regions 1, "value holds more than its capture qualifier allows";
    G002 = Regions 2, "scoped value leaves its scope";
    G003 = Regions 3, "child may outlive what it holds";
    G004 = Regions 4, "resource acquired outside a scope";
    G005 = Regions 5, "`let mut` variable used in a function it is not a variable of";
    F001 = Ffi 1, "invalid foreign metadata";
    F002 = Ffi 2, "foreign parameter may hold a scope without a retention contract";
    F003 = Ffi 3, "catch mapping does not make an error of the signature";
    F004 = Ffi 4, "export has no TypeScript form";
    F005 = Ffi 5, "use of a blocked foreign import";
    F006 = Ffi 6, "generated bindings are out of date";
    F007 = Ffi 7, "reviewed binding does not match its declaration";
    F008 = Ffi 8, "cannot import the declarations of a package";
    F009 = Ffi 9, "pure foreign function over a scope's capability";
    F010 = Ffi 10, "retention owner needs the std package";
    D001 = Schema 1, "type has no Schema witness";
    D002 = Schema 2, "codec misses a field";
    D003 = Schema 3, "codec gives a field twice";
    D004 = Schema 4, "codec gives a field the record does not have";
    D005 = Schema 5, "two codec fields share a wire name";
    D006 = Schema 6, "constant fails the check of its refined type";
    D007 = Schema 7, "codec block over a schema that is not a record";
    D008 = Schema 8, "codec has no strict JSON Schema";
    D009 = Schema 9, "type has no URL form";
    D010 = Schema 10, "type has no form";
    D011 = Schema 11, "type has no wire codec";
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
    Q013 = Sql 13, "migration does not apply";
    U001 = Ui 1, "event policy needs a positive integer";
    U002 = Ui 2, "handler made while a view renders";
    U003 = Ui 3, "serializable atom without a key of its own";
    U004 = Ui 4, "render does more than read atoms";
    U005 = Ui 5, "atom of a type for every type";
    U006 = Ui 6, "atom, action or view made while rendering";
    U007 = Ui 7, "function that reads atoms to build a `Ui`, which a `component` says";
    L001 = Cli 1, "name taken twice in a command";
    L002 = Cli 2, "default of an optional parameter";
    L003 = Cli 3, "positional argument of many values before the last";
    L004 = Cli 4, "clause given twice";
    L005 = Cli 5, "command without a handler or subcommands";
    L006 = Cli 6, "positional argument next to subcommands";
    L007 = Cli 7, "other names of a positional argument";
    L008 = Cli 8, "CLI expression needs server stdlib";
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
    P012 = Project 12, "construct is not compiled for this target";
    P013 = Project 13, "cannot write build output";
    P014 = Project 14, "package built against another interface of a dependency";
    P015 = Project 15, "incompatible toolchain or runtime";
    P016 = Project 16, "package manager unavailable or of another version";
    P017 = Project 17, "package manager failed";
    P018 = Project 18, "unknown module or value for a tool";
    P019 = Project 19, "file exists";
    P020 = Project 20, "incomplete package";
    P021 = Project 21, "application cleanup failed";
    P022 = Project 22, "SQL dialect not supported yet";
    P023 = Project 23, "unknown SQL dialect";
    P024 = Project 24, "package with `todo` or `echo` left in it";
    P025 = Project 25, "cannot compare the API with a git revision";
    P026 = Project 26, "entry `main` has a type no runner starts";
    P027 = Project 27, "package built only without Effect";
    P028 = Project 28, "output directory holds the project's files";
    P029 = Project 29, "Go toolchain cannot build the program";
    P030 = Project 30, "module not available on the go target";
    P031 = Project 31, "invalid workspace or path dependency";
    P032 = Project 32, "invalid dependency or package";
    P033 = Project 33, "package needs another version of Flick";
    P034 = Project 34, "dependency is not installed";
    P035 = Project 35, "plugin cannot be loaded";
    P036 = Project 36, "plugin gave no code for a block";
    P037 = Project 37, "plugin block without its import";
    P038 = Project 38, "plugin reaches what the project does not grant";
    P039 = Project 39, "plugin's answer is in no snapshot of an offline run";
    P040 = Project 40, "packages need different versions of an npm package";
    P041 = Project 41, "foreign module of a library cannot ship";
    P042 = Project 42, "the manifest disables this part of a plugin";
    P043 = Project 43, "the `std` does not fit this compiler";
    P044 = Project 44, "another plugin gives a head of std's plugin";
    P045 = Project 45, "the plugins of two packages load one extension";
    P046 = Project 46, "a plugin's emit changed code without its map";
    P047 = Project 47, "the IR a plugin gave does not hold together";
    P048 = Project 48, "a plugin rewrote the IR of the build";
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
            assert_eq!(code.parse::<Code>(), Ok(info.code));
        }
    }

    #[test]
    fn area_letters_are_distinct() {
        let letters: HashSet<_> = Area::ALL.iter().map(|a| a.letter()).collect();
        assert_eq!(letters.len(), Area::ALL.len());
    }

    #[test]
    fn a_plugins_code_shows_under_its_package() {
        let code = plugin("std", "L001").expect("well formed");
        assert_eq!(code.to_string(), "std/L001");
        assert_eq!(plugin("std", "L001"), Some(code));
        assert_ne!(plugin("other", "L001"), Some(code));
        for bad in [
            "",
            "L01",
            "l001",
            "L0012",
            "FLICK-L001",
            "../001",
            "ABCDE001",
            "Lé01",
            "é",
        ] {
            assert_eq!(plugin("std", bad), None, "{bad}");
        }
    }

    #[test]
    fn display_pads_to_three_digits() {
        assert_eq!(S001.to_string(), "FLICK-S001");
        assert_eq!(lookup("FLICK-X001").map(|i| i.code), None);
    }
}
