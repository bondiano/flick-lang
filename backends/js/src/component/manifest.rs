//! The `package.json` that `install` and `update` write: the project's own
//! with the packages the closure's `[target.js.dependencies]` ask for that
//! it lacks among its `dependencies`, the rest kept as written.

use flick_backend::command::Dependency;

/// The text of a `package.json` with the packages of `wanted` it lacks among
/// its `dependencies`, keeping the rest as written; `None` when it has them
/// all. `text` is `None` for a project without one.
pub(super) fn merged(text: Option<&str>, wanted: &[Dependency]) -> Result<Option<String>, String> {
    let parsed: serde_json::Value = match text {
        Some(text) => {
            serde_json::from_str(text).map_err(|e| format!("`package.json` is not JSON: {e}"))?
        }
        None if wanted.is_empty() => return Ok(None),
        None => serde_json::json!({}),
    };
    if !parsed.is_object() {
        return Err("`package.json` is not an object".into());
    }
    let mut dependencies = parsed
        .get("dependencies")
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default();
    let before = dependencies.len();
    for w in wanted {
        dependencies
            .entry(w.name.clone())
            .or_insert_with(|| w.version.clone().into());
    }
    if dependencies.len() == before {
        return Ok(None);
    }
    let lines: Vec<String> = (dependencies.iter())
        .map(|(name, version)| format!("    {}: {version}", serde_json::json!(name)))
        .collect();
    let table = format!("{{\n{}\n  }}", lines.join(",\n"));
    let Some(text) = text else {
        return Ok(Some(format!(
            "{{\n  \"private\": true,\n  \"dependencies\": {table}\n}}\n"
        )));
    };
    if let Some((start, end)) = top_level_value(text, "dependencies") {
        return Ok(Some(format!("{}{table}{}", &text[..start], &text[end..])));
    }
    let close = text
        .rfind('}')
        .ok_or_else(|| "`package.json` is not an object".to_owned())?;
    let last = text[..close].trim_end();
    let comma = if last.ends_with('{') { "" } else { "," };
    Ok(Some(format!(
        "{last}{comma}\n  \"dependencies\": {table}\n{}",
        &text[close..]
    )))
}

/// The bytes of the value of `key` in the object that is the valid JSON
/// `text`: a parse would forget the order of the rest.
fn top_level_value(text: &str, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let (mut depth, mut i) = (0, 0);
    let mut last_string = "";
    let mut value_start = None;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i + 1;
                i += 1;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                if depth == 1 && value_start.is_none() {
                    last_string = &text[start..i];
                }
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' | b',' if depth == 1 => {
                if let Some(start) = value_start {
                    return Some((start, text[..i].trim_end().len()));
                }
                if bytes[i] != b',' {
                    return None;
                }
            }
            b'}' | b']' => depth -= 1,
            b':' if depth == 1 && last_string == key && value_start.is_none() => {
                let rest = &text[i + 1..];
                value_start = Some(i + 1 + rest.len() - rest.trim_start().len());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn want(name: &str, version: &str) -> Dependency {
        Dependency {
            name: name.into(),
            version: version.into(),
            by: Some("lib".into()),
        }
    }

    #[test]
    fn a_manifest_gains_only_what_it_lacks_and_keeps_its_order() {
        let text = "{\n  \"name\": \"app\",\n  \"scripts\": { \"dependencies\": \"x\" },\n  \"dependencies\": {\n    \"b\": \"1\"\n  },\n  \"type\": \"module\"\n}\n";
        let out = merged(Some(text), &[want("a", "^2"), want("b", "1")])
            .expect("valid")
            .expect("changed");
        assert_eq!(
            out,
            "{\n  \"name\": \"app\",\n  \"scripts\": { \"dependencies\": \"x\" },\n  \"dependencies\": {\n    \"a\": \"^2\",\n    \"b\": \"1\"\n  },\n  \"type\": \"module\"\n}\n"
        );
        assert_eq!(merged(Some(&out), &[want("a", "^2")]), Ok(None));
    }

    #[test]
    fn a_manifest_without_dependencies_gains_the_table() {
        let added = merged(Some("{\n  \"name\": \"app\"\n}\n"), &[want("a", "1")]);
        assert_eq!(
            added,
            Ok(Some(
                "{\n  \"name\": \"app\",\n  \"dependencies\": {\n    \"a\": \"1\"\n  }\n}\n".into()
            ))
        );
        let empty = merged(Some("{}"), &[want("a", "1")])
            .expect("valid")
            .expect("changed");
        let value: serde_json::Value = serde_json::from_str(&empty).expect("valid JSON");
        assert_eq!(value["dependencies"]["a"], "1");
    }

    #[test]
    fn a_project_without_a_manifest_gets_one_only_when_it_needs_it() {
        assert_eq!(merged(None, &[]), Ok(None));
        let made = merged(None, &[want("a", "1")])
            .expect("valid")
            .expect("made");
        let value: serde_json::Value = serde_json::from_str(&made).expect("valid JSON");
        assert_eq!(value["dependencies"]["a"], "1");
    }

    #[test]
    fn non_object_json_returns_an_error_instead_of_panicking() {
        for bad in ["[]", "null", "123", "\"x\"", "[{\"a\":1}]"] {
            assert_eq!(
                merged(Some(bad), &[]),
                Err("`package.json` is not an object".into()),
                "empty wanted for {bad}"
            );
            assert_eq!(
                merged(Some(bad), &[want("a", "1")]),
                Err("`package.json` is not an object".into()),
                "non-empty wanted for {bad}"
            );
        }
    }
}
