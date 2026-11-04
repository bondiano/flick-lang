//! `deriving Form`: `T$form : Form T` of a record type, its fields `String`,
//! `Float`, `Bool`, refined types over them or `Option`s of one, and
//! `T$fields`, a `Form.Field` for each of its fields.

use flick_plugin::{Builder, Decl, Expr, Field, NodeId, Ty as Type};

use crate::module::{Form, Local, Module};
use crate::schema::{Fail, Gen, call, name};
use crate::ty::Ty;

pub fn declarations(b: &mut Builder, module: &Module, local: &Local) -> Result<NodeId, Fail> {
    let me = local.name.clone();
    let fail = |why: &str| -> Fail { (me.clone(), why.to_owned()) };
    if !local.derives("Schema") {
        return Err(fail("a type that derives `Form` derives `Schema` too"));
    }
    let Form::Alias {
        body: Ty::Record {
            fields,
            open: false,
        },
        opaque: false,
    } = &local.form
    else {
        return Err(fail("a form is of a record type"));
    };
    let mut schemas = Gen::new(module, local);
    let mut specs = Vec::new();
    for (label, ty) in fields {
        let (inner, optional) = match ty {
            Ty::Con { name, args }
                if (name == "Option" || name == "Prelude.Option") && args.len() == 1 =>
            {
                (&args[0], true)
            }
            other => (other, false),
        };
        if !scalar(module, inner) {
            return Err((
                format!("{me}.{label}"),
                "a form field is a `String`, a `Float`, a `Bool`, a refined type over them or an `Option` of one".into(),
            ));
        }
        let s = schemas.schema(b, inner, &format!("{me}.{label}"))?;
        let l = b.expr(Expr::Text(label.clone()));
        let o = b.expr(Expr::Boolean(optional));
        specs.push(call(b, "Form.spec", vec![l, s, o]));
    }
    let schema = name(b, &format!("{me}$schema"));
    let text = b.expr(Expr::Text(me.clone()));
    let specs = b.expr(Expr::Items(specs));
    let body = call(b, "Form.make", vec![schema, text, specs]);
    let form = b.ty(Type::Name("Form".into()));
    let this = b.ty(Type::Name(me.clone()));
    let ty = b.ty(Type::Apply {
        function: form,
        arguments: vec![this],
    });
    let mut decls = vec![
        b.decl(Decl::Signature {
            name: format!("{me}.form"),
            ty,
        }),
        b.decl(Decl::Let {
            name: format!("{me}.form"),
            params: Vec::new(),
            ty: None,
            body,
            rec: false,
        }),
    ];
    let mut types = Vec::new();
    let mut values = Vec::new();
    for (label, _) in fields {
        let field = b.ty(Type::Name("Form.Field".into()));
        types.push(Field {
            label: label.clone(),
            value: field,
        });
        let l = b.expr(Expr::Text(label.clone()));
        let v = call(b, "Form.unsafe_field", vec![l]);
        values.push(Field {
            label: label.clone(),
            value: v,
        });
    }
    let ty = b.ty(Type::Record(types));
    let body = b.expr(Expr::Record(values));
    decls.push(b.decl(Decl::Signature {
        name: format!("{me}.fields"),
        ty,
    }));
    decls.push(b.decl(Decl::Let {
        name: format!("{me}.fields"),
        params: Vec::new(),
        ty: None,
        body,
        rec: false,
    }));
    Ok(b.group(decls))
}

/// Whether a form field of `ty` is one value of text.
fn scalar(module: &Module, ty: &Ty) -> bool {
    let Ty::Con { name, args } = ty else {
        return false;
    };
    if !args.is_empty() {
        return false;
    }
    let short = name.strip_prefix("Prelude.").unwrap_or(name);
    if ["String", "Int", "Float", "Bool"].contains(&short) {
        return true;
    }
    match module.local(name) {
        Some(Local {
            form: Form::Refined { base, .. },
            ..
        }) => scalar(module, base),
        Some(Local {
            form: Form::Abstract,
            ..
        }) => true,
        Some(_) => false,
        // shortcut: a type of another module counts as one, its schema checking the text
        None => !["Unit", "Never", "Unknown", "BigInt"].contains(&short),
    }
}
