//! Edits of `[dependencies]` in the text of `flick.toml`, keeping the rest
//! of the file as written.

use flick_core::project::config::Source;

/// `name = { … }` for `source`.
pub fn entry(name: &str, source: &Source) -> String {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let fields = match source {
        Source::Path(path) => format!("path = {}", q(path)),
        Source::Git { url, tag, subdir } => {
            let subdir = subdir
                .as_ref()
                .map_or(String::new(), |s| format!(", subdir = {}", q(s)));
            format!("git = {}, tag = {}{subdir}", q(url), q(tag))
        }
        Source::Url { url, sha256 } => format!("url = {}, sha256 = {}", q(url), q(sha256)),
    };
    format!("{name} = {{ {fields} }}")
}

/// The lines of the `[dependencies]` table: the header's index and the end.
fn table(lines: &[&str]) -> Option<(usize, usize)> {
    let header = lines.iter().position(|l| l.trim() == "[dependencies]")?;
    let end = (header + 1..lines.len())
        .find(|&i| lines[i].trim_start().starts_with('['))
        .unwrap_or(lines.len());
    Some((header, end))
}

/// The line of the dependency `name` among `lines[from..to]`.
fn line_of(lines: &[&str], (from, to): (usize, usize), name: &str) -> Option<usize> {
    (from..to).find(|&i| {
        let line = lines[i].trim_start();
        line.strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

/// `text` with the dependency `line` in `[dependencies]`, replacing one of
/// the same name.
// shortcut: entries are one line each; a multi-line inline table of the same name is not replaced, take toml_edit when one is
pub fn add(text: &str, name: &str, line: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    let Some((header, end)) = table(&lines) else {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[dependencies]".to_owned());
        out.push(line.to_owned());
        return out.join("\n") + "\n";
    };
    if let Some(i) = line_of(&lines, (header + 1, end), name) {
        line.clone_into(&mut out[i]);
    } else {
        let last = (header + 1..end)
            .rev()
            .find(|&i| !lines[i].trim().is_empty())
            .unwrap_or(header);
        out.insert(last + 1, line.to_owned());
    }
    out.join("\n") + "\n"
}

/// `text` without the dependency `name`, if `[dependencies]` has it.
pub fn remove(text: &str, name: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (header, end) = table(&lines)?;
    let at = line_of(&lines, (header + 1, end), name)?;
    let kept: Vec<&str> = (lines.iter().enumerate())
        .filter(|&(i, _)| i != at)
        .map(|(_, l)| *l)
        .collect();
    Some(kept.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
    use super::{add, entry, remove};
    use flick_core::project::config::Source;

    #[test]
    fn adds_replaces_and_removes_one_dependency() {
        let base = "schema-version = 1\n\n[project]\nkind = \"app\"\n";
        let geo = entry("geo", &Source::Path("../geo".into()));
        let one = add(base, "geo", &geo);
        assert_eq!(one, format!("{base}\n[dependencies]\n{geo}\n"));
        let text = Source::Git {
            url: "https://x/text".into(),
            tag: "v1".into(),
            subdir: None,
        };
        let two = add(&one, "text", &entry("text", &text));
        assert!(two.ends_with(
            "geo = { path = \"../geo\" }\ntext = { git = \"https://x/text\", tag = \"v1\" }\n"
        ));
        let moved = add(&two, "geo", &entry("geo", &Source::Path("../geo2".into())));
        assert!(moved.contains("geo = { path = \"../geo2\" }") && !moved.contains("\"../geo\""));
        assert_eq!(remove(&two, "text"), Some(one.clone()));
        assert_eq!(remove(&one, "text"), None);
        // a name that only starts like another is another
        assert_eq!(
            remove(&add(&one, "geom", "geom = { path = \"x\" }"), "geo")
                .map(|t| t.contains("geom")),
            Some(true)
        );
    }
}
