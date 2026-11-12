//! Lang items: the built-in entities the compiler itself relies on. A
//! built-in module marks one with `@lang name` before its declaration (or
//! before a constructor), and the compiler asks for it by [`Lang`] instead
//! of spelling its module and name, so the library may move or rename it.

#[cfg(test)]
use std::cell::Cell;
use std::sync::OnceLock;

use crate::iface::{Entity, Namespace};
use crate::source::SourceMap;
use crate::syntax::{self, cst::Node, kind::SyntaxKind as K};

pub use crate::syntax::lang::Lang;

/// Where a lang item is declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub module: String,
    pub namespace: Namespace,
    pub name: String,
    /// `module.name`, as a service row spells it.
    pub label: String,
}

impl Item {
    /// Whether the item is `module.name`.
    pub fn is(&self, module: &str, name: &str) -> bool {
        self.module == module && self.name == name
    }

    pub fn entity(&self) -> Entity {
        Entity {
            module: self.module.clone(),
            namespace: self.namespace,
            name: self.name.clone(),
        }
    }
}

/// A service marked `@sync`: its operations run at once, without a runtime.
/// A `reader` only reads, so a stage that has it lends it to code that runs
/// later, as the clauses of a `for`.
#[derive(Debug, Clone)]
pub struct Sync {
    pub module: String,
    pub name: String,
    /// `module.name`, as a service row spells it.
    pub label: String,
    pub reader: bool,
}

/// The lang items of a set of modules, indexed by [`Lang`], and the
/// services they mark `@sync`.
#[derive(Debug)]
pub struct Table {
    items: Vec<Option<Item>>,
    pub sync: Vec<Sync>,
}

impl Table {
    /// The lang items the modules `sources` declare, as `(module, text)`.
    pub fn of(sources: &[(&str, &str)]) -> Table {
        let mut table = Table {
            items: vec![None; Lang::ALL.len()],
            sync: Vec::new(),
        };
        for &(module, text) in sources {
            // most modules declare none: only one that may is parsed
            if !may_declare(text) {
                continue;
            }
            let mut map = SourceMap::default();
            let file = map.add(format!("<builtin>/{module}.fl"), text);
            let parse = syntax::parse(text, file);
            table.sync.extend(sync_services(&parse.tree, text, module));
            for (lang, namespace, name) in declared(&parse.tree, text) {
                let name = name.unwrap_or_else(|| module.to_owned());
                let item = Item {
                    label: format!("{module}.{name}"),
                    module: module.to_owned(),
                    namespace,
                    name,
                };
                if let Some(old) = table.items[lang as usize].replace(item) {
                    panic!(
                        "lang item `{}` is declared twice, in {} and {module}",
                        lang.name(),
                        old.module
                    );
                }
            }
        }
        table
    }

    /// The declaration of `lang`, if the modules declare it.
    pub fn find(&self, lang: Lang) -> Option<&Item> {
        self.items[lang as usize].as_ref()
    }

    pub fn get(&self, lang: Lang) -> &Item {
        self.find(lang)
            .unwrap_or_else(|| panic!("no built-in module declares `@lang {}`", lang.name()))
    }
}

/// Whether `text` may hold a `@lang` or a `@sync` attribute: an `@` whose
/// next word is one of them, however they are spaced, or one that no word
/// follows, which only parsing tells apart.
fn may_declare(text: &str) -> bool {
    text.match_indices('@').any(|(at, _)| {
        let rest = text[at + 1..].trim_start();
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_'));
        let word = end.map_or(rest, |end| &rest[..end]);
        word.is_empty() || word == "lang" || word == "sync"
    })
}

/// The lang items of the built-in modules.
pub fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    #[cfg(test)]
    if let Some(table) = OVERRIDE.with(Cell::get) {
        return table;
    }
    TABLE.get_or_init(|| Table::of(super::builtin_sources()))
}

#[cfg(test)]
thread_local! {
    static OVERRIDE: Cell<Option<&'static Table>> = const { Cell::new(None) };
}

/// Runs `f` with the lang items of `table` instead of the built-in ones, on
/// this thread.
#[cfg(test)]
pub fn with_table<R>(table: Table, f: impl FnOnce() -> R) -> R {
    let table: &'static Table = Box::leak(Box::new(table));
    let old = OVERRIDE.with(|o| o.replace(Some(table)));
    let out = f();
    OVERRIDE.with(|o| o.set(old));
    out
}

/// The declaration of `lang` among the built-in modules.
pub fn item(lang: Lang) -> &'static Item {
    table().get(lang)
}

