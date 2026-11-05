//! `deriving Schema`: the value `T$schema : Schema T`, a function of the
//! witnesses of `T`'s parameters, written with the combinators of `Schema`.
//! A reference to a type of the module is lazy, named in JSON Schema; a
//! recursive type without witnesses is `Schema.recursive`, the other types
//! of its group written in place.

use flick_plugin::{Builder, Decl, Expr, NodeId, Pat, Ty as Type};

use crate::module::{Abi, Form, Kind, Local, Module};
use crate::ty::Ty;

/// Where a derive fails and why: `T.C[0]`, and the reason.
pub type Fail = (String, String);

/// The Prelude's types that take no argument, with their schemas.
const PRIMITIVES: [(&str, &str); 8] = [
    ("Int", "Schema.int"),
    ("Float", "Schema.number"),
    ("String", "Schema.string"),
    ("Bool", "Schema.bool"),
    ("BigInt", "Schema.bigint"),
    ("Unit", "Schema.unit"),
    ("Never", "Schema.never"),
    ("Unknown", "Schema.unknown"),
];

/// The Prelude's types of one argument, with the schemas of them.
const CONTAINERS: [(&str, &str); 3] = [
    ("List", "Schema.list"),
    ("Option", "Schema.option"),
    ("Redacted", "Schema.redacted"),
];

/// The Prelude's types that describe effects or code: no data.
const DESCRIPTIONS: [&str; 7] = [
    "Eff",
    "Layer",
    "ServiceTag",
    "ServiceImpl",
    "Codec",
    "PureCodec",
    "Schema",
];

/// The unqualified name of a Prelude type `name` names, if it does.
fn prelude(name: &str) -> Option<&str> {
    let short = name.strip_prefix("Prelude.").unwrap_or(name);
    (!short.contains('.')).then_some(short)
}

/// What builds the schemas of one type of a module.
pub struct Gen<'m> {
    pub module: &'m Module,
    /// The parameters of the type and the names of their witnesses.
    witnesses: Vec<(String, Kind, String)>,
    /// The types of the group whose own schema is a variable here.
    selves: Vec<(String, String)>,
    /// The type's recursive group.
    group: Vec<String>,
    /// `deriving Schema { wire = "tag" }`: an enum on the wire as a
    /// variant type.
    pub wire_tag: bool,
}

