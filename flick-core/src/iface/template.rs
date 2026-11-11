//! Checked static module templates and their lexical bindings. These travel
//! with an interface; module arguments and dictionaries remain explicit.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleTemplate {
    /// Body checked against the interface's higher-order signature. A reader
    /// may instantiate it for either backend without the original checkout.
    pub source: String,
    /// Canonical target selected while checking an alias, independent of its source import aliases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materialized_body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<StaticCapture>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticCapture {
    pub bindings: Vec<StaticBinding>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<LexicalScope>,
    pub callee: Option<CalleeOrigin>,
    pub arguments: Vec<StaticArgument>,
    /// Leading import declarations copied from the lexical environment. Their
    /// use diagnostics belong to the source that originally wrote the import.
    #[serde(default)]
    pub inherited_imports: usize,
    /// Original template retained by a partial application.
    pub template: Option<String>,
    /// Lexically active expansions, including captures of anonymous parameterized modules.
    pub trace: Vec<ExpansionFrame>,
    /// Packages whose private implementation names this lexical body may use.
    pub access_packages: Vec<String>,
    /// Lexical namespaces whose implementation members this body may use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub access_modules: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticMember {
    pub parent: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticBinding {
    pub alias: String,
    pub argument: StaticArgument,
    pub origin: BindingOrigin,
}

/// The checked parameter signature that a static binding was introduced with.
/// Prefix arguments instantiate dependencies on earlier parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingOrigin {
    pub module: String,
    pub group: usize,
    pub prefix: Vec<StaticArgument>,
}

/// An argument's implementation identity and the namespaces the caller could
/// access when it supplied that argument. A callee cannot widen this view with
/// its own implementation privileges; closures retain it across partial calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticArgument {
    pub module: String,
    pub access_modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum CalleeOrigin {
    Module(String),
    Member(StaticArgument),
    Parameter {
        binding: BindingOrigin,
        members: Vec<String>,
    },
}

impl CalleeOrigin {
    pub(crate) fn signature_owner(&self) -> &str {
        match self {
            Self::Module(module) => module,
            Self::Member(view) => view
                .module
                .rsplit_once('.')
                .map_or(&view.module, |(parent, _)| parent),
            Self::Parameter { binding, .. } => &binding.module,
        }
    }
}

/// A lexical environment before a module-expression declaration. Binding
/// counts keep later applications' dictionaries out of its definition scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexicalScope {
    pub module: String,
    pub before: u32,
    pub bindings: usize,
    pub inherited_imports: usize,
}

impl LexicalScope {
    pub(crate) fn key(&self) -> String {
        format!("$scope:{}:{}", self.module, self.before)
    }
}

/// An active static application, independent of generated module names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpansionFrame {
    pub template: String,
    pub arguments: Vec<String>,
}
