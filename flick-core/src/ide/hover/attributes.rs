//! Attributes are compiler metadata, not ordinary resolved names. Explain
//! their contract even in incomplete or otherwise invalid editor buffers.

use std::fmt::Write as _;

use super::{Ide, documentation, tree};
use crate::doc;
use crate::source::FileId;
use crate::syntax::attr::attr_parts;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::describe_targets;
use crate::types::lang::{self, Lang};

const LANG: &str = "Marks a declaration as a lang item: a built-in entity the compiler refers to by its role rather than its module and name. Used in built-in modules only; user modules cannot register lang items. Takes one item name, for example `@lang action`.";
const MAKER: &str = "Marks a value as a maker of an atom, action or view. The compiler tracks calls to it, including through other functions, to reject creating a new one on every render (`FLICK-U006`). Create it once at the top level or during component setup instead. Takes no argument.";
const CELL: &str = "Marks a type whose values hold one value each, as a state made by a pure call does. A value of such a type that is no function is not generalized over what the cell holds, and one whose type gives it variables is rejected (`FLICK-T072`). Takes no argument and belongs before a type declaration.";
const DERIVE: &str = "Marks a class as derivable with `deriving`. The implementation comes from the plugin of the class's package, or from the package named by the attribute, for example `@derive std`. Belongs before a class declaration.";
const TS: &str = "Says what the type or service declared next is in TypeScript: it is not declared in `.d.ts`, and every module that names it writes this form. In a form `{0}` is the first argument of the type, `{0:row}` that argument as a row, and `$Export` an export that `@ts import \"module\" \"Export\"` brings from a package or from a file beside the module. `@ts none` says the type has no form. The built-in types have their forms from the backend. The compiler does not check that a form is true of the values.";
const LABEL: &str = "Marks a type whose top-level values carry their names where the program runs: a value of the type that is no function is labelled `Module.name`, for the tools that show it. Takes one kind, as `@label atom`; a build keeps the labels of the kinds its `labels` setting names. Belongs before a type declaration.";
const OWNER: &str = "Marks a service whose handler owns what is made under it: a function that needs the service is no maker to those that name it (`FLICK-U006`). Takes no argument and belongs before a service declaration.";
const SYNC: &str = "Marks a service whose operations run at once, without a runtime: a stage that needs only such services and has no errors is Sync. `@sync reader` marks one that only reads, which a stage lends to code that runs later, as the clauses of a `for`. Used in built-in modules only; elsewhere the mark has no effect. Belongs before a service declaration.";
const IMPLICIT: &str = "Opts an instance into automatic conversion between two distinct ground nominal types when the expected type is known. The class must have two parameters and one pure method of type `a -> b`; the instance must have no context or type variables. Exactly one visible instance must match; conversions do not chain. Takes no argument and belongs before an instance declaration.";
const WHEN: &str = "Restricts the module to environments whose active backend and host tags satisfy the boolean condition (`&&`, `||`, `!`, and parentheses), for example `@when \"js && !browser\"`. Unused imports of the module are allowed in other environments; reachable uses report `FLICK-P064`. Belongs before the module header.";
const EXPORT: &str = "Exports a top-level function to the host under an explicit name, for example `@export \"addNumbers\"`. The target backend validates the host name, enforces uniqueness across the package, and rejects generic or dictionary-taking functions.";
const NATIVE: &str = "Attaches a target-specific native binding override to a top-level Flick function while keeping the Flick body as its reference semantics and fallback. When multiple `@native` clauses match the active environment, the one whose `when` condition implies every other matching clause wins; incomparable or equivalent matching pairs report `FLICK-F012`.";

