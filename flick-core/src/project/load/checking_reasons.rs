/// What a run did with a module: reused what an earlier run kept, or
/// checked it again for the inputs that differ from what was kept.
#[derive(Clone, Debug, PartialEq)]
enum Reason {
    Reused,
    #[cfg_attr(not(feature = "native-project"), expect(dead_code, reason = "only a store restores"))]
    Restored,
    /// A store keeps no module whose inputs its key does not cover.
    #[cfg_attr(not(feature = "native-project"), expect(dead_code, reason = "only a store keys"))]
    NotStorable,
    /// Checked; the inputs that differ from the kept entry's, as a person
    /// reads them, and none when nothing was kept.
    Changed(Vec<String>),
}

/// How deep an explanation follows imports that changed; deeper chains
/// end without the rest.
const CHAIN: usize = 32;

#[cfg(test)]
fn stale_reuse(cache: &ModuleCache, cached: &Key, key: &Key) -> bool {
    cache.stale && cached.text == key.text
}

#[cfg(not(test))]
const fn stale_reuse(_: &ModuleCache, _: &Key, _: &Key) -> bool {
    false
}

impl ModuleCache {
    /// Whether later runs record what they do with each module, for
    /// [`ModuleCache::reasons`].
    pub fn explain(&mut self, on: bool) {
        self.explain = on;
    }

    /// What the last run did with each module, by name, when it was asked
    /// to explain (else nothing): `reused`, `taken
    /// from the store`, `not storable`, `no earlier entry`, or the inputs
    /// that changed, each changed import followed to the change behind it
    /// (`import A.Base ← source`). Inputs are named, never their values.
    pub fn reasons(&self) -> Vec<(String, String)> {
        let mut reasons: Vec<(String, String)> = (self.reasons.iter())
            .map(|(module, _)| (module.clone(), self.explained(module, 0)))
            .collect();
        reasons.sort_unstable();
        reasons
    }

    fn explained(&self, module: &str, depth: usize) -> String {
        let Some((_, reason)) = self.reasons.iter().find(|(m, _)| m == module) else {
            return String::new();
        };
        match reason {
            Reason::Reused => "reused".to_owned(),
            Reason::Restored => "taken from the store".to_owned(),
            Reason::NotStorable => "not storable".to_owned(),
            Reason::Changed(labels) if labels.is_empty() => "no earlier entry".to_owned(),
            Reason::Changed(labels) => {
                let explained: Vec<String> = (labels.iter())
                    .map(|label| {
                        let behind = (label.strip_prefix("import "))
                            .filter(|_| depth < CHAIN)
                            .filter(|import| {
                                (self.reasons.iter()).any(|(m, r)| {
                                    m == import && matches!(r, Reason::Changed(l) if !l.is_empty())
                                })
                            })
                            .map(|import| self.explained(import, depth + 1));
                        match behind {
                            Some(behind) => format!("{label} ← {behind}"),
                            None => label.clone(),
                        }
                    })
                    .collect();
                explained.join("; ")
            }
        }
    }
}

impl Key {
    /// The inputs this kept key differs from `current` in; `imports` names
    /// the imports in the order of the key's.
    fn differs<'a>(&self, current: &Key, imports: impl Iterator<Item = &'a str>) -> Vec<String> {
        let mut labels = Vec::new();
        let flags = [
            (self.file != current.file, "file"),
            (self.text != current.text, "source"),
            (self.config != current.config, "config"),
            (
                self.lower != current.lower || self.cached_ir != current.cached_ir,
                "mode",
            ),
            (
                self.virtual_contracts != current.virtual_contracts
                    || self.virtual_capture != current.virtual_capture,
                "module arguments",
            ),
            (self.watching != current.watching, "plugin inputs"),
        ];
        labels.extend((flags.iter()).filter(|(differs, _)| *differs).map(|(_, l)| (*l).to_owned()));
        if self.imports.len() == current.imports.len() {
            for ((kept, now), name) in self.imports.iter().zip(&current.imports).zip(imports) {
                if kept != now {
                    labels.push(format!("import {name}"));
                }
            }
        } else if self.text == current.text {
            labels.push("imports".to_owned());
        }
        labels
    }
}

impl Checking<'_> {
    /// The key a memory cache keeps `loaded`'s output by: every input its
    /// checking reads.
    fn memory_key(
        &self,
        loaded: &Loaded,
        interfaces: &[Option<Interface>],
        comparable: &[Option<Arc<Interface>>],
        lookup: &dyn Fn(&str) -> Option<Interface>,
        contracts: &dyn Fn(&str) -> Option<Interface>,
    ) -> Key {
        let known = |name: &str| self.visible_interface(loaded, interfaces, name);
        Key {
            file: loaded.file,
            // the code of its blocks too, which other modules' blocks change
            text: self.text_of(loaded).to_owned(),
            config: self.project.map(|p| p.config.clone()),
            lower: self.lower,
            cached_ir: self.cached_ir,
            imports: loaded
                .syntax
                .imports
                .iter()
                .chain(&loaded.syntax.inline_imports)
                .map(|i| {
                    let interface =
                        if self
                            .raw_interface(interfaces, &i.module)
                            .is_some_and(|raw| {
                                raw.member_of
                                    .as_ref()
                                    .is_some_and(|member| !loaded.can_access_member(member))
                            })
                        {
                            known(&i.module).map(comparable_copy)
                        } else {
                            match self.by_name.get(&i.module) {
                                Some(&j) => comparable[j].clone(),
                                None => self.from_packages.get(&i.module).map(comparable_copy),
                            }
                        };
                    (interface, self.by_name.contains_key(&i.module))
                })
                .collect(),
            virtual_contracts: self.virtual_contract_inputs(loaded, comparable, lookup, contracts),
            virtual_capture: loaded.virtual_of.as_ref().map(|v| v.capture.clone()),
            watching: (self.watching.iter())
                .map(|w| w.plugin.id().to_owned())
                .collect(),
        }
    }
}

/// The inputs whose digests differ between the parts of the key a module
/// was saved under last and those of its key now, as a person reads them;
/// `imports` names the key's `import n` parts.
#[cfg(feature = "native-project")]
pub(super) fn changed_inputs(saved: &[(String, String)], now: &[(String, String)], imports: &[&str]) -> Vec<String> {
    let only = |these: &[(String, String)], those: &[(String, String)]| -> Vec<String> {
        (these.iter())
            .filter(|part| !those.contains(part))
            .map(|(label, _)| label.clone())
            .collect()
    };
    let mut labels = only(saved, now);
    labels.extend(only(now, saved));
    let mut named: Vec<String> = (labels.iter())
        .map(|label| match label.as_str() {
            "salt" => "compiler".to_owned(),
            "mode" | "config" | "path" | "source" => label.clone(),
            // the text checking reads is the source with the code of its blocks
            "text" => "plugin output".to_owned(),
            "modules" => "project modules".to_owned(),
            _ => match label.strip_prefix("import ") {
                Some(n) => (n.parse::<usize>().ok())
                    .and_then(|n| imports.get(n))
                    .map_or_else(|| "imports".to_owned(), |module| format!("import {module}")),
                None => "plugin inputs".to_owned(),
            },
        })
        .collect();
    if named.iter().any(|l| l == "source") {
        named.retain(|l| l != "plugin output");
    }
    named.sort_unstable();
    named.dedup();
    named
}
