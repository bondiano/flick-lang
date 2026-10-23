//! Built-in modules, written in Flick and checked by this checker: `Prelude`
//! (the built-in types every module sees) and small modules a program may
//! qualify without an import (`Ref.make`, `String.trim`, `Layer.succeed`, …).

use std::sync::OnceLock;

use crate::iface::Interface;
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::syntax;

/// In dependency order: each module sees the ones before it. The modules
/// over Effect sit in blocks by area, each module where its dependencies
/// are already in.
const SOURCES: &[(&str, &str)] = &[
    ("Prelude", include_str!("builtins/Prelude.fl")),
    ("Instances", include_str!("builtins/Instances.fl")),
    ("Record", include_str!("builtins/Record.fl")),
    ("Ref", include_str!("builtins/Ref.fl")),
    ("String", include_str!("builtins/String.fl")),
    ("Number", include_str!("builtins/Number.fl")),
    ("Platform", include_str!("builtins/Platform.fl")),
    ("App", include_str!("builtins/App.fl")),
    ("Layer", include_str!("builtins/Layer.fl")),
    ("Fiber", include_str!("builtins/Fiber.fl")),
    ("RawCause", include_str!("builtins/RawCause.fl")),
    ("Failure", include_str!("builtins/Failure.fl")),
    ("Exit", include_str!("builtins/Exit.fl")),
    ("NonEmpty", include_str!("builtins/NonEmpty.fl")),
    ("Option", include_str!("builtins/Option.fl")),
    ("Result", include_str!("builtins/Result.fl")),
    ("List.Core", include_str!("builtins/List/Core.fl")),
    ("List.Slice", include_str!("builtins/List/Slice.fl")),
    ("List.Search", include_str!("builtins/List/Search.fl")),
    ("List.Transform", include_str!("builtins/List/Transform.fl")),
    ("List", include_str!("builtins/List.fl")),
    ("Chunk.Core", include_str!("builtins/Chunk/Core.fl")),
    ("Chunk.Ops", include_str!("builtins/Chunk/Ops.fl")),
    ("Chunk", include_str!("builtins/Chunk.fl")),
    ("Js.Array", include_str!("builtins/Js/Array.fl")),
    ("Js", include_str!("builtins/Js.fl")),
    ("JsError", include_str!("builtins/JsError.fl")),
    ("BigInt", include_str!("builtins/BigInt.fl")),
    ("Redacted", include_str!("builtins/Redacted.fl")),
    ("BigDecimal", include_str!("builtins/BigDecimal.fl")),
    ("Duration", include_str!("builtins/Duration.fl")),
    ("EqHash", include_str!("builtins/EqHash.fl")),
    ("HashSet", include_str!("builtins/HashSet.fl")),
    ("HashMap", include_str!("builtins/HashMap.fl")),
    ("Loop", include_str!("builtins/Loop.fl")),
    ("Log", include_str!("builtins/Log.fl")),
    ("File", include_str!("builtins/File.fl")),
    ("Resource", include_str!("builtins/Resource.fl")),
    ("Events", include_str!("builtins/Events.fl")),
    ("Ui", include_str!("builtins/Ui.fl")),
    ("Ui.Event", include_str!("builtins/Ui/Event.fl")),
    ("Gen", include_str!("builtins/Gen.fl")),
    ("Checks", include_str!("builtins/Checks.fl")),
    ("Schema", include_str!("builtins/Schema.fl")),
    ("Codec", include_str!("builtins/Codec.fl")),
    ("Json", include_str!("builtins/Json.fl")),
    ("Sql", include_str!("builtins/Sql.fl")),
    ("Database", include_str!("builtins/Database.fl")),
    ("Test", include_str!("builtins/Test.fl")),
    // Effect: values and classes
    ("RegExp", include_str!("builtins/RegExp.fl")),
    ("Combiner", include_str!("builtins/Combiner.fl")),
    ("Reducer", include_str!("builtins/Reducer.fl")),
    ("Ordering", include_str!("builtins/Ordering.fl")),
    ("Order", include_str!("builtins/Order.fl")),
    ("Equivalence", include_str!("builtins/Equivalence.fl")),
    ("Hash", include_str!("builtins/Hash.fl")),
    ("Optic", include_str!("builtins/Optic.fl")),
    ("JsonPointer", include_str!("builtins/JsonPointer.fl")),
    ("JsonPatch", include_str!("builtins/JsonPatch.fl")),
    ("Inspectable", include_str!("builtins/Inspectable.fl")),
    ("Formatter", include_str!("builtins/Formatter.fl")),
    ("Version", include_str!("builtins/Version.fl")),
    ("ByteSize", include_str!("builtins/ByteSize.fl")),
    ("Crypto", include_str!("builtins/Crypto.fl")),
    // end of Effect: values and classes
    // Effect: lists and records
    ("Bool", include_str!("builtins/Bool.fl")),
    ("Predicate", include_str!("builtins/Predicate.fl")),
    ("Filter", include_str!("builtins/Filter.fl")),
    ("Function", include_str!("builtins/Function.fl")),
    // end of Effect: lists and records
    // Effect: collections
    // end of Effect: collections
    // Effect: time
    // end of Effect: time
    // Effect: concurrency
    // end of Effect: concurrency
    // Effect: transactions
    // end of Effect: transactions
    // Effect: schema
    // end of Effect: schema
    // Effect: platform and observability
    // end of Effect: platform and observability
    // Effect: context, layers and requests
    // end of Effect: context, layers and requests
    // Effect: effects, schedules and fibers
    ("Schedule", include_str!("builtins/Schedule.fl")),
    ("Effect", include_str!("builtins/Effect.fl")),
    // end of Effect: effects, schedules and fibers
    // Effect: streams
    // end of Effect: streams
    // Effect: HTTP
    // end of Effect: HTTP
    // Effect: CLI
    ("Cli", include_str!("builtins/Cli.fl")),
    // end of Effect: CLI
];

