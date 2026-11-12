//! The TypeScript forms of the built-in types, by lang item. Each backend
//! gives the built-in types its own representation; these are the JS
//! backend's, with the Effect adapter and on Flick's own runtime. A package
//! gives the types it declares their forms with `@ts`.

use std::sync::{Arc, OnceLock};

use super::{Lang, Origin, TsForm};
use crate::syntax::ts_mark::{self, Import};
use crate::types::lang;

/// A form or an export with the Effect adapter, and on Flick's own runtime.
type Pair = (&'static str, &'static str);

/// Where an export a form names comes from.
#[derive(Clone, Copy)]
enum Slot {
    Runtime,
    Ui,
}

/// What a form names `$Name` stands for: the slot, `Name` and the export
/// of each runtime.
type Imports = &'static [(Slot, &'static str, Pair)];

/// Each lang item with a form: its arity, the form (`None`: it has none)
/// and the exports the form names.
const FORMS: &[(Lang, usize, Option<Pair>, Imports)] = &[
    (Lang::Unit, 0, Some(("undefined", "undefined")), &[]),
    (Lang::Never, 0, Some(("never", "never")), &[]),
    (Lang::Bool, 0, Some(("boolean", "boolean")), &[]),
    (Lang::Int, 0, Some(("number", "number")), &[]),
    (Lang::Float, 0, Some(("number", "number")), &[]),
    (Lang::BigInt, 0, Some(("bigint", "bigint")), &[]),
    (Lang::String, 0, Some(("string", "string")), &[]),
    (Lang::Unknown, 0, Some(("unknown", "unknown")), &[]),
    (Lang::Json, 0, Some(("unknown", "unknown")), &[]),
    (
        Lang::List,
        1,
        Some(("ReadonlyArray<{0}>", "ReadonlyArray<{0}>")),
        &[],
    ),
    (Lang::Seq, 1, Some(("Iterable<{0}>", "Iterable<{0}>")), &[]),
    (Lang::Errors, 1, Some(("{0:row}", "{0:row}")), &[]),
    (
        Lang::Ui,
        0,
        Some(("$Ui", "$Ui")),
        &[(Slot::Ui, "Ui", ("Ui", "Ui"))],
    ),
    (
        Lang::Option,
        1,
        Some(("$Option.Option<{0}>", "$Option<{0}>")),
        &[(Slot::Runtime, "Option", ("Option", "OptionValue"))],
    ),
    (
        Lang::Result,
        2,
        Some(("$Result.Result<{1}, {0}>", "$Result<{1}, {0}>")),
        &[(Slot::Runtime, "Result", ("Result", "ResultValue"))],
    ),
    (
        Lang::Eff,
        3,
        Some((
            "$Effect.Effect<{2}, {1:row}, {0:row}>",
            "$Effect<{2}, {1:row}, {0:row}>",
        )),
        &[(Slot::Runtime, "Effect", ("Effect", "EffectValue"))],
    ),
    (
        Lang::Layer,
        3,
        Some((
            "$Layer.Layer<{2:row}, {1:row}, {0:row}>",
            "$Layer<{2:row}, {1:row}, {0:row}>",
        )),
        &[(Slot::Runtime, "Layer", ("Layer", "LayerValue"))],
    ),
    (
        Lang::Stream,
        3,
        Some((
            "$Stream.Stream<{2}, {1:row}, {0:row}>",
            "$Stream<{2}, {1:row}, {0:row}>",
        )),
        &[(Slot::Runtime, "Stream", ("Stream", "StreamValue"))],
    ),
    (
        Lang::Schema,
        1,
        Some(("$Schema.Codec<{0}, {0}>", "$Schema<{0}, {0}>")),
        &[(Slot::Runtime, "Schema", ("Schema", "SchemaValue"))],
    ),
    (
        Lang::PureCodec,
        2,
        Some(("$Schema.Codec<{1}, {0}>", "$Schema<{1}, {0}>")),
        &[(Slot::Runtime, "Schema", ("Schema", "SchemaValue"))],
    ),
    (
        Lang::Codec,
        4,
        Some((
            "$Schema.Codec<{3}, {2}, {0:row}, {1:row}>",
            "$Schema<{3}, {2}, {0:row}, {1:row}>",
        )),
        &[(Slot::Runtime, "Schema", ("Schema", "SchemaValue"))],
    ),
    (Lang::ServiceTag, 1, None, &[]),
    (Lang::ServiceImpl, 1, None, &[]),
    (Lang::Scope, 1, None, &[]),
];

/// The forms of [`FORMS`], with the Effect adapter and on Flick's own runtime.
type Read = Vec<(Lang, [Arc<TsForm>; 2])>;

fn read() -> Result<Read, String> {
    let mut out = Vec::new();
    for &(item, arity, form, imports) in FORMS {
        let on = |rt: bool| -> Result<Arc<TsForm>, String> {
            let pick = |(effect, on_rt): Pair| if rt { on_rt } else { effect };
            let imports = (imports.iter())
                .map(|&(slot, name, export)| Import {
                    from: match slot {
                        Slot::Runtime => Origin::Runtime,
                        Slot::Ui => Origin::Ui,
                    },
                    name: name.to_owned(),
                    export: pick(export).to_owned(),
                })
                .collect();
            let form = match form {
                Some(form) => ts_mark::checked(arity, ts_mark::pieces(pick(form))?, imports)
                    .map_err(|why| format!("the form of `@lang {}`: {why}", item.name()))?,
                None => TsForm {
                    arity,
                    form: None,
                    imports,
                },
            };
            Ok(Arc::new(form))
        };
        out.push((item, [on(false)?, on(true)?]));
    }
    Ok(out)
}

/// The form of the type `module.name` if it is a lang item with one, or
/// one that has none; on Flick's own runtime when `rt`.
pub(super) fn of(module: &str, name: &str, rt: bool) -> Option<Arc<TsForm>> {
    static READ: OnceLock<Read> = OnceLock::new();
    let read = READ.get_or_init(|| read().expect("the backend's forms read"));
    (read.iter())
        .find(|(item, _)| lang::is(*item, module, name))
        .map(|(_, forms)| Arc::clone(&forms[usize::from(rt)]))
}

#[cfg(test)]
mod tests {
    use super::{FORMS, read};
    use crate::codegen::ts::params;
    use crate::iface::Namespace;
    use crate::types::{self, lang};

    /// Each form reads, and has the arity of the type it is the form of.
    #[test]
    fn each_built_in_form_reads_and_fits_its_type() {
        read().expect("the forms read");
        for &(item, arity, ..) in FORMS {
            let at = lang::item(item);
            let interface = (types::builtins().iter())
                .find(|i| i.module == at.module)
                .expect("a built-in module");
            let def = (interface.find(Namespace::Type, &at.name))
                .and_then(|e| e.definition.as_ref())
                .expect("a type of the module");
            assert_eq!(params(&def.kind).len(), arity, "`@lang {}`", item.name());
        }
    }
}
