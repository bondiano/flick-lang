//! The sources of the built-in modules, in the order `builtins` checks
//! them.

/// The file `$path` of the sources of the package `$package` in
/// `packages/`, as written or, without `builtin-docs`, without its docs.
#[cfg(feature = "builtin-docs")]
macro_rules! builtin_source {
    ($package:literal, $path:literal) => {
        include_str!(concat!("../../../packages/", $package, "/src/", $path))
    };
}

#[cfg(not(feature = "builtin-docs"))]
macro_rules! builtin_source {
    ($package:literal, $path:literal) => {
        include_str!(concat!(env!("OUT_DIR"), "/", $package, "/", $path))
    };
}

/// In dependency order: each module sees the ones before it. The modules
/// over Effect sit in blocks by area, each module where its dependencies
/// are already in.
pub(super) const SOURCES: &[(&str, &str)] = &[
    ("Prelude", builtin_source!("core", "Prelude.fl")),
    ("Instances", builtin_source!("core", "Instances.fl")),
    ("Record", builtin_source!("core", "Record.fl")),
    ("Ref", builtin_source!("core", "Ref.fl")),
    ("String", builtin_source!("core", "String.fl")),
    ("Float", builtin_source!("core", "Float.fl")),
    ("Int", builtin_source!("core", "Int.fl")),
    ("Numeric", builtin_source!("core", "Numeric.fl")),
    ("Platform", builtin_source!("std", "Platform.fl")),
    ("App", builtin_source!("std", "App.fl")),
    ("Layer", builtin_source!("std", "Layer.fl")),
    ("Fiber", builtin_source!("std", "Fiber.fl")),
    ("RawCause", builtin_source!("std", "RawCause.fl")),
    ("Failure", builtin_source!("std", "Failure.fl")),
    ("Exit", builtin_source!("std", "Exit.fl")),
    ("NonEmpty", builtin_source!("core", "NonEmpty.fl")),
    ("Option", builtin_source!("core", "Option.fl")),
    ("Result", builtin_source!("core", "Result.fl")),
    ("List.Core", builtin_source!("core", "List/Core.fl")),
    ("List.Slice", builtin_source!("core", "List/Slice.fl")),
    ("List.Search", builtin_source!("core", "List/Search.fl")),
    ("List.Transform", builtin_source!("core", "List/Transform.fl")),
    ("List", builtin_source!("core", "List.fl")),
    ("Markdown", builtin_source!("std", "Markdown.fl")),
    ("Seq", builtin_source!("core", "Seq.fl")),
    ("Monoids", builtin_source!("core", "Monoids.fl")),
    ("Validation", builtin_source!("core", "Validation.fl")),
    ("Chunk.Core", builtin_source!("std", "Chunk/Core.fl")),
    ("Chunk.Ops", builtin_source!("std", "Chunk/Ops.fl")),
    ("Chunk", builtin_source!("std", "Chunk.fl")),
    ("Js.Array", builtin_source!("std", "Js/Array.fl")),
    ("Js", builtin_source!("std", "Js.fl")),
    ("JsError", builtin_source!("std", "JsError.fl")),
    ("BigInt", builtin_source!("std", "BigInt.fl")),
    ("Redacted", builtin_source!("std", "Redacted.fl")),
    ("BigDecimal", builtin_source!("std", "BigDecimal.fl")),
    ("Duration", builtin_source!("std", "Duration.fl")),
    ("EqHash", builtin_source!("std", "EqHash.fl")),
    ("HashSet", builtin_source!("std", "HashSet.fl")),
    ("HashMap", builtin_source!("std", "HashMap.fl")),
    ("Loop", builtin_source!("std", "Loop.fl")),
    ("Log", builtin_source!("std", "Log.fl")),
    #[cfg(feature = "server-stdlib")]
    ("File", builtin_source!("std", "File.fl")),
    ("Resource", builtin_source!("std", "Resource.fl")),
    ("Events", builtin_source!("std", "Events.fl")),
    ("Gen", builtin_source!("std", "Gen.fl")),
    ("Checks", builtin_source!("std", "Checks.fl")),
    ("Schema", builtin_source!("std", "Schema.fl")),
    ("Route", builtin_source!("std", "Route.fl")),
    ("Codec", builtin_source!("std", "Codec.fl")),
    ("Json", builtin_source!("std", "Json.fl")),
    ("Sql", builtin_source!("std", "Sql.fl")),
    ("Database", builtin_source!("std", "Database.fl")),
    ("Database.Migrate", builtin_source!("std", "Database/Migrate.fl")),
    ("Test", builtin_source!("std", "Test.fl")),
    ("Flick.Debug", builtin_source!("core", "Flick/Debug.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Runner.Node", builtin_source!("std", "Runner/Node.fl")),
    ("Runner.Browser", builtin_source!("std", "Runner/Browser.fl")),
    // Effect: values and classes
    ("RegExp", builtin_source!("std", "RegExp.fl")),
    ("Combiner", builtin_source!("std", "Combiner.fl")),
    ("Reducer", builtin_source!("std", "Reducer.fl")),
    ("Ordering", builtin_source!("std", "Ordering.fl")),
    ("Order", builtin_source!("std", "Order.fl")),
    ("Equivalence", builtin_source!("std", "Equivalence.fl")),
    ("Hash", builtin_source!("std", "Hash.fl")),
    ("Optic", builtin_source!("std", "Optic.fl")),
    ("JsonPointer", builtin_source!("std", "JsonPointer.fl")),
    ("JsonPatch", builtin_source!("std", "JsonPatch.fl")),
    ("Inspectable", builtin_source!("std", "Inspectable.fl")),
    ("Formatter", builtin_source!("std", "Formatter.fl")),
    ("Version", builtin_source!("std", "Version.fl")),
    ("ByteSize", builtin_source!("std", "ByteSize.fl")),
    ("Bytes", builtin_source!("std", "Bytes.fl")),
    ("Crypto", builtin_source!("std", "Crypto.fl")),
    // end of Effect: values and classes
    // Effect: lists and records
    ("Bool", builtin_source!("std", "Bool.fl")),
    ("Predicate", builtin_source!("std", "Predicate.fl")),
    ("Filter", builtin_source!("std", "Filter.fl")),
    ("Function", builtin_source!("std", "Function.fl")),
    // end of Effect: lists and records
    // Effect: collections
    ("Trie", builtin_source!("std", "Trie.fl")),
    ("HashRing", builtin_source!("std", "HashRing.fl")),
    ("Graph.Types", builtin_source!("std", "Graph/Types.fl")),
    ("Graph.Build", builtin_source!("std", "Graph/Build.fl")),
    ("Graph.Query", builtin_source!("std", "Graph/Query.fl")),
    ("Graph.Algorithms", builtin_source!("std", "Graph/Algorithms.fl")),
    ("Graph.Walk", builtin_source!("std", "Graph/Walk.fl")),
    ("Graph", builtin_source!("std", "Graph.fl")),
    // end of Effect: collections
    // Effect: time
    ("DateTime", builtin_source!("std", "DateTime.fl")),
    ("Cron", builtin_source!("std", "Cron.fl")),
    ("Clock", builtin_source!("std", "Clock.fl")),
    ("Jwt", builtin_source!("std", "Jwt.fl")),
    ("Random", builtin_source!("std", "Random.fl")),
    // end of Effect: time
    // Effect: concurrency
    ("SynchronizedRef", builtin_source!("std", "SynchronizedRef.fl")),
    ("SubscriptionRef", builtin_source!("std", "SubscriptionRef.fl")),
    ("Deferred", builtin_source!("std", "Deferred.fl")),
    ("Latch", builtin_source!("std", "Latch.fl")),
    ("Semaphore", builtin_source!("std", "Semaphore.fl")),
    ("PartitionedSemaphore", builtin_source!("std", "PartitionedSemaphore.fl")),
    ("Queue", builtin_source!("std", "Queue.fl")),
    ("PubSub", builtin_source!("std", "PubSub.fl")),
    ("Cache", builtin_source!("std", "Cache.fl")),
    ("ScopedCache", builtin_source!("std", "ScopedCache.fl")),
    ("ScopedRef", builtin_source!("std", "ScopedRef.fl")),
    ("RcRef", builtin_source!("std", "RcRef.fl")),
    ("RcMap", builtin_source!("std", "RcMap.fl")),
    ("Pool", builtin_source!("std", "Pool.fl")),
    // end of Effect: concurrency
    // Effect: transactions
    ("Tx", builtin_source!("std", "Tx.fl")),
    ("TxRef", builtin_source!("std", "TxRef.fl")),
    ("TxChunk", builtin_source!("std", "TxChunk.fl")),
    ("TxDeferred", builtin_source!("std", "TxDeferred.fl")),
    ("TxHashMap", builtin_source!("std", "TxHashMap.fl")),
    ("TxHashSet", builtin_source!("std", "TxHashSet.fl")),
    ("TxPriorityQueue", builtin_source!("std", "TxPriorityQueue.fl")),
    ("TxQueue", builtin_source!("std", "TxQueue.fl")),
    ("TxPubSub", builtin_source!("std", "TxPubSub.fl")),
    ("TxReentrantLock", builtin_source!("std", "TxReentrantLock.fl")),
    ("TxSemaphore", builtin_source!("std", "TxSemaphore.fl")),
    ("TxSubscriptionRef", builtin_source!("std", "TxSubscriptionRef.fl")),
    // end of Effect: transactions
    // Effect: schema
    ("Check", builtin_source!("std", "Check.fl")),
    ("Arbitrary", builtin_source!("std", "Arbitrary.fl")),
    ("JsonSchema", builtin_source!("std", "JsonSchema.fl")),
    ("Differ", builtin_source!("std", "Differ.fl")),
    ("Net", builtin_source!("std", "Net.fl")),
    // end of Effect: schema
    // Effect: platform and observability
    ("PlatformError", builtin_source!("std", "PlatformError.fl")),
    ("Path", builtin_source!("std", "Path.fl")),
    ("LogLevel", builtin_source!("std", "LogLevel.fl")),
    ("Console", builtin_source!("std", "Console.fl")),
    ("FileSystem", builtin_source!("std", "FileSystem.fl")),
    ("Terminal", builtin_source!("std", "Terminal.fl")),
    ("Stdio", builtin_source!("std", "Stdio.fl")),
    ("Logger", builtin_source!("std", "Logger.fl")),
    ("ConfigProvider", builtin_source!("std", "ConfigProvider.fl")),
    ("Metric", builtin_source!("std", "Metric.fl")),
    ("Tracer", builtin_source!("std", "Tracer.fl")),
    // end of Effect: platform and observability
    // Effect: context, layers and requests
    ("Context", builtin_source!("std", "Context.fl")),
    ("Scope", builtin_source!("std", "Scope.fl")),
    ("Layer.Trace", builtin_source!("std", "Layer/Trace.fl")),
    ("LayerMap", builtin_source!("std", "LayerMap.fl")),
    ("LayerRef", builtin_source!("std", "LayerRef.fl")),
    ("ErrorReporter", builtin_source!("std", "ErrorReporter.fl")),
    ("Request", builtin_source!("std", "Request.fl")),
    ("RequestResolver", builtin_source!("std", "RequestResolver.fl")),
    // end of Effect: context, layers and requests
    // Effect: effects, schedules and fibers
    ("Schedule", builtin_source!("std", "Schedule.fl")),
    ("Cause", builtin_source!("std", "Cause.fl")),
    ("Scheduler", builtin_source!("std", "Scheduler.fl")),
    ("References", builtin_source!("std", "References.fl")),
    ("ExecutionPlan", builtin_source!("std", "ExecutionPlan.fl")),
    ("Effect.Core", builtin_source!("std", "Effect/Core.fl")),
    ("Effect.Collections", builtin_source!("std", "Effect/Collections.fl")),
    ("Effect.Errors", builtin_source!("std", "Effect/Errors.fl")),
    ("Effect.Concurrency", builtin_source!("std", "Effect/Concurrency.fl")),
    ("Effect.Observe", builtin_source!("std", "Effect/Observe.fl")),
    ("Effect", builtin_source!("std", "Effect.fl")),
    ("FiberHandle", builtin_source!("std", "FiberHandle.fl")),
    ("FiberMap", builtin_source!("std", "FiberMap.fl")),
    ("FiberSet", builtin_source!("std", "FiberSet.fl")),
    // end of Effect: effects, schedules and fibers
    // Effect: streams, sinks and channels
    ("Channel.Core", builtin_source!("std", "Channel/Core.fl")),
    ("Channel.Transform", builtin_source!("std", "Channel/Transform.fl")),
    ("Channel.Control", builtin_source!("std", "Channel/Control.fl")),
    ("Channel.Run", builtin_source!("std", "Channel/Run.fl")),
    ("Channel", builtin_source!("std", "Channel.fl")),
    ("ChannelSchema", builtin_source!("std", "ChannelSchema.fl")),
    // end of Effect: streams, sinks and channels
    // Effect: streams, Stream
    ("Stream.Core", builtin_source!("std", "Stream/Core.fl")),
    ("Sink", builtin_source!("std", "Sink.fl")),
    ("Stream.Transform", builtin_source!("std", "Stream/Transform.fl")),
    ("Stream.Combine", builtin_source!("std", "Stream/Combine.fl")),
    ("Stream.Errors", builtin_source!("std", "Stream/Errors.fl")),
    ("Stream.Run", builtin_source!("std", "Stream/Run.fl")),
    ("Stream.Pipe", builtin_source!("std", "Stream/Pipe.fl")),
    ("Stream.Platform", builtin_source!("std", "Stream/Platform.fl")),
    ("Stream", builtin_source!("std", "Stream.fl")),
    // end of Effect: streams, Stream
    // Effect: child processes
    ("ChildProcess", builtin_source!("std", "ChildProcess.fl")),
    // Effect: reactivity (unstable in Effect 4.1.2)
    ("Reactivity.AsyncResult", builtin_source!("std", "Reactivity/AsyncResult.fl")),
    ("Reactivity.Atom", builtin_source!("std", "Reactivity/Atom.fl")),
    // UI on atoms: `Atom` over the reactivity modules, then `Ui`
    ("Atom", builtin_source!("std", "Atom.fl")),
    ("Ui", builtin_source!("std", "Ui.fl")),
    ("Ui.Event", builtin_source!("std", "Ui/Event.fl")),
    ("Ui.Dom", builtin_source!("std", "Ui/Dom.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Ui.Server", builtin_source!("std", "Ui/Server.fl")),
    ("Form", builtin_source!("std", "Form.fl")),
    ("Action", builtin_source!("std", "Action.fl")),
    // Effect: HTTP
    ("Http.Method", builtin_source!("std", "Http/Method.fl")),
    ("Http.Status", builtin_source!("std", "Http/Status.fl")),
    ("Http.Headers", builtin_source!("std", "Http/Headers.fl")),
    ("Http.UrlParams", builtin_source!("std", "Http/UrlParams.fl")),
    ("Http.Url", builtin_source!("std", "Http/Url.fl")),
    ("Http.Mime", builtin_source!("std", "Http/Mime.fl")),
    ("Http.Cookies", builtin_source!("std", "Http/Cookies.fl")),
    ("Http.Etag", builtin_source!("std", "Http/Etag.fl")),
    ("Http.Codecs", builtin_source!("std", "Http/Codecs.fl")),
    ("Http.Body", builtin_source!("std", "Http/Body.fl")),
    ("Http.ClientError", builtin_source!("std", "Http/ClientError.fl")),
    ("Http.ClientRequest", builtin_source!("std", "Http/ClientRequest.fl")),
    ("Http.ClientResponse", builtin_source!("std", "Http/ClientResponse.fl")),
    ("Http.Client", builtin_source!("std", "Http/Client.fl")),
    ("Http.FetchClient", builtin_source!("std", "Http/FetchClient.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.ServerError", builtin_source!("std", "Http/ServerError.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.ServerRequest", builtin_source!("std", "Http/ServerRequest.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.ServerResponse", builtin_source!("std", "Http/ServerResponse.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Multipart", builtin_source!("std", "Http/Multipart.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Platform", builtin_source!("std", "Http/Platform.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Middleware", builtin_source!("std", "Http/Middleware.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Router", builtin_source!("std", "Http/Router.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Server", builtin_source!("std", "Http/Server.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.StaticServer", builtin_source!("std", "Http/StaticServer.fl")),
    ("Http.TraceContext", builtin_source!("std", "Http/TraceContext.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Runner.Cloudflare", builtin_source!("std", "Runner/Cloudflare.fl")),
    // end of Effect: HTTP
    // Effect: HttpApi
    #[cfg(feature = "server-stdlib")]
    ("HttpApi", builtin_source!("std", "HttpApi.fl")),
    #[cfg(feature = "server-stdlib")]
    ("HttpApi.Derive", builtin_source!("std", "HttpApi/Derive.fl")),
    #[cfg(feature = "server-stdlib")]
    ("HttpApi.Mount", builtin_source!("std", "HttpApi/Mount.fl")),
    #[cfg(feature = "server-stdlib")]
    ("HttpApi.Inject", builtin_source!("std", "HttpApi/Inject.fl")),
    // end of Effect: HttpApi
    // Effect: RPC (unstable in Effect 4.1.2)
    #[cfg(feature = "server-stdlib")]
    ("Rpc", builtin_source!("std", "Rpc.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Server", builtin_source!("std", "Server.fl")),
    // after `Server`: a secret is read only on a server
    #[cfg(feature = "server-stdlib")]
    ("Config", builtin_source!("std", "Config.fl")),
    // end of Effect: RPC
    // Effect: observability (unstable in Effect 4.1.2)
    #[cfg(feature = "server-stdlib")]
    ("Otlp", builtin_source!("std", "Otlp.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Prometheus", builtin_source!("std", "Prometheus.fl")),
    // end of Effect: observability
    // Effect: devtools (unstable in Effect 4.1.2)
    #[cfg(feature = "server-stdlib")]
    ("DevTools", builtin_source!("std", "DevTools.fl")),
    // end of Effect: devtools
    // Effect: CLI
    #[cfg(feature = "server-stdlib")]
    ("Cli.Prompt", builtin_source!("std", "Cli/Prompt.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Cli", builtin_source!("std", "Cli.fl")),
    // end of Effect: CLI
];
