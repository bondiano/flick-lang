//! Rows and stages of the core as the certificates and the elision theorems write them: in a
//! type a row term (`Ty`), in a judgment the predicate it stands for (`Row`, `Ty.asRow`), which
//! alone holds obligations.

use crate::check::{live_service, scope_service};
use crate::syntax::Path;
use crate::types::{Caps, PROVIDED, Row, Stage, Type, Var, provided};

use super::{Env, disjunction};

/// The atoms of `Live C` or `Scope ϱ`, an element of a row.
fn live_atoms(name: &Path, args: &[Type], env: &Env) -> Result<Vec<usize>, String> {
    let c = match args {
        [Type::Captured(_, c)] => c.clone(),
        [Type::Var(v)] => Caps::atom(v.clone()),
        _ => return Err(format!("`{name}` with arguments the core does not write")),
    };
    c.atoms
        .iter()
        .chain(&c.vars)
        .map(|a| env.index_of(a))
        .collect()
}

/// A row as a row term of the core: each name at itself, each variable, and its atoms in one
/// `caps`. An obligation is no element of a row term; only a stage holds them (`stage`).
pub(crate) fn row(r: &Row, env: &Env) -> Result<String, String> {
    let mut atoms = Vec::new();
    let mut parts = Vec::new();
    for (name, args) in &r.elems {
        if *name == live_service() || *name == scope_service() {
            atoms.extend(live_atoms(name, args, env)?);
        } else if *name == provided() && provides_nothing(args, env) {
            // `Provided {} ϱ` holds no obligation
        } else if *name == provided() || &*name.module == PROVIDED {
            return Err(format!("the obligation `{name}` in a row term"));
        } else if args.is_empty() {
            parts.push(format!("(.rnm \"{name}\" (.adt \"{name}\"))"));
        } else {
            return Err(format!("the element `{name}` with arguments"));
        }
    }
    for v in env.kept(r.vars.iter()) {
        if let Some(n) = r.less_of(v).next() {
            return Err(format!("the row variable `{v}` without `{}`", n.name));
        }
        parts.push(format!("(.tvar {})", env.index_of(v)?));
    }
    if !atoms.is_empty() {
        let atoms: Vec<String> = atoms.iter().map(|a| format!("x = .v {a}")).collect();
        parts.push(format!("(.caps {})", disjunction(&atoms, "Caps.none")));
    }
    Ok((parts.into_iter())
        .reduce(|a, b| format!("(.run {a} {b})"))
        .unwrap_or_else(|| ".cnil".into()))
}

/// The arguments of `Provided {r} ϱ` with a row `r` of no names and no variables.
fn provides_nothing(args: &[Type], env: &Env) -> bool {
    matches!(args, [Type::Tag(o), _] if o.elems.is_empty() && env.kept(o.vars.iter()).next().is_none())
}

/// A row as the predicate a judgment holds (`Row`), obligations included: what a row term
/// stands for (`Ty.asRow`), written out.
pub(crate) fn row_pred(r: &Row, env: &Env) -> Result<String, String> {
    let mut parts = Vec::new();
    for (name, args) in &r.elems {
        if *name == live_service() || *name == scope_service() {
            for a in live_atoms(name, args, env)? {
                parts.push(format!("x = .atm (.v {a})"));
            }
        } else if *name == provided() {
            // `Provided {r} ϱ` of a scheme: an obligation for each service of `r`
            let [Type::Tag(r), s] = &args[..] else {
                return Err(format!("`{name}` with arguments the core does not write"));
            };
            let a = region(s, env)?;
            for n in r.elems.keys() {
                parts.push(format!("x = .ob \"{n}\" (Ty.adt \"{n}\").classOf (.v {a})"));
            }
            for v in env.kept(r.vars.iter()) {
                parts.push(format!(
                    "x = .obv (.v {}) (fun _ => False) (.v {a})",
                    env.index_of(v)?
                ));
            }
        } else if &*name.module == PROVIDED {
            // `{Provided {S} ϱ}`, split by service or by the rigid tail `~ρ`
            let [s] = &args[..] else {
                return Err(format!("`{name}` with arguments the core does not write"));
            };
            let a = region(s, env)?;
            parts.push(match split_key(&name.name, env)? {
                Owed::Service(n) => format!("x = .ob \"{n}\" (Ty.adt \"{n}\").classOf (.v {a})"),
                Owed::Var(i) => format!("x = .obv (.v {i}) (fun _ => False) (.v {a})"),
            });
        } else if args.is_empty() {
            parts.push(format!("x = .nm₀ \"{name}\""));
        } else {
            return Err(format!("the element `{name}` with arguments"));
        }
    }
    for v in env.kept(r.vars.iter()) {
        if let Some(n) = r.less_of(v).next() {
            return Err(format!("the row variable `{v}` without `{}`", n.name));
        }
        parts.push(format!(
            "x = .rv (.v {}) (fun _ => False)",
            env.index_of(v)?
        ));
    }
    Ok(disjunction(&parts, "Row.none"))
}