impl<'m> Gen<'m> {
    pub fn new(module: &'m Module, me: &Local) -> Gen<'m> {
        Gen {
            module,
            witnesses: (me.witnesses())
                .map(|(p, k)| (p.clone(), *k, format!("{p}_schema")))
                .collect(),
            selves: Vec::new(),
            group: module.group(&me.name, "Schema"),
            wire_tag: false,
        }
    }

    /// The schema of `ty`, a field at `path`.
    pub fn schema(&mut self, b: &mut Builder, ty: &Ty, path: &str) -> Result<NodeId, Fail> {
        let no = |why: &str| -> Fail {
            (
                path.to_owned(),
                format!("a field of type `{ty}` has no Schema witness: {why}"),
            )
        };
        match ty {
            Ty::Fun(_) => Err(no("a function cannot be checked or written")),
            Ty::Captured(_) => Err(no("it holds a scope's capabilities")),
            Ty::Unit => Ok(name(b, "Schema.unit")),
            Ty::Record { open: true, .. } => Err(no("an open record")),
            Ty::Record { fields, .. } => {
                let mut items = Vec::new();
                for (label, t) in fields {
                    let s = self.schema(b, t, &format!("{path}.{label}"))?;
                    let l = b.expr(Expr::Text(label.clone()));
                    items.push(call(b, "Schema.field", vec![l, s]));
                }
                let list = b.expr(Expr::Items(items));
                Ok(call(b, "Schema.unsafe_record", vec![list]))
            }
            Ty::Tuple(items) => {
                let mut out = Vec::new();
                for (i, t) in items.iter().enumerate() {
                    let s = self.schema(b, t, &format!("{path}[{i}]"))?;
                    out.push(call(b, "Schema.item", vec![s]));
                }
                let list = b.expr(Expr::Items(out));
                Ok(call(b, "Schema.unsafe_tuple", vec![list]))
            }
            Ty::Var { name: v, args } => {
                let witness = self.witnesses.iter().find(|(p, ..)| p == v);
                match (witness, &args[..]) {
                    (Some((_, Kind::Type, w)), []) => Ok(name(b, w)),
                    (Some((_, Kind::Arrow, w)), [arg]) => {
                        let w = name(b, w);
                        let arg = self.schema(b, arg, path)?;
                        Ok(b.expr(Expr::Apply {
                            function: w,
                            arguments: vec![arg],
                        }))
                    }
                    (_, []) => Err(no("a type variable without a witness")),
                    _ => Err(no("a type constructor variable without a witness")),
                }
            }
            Ty::Con { name: n, args } => self.named(b, n, args, path, &no),
        }
    }

    fn named(
        &mut self,
        b: &mut Builder,
        n: &str,
        args: &[Ty],
        path: &str,
        no: &dyn Fn(&str) -> Fail,
    ) -> Result<NodeId, Fail> {
        let local = (!n.contains('.')).then(|| self.module.local(n)).flatten();
        if local.is_none()
            && let Some(short) = prelude(n)
        {
            if let (Some((_, s)), []) = (PRIMITIVES.iter().find(|(p, _)| *p == short), args) {
                return Ok(name(b, s));
            }
            if let (Some((_, s)), [arg]) = (CONTAINERS.iter().find(|(p, _)| *p == short), args) {
                let item = self.schema(b, arg, path)?;
                return Ok(call(b, s, vec![item]));
            }
            if let ("Result", [e, a]) = (short, args) {
                let (e, a) = (self.schema(b, e, path)?, self.schema(b, a, path)?);
                return Ok(call(b, "Schema.result", vec![e, a]));
            }
            if short == "Ref" {
                return Err(no("a mutable cell is snapshot at run time, not checked"));
            }
            if DESCRIPTIONS.contains(&short) {
                return Err(no("it is a runtime description, not data"));
            }
        }
        let Some(local) = local else {
            // a type of another module: its own witness, which the checker
            // finds or reports
            let witness = match n.rsplit_once('.') {
                Some((module, ty)) => format!("{module}.{ty}$schema"),
                None => format!("{n}.schema"),
            };
            let args = self.witness_args(b, args, None, path)?;
            return Ok(apply(b, &witness, args));
        };
        if let Form::Alias {
            body,
            opaque: false,
        } = &local.form
        {
            let with: Vec<(String, Ty)> = (local.params.iter())
                .map(|(p, _)| p.clone())
                .zip(args.iter().cloned())
                .collect();
            let body = body.substitute(&with);
            return self.schema(b, &body, path);
        }
        if !local.derives("Schema") {
            if local.params.iter().any(|(_, k)| *k == Kind::Region) {
                return Err(no("a scoped resource has no wire form"));
            }
            return Err(no("derive `Schema` for it, or give its field a codec"));
        }
        let own = local.name.clone();
        if let Some((_, v)) = self.selves.iter().find(|(t, _)| *t == own) {
            return Ok(name(b, v));
        }
        let kinds: Vec<Kind> = local.witnesses().map(|(_, k)| *k).collect();
        let witnessed = !kinds.is_empty();
        let target = if !witnessed && self.group.contains(&own) {
            // another type of the group, without witnesses: written here
            let local = local.clone();
            self.recursive(b, &local)?
        } else {
            let args = self.witness_args(b, args, Some(&kinds), path)?;
            apply(b, &format!("{own}$schema"), args)
        };
        Ok(lazy(b, &own, target))
    }

    /// The witnesses `args` give, for parameters of `kinds` when known.
    fn witness_args(
        &mut self,
        b: &mut Builder,
        args: &[Ty],
        kinds: Option<&[Kind]>,
        path: &str,
    ) -> Result<Vec<NodeId>, Fail> {
        let mut out = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let arrow = kinds.map_or_else(
                || constructor(arg).is_some(),
                |k| k.get(i) == Some(&Kind::Arrow),
            );
            out.push(if arrow {
                self.constructor_witness(b, arg, path)?
            } else {
                self.schema(b, arg, path)?
            });
        }
        Ok(out)
    }

    /// The witness of a type constructor, `List` or a parameter `f`.
    fn constructor_witness(
        &mut self,
        b: &mut Builder,
        arg: &Ty,
        path: &str,
    ) -> Result<NodeId, Fail> {
        let fail = |what: String| -> Fail { (path.to_owned(), what) };
        match arg {
            Ty::Var { name: v, args } if args.is_empty() => {
                match self.witnesses.iter().find(|(p, ..)| p == v) {
                    Some((_, Kind::Arrow, w)) => Ok(name(b, w)),
                    _ => Err(fail(format!("`{v}` has no witness"))),
                }
            }
            Ty::Con { name: n, args } if args.is_empty() => {
                if let Some(s) = constructor(arg) {
                    return Ok(name(b, s));
                }
                match self.module.local(n) {
                    Some(l) if l.derives("Schema") => Ok(name(b, &format!("{n}$schema"))),
                    Some(_) => Err(fail(format!("`{n}` derives no Schema"))),
                    None => Ok(name(
                        b,
                        &match n.rsplit_once('.') {
                            Some((m, t)) => format!("{m}.{t}$schema"),
                            None => format!("{n}.schema"),
                        },
                    )),
                }
            }
            _ => Err(fail("a type constructor without a witness".into())),
        }
    }

