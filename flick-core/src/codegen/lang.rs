//! The lang items code generation knows by their role, found in the table
//! the program gives its backend: the built-in entities a backend has its
//! own representation of, whatever the library names them.

use crate::ir::LangItem;
use flick_ir::Lang;

/// The module and name of each lang item of a program, by [`Lang`].
pub struct Known<'a> {
    items: Vec<Option<(&'a str, &'a str)>>,
}

impl<'a> Known<'a> {
    /// The items of `table`; one whose role this compiler does not know is
    /// of no use to it.
    pub fn of(table: &'a [LangItem]) -> Known<'a> {
        let mut items = vec![None; Lang::ALL.len()];
        for item in table {
            if let Some(lang) = Lang::from_name(&item.lang) {
                items[lang as usize] = Some((item.module.as_str(), item.name.as_str()));
            }
        }
        Known { items }
    }

    /// Whether `module.name` is the lang item `lang`.
    pub fn is(&self, lang: Lang, module: &str, name: &str) -> bool {
        self.items[lang as usize].is_some_and(|(m, n)| m == module && n == name)
    }
}

#[cfg(test)]
mod tests {
    use super::Known;
    use crate::ir::LangItem;
    use flick_ir::Lang;
    use flick_ir::Namespace;

    /// What the table says, whatever the library calls the entities.
    #[test]
    fn a_lang_item_is_what_the_table_says() {
        let item = |lang: &str, name: &str| LangItem {
            lang: lang.to_owned(),
            module: "Prelude".to_owned(),
            namespace: Namespace::Constructor,
            name: name.to_owned(),
        };
        let table = [item("some", "Just"), item("no_such_role", "Some")];
        let known = Known::of(&table);
        assert!(known.is(Lang::Some, "Prelude", "Just"));
        assert!(!known.is(Lang::Some, "Prelude", "Some"));
        assert!(!known.is(Lang::None, "Prelude", "Nothing"));
    }
}
