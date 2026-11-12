//! The types of a module as a derive reads them: what each declares, its
//! parameters and the classes it derives.

use flick_plugin::{DeriveTarget, literal_text};

use crate::ty::{self, Ty};

/// What a type declaration declares.
#[derive(Debug, Clone)]
pub enum Form {
    /// Constructors and the types of their fields.
    Adt {
        cases: Vec<(String, Vec<Ty>)>,
        abi: Abi,
        /// `type error E = E …`.
        error: bool,
        /// The value of each constructor's literal, of an enum.
        literals: Vec<String>,
    },
    Alias {
        body: Ty,
        opaque: bool,
    },
    Refined {
        base: Ty,
        /// The check an exporter knows, as `Checks.email` is `"email"`.
        known: Option<&'static str>,
    },
    /// A type without a definition here.
    Abstract,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abi {
    Tagged,
    Unboxed,
    Untagged,
    /// An enum, of `Int` literals or of `String` ones.
    Literal {
        int: bool,
    },
}

/// The kind of a parameter, for its witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `Type`: a witness `Schema a`.
    Type,
    /// `Type -> Type`: a witness `forall x. Schema x -> Schema (f x)`.
    Arrow,
    /// A region: a value of the type is a scoped resource.
    Region,
    /// Rows and other kinds: no witness.
    Other,
}

/// A type of the module.
#[derive(Debug, Clone)]
pub struct Local {
    /// Its name, unqualified.
    pub name: String,
    pub params: Vec<(String, Kind)>,
    pub form: Form,
    /// The classes its `deriving` names, as written: `Schema`, `Cli.Value`.
    pub deriving: Vec<String>,
}

impl Local {
    pub fn derives(&self, class: &str) -> bool {
        self.deriving
            .iter()
            .any(|d| d == class || d == &format!("Prelude.{class}"))
    }

    /// The parameters that take a witness, with their kinds.
    pub fn witnesses(&self) -> impl Iterator<Item = &(String, Kind)> {
        self.params
            .iter()
            .filter(|(_, k)| matches!(k, Kind::Type | Kind::Arrow))
    }

    pub fn has_witnesses(&self) -> bool {
        self.witnesses().next().is_some()
    }
}

