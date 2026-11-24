//! The primitive kernel: the primitives every backend implements itself, and
//! the class of every binding a `foreign` of `packages/*/src` names. The data
//! is `kernel/manifest.txt`; `reference/kernel.md` of the site specifies it.

// `VERSION`, read from the manifest by the build script.
include!(concat!(env!("OUT_DIR"), "/kernel.rs"));

/// The runtime ABI a build request names with [`VERSION`].
pub const ABI: &str = "flick.kernel";

/// The manifest this crate was built with.
pub const MANIFEST: &str = include_str!("kernel/manifest.txt");

/// What a binding is to a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// A primitive every backend implements.
    Kernel,
    /// Expressible in Flick over the kernel.
    Flick,
    /// A capability of the host, which host packages provide.
    Host,
    /// A binding to the values of one host library, kept by its adapter.
    Adapter,
}

impl Class {
    pub fn name(self) -> &'static str {
        match self {
            Class::Kernel => "kernel",
            Class::Flick => "flick",
            Class::Host => "host",
            Class::Adapter => "adapter",
        }
    }

    fn parse(word: &str) -> Option<Class> {
        [Class::Kernel, Class::Flick, Class::Host, Class::Adapter]
            .into_iter()
            .find(|class| class.name() == word)
    }
}

/// A specifier's class and the exports that have another.
#[derive(Debug)]
struct Section<'m> {
    specifier: &'m str,
    class: Class,
    exports: Vec<(&'m str, Class)>,
}

/// A parsed manifest.
#[derive(Debug)]
pub struct Manifest<'m> {
    pub version: &'m str,
    sections: Vec<Section<'m>>,
}

impl<'m> Manifest<'m> {
    /// Reads the format `kernel/manifest.txt` describes; a line it cannot
    /// read, a name given twice, a kernel default or an export line that
    /// repeats its specifier's class is an error.
    pub fn parse(text: &'m str) -> Result<Self, String> {
        let mut version = None;
        let mut sections: Vec<Section<'m>> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |why: &str| Err(format!("line {}: {why}: `{line}`", n + 1));
            let Some((head, class)) = line.rsplit_once(' ') else {
                return fail("expected a name and a class");
            };
            if head == "version" {
                if version.replace(class).is_some() {
                    return fail("a second version");
                }
                continue;
            }
            let Some(class) = Class::parse(class) else {
                return fail("unknown class");
            };
            if let Some(specifier) = head.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
                if class == Class::Kernel {
                    return fail("a kernel export has a line of its own");
                }
                if sections.iter().any(|s| s.specifier == specifier) {
                    return fail("the specifier is named twice");
                }
                sections.push(Section {
                    specifier,
                    class,
                    exports: Vec::new(),
                });
                continue;
            }
            let Some(section) = sections.last_mut() else {
                return fail("an export before any specifier");
            };
            if class == section.class {
                return fail("the export has its specifier's class");
            }
            if section.exports.iter().any(|(e, _)| *e == head) {
                return fail("the export is named twice");
            }
            section.exports.push((head, class));
        }
        Ok(Manifest {
            version: version.ok_or("the manifest states no version")?,
            sections,
        })
    }

    /// The class of `export` of `specifier` (`global` for a JS global), or
    /// `None` when the manifest does not name the specifier.
    pub fn class(&self, specifier: &str, export: &str) -> Option<Class> {
        let section = self.sections.iter().find(|s| s.specifier == specifier)?;
        Some(
            section
                .exports
                .iter()
                .find_map(|&(e, class)| (e == export).then_some(class))
                .unwrap_or(section.class),
        )
    }

    /// The specifiers the manifest names.
    pub fn specifiers(&self) -> impl Iterator<Item = &'m str> + '_ {
        self.sections.iter().map(|s| s.specifier)
    }

    /// The exports named on lines of their own, with their specifier and class.
    pub fn exports(&self) -> impl Iterator<Item = (&'m str, &'m str, Class)> + '_ {
        self.sections.iter().flat_map(|s| {
            s.exports
                .iter()
                .map(move |&(export, class)| (s.specifier, export, class))
        })
    }
}

/// The manifest this crate was built with.
pub fn manifest() -> Manifest<'static> {
    Manifest::parse(MANIFEST).expect("the kernel manifest parses")
}

#[cfg(test)]
mod tests;
