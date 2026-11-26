//! Data-driven classification of plugin block heads for the parser, plus the
//! compiler-internal syntax markers generated code uses.

use flick_plugin::HeadSpec;

/// Syntactic classification of one plugin block head.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRule {
    pub head: String,
    pub module: String,
    /// Starts blocks without an explicit `import ... exposing (plugin head)`.
    pub implicit: bool,
    /// Allowed bare in expression position without `{ ... }` before a name
    /// (e.g. `server App.Api.place`).
    pub bare: bool,
    /// Second-word keywords that start a declaration block
    /// (e.g. `server fn`, `sql table`, `sql migration`, `refined type`).
    pub declarations: Vec<String>,
    /// Starts a named declaration before any lowercase identifier
    /// (e.g. `component counter (start : Float) = ...`).
    pub named: bool,
    /// Starts a block only at top level of a module, never inside expressions
    /// (e.g. `suite "name" { ... }`, `test.skip "name" { ... }`).
    pub top: bool,
    /// Accepts `[ ... ]` bracketed arguments among head arguments
    /// (e.g. `test.each [(1, 2)] ...`).
    pub bracket_args: bool,
}

impl HeadRule {
    /// Builds a parser head rule from a plugin's declared [`HeadSpec`].
    #[must_use]
    pub fn from_spec(spec: &HeadSpec, implicit: bool) -> Self {
        Self {
            head: spec.head.clone(),
            module: spec.module.clone(),
            implicit,
            bare: spec.bare,
            declarations: spec.declarations.clone(),
            named: spec.named,
            top: spec.top,
            bracket_args: spec.bracket_args,
        }
    }
}

/// Data-driven catalog of plugin block head rules used by the parser.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeadCatalog {
    rules: Vec<HeadRule>,
}

impl HeadCatalog {
    /// A catalog with no plugin heads: pure core Flick syntax.
    pub const EMPTY: Self = Self { rules: Vec::new() };

    /// Creates a catalog from the given head rules.
    #[must_use]
    pub fn new(rules: Vec<HeadRule>) -> Self {
        Self { rules }
    }

    /// Creates a catalog from `(HeadSpec, implicit)` pairs.
    #[must_use]
    pub fn from_specs<'a>(specs: impl IntoIterator<Item = (&'a HeadSpec, bool)>) -> Self {
        Self {
            rules: specs
                .into_iter()
                .map(|(spec, implicit)| HeadRule::from_spec(spec, implicit))
                .collect(),
        }
    }

    /// Inserts or replaces the rule for `rule.head`.
    pub fn push(&mut self, rule: HeadRule) {
        if let Some(existing) = self.rules.iter_mut().find(|r| r.head == rule.head) {
            *existing = rule;
        } else {
            self.rules.push(rule);
        }
    }

    /// Copies every rule from `other` into `self`.
    pub fn extend_from(&mut self, other: &Self) {
        for rule in &other.rules {
            self.push(rule.clone());
        }
    }

    /// All rules in registration order.
    #[must_use]
    pub fn rules(&self) -> &[HeadRule] {
        &self.rules
    }

    /// Returns the rule for `head`, if any.
    #[must_use]
    pub fn get(&self, head: &str) -> Option<&HeadRule> {
        self.rules.iter().find(|r| r.head == head)
    }

    /// Whether `head` is an implicit head (usable without `import ... exposing (plugin head)`).
    #[must_use]
    pub fn is_implicit(&self, head: &str) -> bool {
        self.get(head).is_some_and(|r| r.implicit)
    }

    /// Whether `head` is allowed bare in expression position before a name.
    #[must_use]
    pub fn is_bare(&self, head: &str) -> bool {
        self.get(head).is_some_and(|r| r.bare)
    }

    /// Whether `(head, word)` introduces a declaration block.
    #[must_use]
    pub fn is_declaration(&self, head: &str, word: &str) -> bool {
        self.get(head)
            .is_some_and(|r| r.declarations.iter().any(|w| w == word))
    }

    /// Whether `head <ident>` introduces a named declaration block.
    #[must_use]
    pub fn is_named(&self, head: &str) -> bool {
        self.get(head).is_some_and(|r| r.named)
    }

    /// Whether `head` starts blocks only at top level.
    #[must_use]
    pub fn is_top(&self, head: &str) -> bool {
        self.get(head).is_some_and(|r| r.top)
    }

    /// Whether `head` accepts `[ ... ]` bracketed arguments.
    #[must_use]
    pub fn bracket_args(&self, head: &str) -> bool {
        self.get(head).is_some_and(|r| r.bracket_args)
    }

    /// The module that exposes `head`, if declared.
    #[must_use]
    pub fn module_of(&self, head: &str) -> Option<&str> {
        self.get(head).map(|r| r.module.as_str())
    }
}

/// The word of generated code before the module a body opens: `open$module
/// Ui (body)`. No source writes a name with `$` in it.
pub const OPEN: &str = "open$module";

/// The word of generated code before the function an opened body of
/// several lines is applied to the list of them: `open$module Ui Ui
/// siblings$by Ui.fragment (body)`.
pub const SIBLINGS: &str = "siblings$by";

/// The attribute of generated code before a function a constant is given
/// to, naming the value that reads the constant: `@checked$by Port$check`.
pub const CHECKED_BY: &str = "@checked$by";

/// The attribute of generated code before an `unboxed` type whose values its
/// check holds for, naming the members of the check, of the value and of the
/// operation that proves one: `@invariant$of check value prove`.
pub const INVARIANT_OF: &str = "@invariant$of";

/// The names a proof reads as its goals and rules where no binder of the
/// proof has them: `equal l r`, `refl`, `trans m p q` …
pub const PROOF_WORDS: [&str; 7] = ["equal", "holds", "refl", "symm", "trans", "cong", "funext"];