    /// `Schema.recursive "T" (fun self -> …)`: `local`'s schema, its own
    /// references the variable.
    fn recursive(&mut self, b: &mut Builder, local: &Local) -> Result<NodeId, Fail> {
        let var = format!("{}_self", local.name.to_lowercase());
        self.selves.push((local.name.clone(), var.clone()));
        let body = self.definition(b, local);
        self.selves.pop();
        let body = body?;
        let param = b.pat(Pat::Var(var));
        let of = b.expr(Expr::Lambda {
            params: vec![param],
            body,
        });
        let text = b.expr(Expr::Text(local.name.clone()));
        Ok(call(b, "Schema.recursive", vec![text, of]))
    }

    /// The schema of what `local` declares.
    pub fn definition(&mut self, b: &mut Builder, local: &Local) -> Result<NodeId, Fail> {
        let me = local.name.clone();
        let fail = |why: &str| -> Fail { (me.clone(), why.to_owned()) };
        match &local.form {
            Form::Adt {
                cases,
                abi: Abi::Literal { int },
                ..
            } => {
                let base = name(b, if *int { "Schema.int" } else { "Schema.string" });
                if !self.wire_tag {
                    return Ok(call(b, "Schema.enum", vec![base]));
                }
                let pairs: Vec<NodeId> = (cases.iter())
                    .map(|(con, _)| {
                        let wire = con.rsplit('.').next().unwrap_or(con);
                        let wire = b.expr(Expr::Text(wire.to_owned()));
                        let c = name(b, con);
                        b.expr(Expr::Tuple(vec![wire, c]))
                    })
                    .collect();
                let pairs = b.expr(Expr::Items(pairs));
                Ok(call(b, "Schema.unsafe_enum_tags", vec![base, pairs]))
            }
            Form::Adt {
                abi: Abi::Untagged, ..
            } => Err(fail(
                "an `untagged type` has no derived witness: write it with `Schema` combinators",
            )),
            Form::Adt {
                cases,
                abi: Abi::Unboxed,
                ..
            } => {
                let field = cases
                    .first()
                    .and_then(|(_, f)| f.first())
                    .ok_or_else(|| fail("no field"))?;
                let s = self.schema(b, field, &format!("{me}[0]"))?;
                Ok(call(b, "Schema.unsafe_unboxed", vec![s]))
            }
            Form::Adt { cases, error, .. } => {
                let mut out = Vec::new();
                for (con, fields) in cases {
                    let mut items = Vec::new();
                    for (i, f) in fields.iter().enumerate() {
                        let at = if *error {
                            format!("{me}[{i}]")
                        } else {
                            format!("{me}.{con}[{i}]")
                        };
                        let s = self.schema(b, f, &at)?;
                        items.push(call(b, "Schema.item", vec![s]));
                    }
                    let c = name(b, con);
                    let items = b.expr(Expr::Items(items));
                    out.push(call(b, "Schema.unsafe_case", vec![c, items]));
                }
                let text = b.expr(Expr::Text(me.clone()));
                let cases = b.expr(Expr::Items(out));
                Ok(call(b, "Schema.unsafe_adt", vec![text, cases]))
            }
            Form::Alias { opaque: true, .. } => Err(fail(
                "an opaque type's witness is its owner's to give: write it with `Schema` combinators",
            )),
            Form::Alias { body, .. } => self.schema(b, body, &me),
            Form::Refined { base, known } => {
                let s = self.schema(b, base, &me)?;
                let text = b.expr(Expr::Text(me.clone()));
                let known = match known {
                    Some(k) => {
                        let k = b.expr(Expr::Text((*k).to_owned()));
                        call(b, "Some", vec![k])
                    }
                    None => name(b, "None"),
                };
                let parse = name(b, &format!("{me}.parse"));
                Ok(call(
                    b,
                    "Schema.unsafe_refined",
                    vec![text, known, s, parse],
                ))
            }
            Form::Abstract => Err(fail(
                "an abstract type has no definition to derive a schema from",
            )),
        }
    }

