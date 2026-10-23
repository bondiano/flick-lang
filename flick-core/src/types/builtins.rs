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
    ("Trie", include_str!("builtins/Trie.fl")),
    ("HashRing", include_str!("builtins/HashRing.fl")),
    ("Graph.Types", include_str!("builtins/Graph/Types.fl")),
    ("Graph.Build", include_str!("builtins/Graph/Build.fl")),
    ("Graph.Query", include_str!("builtins/Graph/Query.fl")),
    (
        "Graph.Algorithms",
        include_str!("builtins/Graph/Algorithms.fl"),
    ),
    ("Graph.Walk", include_str!("builtins/Graph/Walk.fl")),
    ("Graph", include_str!("builtins/Graph.fl")),
    // end of Effect: collections
    // Effect: time
    ("DateTime", include_str!("builtins/DateTime.fl")),
    ("Cron", include_str!("builtins/Cron.fl")),
    ("Clock", include_str!("builtins/Clock.fl")),
    ("Random", include_str!("builtins/Random.fl")),
    // end of Effect: time
    // Effect: concurrency
    (
        "SynchronizedRef",
        include_str!("builtins/SynchronizedRef.fl"),
    ),
    (
        "SubscriptionRef",
        include_str!("builtins/SubscriptionRef.fl"),
    ),
    ("Deferred", include_str!("builtins/Deferred.fl")),
    ("Latch", include_str!("builtins/Latch.fl")),
    ("Semaphore", include_str!("builtins/Semaphore.fl")),
    (
        "PartitionedSemaphore",
        include_str!("builtins/PartitionedSemaphore.fl"),
    ),
    ("Queue", include_str!("builtins/Queue.fl")),
    ("PubSub", include_str!("builtins/PubSub.fl")),
    ("Cache", include_str!("builtins/Cache.fl")),
    ("ScopedCache", include_str!("builtins/ScopedCache.fl")),
    ("ScopedRef", include_str!("builtins/ScopedRef.fl")),
    ("RcRef", include_str!("builtins/RcRef.fl")),
    ("RcMap", include_str!("builtins/RcMap.fl")),
    ("Pool", include_str!("builtins/Pool.fl")),
    // end of Effect: concurrency
    // Effect: transactions
    ("Tx", include_str!("builtins/Tx.fl")),
    ("TxRef", include_str!("builtins/TxRef.fl")),
    ("TxChunk", include_str!("builtins/TxChunk.fl")),
    ("TxDeferred", include_str!("builtins/TxDeferred.fl")),
    ("TxHashMap", include_str!("builtins/TxHashMap.fl")),
    ("TxHashSet", include_str!("builtins/TxHashSet.fl")),
    (
        "TxPriorityQueue",
        include_str!("builtins/TxPriorityQueue.fl"),
    ),
    ("TxQueue", include_str!("builtins/TxQueue.fl")),
    ("TxPubSub", include_str!("builtins/TxPubSub.fl")),
    (
        "TxReentrantLock",
        include_str!("builtins/TxReentrantLock.fl"),
    ),
    ("TxSemaphore", include_str!("builtins/TxSemaphore.fl")),
    (
        "TxSubscriptionRef",
        include_str!("builtins/TxSubscriptionRef.fl"),
    ),
    // end of Effect: transactions
    // Effect: schema
    ("Check", include_str!("builtins/Check.fl")),
    ("Arbitrary", include_str!("builtins/Arbitrary.fl")),
    ("JsonSchema", include_str!("builtins/JsonSchema.fl")),
    ("Differ", include_str!("builtins/Differ.fl")),
    // end of Effect: schema
    // Effect: platform and observability
    ("PlatformError", include_str!("builtins/PlatformError.fl")),
    ("Path", include_str!("builtins/Path.fl")),
    ("LogLevel", include_str!("builtins/LogLevel.fl")),
    ("Console", include_str!("builtins/Console.fl")),
    ("FileSystem", include_str!("builtins/FileSystem.fl")),
    ("Terminal", include_str!("builtins/Terminal.fl")),
    ("Stdio", include_str!("builtins/Stdio.fl")),
    ("Logger", include_str!("builtins/Logger.fl")),
    ("ConfigProvider", include_str!("builtins/ConfigProvider.fl")),
    ("Config", include_str!("builtins/Config.fl")),
    ("Metric", include_str!("builtins/Metric.fl")),
    ("Tracer", include_str!("builtins/Tracer.fl")),
    // end of Effect: platform and observability
    // Effect: context, layers and requests
    // end of Effect: context, layers and requests
    // Effect: effects, schedules and fibers
    ("Schedule", include_str!("builtins/Schedule.fl")),
    ("Effect", include_str!("builtins/Effect.fl")),
    // end of Effect: effects, schedules and fibers
    // Effect: streams, sinks and channels
    // end of Effect: streams, sinks and channels
    // Effect: streams, Stream
    // end of Effect: streams, Stream
    // Effect: HTTP
    ("Http.Method", include_str!("builtins/Http/Method.fl")),
    ("Http.Status", include_str!("builtins/Http/Status.fl")),
    ("Http.Headers", include_str!("builtins/Http/Headers.fl")),
    ("Http.UrlParams", include_str!("builtins/Http/UrlParams.fl")),
    ("Http.Url", include_str!("builtins/Http/Url.fl")),
    ("Http.Mime", include_str!("builtins/Http/Mime.fl")),
    ("Http.Cookies", include_str!("builtins/Http/Cookies.fl")),
    ("Http.Etag", include_str!("builtins/Http/Etag.fl")),
    ("Http.Body", include_str!("builtins/Http/Body.fl")),
    (
        "Http.ClientError",
        include_str!("builtins/Http/ClientError.fl"),
    ),
    (
        "Http.ClientRequest",
        include_str!("builtins/Http/ClientRequest.fl"),
    ),
    (
        "Http.ClientResponse",
        include_str!("builtins/Http/ClientResponse.fl"),
    ),
    ("Http.Client", include_str!("builtins/Http/Client.fl")),
    (
        "Http.FetchClient",
        include_str!("builtins/Http/FetchClient.fl"),
    ),
    (
        "Http.ServerError",
        include_str!("builtins/Http/ServerError.fl"),
    ),
    (
        "Http.ServerRequest",
        include_str!("builtins/Http/ServerRequest.fl"),
    ),
    (
        "Http.ServerResponse",
        include_str!("builtins/Http/ServerResponse.fl"),
    ),
    ("Http.Multipart", include_str!("builtins/Http/Multipart.fl")),
    ("Http.Platform", include_str!("builtins/Http/Platform.fl")),
    (
        "Http.Middleware",
        include_str!("builtins/Http/Middleware.fl"),
    ),
    ("Http.Router", include_str!("builtins/Http/Router.fl")),
    ("Http.Server", include_str!("builtins/Http/Server.fl")),
    (
        "Http.StaticServer",
        include_str!("builtins/Http/StaticServer.fl"),
    ),
    (
        "Http.TraceContext",
        include_str!("builtins/Http/TraceContext.fl"),
    ),
    // end of Effect: HTTP
    // Effect: HttpApi
    // end of Effect: HttpApi
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