/// Names and sources of the built-in modules, in dependency order.
pub fn builtin_sources() -> &'static [(&'static str, &'static str)] {
    SOURCES
}

/// Interfaces of the built-in modules.
pub fn builtins() -> &'static [Interface] {
    static BUILTINS: OnceLock<Vec<Interface>> = OnceLock::new();
    BUILTINS.get_or_init(|| {
        let mut done: Vec<Interface> = Vec::new();
        for &(name, text) in SOURCES {
            let mut sources = SourceMap::default();
            let file = sources.add(format!("<builtin>/{name}.fl"), text);
            let parse = syntax::parse(text, file);
            let module_syntax = resolve::module_syntax(&parse.tree, text, file);
            let module = Module {
                name,
                file,
                text,
                tree: &parse.tree,
                syntax: &module_syntax,
            };
            // a facade re-exports the names of the modules before it
            let imported = |m: &str| done.iter().find(|i| i.module == m).cloned();
            let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
            diagnostics.extend(parse.diagnostics);
            // the built-in modules are one package
            let local = |name: &str| SOURCES.iter().any(|(n, _)| *n == name);
            let options = super::Options {
                local: &local,
                limits: super::Limits::default(),
            };
            diagnostics.extend(super::check(
                &module,
                &imported,
                &done,
                options,
                &mut interface,
            ));
            assert!(
                diagnostics.is_empty(),
                "built-in module {name} does not check: {diagnostics:?}"
            );
            done.push(interface);
        }
        done
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    #[test]
    fn builtins_check_cleanly() {
        let names: Vec<&str> = super::builtins()
            .iter()
            .map(|i| i.module.as_str())
            .collect();
        let sources: Vec<&str> = super::SOURCES.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, sources);
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
        assert_eq!(names[..2], ["Prelude", "Instances"]);
    }
}