    /// `T$schema` and its signature, with `T$json` for a refined type.
    pub fn declarations(&mut self, b: &mut Builder, local: &Local) -> Result<NodeId, Fail> {
        let me = local.name.clone();
        let recursive = self.group.contains(&me);
        let body = if recursive && !local.has_witnesses() {
            self.recursive(b, local)?
        } else {
            self.definition(b, local)?
        };
        let witnesses = self.witnesses.clone();
        let params: Vec<NodeId> = witnesses
            .iter()
            .map(|(_, _, w)| b.pat(Pat::Var(w.clone())))
            .collect();
        let signature = signature(b, local, &witnesses);
        let sig = b.decl(Decl::Signature {
            name: format!("{me}.schema"),
            ty: signature,
        });
        let value = b.decl(Decl::Let {
            name: format!("{me}.schema"),
            params,
            ty: None,
            body,
            rec: recursive && local.has_witnesses(),
        });
        let mut decls = vec![sig, value];
        if let Form::Refined { .. } = local.form {
            let pure = name_ty(b, "PureCodec");
            let json = name_ty(b, "Json");
            let this = name_ty(b, &me);
            let ty = b.ty(Type::Apply {
                function: pure,
                arguments: vec![json, this],
            });
            decls.push(b.decl(Decl::Signature {
                name: format!("{me}.json"),
                ty,
            }));
            let schema = name(b, &format!("{me}$schema"));
            let body = call(b, "Codec.json", vec![schema]);
            decls.push(b.decl(Decl::Let {
                name: format!("{me}.json"),
                params: Vec::new(),
                ty: None,
                body,
                rec: false,
            }));
        }
        Ok(b.group(decls))
    }
}

/// `Schema w1 -> … -> Schema (T a …)`.
fn signature(b: &mut Builder, local: &Local, witnesses: &[(String, Kind, String)]) -> NodeId {
    let schema = |b: &mut Builder, t: NodeId| {
        let s = name_ty(b, "Schema");
        b.ty(Type::Apply {
            function: s,
            arguments: vec![t],
        })
    };
    let params: Vec<NodeId> = local.params.iter().map(|(p, _)| name_ty(b, p)).collect();
    let this = name_ty(b, &local.name);
    let applied = if params.is_empty() {
        this
    } else {
        b.ty(Type::Apply {
            function: this,
            arguments: params,
        })
    };
    let mut ty = schema(b, applied);
    for (p, kind, _) in witnesses.iter().rev() {
        let input = if *kind == Kind::Arrow {
            {
                let x = name_ty(b, "x");
                let from = schema(b, x);
                let f = name_ty(b, p);
                let x = name_ty(b, "x");
                let fx = b.ty(Type::Apply {
                    function: f,
                    arguments: vec![x],
                });
                let to = schema(b, fx);
                let body = b.ty(Type::Function {
                    input: from,
                    output: to,
                });
                b.ty(Type::Forall {
                    vars: vec!["x".into()],
                    body,
                })
            }
        } else {
            let v = name_ty(b, p);
            schema(b, v)
        };
        ty = b.ty(Type::Function { input, output: ty });
    }
    ty
}

/// The witness of an unapplied constructor of the Prelude: `List`.
fn constructor(ty: &Ty) -> Option<&'static str> {
    let Ty::Con { name, args } = ty else {
        return None;
    };
    let short = prelude(name)?;
    if !args.is_empty() {
        return None;
    }
    CONTAINERS
        .iter()
        .find(|(c, _)| *c == short)
        .map(|(_, s)| *s)
}

pub fn name(b: &mut Builder, n: &str) -> NodeId {
    b.expr(Expr::Name(n.to_owned()))
}

fn name_ty(b: &mut Builder, n: &str) -> NodeId {
    b.ty(Type::Name(n.to_owned()))
}

pub fn call(b: &mut Builder, f: &str, arguments: Vec<NodeId>) -> NodeId {
    let function = name(b, f);
    b.expr(Expr::Apply {
        function,
        arguments,
    })
}

/// `f`, applied to `args` when there are any.
fn apply(b: &mut Builder, f: &str, args: Vec<NodeId>) -> NodeId {
    if args.is_empty() {
        name(b, f)
    } else {
        call(b, f, args)
    }
}

/// `Schema.lazy "T" (fun _ -> of)`.
pub fn lazy(b: &mut Builder, ty: &str, of: NodeId) -> NodeId {
    let unit = b.pat(Pat::Wildcard);
    let thunk = b.expr(Expr::Lambda {
        params: vec![unit],
        body: of,
    });
    let text = b.expr(Expr::Text(ty.to_owned()));
    call(b, "Schema.lazy", vec![text, thunk])
}
