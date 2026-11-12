//! The types of a declaration as the compiler parsed them: as much of
//! Flick's types as a derive needs to tell what a field holds.

use std::fmt::{self, Display, Formatter};

use flick_plugin::{Written, WrittenKind};

/// A type as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    /// A named type, maybe qualified (`Shapes.Shape`), and its arguments.
    Con {
        name: String,
        args: Vec<Ty>,
    },
    /// A type variable and its arguments: `a`, `f String`.
    Var {
        name: String,
        args: Vec<Ty>,
    },
    /// A record; `open` with a row `| r`.
    Record {
        fields: Vec<(String, Ty)>,
        open: bool,
    },
    Tuple(Vec<Ty>),
    Unit,
    /// A function, as written: no wire form.
    Fun(String),
    /// A type with a capture qualifier, as written: what it holds is a
    /// scope's.
    Captured(String),
}

impl Display for Ty {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let atom = |t: &Ty| match t {
            Ty::Con { args, .. } | Ty::Var { args, .. } if !args.is_empty() => format!("({t})"),
            Ty::Fun(_) => format!("({t})"),
            _ => t.to_string(),
        };
        match self {
            Ty::Con { name, args } | Ty::Var { name, args } => {
                write!(f, "{name}")?;
                for a in args {
                    write!(f, " {}", atom(a))?;
                }
                Ok(())
            }
            Ty::Record { fields, open } => {
                let shown: Vec<String> = fields.iter().map(|(l, t)| format!("{l} : {t}")).collect();
                let tail = if *open { " | r" } else { "" };
                write!(f, "{{ {}{tail} }}", shown.join("; "))
            }
            Ty::Tuple(items) => {
                let shown: Vec<String> = items.iter().map(ToString::to_string).collect();
                write!(f, "({})", shown.join(", "))
            }
            Ty::Unit => write!(f, "()"),
            Ty::Fun(text) | Ty::Captured(text) => write!(f, "{text}"),
        }
    }
}

impl Ty {
    /// The type `self`, its variables replaced by `with`.
    pub fn substitute(&self, with: &[(String, Ty)]) -> Ty {
        let each = |args: &[Ty]| args.iter().map(|a| a.substitute(with)).collect::<Vec<_>>();
        match self {
            Ty::Var { name, args } => match with.iter().find(|(n, _)| n == name) {
                Some((_, Ty::Con { name, args: before })) => Ty::Con {
                    name: name.clone(),
                    args: before.iter().cloned().chain(each(args)).collect(),
                },
                Some((_, Ty::Var { name, args: before })) => Ty::Var {
                    name: name.clone(),
                    args: before.iter().cloned().chain(each(args)).collect(),
                },
                Some((_, other)) if args.is_empty() => other.clone(),
                _ => Ty::Var {
                    name: name.clone(),
                    args: each(args),
                },
            },
            Ty::Con { name, args } => Ty::Con {
                name: name.clone(),
                args: each(args),
            },
            Ty::Record { fields, open } => Ty::Record {
                fields: (fields.iter())
                    .map(|(l, t)| (l.clone(), t.substitute(with)))
                    .collect(),
                open: *open,
            },
            Ty::Tuple(items) => Ty::Tuple(each(items)),
            other => other.clone(),
        }
    }
}

/// The type `written` is, `text` the text its spans count in, or why a
/// derive does not read it as one.
pub fn read(text: &str, written: &Written) -> Result<Ty, String> {
    let as_written = || {
        text.get(written.span.start as usize..written.span.end as usize)
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let all = |ts: &[Written]| {
        ts.iter()
            .map(|t| read(text, t))
            .collect::<Result<Vec<_>, _>>()
    };
    Ok(match &written.kind {
        WrittenKind::Con { name, args } => Ty::Con {
            name: name.clone(),
            args: all(args)?,
        },
        WrittenKind::Var { name, args } => Ty::Var {
            name: name.clone(),
            args: all(args)?,
        },
        WrittenKind::Record { fields, open } => Ty::Record {
            fields: (fields.iter())
                .map(|(label, t)| Ok((label.clone(), read(text, t)?)))
                .collect::<Result<_, String>>()?,
            open: *open,
        },
        WrittenKind::Tuple(items) => Ty::Tuple(all(items)?),
        WrittenKind::Unit => Ty::Unit,
        // the rest of a function type does not matter
        WrittenKind::Arrow { .. } | WrittenKind::Generic => Ty::Fun(as_written()),
        WrittenKind::Captured(_) => Ty::Captured(as_written()),
        WrittenKind::Other => return Err(format!("`{}` is not a type", as_written())),
    })
}