impl Ide<'_> {
    fn native_override_hover(ov: &Node, text: &str, offset: u32) -> Option<(String, (u32, u32))> {
        let at = ov.child_tokens().find(|t| t.kind == K::At)?;
        let name = ov.child_tokens().find(|t| t.kind == K::Ident)?;
        if offset >= at.start && offset < name.end {
            let end = ov
                .nodes()
                .find(|n| n.kind == K::WhenCond)
                .map_or_else(|| ov.range().1, |w| w.range().1);
            let header = text[at.start as usize..end as usize].trim();
            return Some((
                format!("```flick\n{header}\n```\n\n{NATIVE}"),
                (at.start, name.end),
            ));
        }
        None
    }

    fn custom_attribute_description(&self, attr_name: &str, namespaced: bool) -> Option<String> {
        if !namespaced
            && let Some(spec) = self.analysis.foreign_backend.type_form_attribute(attr_name)
        {
            return Some(format!(
                "{}\n\nBackend type-form attribute (`{}`).",
                spec.doc, self.analysis.foreign_backend.backend_id
            ));
        }
        let pa = self
            .analysis
            .plugin_attributes
            .iter()
            .find(|a| a.spec.name == attr_name)?;
        let allowed = describe_targets(&pa.spec.targets);
        Some(if pa.spec.doc.is_empty() {
            format!(
                "Plugin attribute from `{}` (allowed on {allowed}).",
                pa.package
            )
        } else {
            format!(
                "{}\n\nPlugin attribute from `{}` (allowed on {allowed}).",
                pa.spec.doc, pa.package
            )
        })
    }

    pub(super) fn attribute_hover(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<(String, (u32, u32))> {
        let m = self.module(file)?;
        let text = self.text(file);
        let path = tree::path_to(&m.tree, offset);
        if let Some(ov) = path.iter().rev().find(|n| n.kind == K::NativeOverride) {
            return Self::native_override_hover(ov, text, offset);
        }
        let attribute = path.iter().find(|n| n.kind == K::Attribute)?;
        let token = attribute
            .child_tokens()
            .find(|t| t.start <= offset && offset < t.end)?;
        let parts = attr_parts(attribute, text)?;
        let mut words = attribute.child_tokens().filter(|t| t.kind == K::Ident);
        let name = words.next()?;
        let argument = if parts.namespaced {
            words.nth(1)
        } else {
            words.next()
        };
        let on_name = offset >= parts.name_span.0 && offset < parts.name_span.1;
        let range = if on_name {
            parts.name_span
        } else {
            (token.start, token.end)
        };
        let attr_name = parts.name;
        let backend_attr = (!parts.namespaced)
            .then(|| self.analysis.foreign_backend.type_form_attribute(attr_name))
            .flatten();
        let plugin_attr = self
            .analysis
            .plugin_attributes
            .iter()
            .any(|a| a.spec.name == attr_name);
        let owned_desc;
        let description = match attr_name {
            "maker" if !parts.namespaced && on_name => MAKER,
            "cell" if !parts.namespaced && on_name => CELL,
            "implicit" if !parts.namespaced && on_name => IMPLICIT,
            "derive" if !parts.namespaced => DERIVE,
            "label" if !parts.namespaced => LABEL,
            "owner" if !parts.namespaced && on_name => OWNER,
            "sync" if !parts.namespaced => SYNC,
            "when" if !parts.namespaced => WHEN,
            "export" if !parts.namespaced => EXPORT,
            "lang"
                if !parts.namespaced
                    && (on_name || Lang::from_name(token.text(text)).is_some()) =>
            {
                LANG
            }
            "ts" if backend_attr.is_some() => TS,
            _ => {
                owned_desc = self.custom_attribute_description(attr_name, parts.namespaced)?;
                owned_desc.as_str()
            }
        };
        let mut code = format!("@{attr_name}");
        if attr_name == "when" || attr_name == "export" || backend_attr.is_some() || plugin_attr {
            // its arguments are strings/numbers too: the attribute as written
            let (start, end) = attribute.range();
            text[start as usize..end as usize]
                .trim()
                .clone_into(&mut code);
        } else if let Some(argument) = argument {
            let _ = write!(code, " {}", argument.text(text));
        }
        let mut markdown = format!("```flick\n{code}\n```\n\n{description}");
        if !parts.namespaced {
            match name.text(text) {
                "derive" => {
                    if let Some(package) = argument {
                        let _ = write!(
                            markdown,
                            "\n\nThe plugin of `{}` supplies this class's `deriving` implementation.",
                            package.text(text),
                        );
                    }
                }
                "lang" => {
                    if let Some(item) = argument.and_then(|t| Lang::from_name(t.text(text))) {
                        self.lang_item_documentation(item, &mut markdown);
                    }
                }
                _ => {}
            }
        }
        Some((markdown, range))
    }

    /// Use the same registry and declaration docs as the checker. New lang
    /// items and library renames automatically get the corresponding hover.
    fn lang_item_documentation(&self, lang: Lang, markdown: &mut String) {
        let Some(item) = lang::table().find(lang) else {
            return;
        };
        let source = self.source(&item.module);
        let module = matches!(lang, Lang::Prelude | Lang::Instances);
        let name = if module {
            item.module.clone()
        } else {
            format!("{}.{}", item.module, item.name)
        };
        let _ = write!(markdown, "\n\nCompiler role `{}`: `{name}`.", lang.name());
        let docs = source.and_then(|source| {
            if module {
                doc::module_comment(&source.text)
            } else {
                documentation(&source, &item.name)
            }
        });
        if let Some(docs) = docs {
            let _ = write!(markdown, "\n\n{docs}");
        }
    }
}