/// The module that declares `lang`, if a built-in module does.
pub fn module_of(lang: Lang) -> Option<&'static str> {
    table().find(lang).map(|item| item.module.as_str())
}

/// `module.name` of `lang`, as a service row spells it, if the modules
/// declare it.
pub fn label(lang: Lang) -> Option<&'static str> {
    table().find(lang).map(|item| item.label.as_str())
}

/// The module whose names every module sees unqualified.
pub fn prelude() -> &'static str {
    &item(Lang::Prelude).module
}

/// Whether `module.name` is the lang item `lang`; nothing is an item the
/// built-in modules leave out.
pub fn is(lang: Lang, module: &str, name: &str) -> bool {
    table().find(lang).is_some_and(|item| item.is(module, name))
}

/// The services of `module` an attribute `@sync` or `@sync reader` stands
/// before.
fn sync_services(tree: &Node, text: &str, module: &str) -> Vec<Sync> {
    let mut out = Vec::new();
    let mut pending = None;
    for child in tree.nodes() {
        if child.kind == K::Attribute {
            let mut words = (child.child_tokens()).filter(|t| t.kind == K::Ident);
            if words.next().is_some_and(|w| w.text(text) == "sync") {
                pending = Some(words.next().is_some_and(|w| w.text(text) == "reader"));
            }
            continue;
        }
        if let Some(reader) = pending.take()
            && child.kind == K::ServiceDecl
            && let Some(name) = first(child, K::Con, text)
        {
            out.push(Sync {
                label: format!("{module}.{name}"),
                module: module.to_owned(),
                name,
                reader,
            });
        }
    }
    out
}

/// The `@lang` attributes of a module with the names they mark: `None` for
/// the module itself, an attribute before its header.
pub fn declared(tree: &Node, text: &str) -> Vec<(Lang, Namespace, Option<String>)> {
    let mut found = Vec::new();
    collect(tree, text, &mut found);
    found
}

fn collect(node: &Node, text: &str, found: &mut Vec<(Lang, Namespace, Option<String>)>) {
    let mut pending: Option<Lang> = None;
    for child in node.nodes() {
        match child.kind {
            // `@derive` beside `@lang` marks the same declaration
            K::Attribute => pending = lang_of(child, text).or(pending),
            K::Members => collect(child, text, found),
            K::TypeDecl => {
                for variant in child.nodes().filter(|n| n.kind == K::Variant) {
                    let marked = variant.nodes().find(|n| n.kind == K::Attribute);
                    if let Some(lang) = marked.and_then(|a| lang_of(a, text)) {
                        let name = first(variant, K::Con, text);
                        found.push((lang, Namespace::Constructor, name));
                    }
                }
            }
            _ => {}
        }
        if child.kind == K::Attribute {
            continue;
        }
        if let Some(lang) = pending.take() {
            found.push(marked(lang, child, text));
        }
        if matches!(child.kind, K::ClassDecl | K::ServiceDecl) {
            collect(child, text, found);
        }
    }
}

/// The lang item `lang` marks on `decl`.
fn marked(lang: Lang, decl: &Node, text: &str) -> (Lang, Namespace, Option<String>) {
    match decl.kind {
        K::ModuleHeader => (lang, Namespace::Type, None),
        K::TypeDecl | K::ErrorDecl | K::ClassDecl | K::ServiceDecl => {
            (lang, Namespace::Type, first(decl, K::Con, text))
        }
        _ => (lang, Namespace::Value, first(decl, K::Ident, text)),
    }
}

/// The text of the first token of `kind` right under `node`.
fn first(node: &Node, kind: K, text: &str) -> Option<String> {
    node.child_tokens()
        .find(|t| t.kind == kind)
        .map(|t| t.text(text).to_owned())
}

/// The attributes of `tree`, at any depth.
pub fn attributes(tree: &Node) -> Vec<&Node> {
    let mut found = Vec::new();
    let mut stack = vec![tree];
    while let Some(node) = stack.pop() {
        for child in node.nodes() {
            if child.kind == K::Attribute {
                found.push(child);
            } else {
                stack.push(child);
            }
        }
    }
    found
}

