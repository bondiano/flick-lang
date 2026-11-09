//! The names a plugin watches: the places of the module that call one go to
//! its `check-calls`, with the types the checker found there.

/// The names whose places of call plugins watch, as the `watch` of their
/// packages declares them. A module checked without plugins watches none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Watched {
    /// `(module, name)`, the module empty for `*.name`, which watches the
    /// name of any module.
    names: Vec<(String, String)>,
}

/// Nothing watched, as a module checked on its own has it.
static NONE: Watched = Watched { names: Vec::new() };

impl Watched {
    /// The names `entries` declares, each `Module.name`, `Module.Sub.name`
    /// or `*.name`; anything else is left out, the manifest having refused
    /// it already.
    pub fn new<'e>(entries: impl IntoIterator<Item = &'e str>) -> Watched {
        let names = (entries.into_iter())
            .filter_map(|entry| {
                let (module, name) = entry.rsplit_once('.')?;
                let module = if module == "*" { "" } else { module };
                Some((module.to_owned(), name.to_owned()))
            })
            .collect();
        Watched { names }
    }

    /// Nothing watched.
    pub fn none() -> &'static Watched {
        &NONE
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The names of `other` watched here too.
    pub fn extend(&mut self, other: &Watched) {
        for name in &other.names {
            if !self.names.contains(name) {
                self.names.push(name.clone());
            }
        }
    }

    /// Whether a plugin watches `name` of `module`: the value itself, or a
    /// companion `T$name` of a type, as `Email$parse` is `Email.parse`.
    pub fn holds(&self, module: &str, name: &str) -> bool {
        (self.names.iter()).any(|(m, n)| {
            let same = name == n
                || (name.strip_suffix(n.as_str())).is_some_and(|owner| owner.ends_with('$'));
            same && (m.is_empty() || m == module)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Watched;

    #[test]
    fn a_watched_name_is_of_a_module_or_of_any() {
        let watched = Watched::new(["Ui.Event.queue", "*.parse"]);
        assert!(watched.holds("Ui.Event", "queue"));
        assert!(!watched.holds("Action", "queue"));
        // the companion of a refined type is its `parse`, in any module
        assert!(watched.holds("App.Users", "Email$parse"));
        assert!(watched.holds("App.Users", "parse"));
        assert!(!watched.holds("App.Users", "parsed"));
        assert!(!watched.holds("App.Users", "Email$parsed"));
        assert!(Watched::none().is_empty());
        assert!(!Watched::none().holds("Ui.Event", "queue"));
    }
}