/// The type `target` declares, or why it does not read.
pub fn read(target: &DeriveTarget) -> Result<Local, String> {
    let block = &target.decl;
    let has = |w: &str| target.modifiers.iter().any(|m| m == w);
    let form = if let Some(base) = &target.wraps {
        Form::Refined {
            base: ty::read(&block.text, base)?,
            known: known(target),
        }
    } else if let Some(body) = &target.alias {
        Form::Alias {
            body: ty::read(&block.text, body)?,
            opaque: has("opaque"),
        }
    } else if target.cases.is_empty() {
        Form::Abstract
    } else {
        let cases = (target.cases.iter())
            .map(|c| {
                let fields = (c.fields.iter())
                    .map(|f| ty::read(&block.text, f))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((c.name.clone(), fields))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let written: Vec<&str> = (target.cases.iter())
            .filter_map(|c| c.literal.map(|s| block.text_of(s)))
            .collect();
        let abi = if let Some(first) = written.first() {
            Abi::Literal {
                int: !first.ends_with('"'),
            }
        } else if has("unboxed") {
            Abi::Unboxed
        } else if has("untagged") {
            Abi::Untagged
        } else {
            Abi::Tagged
        };
        Form::Adt {
            cases,
            abi,
            error: has("error"),
            literals: written.into_iter().map(literal_text).collect(),
        }
    };
    let mut params: Vec<(String, Kind)> = (target.params.iter())
        .enumerate()
        .map(|(i, p)| {
            (
                p.clone(),
                kind(target.kinds.get(i).map_or("", String::as_str)),
            )
        })
        .collect();
    for (name, kind) in &mut params {
        if *kind == Kind::Type && applied(&form, name) {
            *kind = Kind::Arrow;
        }
    }
    Ok(Local {
        name: target
            .name
            .rsplit('.')
            .next()
            .unwrap_or(&target.name)
            .to_owned(),
        params,
        form,
        deriving: target.deriving.clone(),
    })
}

/// The kind a parameter is written with, `(r : ServiceRow)`; `Type` without.
fn kind(written: &str) -> Kind {
    match written {
        "" | "Type" => Kind::Type,
        "Type -> Type" => Kind::Arrow,
        "Region" => Kind::Region,
        _ => Kind::Other,
    }
}

/// Whether the parameter `name` is applied to arguments in `form`.
fn applied(form: &Form, name: &str) -> bool {
    fn go(ty: &Ty, name: &str) -> bool {
        match ty {
            Ty::Var { name: n, args } => {
                (n == name && !args.is_empty()) || args.iter().any(|a| go(a, name))
            }
            Ty::Con { args, .. } | Ty::Tuple(args) => args.iter().any(|a| go(a, name)),
            Ty::Record { fields, .. } => fields.iter().any(|(_, t)| go(t, name)),
            _ => false,
        }
    }
    match form {
        Form::Adt { cases, .. } => cases.iter().flat_map(|(_, f)| f).any(|t| go(t, name)),
        Form::Alias { body, .. } => go(body, name),
        Form::Refined { base, .. } => go(base, name),
        Form::Abstract => false,
    }
}

/// The check an exporter knows of a refined type's predicate.
fn known(target: &DeriveTarget) -> Option<&'static str> {
    let block = &target.decl;
    let texts: Vec<&str> = block.tokens.iter().map(|t| block.text_of(t.span)).collect();
    let at = texts.iter().position(|t| *t == "where")?;
    let rest: Vec<&str> = texts[at + 1..]
        .iter()
        .take_while(|t| **t != "deriving")
        .copied()
        .collect();
    match rest[..] {
        ["Checks", ".", "email"] => Some("email"),
        ["Checks", ".", "non_empty"] => Some("non_empty"),
        ["Checks", ".", "positive"] => Some("positive"),
        _ => None,
    }
}

/// The types of a module.
pub struct Module {
    pub types: Vec<Local>,
}

impl Module {
    /// The types of `targets` that read; one that does not is left out.
    pub fn new(targets: &[DeriveTarget]) -> Module {
        Module {
            types: targets.iter().filter_map(|t| read(t).ok()).collect(),
        }
    }

    pub fn local(&self, name: &str) -> Option<&Local> {
        self.types.iter().find(|t| t.name == name)
    }

    /// The local types that derive `class` and that `name`'s, which does,
    /// refers to and that refer back to it: its recursive group.
    pub fn group(&self, name: &str, class: &str) -> Vec<String> {
        let reaches = |from: &str| self.reached(from, class);
        let mine = reaches(name);
        (mine.iter())
            .filter(|other| reaches(other).iter().any(|n| n == name))
            .cloned()
            .collect()
    }

    /// The local types deriving `class` that `from` refers to, at any depth.
    fn reached(&self, from: &str, class: &str) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        let mut todo = vec![from.to_owned()];
        while let Some(next) = todo.pop() {
            let Some(local) = self.local(&next) else {
                continue;
            };
            for named in self.mentions(local) {
                let derives = self.local(&named).is_some_and(|l| l.derives(class));
                if derives && !seen.contains(&named) {
                    seen.push(named.clone());
                    todo.push(named);
                }
            }
        }
        seen
    }

    /// The unqualified names `local`'s representation mentions, through the
    /// local aliases it uses.
    fn mentions(&self, local: &Local) -> Vec<String> {
        let mut out = Vec::new();
        let mut aliases: Vec<String> = Vec::new();
        let mut todo: Vec<Ty> = match &local.form {
            Form::Adt { cases, .. } => cases.iter().flat_map(|(_, f)| f.clone()).collect(),
            Form::Alias { body, .. } => vec![body.clone()],
            Form::Refined { base, .. } => vec![base.clone()],
            Form::Abstract => Vec::new(),
        };
        while let Some(ty) = todo.pop() {
            match ty {
                Ty::Con { name, args } => {
                    todo.extend(args);
                    if let Some(Local {
                        form:
                            Form::Alias {
                                body,
                                opaque: false,
                            },
                        ..
                    }) = self.local(&name)
                        && !aliases.contains(&name)
                    {
                        aliases.push(name.clone());
                        todo.push(body.clone());
                    }
                    out.push(name);
                }
                Ty::Var { args, .. } | Ty::Tuple(args) => todo.extend(args),
                Ty::Record { fields, .. } => todo.extend(fields.into_iter().map(|(_, t)| t)),
                _ => {}
            }
        }
        out
    }
}
