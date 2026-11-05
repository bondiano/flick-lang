//! The checker's schemes of the exports of the Prelude and of the modules
//! the sugar may take as terms of `Elab.Scheme` in Lean
//! (`formal/Formal/Elab/PreludeSchemes.lean`), by the translation of the Prelude schemes with `CRow` qualifiers
//! (`docs/formal/elaboration.md`):
//!
//! 1. `(T ā)^q` is `T q ā` (`appQ`) for a type with hidden storage and for
//!    a variable over containers; data without storage and a type variable
//!    drop `q`; `(A → B)^q` is `.arr φ A B q`, without `q` `.cnil`; an arrow
//!    with rows `{r; e}` is at the stage `R r e`; a row is a variable,
//!    `.cnil`, a named element `.rnm c t` or a union `.run`, also as an
//!    argument of a type.
//! 2. `∀v. a^v → …` of a parameter is instantiated `v := a`.
//! 3. A qualifier in a negative position is a variable: a single variable
//!    of the checker stays itself at its first such position, the rest are
//!    new; a missing qualifier of a stored type is new.
//! 4. In a positive position a variable that qualified negative positions
//!    is what those arguments reach, left to right through `cun`: `q ∪ ā` of
//!    data, `q` and the new variables inside of a function.
//! 5. Binders: the class parameter, the checker's `∀` that remain in their
//!    order, then the new variables in the order they appear.
//!
//! A scheme the model cannot write is a line of the skipped list, with its
//! reason (`docs/formal/elaboration.md#the-scheme-generator-translated-only-the-sugar-names`).

mod tr;

use std::fmt::{self, Write};

use flick_core::iface::{Decl, Export};
use flick_core::types::{self as fc, Kind, show_type};

use self::tr::{Tr, Ty, occurs};

/// The modules whose exports the model takes, in this order.
const MODULES: [&str; 5] = ["Prelude", "Seq", "Option", "Result", "List"];

/// Identifiers of the checker that are Lean keywords.
const KEYWORDS: [&str; 4] = ["show", "from", "at", "fun"];

/// Why an export has no `def`.
#[derive(Debug)]
enum Skip {
    /// `∀` of a parameter other than `∀v. a^v → …` (`docs/formal/elaboration.md#the-model-allows-polymorphic-types-only-in-dictionary-fields`)
    Rank,
    /// a region parameter
    Region,
    Other(String),
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Skip::Rank => f.write_str("rank"),
            Skip::Region => f.write_str("region"),
            Skip::Other(why) => write!(f, "other: {why}"),
        }
    }
}

fn ident(s: &str) -> String {
    if KEYWORDS.contains(&s) {
        format!("«{s}»")
    } else {
        s.to_string()
    }
}

/// The Lean name of an export: a name of `Prelude` stays bare.
fn lean_name(module: &str, name: &str) -> String {
    if module == "Prelude" {
        ident(name)
    } else {
        format!("{module}.{}", ident(name))
    }
}

fn lean(t: &Ty, vars: &[String], out: &mut String) {
    let arg = |t: &Ty, out: &mut String| {
        if matches!(t, Ty::Nil) {
            lean(t, vars, out);
        } else {
            out.push('(');
            lean(t, vars, out);
            out.push(')');
        }
    };
    let bin = |op: &str, a: &Ty, b: &Ty, out: &mut String| {
        out.push_str(op);
        arg(a, out);
        out.push(' ');
        arg(b, out);
    };
    match t {
        Ty::Var(v) => {
            let i = vars.iter().position(|w| w == v).expect("bound");
            let _ = write!(out, ".var {i}");
        }
        Ty::Con(c) => {
            let _ = write!(out, ".con {}", ident(c));
        }
        Ty::App(a, b) => bin(".app ", a, b, out),
        Ty::Arr(rows, a, b, q) => {
            match rows {
                Some((r, e)) => {
                    out.push_str(".arr (.R ");
                    arg(r, out);
                    out.push(' ');
                    arg(e, out);
                    out.push_str(") ");
                }
                None => out.push_str(".arr .P "),
            }
            for (i, t) in [a, b, q].into_iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                arg(t, out);
            }
        }
        Ty::Nil => out.push_str(".cnil"),
        Ty::Un(a, b) => bin(".cun ", a, b, out),
        Ty::RUn(a, b) => bin(".run ", a, b, out),
        Ty::Nm(c, a) => {
            let _ = write!(out, ".rnm {} ", ident(c));
            arg(a, out);
        }
        Ty::Pos(_) => unreachable!("resolved"),
    }
}

