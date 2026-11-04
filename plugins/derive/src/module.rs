//! The types of a module as a derive reads them: what each declares, its
//! parameters and the classes it derives.

use std::mem;

use flick_plugin::{DeriveTarget, TokenKind};

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
    let words: Vec<&str> = (block.tokens.iter())
        .take_while(|t| t.kind != TokenKind::Con)
        .map(|t| block.text_of(t.span))
        .collect();
    let has = |w: &str| words.contains(&w);
    let form = if let Some(span) = target.refined {
        Form::Refined {
            base: ty::read(block, span)?,
            known: known(target),
        }
    } else if let Some(span) = target.alias {
        Form::Alias {
            body: ty::read(block, span)?,
            opaque: has("opaque"),
        }
    } else if target.cases.is_empty() {
        Form::Abstract
    } else {
        let cases = (target.cases.iter())
            .map(|c| {
                let fields = (c.fields.iter())
                    .map(|&f| ty::read(block, f))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((c.name.clone(), fields))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let abi = if has("unboxed") {
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
        }
    };
    let mut params: Vec<(String, Kind)> = (target.params.iter())
        .map(|p| (p.clone(), annotated(target, p)))
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
        deriving: deriving(target),
    })
}

/// The kind a parameter is written with, `(r : ServiceRow)`; `Type` without.
fn annotated(target: &DeriveTarget, param: &str) -> Kind {
    let block = &target.decl;
    let texts: Vec<&str> = block.tokens.iter().map(|t| block.text_of(t.span)).collect();
    let at = texts
        .windows(3)
        .position(|w| w[0] == "(" && w[1] == param && w[2] == ":");
    let Some(at) = at else {
        return Kind::Type;
    };
    let kind: Vec<&str> = texts[at + 3..]
        .iter()
        .take_while(|t| **t != ")")
        .copied()
        .collect();
    match kind[..] {
        ["Type"] => Kind::Type,
        ["Type", "->", "Type"] => Kind::Arrow,
        ["Region"] => Kind::Region,
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

/// The classes the `deriving` of `target` names, as written.
fn deriving(target: &DeriveTarget) -> Vec<String> {
    let block = &target.decl;
    let texts: Vec<(TokenKind, &str)> = (block.tokens.iter())
        .map(|t| (t.kind, block.text_of(t.span)))
        .collect();
    let Some(at) = texts.iter().position(|(_, t)| *t == "deriving") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut path = String::new();
    let mut depth = 0usize;
    for (kind, text) in &texts[at + 1..] {
        match *text {
            "{" => depth += 1,
            "}" => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            "," => out.push(mem::take(&mut path)),
            "." => path.push('.'),
            _ if *kind == TokenKind::Con => path.push_str(text),
            _ => {}
        }
    }
    if !path.is_empty() {
        out.push(path);
    }
    out
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