/// What a split obligation `{Provided {k} ϱ}` is owed for.
enum Owed<'a> {
    Service(&'a str),
    Var(usize),
}

/// The key `k` of a split obligation: a service, or `~ρ` for the rigid tail `ρ`.
fn split_key<'a>(k: &'a str, env: &Env) -> Result<Owed<'a>, String> {
    match k.strip_prefix('~') {
        Some(v) => Ok(Owed::Var(env.index_of(&Var::Named(v.into()))?)),
        None => Ok(Owed::Service(k)),
    }
}

/// The atom of the region an obligation is about.
fn region(s: &Type, env: &Env) -> Result<usize, String> {
    match s {
        Type::Var(v) => env.index_of(v),
        other => Err(format!("an obligation about `{other}`, no region")),
    }
}

/// A stage as a stage term of a type. Its obligations, if any, are `Provided` of the rest of
/// its row about one region `ϱ`: an obligation for each name and each variable of the rest, the
/// stage `rtob rest ϱ e` (`Ty.asStage`); other obligations have no stage term.
pub(crate) fn stage(s: &Stage, env: &Env) -> Result<String, String> {
    let Stage::Runtime(r, e) = s else {
        return Ok(".pure".into());
    };
    let (rest, obligations) = split_obligations(r, env)?;
    let (r, e) = (row(&rest, env)?, row(e, env)?);
    let Some(&(_, a)) = obligations.first() else {
        return Ok(format!("(.rt {r} {e})"));
    };
    let names = (rest.elems.keys())
        .filter(|n| **n != live_service() && **n != scope_service())
        .map(ToString::to_string);
    let vars = (env.kept(rest.vars.iter())).map(|v| env.index_of(v).map(|i| format!("v{i}")));
    let mut expected: Vec<(String, usize)> = names
        .map(Ok)
        .chain(vars)
        .map(|k| k.map(|k| (k, a)))
        .collect::<Result<_, String>>()?;
    let mut held = obligations;
    expected.sort();
    held.sort();
    held.dedup();
    if held != expected {
        return Err("obligations other than `Provided` of the rest of the row".into());
    }
    Ok(format!("(.rtob {r} (.tvar {a}) {e})"))
}

/// The row without its obligations, and the obligations: a service `N` or a variable `v{i}`,
/// each with the region it is about.
fn split_obligations(r: &Row, env: &Env) -> Result<(Row, Vec<(String, usize)>), String> {
    let mut rest = r.clone();
    let mut held = Vec::new();
    for (name, args) in &r.elems {
        if *name == provided() {
            let [Type::Tag(o), s] = &args[..] else {
                return Err(format!("`{name}` with arguments the core does not write"));
            };
            let a = region(s, env)?;
            held.extend(o.elems.keys().map(|n| (n.to_string(), a)));
            for v in env.kept(o.vars.iter()) {
                held.push((format!("v{}", env.index_of(v)?), a));
            }
            rest.elems.remove(name);
        } else if &*name.module == PROVIDED {
            let [s] = &args[..] else {
                return Err(format!("`{name}` with arguments the core does not write"));
            };
            let owed = match split_key(&name.name, env)? {
                Owed::Service(n) => n.to_owned(),
                Owed::Var(i) => format!("v{i}"),
            };
            held.push((owed, region(s, env)?));
            rest.elems.remove(name);
        }
    }
    Ok((rest, held))
}

/// A stage as the one a judgment holds (`Stage`), obligations included.
pub(crate) fn stage_pred(s: &Stage, env: &Env) -> Result<String, String> {
    Ok(match s {
        Stage::Pure => ".pure".into(),
        Stage::Runtime(r, e) => format!("(.rt {} {})", row_pred(r, env)?, row_pred(e, env)?),
    })
}