/// The lang item an attribute names, if it is `@lang item`.
pub fn lang_of(attribute: &Node, text: &str) -> Option<Lang> {
    let words: Vec<&str> = attribute
        .child_tokens()
        .filter(|t| t.kind == K::Ident)
        .map(|t| t.text(text))
        .collect();
    match words[..] {
        ["lang", item] => Lang::from_name(item),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Lang, Table, may_declare, table};

    /// A module is parsed for its lang items however the parser may read
    /// the attribute, and not for an `@` of something else.
    #[test]
    fn a_lang_attribute_is_found_however_it_is_spaced() {
        for spelling in ["@lang owner", "@ lang owner", "@\tlang owner"] {
            let text =
                format!("module Fiber exposing (Owner)\n\n{spelling}\ntype Owner (s : Region)\n");
            let table = Table::of(&[("Fiber", &text)]);
            let found = table.find(Lang::Owner).map(|item| item.name.as_str());
            assert_eq!(found, Some("Owner"), "{spelling}");
        }
        assert!(may_declare("@"));
        assert!(!may_declare(
            "foreign f from \"@flick-lang/effect-ts\" \"f\""
        ));
        assert!(!may_declare("@language of the page"));
    }

    /// Without the `std` that declares `@lang owner`, as a host that hands
    /// over no implicit package checks, a named `scope` is a diagnostic, not a
    /// panic.
    #[test]
    fn a_named_scope_without_the_owner_of_std_is_a_diagnostic() {
        use crate::diagnostics::codes;
        use crate::source::SourceMap;
        use crate::types::{builtin_sources, builtins_of};
        // the built-in modules are checked once, with every lang item
        let _ = builtins_of(false);
        let sources: Vec<(&str, String)> = (builtin_sources().iter())
            .map(|(module, text)| (*module, text.replace("@lang owner", "")))
            .collect();
        let sources: Vec<(&str, &str)> = sources.iter().map(|(m, t)| (*m, t.as_str())).collect();
        let text = "module M exposing (f)\n\nf : Unit ->{} Int\nlet f () = scope s {\n  1\n}\n";
        let mut map = SourceMap::default();
        let file = map.add("M.fl".to_owned(), text);
        let said = super::with_table(Table::of(&sources), || crate::check(&map, file));
        let found: Vec<_> = (said.iter())
            .map(|d| (d.code, &text[d.span.expect("a span").range()]))
            .collect();
        assert_eq!(found, [(codes::T001, "s")], "{said:?}");
    }

    /// A service is Sync by its mark, read from the modules as the lang
    /// items are: the standard library marks its own two.
    #[test]
    fn a_service_marked_sync_runs_at_once_and_a_reader_is_lent() {
        use crate::types::ty::{Row, Type, sync_rows};
        use crate::types::{Elem, builtin_sources};
        let marked: Vec<(&str, bool)> = (table().sync.iter())
            .map(|s| (s.label.as_str(), s.reader))
            .collect();
        assert_eq!(marked, [("Prelude.Setup", false), ("Atom.Read", true)]);
        let text = "module Clock exposing (..)\n\n@sync\nservice Tick =\n  now : Unit ->{} Float\n\n@sync reader\nservice Peek =\n  see : Unit ->{} Float\n\nservice Late =\n  wait : Unit ->{} Float\n";
        let table = Table::of(&[("Clock", text)]);
        let found: Vec<(&str, bool)> = (table.sync.iter())
            .map(|s| (s.label.as_str(), s.reader))
            .collect();
        assert_eq!(found, [("Clock.Tick", false), ("Clock.Peek", true)]);
        // a closed row of marked services and no errors is a Sync stage
        let row = |labels: &[&str]| {
            Type::Row(Row {
                entries: (labels.iter())
                    .map(|l| {
                        (
                            Elem::named(*l),
                            Type::con("Clock", l.rsplit('.').next().unwrap_or(l)),
                        )
                    })
                    .collect(),
                tail: None,
                ..Row::default()
            })
        };
        let none = Type::empty_row();
        let sources: Vec<(&str, &str)> = (builtin_sources().iter().copied())
            .chain([("Clock", text)])
            .collect();
        super::with_table(Table::of(&sources), || {
            assert!(sync_rows(&row(&["Clock.Tick"]), &none));
            assert!(sync_rows(&row(&["Clock.Tick", "Clock.Peek"]), &none));
            assert!(!sync_rows(&row(&["Clock.Late"]), &none));
            assert!(!sync_rows(&row(&["Clock.Tick", "Clock.Late"]), &none));
        });
        // without the mark the service is no Sync one
        assert!(!sync_rows(&row(&["Clock.Tick"]), &none));
    }

    /// Each lang item names one declaration of the built-in modules.
    #[test]
    fn every_lang_item_is_declared() {
        let table = table();
        for &lang in Lang::ALL {
            let item = table.get(lang);
            assert!(!item.name.is_empty(), "{lang:?}");
            assert_eq!(Lang::from_name(lang.name()), Some(lang));
        }
        assert_eq!(table.get(Lang::Some).name, "Some");
        assert_eq!(table.get(Lang::FlatMap).module, "Prelude");
        assert_eq!(table.get(Lang::Instances).name, "Instances");
    }
}
