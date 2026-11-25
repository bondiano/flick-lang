//! The Lua kernel table, `kernel.txt`: what the Lua target does with each
//! binding of Flick's own kernel (a `flick:` specifier), and whether a call
//! of it suspends the fiber. `check-target` reads its facts here.

/// The table this crate was built with.
const TABLE: &str = include_str!("../kernel.txt");

/// What the Lua target does with a binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Row {
    /// Whether a call suspends the fiber until it resumes.
    pub suspends: bool,
}

/// The rows of one specifier: its own and its exports' of their own.
#[derive(Debug)]
struct Section<'t> {
    specifier: &'t str,
    row: Row,
    exports: Vec<(&'t str, Row)>,
}

#[derive(Debug)]
pub(crate) struct Table<'t> {
    sections: Vec<Section<'t>>,
}

/// A status and its flags: `refused`, then `suspends` or nothing.
fn row(words: &[&str]) -> Option<Row> {
    match words {
        ["refused"] => Some(Row { suspends: false }),
        ["refused", "suspends"] => Some(Row { suspends: true }),
        _ => None,
    }
}

impl<'t> Table<'t> {
    /// Reads the format the header of `kernel.txt` describes; a line it
    /// cannot read or a name given twice is an error.
    pub fn parse(text: &'t str) -> Result<Table<'t>, String> {
        let mut sections: Vec<Section<'t>> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |why: &str| Err(format!("line {}: {why}: `{line}`", n + 1));
            let words: Vec<&str> = line.split_whitespace().collect();
            let Some((&head, status)) = words.split_first() else {
                return fail("expected a name and a status");
            };
            let Some(row) = row(status) else {
                return fail("expected `refused`, then `suspends` or nothing");
            };
            if let Some(specifier) = head.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
                if sections.iter().any(|s| s.specifier == specifier) {
                    return fail("the specifier is named twice");
                }
                sections.push(Section {
                    specifier,
                    row,
                    exports: Vec::new(),
                });
                continue;
            }
            let Some(section) = sections.last_mut() else {
                return fail("an export before any specifier");
            };
            if section.exports.iter().any(|(e, _)| *e == head) {
                return fail("the export is named twice");
            }
            section.exports.push((head, row));
        }
        Ok(Table { sections })
    }

    /// The row of `export` of `specifier`, or `None` when the table does
    /// not name the specifier.
    pub fn row(&self, specifier: &str, export: &str) -> Option<Row> {
        let section = self.sections.iter().find(|s| s.specifier == specifier)?;
        Some(
            (section.exports.iter())
                .find_map(|&(e, row)| (e == export).then_some(row))
                .unwrap_or(section.row),
        )
    }
}

/// The table of `kernel.txt`.
pub(crate) fn table() -> Table<'static> {
    Table::parse(TABLE).expect("the Lua kernel table parses")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use flick_backend_support::kernel::{Class, manifest};

    use super::{Row, Table, table};

    #[test]
    fn the_table_names_every_specifier_and_primitive_of_the_kernel() {
        let manifest = manifest();
        let table = table();
        let wanted: BTreeSet<&str> = (manifest.specifiers())
            .filter(|s| s.starts_with("flick:"))
            .collect();
        let named: BTreeSet<&str> = table.sections.iter().map(|s| s.specifier).collect();
        assert_eq!(
            named, wanted,
            "kernel.txt must have a section per `flick:` specifier"
        );
        for (specifier, export, class) in manifest.exports() {
            if class != Class::Kernel || !specifier.starts_with("flick:") {
                continue;
            }
            let section = (table.sections.iter())
                .find(|s| s.specifier == specifier)
                .expect("a section");
            assert!(
                section.exports.iter().any(|(e, _)| *e == export),
                "the kernel primitive `{export}` of `{specifier}` has no line in kernel.txt"
            );
        }
    }

    #[test]
    fn an_export_has_its_own_row_or_its_specifiers() {
        let table = table();
        let suspends = Some(Row { suspends: true });
        let returns = Some(Row { suspends: false });
        assert_eq!(table.row("flick:Effect", "sleep"), suspends);
        assert_eq!(table.row("flick:Deferred", "await"), suspends);
        assert_eq!(table.row("flick:rt", "fiberJoin"), suspends);
        assert_eq!(table.row("flick:Effect", "succeed"), returns);
        assert_eq!(table.row("flick:Unknown", "x"), None);
    }

    #[test]
    fn a_malformed_table_is_an_error() {
        for (text, why) in [
            ("[flick:A] implemented\n", "expected `refused`"),
            ("x refused\n", "an export before any specifier"),
            ("[flick:A] refused\n[flick:A] refused\n", "named twice"),
            (
                "[flick:A] refused\nx refused\nx refused suspends\n",
                "named twice",
            ),
        ] {
            let error = Table::parse(text).expect_err(text);
            assert!(error.contains(why), "{text}: {error}");
        }
    }
}
