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
//! 2. `∀v. a^v → …` of a parameter is instantiated `v := a`; any other
//!    variable of a `∀` stays a binder, `.all k t`.
//! 3. A qualifier in a negative position is a variable: a single variable
//!    of the checker stays itself at its first such position, the rest are
//!    new; a missing qualifier of a stored type is new.
//! 4. In a positive position a variable that qualified negative positions
//!    is what those arguments reach, left to right through `cun`: `q` and
//!    the variables of kind `CaptureRow` and `Region` its arguments reach of
//!    data, as rule 6 counts them; `q` and the new variables inside of a
//!    function.
//! 5. Binders: the class parameter, the checker's `∀` that remain in their
//!    order, then the new variables in the order they appear.
//! 6. Elision, as the checker reads a signature (`regions/live.rs` `elide`
//!    of flick-core): the row of each runtime arrow gets `Live` of what its
//!    parameter and those of the stages before it reach, the variables of
//!    kind `CaptureRow` and `Region` only; `Live C` is the capture row
//!    `.cun C .cnil` in the row. `Eff` drops its rows (rule 1), so its `Live`.
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

/// The scheme of the loop of `for … do`, `Seq.each_effect` read by `for_loop`.
const LOOP: &str = "each_loop";

/// Identifiers of the checker that are Lean keywords.
const KEYWORDS: [&str; 4] = ["show", "from", "at", "fun"];

/// Why an export has no `def`.
#[derive(Debug)]
enum Skip {
    /// `∀` of a parameter with a constraint (`docs/formal/elaboration.md#a-polymorphic-parameter-has-no-constraint-in-the-model`)
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
        Ty::All(names, t) => {
            let inner: Vec<String> = names.iter().chain(vars).cloned().collect();
            let _ = write!(out, ".all {} (", names.len());
            lean(t, &inner, out);
            out.push(')');
        }
        Ty::Pos(_) => unreachable!("resolved"),
    }
}

/// The doc comment of a `def`, and its name.
fn header<'a>(
    module: &str,
    name: &'a str,
    scheme: &fc::Type,
    callback_only: bool,
) -> (String, &'a str) {
    if callback_only {
        let doc = format!(
            "/-- `{module}.{name}` as the checker's `for_loop` reads `for … do`: only the body, its\n\
             first parameter, is kept live, nothing of the source (`types/comprehension.rs`) -/\n"
        );
        (doc, LOOP)
    } else {
        (
            format!("/-- `{module}.{name} : {}` -/\n", show_type(scheme)),
            name,
        )
    }
}

/// The `def` of one scheme: its parameters are the classes of its context,
/// the class of a method first, then the type constructors in the order
/// they appear.
fn def(
    module: &str,
    name: &str,
    scheme: &fc::Type,
    class: Option<&str>,
    callback_only: bool,
) -> Result<String, Skip> {
    let (vars, constraints, body) = match scheme {
        fc::Type::Forall {
            vars,
            constraints,
            body,
        } => (&vars[..], &constraints[..], &**body),
        t => (&[][..], &[][..], t),
    };
    let mut tr = Tr::default();
    tr.callback_only = callback_only;
    for b in vars {
        match &b.kind {
            Kind::CaptureRow => {
                tr.live_kinds.insert(b.name.clone());
            }
            Kind::Type | Kind::ServiceRow | Kind::ErrorRow => {}
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
    let (mut out, name) = header(module, name, scheme, callback_only);
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
        let looped = (module, name.as_str()) == ("Seq", "each_effect");
        match def(module, name, scheme, class, false) {
            Ok(d) => {
                out.push('\n');
                out.push_str(&d);
                if looped {
                    out.push('\n');
                    out.push_str(
                        &def(module, name, scheme, class, true).expect("as `each_effect`"),
                    );
                }
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
             `rank`: `∀` of a parameter with a constraint (`docs/formal/elaboration.md#a-polymorphic-parameter-has-no-constraint-in-the-model`).\n\
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

    use flick_core::types::{self as fc, Binder, Effects, Kind};

    fn var(v: &str) -> fc::Type {
        fc::Type::Var(v.into())
    }

    fn arrow(param: fc::Type, effects: Option<(fc::Type, fc::Type)>, result: fc::Type) -> fc::Type {
        fc::Type::Fun {
            param: Box::new(param),
            effects: effects.map(|(s, e)| Box::new(Effects::new(s, e))),
            result: Box::new(result),
        }
    }

    fn forall(vars: &[(&str, Kind)], body: fc::Type) -> fc::Type {
        fc::Type::Forall {
            vars: vars
                .iter()
                .map(|(name, kind)| Binder {
                    name: (*name).into(),
                    kind: kind.clone(),
                })
                .collect(),
            constraints: Vec::new(),
            body: Box::new(body),
        }
    }

    #[test]
    fn a_parameter_of_rank_two_is_a_binder() {
        let param = forall(&[("a", Kind::Type)], arrow(var("a"), None, var("a")));
        let scheme = arrow(param, None, fc::Type::con("Prelude", "Int"));
        let text = super::def("M", "apply_twice", &scheme, None, false).expect("translated");
        assert!(
            text.contains("⟨0, [],\n    .arr .P (.all 1 (.arr .P (.var 0) (.var 0) .cnil)) (.con int) .cnil, .cnil⟩"),
            "{text}"
        );
    }

    #[test]
    fn scope_run_takes_its_region_as_a_binder() {
        let owner = fc::Type::app(fc::Type::con("Scope", "Owner"), var("s"));
        let services = fc::Type::Row(fc::Row {
            entries: vec![(
                "Scope".into(),
                fc::Type::app(fc::Type::con("Scope", "Scope"), var("s")),
            )],
            tail: Some(Box::new(var("r"))),
            ..fc::Row::default()
        });
        let param = forall(
            &[("s", Kind::Region)],
            arrow(owner, Some((services, var("e"))), var("a")),
        );
        let scheme = forall(
            &[
                ("r", Kind::ServiceRow),
                ("e", Kind::ErrorRow),
                ("a", Kind::Type),
            ],
            arrow(param, Some((var("r"), var("e"))), var("a")),
        );
        let text = super::def("Scope", "run", &scheme, None, false).expect("translated");
        assert!(
            text.contains(".all 1 (.arr (.R (.run (.cun (.var 0) .cnil) (.run (.rnm scope"),
            "{text}"
        );
    }

    #[test]
    fn a_constraint_on_a_parameter_is_skipped() {
        let fc::Type::Forall { vars, body, .. } =
            forall(&[("a", Kind::Type)], arrow(var("a"), None, var("a")))
        else {
            unreachable!()
        };
        let param = fc::Type::Forall {
            vars,
            constraints: vec![fc::Type::app(fc::Type::con("Prelude", "Show"), var("a"))],
            body,
        };
        let scheme = arrow(param, None, fc::Type::con("Prelude", "String"));
        assert!(matches!(
            super::def("M", "f", &scheme, None, false),
            Err(super::Skip::Rank)
        ));
    }

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
