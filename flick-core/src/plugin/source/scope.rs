//! What a block's module has in scope, for its plugin's completion in an
//! editor: its values with their types, its types, each by the name the
//! module writes it with, and the transparent aliases of what they name, so
//! a type the plugin writes as the module does (`PureCodec BigInt UserId`)
//! picks the values whose type instantiates to it.

use std::collections::{BTreeMap, HashMap};

use flick_plugin::{Completion, Namespace};

use crate::types::{Match, Type, match_type, show_type};

/// What a module has in scope, as an editor gathers it.
#[derive(Default)]
pub struct InScope {
    /// Each value, by the name the module writes it with, and its type.
    pub values: Vec<(String, Type)>,
    /// Each type, by the name the module writes it with.
    pub types: BTreeMap<String, Type>,
    /// The transparent aliases, by module and name: their parameters and
    /// bodies.
    pub aliases: HashMap<(String, String), (Vec<String>, Type)>,
}

fn completion(label: &str, detail: Option<String>) -> Completion {
    Completion {
        label: label.to_owned(),
        detail,
        documentation: None,
        snippet: None,
    }
}

impl InScope {
    /// The answer to `in-scope`: the types, or the values whose type
    /// instantiates to `fits`; none for a `fits` this module cannot read.
    pub fn answer(&self, namespace: Namespace, fits: Option<&str>) -> Vec<Completion> {
        if namespace == Namespace::Type {
            return (self.types.keys()).map(|t| completion(t, None)).collect();
        }
        let target = match fits {
            Some(fits) => match self.read(fits) {
                Some(t) => Some(self.expand(&t, 0)),
                None => return Vec::new(),
            },
            None => None,
        };
        (self.values.iter())
            .filter(|(_, scheme)| {
                target.as_ref().is_none_or(|target| {
                    let body = match scheme {
                        Type::Forall { body, .. } => body,
                        other => other,
                    };
                    match_type(&self.expand(body, 0), target, &mut Vec::new()) == Match::Yes
                })
            })
            .map(|(label, scheme)| completion(label, Some(show_type(scheme))))
            .collect()
    }

    /// `ty` with its aliases replaced by their bodies.
    fn expand(&self, ty: &Type, depth: u32) -> Type {
        let (head, args) = ty.spine();
        if depth < 8
            && let Type::Con { module, name } = head
            && let Some((params, body)) = self.aliases.get(&(module.clone(), name.clone()))
            && params.len() == args.len()
        {
            let subst: Vec<(String, Type)> = (params.iter().cloned())
                .zip(args.into_iter().cloned())
                .collect();
            return self.expand(&body.substitute(&subst), depth + 1);
        }
        ty.map_children(&mut |t| self.expand(t, depth))
    }

    /// The type `text` writes, its names as the module writes them or
    /// qualified in full: applications of names, in parentheses or not.
    fn read(&self, text: &str) -> Option<Type> {
        let spaced = text.replace('(', " ( ").replace(')', " ) ");
        let words: Vec<&str> = spaced.split_whitespace().collect();
        let (ty, rest) = self.application(&words)?;
        rest.is_empty().then_some(ty)
    }

    fn application<'w>(&self, words: &'w [&'w str]) -> Option<(Type, &'w [&'w str])> {
        let (head, mut rest) = self.atom(words)?;
        let mut args = Vec::new();
        while let Some((arg, after)) =
            (rest.first().filter(|w| **w != ")")).and_then(|_| self.atom(rest))
        {
            args.push(arg);
            rest = after;
        }
        Some((Type::apps(head, args), rest))
    }

    fn atom<'w>(&self, words: &'w [&'w str]) -> Option<(Type, &'w [&'w str])> {
        let (first, rest) = words.split_first()?;
        if *first == "(" {
            let (inner, rest) = self.application(rest)?;
            let rest = rest.strip_prefix(&[")"])?;
            return Some((inner, rest));
        }
        // a qualified name the module does not see is that module's type
        let ty = (self.types.get(*first).cloned()).or_else(|| {
            let (module, name) = first.rsplit_once('.')?;
            Some(Type::con(module, name))
        })?;
        Some((ty, rest))
    }
}

#[cfg(test)]
mod tests {
    use flick_plugin::Namespace;

    use super::InScope;
    use crate::types::Type;

    #[test]
    fn a_value_fits_the_type_the_plugin_writes_after_its_aliases() {
        let mut scope = InScope::default();
        let codec = Type::con("Prelude", "PureCodec");
        let (big, id) = (Type::con("Prelude", "BigInt"), Type::con("App", "UserId"));
        scope.types.insert("PureCodec".into(), codec.clone());
        scope.types.insert("BigInt".into(), big.clone());
        scope.types.insert("UserId".into(), id.clone());
        scope
            .types
            .insert("Codec".into(), Type::con("App", "Codec"));
        // `type alias Codec = PureCodec BigInt UserId`
        let body = Type::apps(codec.clone(), [big.clone(), id.clone()]);
        scope
            .aliases
            .insert(("App".into(), "Codec".into()), (Vec::new(), body));
        scope.values = vec![
            ("user_id".into(), Type::con("App", "Codec")),
            ("name".into(), Type::apps(codec, [big.clone(), big])),
        ];
        let names = |fits| -> Vec<String> {
            (scope.answer(Namespace::Value, Some(fits)).into_iter())
                .map(|c| c.label)
                .collect()
        };
        assert_eq!(names("PureCodec BigInt UserId"), ["user_id"]);
        assert_eq!(names("PureCodec (BigInt) BigInt"), ["name"]);
        assert_eq!(
            names("Prelude.PureCodec Prelude.BigInt UserId"),
            ["user_id"]
        );
        assert!(names("PureCodec Nope UserId").is_empty());
        assert_eq!(scope.answer(Namespace::Type, None).len(), 4);
    }
}
