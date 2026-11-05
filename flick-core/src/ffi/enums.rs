//! A `type` alias of string or integer literals becomes an enum, an
//! `untagged type` whose constructors are the literals: `"GET" | "POST"`
//! is `Get = "GET" | Post = "POST"`. A string literal's constructor is its
//! words in Pascal case; `[enums.<Type>]` of the policy names the rest, and
//! must name every integer.

use std::collections::BTreeMap;
use std::fmt::Write;

use super::Shape;
use super::convert::{Blocked, is_type_name};

/// The literals of an enum, as TypeScript spells them.
pub(super) type Literals = Vec<String>;

/// The TypeScript texts of `types` when all are literals.
pub(super) fn literals(types: &[Shape]) -> Option<Literals> {
    (types.iter())
        .map(|t| match t {
            Shape::Literal { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// Why the literals `texts` of the union `shown` make no enum, if they do not.
pub(super) fn refused(shown: &str, texts: &[String]) -> Option<String> {
    if let Some(t) = texts.iter().find(|t| t.contains('\\')) {
        return Some(format!(
            "the literal `{t}` has an escape, which an enum of the importer does not spell"
        ));
    }
    let strings = texts.iter().all(|t| string(t).is_some());
    let integers = texts.iter().all(|t| t.parse::<i32>().is_ok());
    (!strings && !integers).then(|| {
        format!("the union `{shown}` is no enum: its literals are not all strings or all integers")
    })
}

/// The value of a string literal's TypeScript text.
fn string(text: &str) -> Option<&str> {
    text.strip_prefix('"')?.strip_suffix('"')
}

/// The enums of the generated module and the constructors they take.
#[derive(Default)]
pub(super) struct Enums {
    /// Each enum's declaration, by name.
    declared: BTreeMap<String, String>,
    /// The enum of each constructor.
    owners: BTreeMap<String, String>,
}

impl Enums {
    /// Adds the enums a binding names, all or none; `names` are the
    /// constructor names of the policy, by enum.
    pub(super) fn admit(
        &mut self,
        enums: &BTreeMap<String, Literals>,
        names: &BTreeMap<String, BTreeMap<String, String>>,
    ) -> Result<(), Blocked> {
        let mut added = Vec::new();
        for (name, texts) in enums {
            let empty = BTreeMap::new();
            let constructors = constructors(name, texts, names.get(name).unwrap_or(&empty))?;
            let declaration = declaration(name, texts, &constructors);
            match self.declared.get(name) {
                Some(d) if *d == declaration => continue,
                Some(_) => {
                    return Err(Blocked::unsupported(format!(
                        "`{name}` names more than one TypeScript type: bind them by hand under distinct names"
                    )));
                }
                None => {}
            }
            if let Some((c, other)) =
                (constructors.iter()).find_map(|c| self.owners.get(c).map(|other| (c, other)))
            {
                return Err(Blocked::policy(format!(
                    "the constructor `{c}` of `{name}` is taken by `{other}`: name it in `[enums.{name}]`"
                )));
            }
            added.push((name, declaration, constructors));
        }
        for (name, declaration, constructors) in added {
            self.declared.insert(name.clone(), declaration);
            for c in constructors {
                self.owners.insert(c, name.clone());
            }
        }
        Ok(())
    }

    /// The declarations, in the order of their names.
    pub(super) fn text(&self) -> String {
        self.declared.values().fold(String::new(), |mut out, d| {
            out.push('\n');
            out.push_str(d);
            out
        })
    }
}

/// The constructor of each literal of the enum `name`.
fn constructors(
    name: &str,
    texts: &[String],
    names: &BTreeMap<String, String>,
) -> Result<Vec<String>, Blocked> {
    let mut out: Vec<String> = Vec::new();
    for text in texts {
        let value = string(text).unwrap_or(text);
        let Some(c) = names.get(value).cloned().or_else(|| default_name(text)) else {
            return Err(Blocked::policy(format!(
                "the literal `{text}` of `{name}` has no constructor name: give it one in `[enums.{name}]`"
            )));
        };
        if out.contains(&c) {
            return Err(Blocked::policy(format!(
                "two literals of `{name}` are the constructor `{c}`: name them in `[enums.{name}]`"
            )));
        }
        out.push(c);
    }
    Ok(out)
}

/// The words of a string literal in Pascal case, when that is a type name:
/// `"GET"` → `Get`, `"content-type"` → `ContentType`, `"getItem"` → `GetItem`.
fn default_name(text: &str) -> Option<String> {
    let words = string(text)?.split(|c: char| !c.is_ascii_alphanumeric());
    let name: String = (words.filter(|w| !w.is_empty()))
        .map(|w| {
            let shouting = !w.chars().any(|c| c.is_ascii_lowercase());
            let (first, rest) = w.split_at(1);
            let rest = if shouting {
                rest.to_ascii_lowercase()
            } else {
                rest.to_owned()
            };
            first.to_ascii_uppercase() + &rest
        })
        .collect();
    is_type_name(&name).then_some(name)
}

fn declaration(name: &str, texts: &[String], constructors: &[String]) -> String {
    let mut out = format!("/// The JS literals of `{name}`.\nuntagged type {name} =");
    for (i, (c, text)) in constructors.iter().zip(texts).enumerate() {
        let bar = if i == 0 { "" } else { " |" };
        let _ = write!(out, "{bar} {c} = {text}");
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::default_name;

    #[test]
    fn a_string_literal_is_its_words_in_pascal_case() {
        let names = ["\"GET\"", "\"content-type\"", "\"getItem\"", "\"HTTP2\""].map(default_name);
        let expected = ["Get", "ContentType", "GetItem", "Http2"].map(|n| Some(n.to_owned()));
        assert_eq!(names, expected);
        assert_eq!(default_name("\"2xx\""), None);
        assert_eq!(default_name("\"\""), None);
        assert_eq!(default_name("\"é\""), None);
    }
}