/// The `def` of one scheme: its parameters are the classes of its context,
/// the class of a method first, then the type constructors in the order
/// they appear.
fn def(module: &str, name: &str, scheme: &fc::Type, class: Option<&str>) -> Result<String, Skip> {
    let (vars, constraints, body) = match scheme {
        fc::Type::Forall {
            vars,
            constraints,
            body,
        } => (&vars[..], &constraints[..], &**body),
        t => (&[][..], &[][..], t),
    };
    let mut tr = Tr::default();
    for b in vars {
        match &b.kind {
            Kind::Type | Kind::CaptureRow | Kind::ServiceRow | Kind::ErrorRow => {}
            Kind::Arrow(..) => {
                tr.containers.insert(b.name.clone());
            }
            Kind::Region => return Err(Skip::Region),
            k => return Err(Skip::Other(format!("a variable of kind {k:?}"))),
        }
    }
    // the class of a method comes first, its parameter is the first binder
    let mut ctx: Vec<(String, Vec<&fc::Type>)> = Vec::new();
    let mut first = None;
    for c in constraints {
        let (head, args) = c.spine();
        let fc::Type::Con { name: cls, .. } = head else {
            return Err(Skip::Other(format!("a constraint {}", show_type(c))));
        };
        let entry = (cls.to_lowercase(), args);
        if Some(cls.as_str()) == class {
            if let [fc::Type::Var(f)] = &entry.1[..] {
                first = Some(f.clone());
            }
            ctx.insert(0, entry);
        } else {
            ctx.push(entry);
        }
    }
    let t = tr.ty(body, false)?;
    let t = tr.resolve(t);
    let ctx: Vec<(String, Vec<Ty>)> = ctx
        .into_iter()
        .map(|(c, args)| {
            let args = args.into_iter().map(|a| tr.ty(a, false));
            Ok((c, args.collect::<Result<_, Skip>>()?))
        })
        .collect::<Result<_, Skip>>()?;
    let binders: Vec<String> = first
        .iter()
        .chain(
            vars.iter()
                .map(|b| &b.name)
                .filter(|v| first.as_ref() != Some(*v)),
        )
        .filter(|v| occurs(&t, v) || ctx.iter().flat_map(|c| &c.1).any(|a| occurs(a, v)))
        .chain(&tr.fresh)
        .cloned()
        .collect();
    let mut params: Vec<String> = Vec::new();
    for p in ctx.iter().map(|c| &c.0).chain(&tr.cons) {
        if !params.contains(p) {
            params.push(p.clone());
        }
    }
    let mut out = format!("/-- `{module}.{name} : {}` -/\n", show_type(scheme));
    let _ = write!(out, "def {}", lean_name(module, name));
    if !params.is_empty() {
        let params: Vec<String> = params.iter().map(|p| ident(p)).collect();
        let _ = write!(out, " ({} : Nat)", params.join(" "));
    }
    let _ = write!(out, " : Scheme :=\n  ⟨{}, [", binders.len());
    for (i, (c, args)) in ctx.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "⟨{}, [", ident(c));
        for (j, a) in args.iter().enumerate() {
            if j > 0 {
                out.push_str(", ");
            }
            lean(a, &binders, &mut out);
        }
        out.push_str("]⟩");
    }
    out.push_str("],\n    ");
    lean(&t, &binders, &mut out);
    out.push_str(", .cnil⟩\n");
    Ok(out)
}

/// The exports with a scheme of [`MODULES`], in order.
fn exports() -> impl Iterator<Item = (&'static str, &'static Export)> {
    MODULES.into_iter().flat_map(|module| {
        fc::builtins()
            .iter()
            .filter(move |i| i.module == module)
            .flat_map(|i| &i.exports)
            .filter(|e| e.scheme.is_some())
            .map(move |e| (module, e))
    })
}

/// The text of `formal/Formal/Elab/PreludeSchemes.lean`.
pub fn prelude_schemes() -> String {
    let mut out = String::from(
        "-- Code generated by `just formal-schemes`. DO NOT EDIT.\n\n\
         import Formal.Elab.Syntax\n\n\
         /-!\n\
         # Elaboration: the schemes of the Prelude\n\n\
         The checker's types of the exports of `Prelude`, `Seq`, `Option`, `Result` and `List`\n\
         translated with `CRow` qualifiers (`docs/formal/elaboration.md`) in `flick-model/src/translate/lean.rs`.\n\
         A `def` takes the numbers of the classes of its context and of the type constructors it\n\
         names. A tuple is `tup` nested to the left, a closed record `rec_ℓ̄` of its fields.\n\
         The exports the model cannot write close the file, each with its reason (`docs/formal/elaboration.md#the-scheme-generator-translated-only-the-sugar-names`).\n\
         -/\n\n\
         namespace Flick.Elab.PreludeSchemes\n",
    );
    let mut skipped = String::new();
    for (module, export) in exports() {
        let scheme = export.scheme.as_ref().expect("filtered");
        let name = &export.entity.name;
        let class = export
            .parent
            .as_deref()
            .filter(|_| export.decl == Decl::Method);
        match def(module, name, scheme, class) {
            Ok(d) => {
                out.push('\n');
                out.push_str(&d);
            }
            Err(why) => {
                let _ = writeln!(
                    skipped,
                    "-- skip {module}.{name} ({why}): {}",
                    show_type(scheme)
                );
            }
        }
    }
    out.push_str("\nend Flick.Elab.PreludeSchemes\n");
    if !skipped.is_empty() {
        out.push_str(
            "\n/-! ## Skipped\n\n\
             `rank`: `∀` of a parameter other than `∀v. a^v → …` (`docs/formal/elaboration.md#the-model-allows-polymorphic-types-only-in-dictionary-fields`);\n\
             `region`: a region parameter.\n\
             -/\n\n",
        );
        out.push_str(&skipped);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    /// The committed file is what the checker's Prelude gives, and each
    /// export is in it, a `def` or a skipped line; with
    /// `FLICK_FORMAL_SCHEMES` set the test writes it.
    #[test]
    fn prelude_schemes_are_generated() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../formal/Formal/Elab/PreludeSchemes.lean"
        );
        let text = super::prelude_schemes();
        if env::var_os("FLICK_FORMAL_SCHEMES").is_some() {
            fs::write(path, &text).expect("write PreludeSchemes.lean");
        }
        let committed = fs::read_to_string(path).unwrap_or_default();
        assert!(
            committed == text,
            "PreludeSchemes.lean is stale: run `just formal-schemes`"
        );
        for (module, e) in super::exports() {
            let name = &e.entity.name;
            let def = format!("\ndef {} ", super::lean_name(module, name));
            let skip = format!("\n-- skip {module}.{name} (");
            assert!(
                committed.contains(&def) || committed.contains(&skip),
                "{module}.{name} is neither a `def` nor skipped"
            );
        }
    }
}
