//! `.d.ts` files of the public TypeScript interface (`--emit-dts`), and the
//! opt-in Promise façade of a module's runtime functions.

use std::fmt::Write;

use super::{Projection, Vars, generic, key};
use crate::codegen::exports::reexports;
use crate::codegen::names;
use crate::iface::{Decl, Export, Interface, Namespace};
use crate::types::{Form, Kind, Type};

/// The declaration file of the module `projection` is for: its types and
/// each exported value, and the exports refused with the reason.
pub fn declaration_file(
    projection: &Projection,
    interface: &Interface,
    path: &dyn Fn(&str) -> String,
) -> (String, Vec<(String, String)>) {
    let (types, mut refused) = projection.declarations(interface);
    let mut values = Vec::new();
    for export in &interface.exports {
        let entity = &export.entity;
        let value = matches!(entity.namespace, Namespace::Value | Namespace::Constructor);
        if !value || entity.module != projection.module || export.decl == Decl::Method {
            continue;
        }
        let Some(scheme) = &export.scheme else {
            continue;
        };
        match projection.value(scheme) {
            Ok(ty) => values.push(format!(
                "export declare const {}: {ty};",
                names::ident(&entity.name)
            )),
            Err(reason) => refused.push((entity.name.clone(), reason)),
        }
    }
    // the key of each service, which TypeScript provides an implementation by
    for export in &interface.exports {
        if export.entity.namespace != Namespace::Type || export.entity.module != projection.module {
            continue;
        }
        let Some(Form::Service { params, operations }) =
            export.definition.as_ref().map(|d| &d.form)
        else {
            continue;
        };
        if !params.is_empty() {
            continue;
        }
        let name = &export.entity.name;
        let ops: Result<Vec<String>, String> = operations
            .iter()
            .map(|op| {
                Ok(format!(
                    "readonly {}: {}",
                    key(&op.name),
                    projection.value(&op.ty)?
                ))
            })
            .collect();
        match ops {
            Ok(ops) => values.push(format!(
                "export declare const {}: $Context.Service<{name}, {{ {} }}>;",
                names::service(name),
                ops.join("; ")
            )),
            Err(reason) => refused.push((names::service(name), reason)),
        }
    }
    // dictionaries of instances without a context, for constrained functions
    for instance in &interface.instances {
        if instance.module != projection.module || !instance.vars.is_empty() {
            continue;
        }
        if let Ok(ty) = projection.ty(&instance.head, &Vars::new()) {
            let name = names::instance(&instance.name);
            values.push(format!("export declare const {name}: {ty};"));
        }
    }
    // a re-export names what the defining module's own file declares
    let value = |export: &Export| {
        let scheme = export.scheme.as_ref();
        scheme.is_some_and(|scheme| {
            Projection::new(&export.entity.module, projection.lookup, projection.package)
                .value(scheme)
                .is_ok()
        })
    };
    let ty = |export: &Export| {
        (projection.lookup)(&export.entity.module).is_some_and(|interface| {
            Projection::new(&export.entity.module, projection.lookup, projection.package)
                .declares(&interface, export)
        })
    };
    let (barrel, _) = reexports(projection.module, interface, path, &value, &ty);
    let mut out = projection.imports(path);
    out.push('\n');
    for line in types.iter().chain(&values) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&barrel);
    (out, refused)
}

/// One function of the Promise façade: the export, how many arguments it
/// takes before it runs, and its TypeScript type.
pub struct Facade {
    pub name: String,
    pub arity: usize,
    pub ty: String,
}

impl Projection<'_> {
    /// The façade of `scheme`, a function whose first runtime arrow needs no
    /// service and comes after `closures` stages that only make closures:
    /// `(a, b, options?) => Promise<Result<A, E>>`.
    pub fn facade(&self, scheme: &Type, closures: usize) -> Result<(usize, String), String> {
        let (vars, constraints, mut ty) = match scheme {
            Type::Forall {
                vars,
                constraints,
                body,
            } => (vars.clone(), constraints.len(), (**body).clone()),
            other => (Vec::new(), 0, other.clone()),
        };
        if constraints > 0 {
            return Err("a constrained function takes dictionaries".to_owned());
        }
        let mut scope = Vars::new();
        let mut generics = Vec::new();
        for b in &vars {
            match b.kind {
                Kind::Type | Kind::ErrorRow | Kind::ServiceRow => {
                    scope.insert(b.name.clone(), Some(generic(&b.name)));
                    generics.push(generic(&b.name));
                }
                _ => return Err(format!("`{}` is a region or a capture row", b.name)),
            }
        }
        let mut params = Vec::new();
        loop {
            let Type::Fun {
                param,
                effects,
                result,
            } = ty
            else {
                return Err("not a runtime function".to_owned());
            };
            params.push(self.ty(&param, &scope)?);
            if let Some(effects) = effects {
                if params.len() > closures {
                    return Err("an earlier stage may do more than make a closure".to_owned());
                }
                if self.row(&effects.services, &scope)? != "never" {
                    return Err(
                        "it needs services: provide them in a Flick function first".to_owned()
                    );
                }
                let error = self.row(&effects.errors, &scope)?;
                let value = self.ty(&result, &scope)?;
                let args: Vec<String> = params
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("arg{i}: {p}"))
                    .collect();
                let generics = if generics.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generics.join(", "))
                };
                let ty = format!(
                    "{generics}({}, options?: {{ readonly signal?: AbortSignal }}) => Promise<$Result.Result<{value}, {error}>>",
                    args.join(", ")
                );
                return Ok((params.len(), ty));
            }
            ty = *result;
        }
    }
}

/// The JS and `.d.ts` of the Promise façade over `module_js` (the module's
/// import specifier from the façade), for `facades`.
pub fn facade_files(
    projection: &Projection,
    module_js: &str,
    facades: &[Facade],
    path: &dyn Fn(&str) -> String,
) -> (String, String) {
    let mut js = format!(
        "import {{ promiseFacade }} from \"@flick-lang/runtime\";\nimport * as $m from {};\n\n",
        names::string(module_js)
    );
    let mut dts = projection.imports(path);
    dts.push('\n');
    for f in facades {
        let name = names::ident(&f.name);
        let _ = writeln!(
            js,
            "export const {name} = promiseFacade($m.{name}, {});",
            f.arity
        );
        let _ = writeln!(dts, "export declare const {name}: {};", f.ty);
    }
    (js, dts)
}
