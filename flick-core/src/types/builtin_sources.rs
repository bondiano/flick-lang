//! The sources of the built-in modules, in the order `builtins` checks
//! them.

#[cfg(feature = "builtin-docs")]
macro_rules! builtin_source {
    ($path:literal) => {
        include_str!($path)
    };
}

#[cfg(not(feature = "builtin-docs"))]
macro_rules! builtin_source {
    ($path:literal) => {
        include_str!(concat!(env!("OUT_DIR"), "/", $path))
    };
}

/// In dependency order: each module sees the ones before it. The modules
/// over Effect sit in blocks by area, each module where its dependencies
/// are already in.
pub(super) const SOURCES: &[(&str, &str)] = &[
    ("Prelude", builtin_source!("builtins/Prelude.fl")),
    ("Instances", builtin_source!("builtins/Instances.fl")),
    ("Record", builtin_source!("builtins/Record.fl")),
    ("Ref", builtin_source!("builtins/Ref.fl")),
    ("String", builtin_source!("builtins/String.fl")),
    ("Float", builtin_source!("builtins/Float.fl")),
    ("Int", builtin_source!("builtins/Int.fl")),
    ("Numeric", builtin_source!("builtins/Numeric.fl")),
    ("Platform", builtin_source!("builtins/Platform.fl")),
    ("App", builtin_source!("builtins/App.fl")),
    ("Layer", builtin_source!("builtins/Layer.fl")),
    ("Fiber", builtin_source!("builtins/Fiber.fl")),
    ("RawCause", builtin_source!("builtins/RawCause.fl")),
    ("Failure", builtin_source!("builtins/Failure.fl")),
    ("Exit", builtin_source!("builtins/Exit.fl")),
    ("NonEmpty", builtin_source!("builtins/NonEmpty.fl")),
    ("Option", builtin_source!("builtins/Option.fl")),
    ("Result", builtin_source!("builtins/Result.fl")),
    ("List.Core", builtin_source!("builtins/List/Core.fl")),
    ("List.Slice", builtin_source!("builtins/List/Slice.fl")),
    ("List.Search", builtin_source!("builtins/List/Search.fl")),
    (
        "List.Transform",
        builtin_source!("builtins/List/Transform.fl"),
    ),
    ("List", builtin_source!("builtins/List.fl")),
    ("Seq", builtin_source!("builtins/Seq.fl")),
    ("Monoids", builtin_source!("builtins/Monoids.fl")),
    ("Validation", builtin_source!("builtins/Validation.fl")),
    ("Chunk.Core", builtin_source!("builtins/Chunk/Core.fl")),
    ("Chunk.Ops", builtin_source!("builtins/Chunk/Ops.fl")),
    ("Chunk", builtin_source!("builtins/Chunk.fl")),
    ("Js.Array", builtin_source!("builtins/Js/Array.fl")),
    ("Js", builtin_source!("builtins/Js.fl")),
    ("JsError", builtin_source!("builtins/JsError.fl")),
    ("BigInt", builtin_source!("builtins/BigInt.fl")),
    ("Redacted", builtin_source!("builtins/Redacted.fl")),
    ("BigDecimal", builtin_source!("builtins/BigDecimal.fl")),
    ("Duration", builtin_source!("builtins/Duration.fl")),
    ("EqHash", builtin_source!("builtins/EqHash.fl")),
    ("HashSet", builtin_source!("builtins/HashSet.fl")),
    ("HashMap", builtin_source!("builtins/HashMap.fl")),
    ("Loop", builtin_source!("builtins/Loop.fl")),
    ("Log", builtin_source!("builtins/Log.fl")),
    #[cfg(feature = "server-stdlib")]
    ("File", builtin_source!("builtins/File.fl")),
    ("Resource", builtin_source!("builtins/Resource.fl")),
    ("Events", builtin_source!("builtins/Events.fl")),
    ("Gen", builtin_source!("builtins/Gen.fl")),
    ("Checks", builtin_source!("builtins/Checks.fl")),
    ("Schema", builtin_source!("builtins/Schema.fl")),
    ("Route", builtin_source!("builtins/Route.fl")),
    ("Codec", builtin_source!("builtins/Codec.fl")),
    ("Json", builtin_source!("builtins/Json.fl")),
    ("Sql", builtin_source!("builtins/Sql.fl")),
    ("Database", builtin_source!("builtins/Database.fl")),
    ("Test", builtin_source!("builtins/Test.fl")),
    ("Flick.Debug", builtin_source!("builtins/Flick/Debug.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Runner.Node", builtin_source!("builtins/Runner/Node.fl")),
    (
        "Runner.Browser",
        builtin_source!("builtins/Runner/Browser.fl"),
    ),
    // Effect: values and classes
    ("RegExp", builtin_source!("builtins/RegExp.fl")),
    ("Combiner", builtin_source!("builtins/Combiner.fl")),
    ("Reducer", builtin_source!("builtins/Reducer.fl")),
    ("Ordering", builtin_source!("builtins/Ordering.fl")),
    ("Order", builtin_source!("builtins/Order.fl")),
    ("Equivalence", builtin_source!("builtins/Equivalence.fl")),
    ("Hash", builtin_source!("builtins/Hash.fl")),
    ("Optic", builtin_source!("builtins/Optic.fl")),
    ("JsonPointer", builtin_source!("builtins/JsonPointer.fl")),
    ("JsonPatch", builtin_source!("builtins/JsonPatch.fl")),
    ("Inspectable", builtin_source!("builtins/Inspectable.fl")),
    ("Formatter", builtin_source!("builtins/Formatter.fl")),
    ("Version", builtin_source!("builtins/Version.fl")),
    ("ByteSize", builtin_source!("builtins/ByteSize.fl")),
    ("Crypto", builtin_source!("builtins/Crypto.fl")),
    // end of Effect: values and classes
    // Effect: lists and records
    ("Bool", builtin_source!("builtins/Bool.fl")),
    ("Predicate", builtin_source!("builtins/Predicate.fl")),
    ("Filter", builtin_source!("builtins/Filter.fl")),
    ("Function", builtin_source!("builtins/Function.fl")),
    // end of Effect: lists and records
    // Effect: collections
    ("Trie", builtin_source!("builtins/Trie.fl")),
    ("HashRing", builtin_source!("builtins/HashRing.fl")),
    ("Graph.Types", builtin_source!("builtins/Graph/Types.fl")),
    ("Graph.Build", builtin_source!("builtins/Graph/Build.fl")),
    ("Graph.Query", builtin_source!("builtins/Graph/Query.fl")),
    (
        "Graph.Algorithms",
        builtin_source!("builtins/Graph/Algorithms.fl"),
    ),
    ("Graph.Walk", builtin_source!("builtins/Graph/Walk.fl")),
    ("Graph", builtin_source!("builtins/Graph.fl")),
    // end of Effect: collections
    // Effect: time
    ("DateTime", builtin_source!("builtins/DateTime.fl")),
    ("Cron", builtin_source!("builtins/Cron.fl")),
    ("Clock", builtin_source!("builtins/Clock.fl")),
    ("Random", builtin_source!("builtins/Random.fl")),
    // end of Effect: time
    // Effect: concurrency
    (
        "SynchronizedRef",
        builtin_source!("builtins/SynchronizedRef.fl"),
    ),
    (
        "SubscriptionRef",
        builtin_source!("builtins/SubscriptionRef.fl"),
    ),
    ("Deferred", builtin_source!("builtins/Deferred.fl")),
    ("Latch", builtin_source!("builtins/Latch.fl")),
    ("Semaphore", builtin_source!("builtins/Semaphore.fl")),
    (
        "PartitionedSemaphore",
        builtin_source!("builtins/PartitionedSemaphore.fl"),
    ),
    ("Queue", builtin_source!("builtins/Queue.fl")),
    ("PubSub", builtin_source!("builtins/PubSub.fl")),
    ("Cache", builtin_source!("builtins/Cache.fl")),
    ("ScopedCache", builtin_source!("builtins/ScopedCache.fl")),
    ("ScopedRef", builtin_source!("builtins/ScopedRef.fl")),
    ("RcRef", builtin_source!("builtins/RcRef.fl")),
    ("RcMap", builtin_source!("builtins/RcMap.fl")),
    ("Pool", builtin_source!("builtins/Pool.fl")),
    // end of Effect: concurrency
    // Effect: transactions
    ("Tx", builtin_source!("builtins/Tx.fl")),
    ("TxRef", builtin_source!("builtins/TxRef.fl")),
    ("TxChunk", builtin_source!("builtins/TxChunk.fl")),
    ("TxDeferred", builtin_source!("builtins/TxDeferred.fl")),
    ("TxHashMap", builtin_source!("builtins/TxHashMap.fl")),
    ("TxHashSet", builtin_source!("builtins/TxHashSet.fl")),
    (
        "TxPriorityQueue",
        builtin_source!("builtins/TxPriorityQueue.fl"),
    ),
    ("TxQueue", builtin_source!("builtins/TxQueue.fl")),
    ("TxPubSub", builtin_source!("builtins/TxPubSub.fl")),
    (
        "TxReentrantLock",
        builtin_source!("builtins/TxReentrantLock.fl"),
    ),
    ("TxSemaphore", builtin_source!("builtins/TxSemaphore.fl")),
    (
        "TxSubscriptionRef",
        builtin_source!("builtins/TxSubscriptionRef.fl"),
    ),
    // end of Effect: transactions
    // Effect: schema
    ("Check", builtin_source!("builtins/Check.fl")),
    ("Arbitrary", builtin_source!("builtins/Arbitrary.fl")),
    ("JsonSchema", builtin_source!("builtins/JsonSchema.fl")),
    ("Differ", builtin_source!("builtins/Differ.fl")),
    ("Net", builtin_source!("builtins/Net.fl")),
    // end of Effect: schema
    // Effect: platform and observability
    (
        "PlatformError",
        builtin_source!("builtins/PlatformError.fl"),
    ),
    ("Path", builtin_source!("builtins/Path.fl")),
    ("LogLevel", builtin_source!("builtins/LogLevel.fl")),
    ("Console", builtin_source!("builtins/Console.fl")),
    ("FileSystem", builtin_source!("builtins/FileSystem.fl")),
    ("Terminal", builtin_source!("builtins/Terminal.fl")),
    ("Stdio", builtin_source!("builtins/Stdio.fl")),
    ("Logger", builtin_source!("builtins/Logger.fl")),
    (
        "ConfigProvider",
        builtin_source!("builtins/ConfigProvider.fl"),
    ),
    ("Metric", builtin_source!("builtins/Metric.fl")),
    ("Tracer", builtin_source!("builtins/Tracer.fl")),
    // end of Effect: platform and observability
    // Effect: context, layers and requests
    ("Context", builtin_source!("builtins/Context.fl")),
    ("Scope", builtin_source!("builtins/Scope.fl")),
    ("Layer.Trace", builtin_source!("builtins/Layer/Trace.fl")),
    ("LayerMap", builtin_source!("builtins/LayerMap.fl")),
    ("LayerRef", builtin_source!("builtins/LayerRef.fl")),
    (
        "ErrorReporter",
        builtin_source!("builtins/ErrorReporter.fl"),
    ),
    ("Request", builtin_source!("builtins/Request.fl")),
    (
        "RequestResolver",
        builtin_source!("builtins/RequestResolver.fl"),
    ),
    // end of Effect: context, layers and requests
    // Effect: effects, schedules and fibers
    ("Schedule", builtin_source!("builtins/Schedule.fl")),
    ("Cause", builtin_source!("builtins/Cause.fl")),
    ("Scheduler", builtin_source!("builtins/Scheduler.fl")),
    ("References", builtin_source!("builtins/References.fl")),
    (
        "ExecutionPlan",
        builtin_source!("builtins/ExecutionPlan.fl"),
    ),
    ("Effect.Core", builtin_source!("builtins/Effect/Core.fl")),
    (
        "Effect.Collections",
        builtin_source!("builtins/Effect/Collections.fl"),
    ),
    (
        "Effect.Errors",
        builtin_source!("builtins/Effect/Errors.fl"),
    ),
    (
        "Effect.Concurrency",
        builtin_source!("builtins/Effect/Concurrency.fl"),
    ),
    (
        "Effect.Observe",
        builtin_source!("builtins/Effect/Observe.fl"),
    ),
    ("Effect", builtin_source!("builtins/Effect.fl")),
    ("FiberHandle", builtin_source!("builtins/FiberHandle.fl")),
    ("FiberMap", builtin_source!("builtins/FiberMap.fl")),
    ("FiberSet", builtin_source!("builtins/FiberSet.fl")),
    // end of Effect: effects, schedules and fibers
    // Effect: streams, sinks and channels
    ("Channel.Core", builtin_source!("builtins/Channel/Core.fl")),
    (
        "Channel.Transform",
        builtin_source!("builtins/Channel/Transform.fl"),
    ),
    (
        "Channel.Control",
        builtin_source!("builtins/Channel/Control.fl"),
    ),
    ("Channel.Run", builtin_source!("builtins/Channel/Run.fl")),
    ("Channel", builtin_source!("builtins/Channel.fl")),
    (
        "ChannelSchema",
        builtin_source!("builtins/ChannelSchema.fl"),
    ),
    // end of Effect: streams, sinks and channels
    // Effect: streams, Stream
    ("Stream.Core", builtin_source!("builtins/Stream/Core.fl")),
    ("Sink", builtin_source!("builtins/Sink.fl")),
    (
        "Stream.Transform",
        builtin_source!("builtins/Stream/Transform.fl"),
    ),
    (
        "Stream.Combine",
        builtin_source!("builtins/Stream/Combine.fl"),
    ),
    (
        "Stream.Errors",
        builtin_source!("builtins/Stream/Errors.fl"),
    ),
    ("Stream.Run", builtin_source!("builtins/Stream/Run.fl")),
    ("Stream.Pipe", builtin_source!("builtins/Stream/Pipe.fl")),
    (
        "Stream.Platform",
        builtin_source!("builtins/Stream/Platform.fl"),
    ),
    ("Stream", builtin_source!("builtins/Stream.fl")),
    // end of Effect: streams, Stream
    // Effect: child processes
    ("ChildProcess", builtin_source!("builtins/ChildProcess.fl")),
    // Effect: reactivity (unstable in Effect 4.0.5)
    (
        "Reactivity.AsyncResult",
        builtin_source!("builtins/Reactivity/AsyncResult.fl"),
    ),
    (
        "Reactivity.Atom",
        builtin_source!("builtins/Reactivity/Atom.fl"),
    ),
    // UI on atoms: `Atom` over the reactivity modules, then `Ui`
    ("Atom", builtin_source!("builtins/Atom.fl")),
    ("Ui", builtin_source!("builtins/Ui.fl")),
    ("Ui.Event", builtin_source!("builtins/Ui/Event.fl")),
    ("Ui.Dom", builtin_source!("builtins/Ui/Dom.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Ui.Server", builtin_source!("builtins/Ui/Server.fl")),
    ("Form", builtin_source!("builtins/Form.fl")),
    ("Action", builtin_source!("builtins/Action.fl")),
    // Effect: HTTP
    ("Http.Method", builtin_source!("builtins/Http/Method.fl")),
    ("Http.Status", builtin_source!("builtins/Http/Status.fl")),
    ("Http.Headers", builtin_source!("builtins/Http/Headers.fl")),
    (
        "Http.UrlParams",
        builtin_source!("builtins/Http/UrlParams.fl"),
    ),
    ("Http.Url", builtin_source!("builtins/Http/Url.fl")),
    ("Http.Mime", builtin_source!("builtins/Http/Mime.fl")),
    ("Http.Cookies", builtin_source!("builtins/Http/Cookies.fl")),
    ("Http.Etag", builtin_source!("builtins/Http/Etag.fl")),
    ("Http.Codecs", builtin_source!("builtins/Http/Codecs.fl")),
    ("Http.Body", builtin_source!("builtins/Http/Body.fl")),
    (
        "Http.ClientError",
        builtin_source!("builtins/Http/ClientError.fl"),
    ),
    (
        "Http.ClientRequest",
        builtin_source!("builtins/Http/ClientRequest.fl"),
    ),
    (
        "Http.ClientResponse",
        builtin_source!("builtins/Http/ClientResponse.fl"),
    ),
    ("Http.Client", builtin_source!("builtins/Http/Client.fl")),
    (
        "Http.FetchClient",
        builtin_source!("builtins/Http/FetchClient.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.ServerError",
        builtin_source!("builtins/Http/ServerError.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.ServerRequest",
        builtin_source!("builtins/Http/ServerRequest.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.ServerResponse",
        builtin_source!("builtins/Http/ServerResponse.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.Multipart",
        builtin_source!("builtins/Http/Multipart.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.Platform",
        builtin_source!("builtins/Http/Platform.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.Middleware",
        builtin_source!("builtins/Http/Middleware.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    ("Http.Router", builtin_source!("builtins/Http/Router.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Http.Server", builtin_source!("builtins/Http/Server.fl")),
    #[cfg(feature = "server-stdlib")]
    (
        "Http.StaticServer",
        builtin_source!("builtins/Http/StaticServer.fl"),
    ),
    (
        "Http.TraceContext",
        builtin_source!("builtins/Http/TraceContext.fl"),
    ),
    #[cfg(feature = "server-stdlib")]
    (
        "Runner.Cloudflare",
        builtin_source!("builtins/Runner/Cloudflare.fl"),
    ),
    // end of Effect: HTTP
    // Effect: HttpApi
    #[cfg(feature = "server-stdlib")]
    ("HttpApi", builtin_source!("builtins/HttpApi.fl")),
    // end of Effect: HttpApi
    // Effect: RPC (unstable in Effect 4.0.5)
    #[cfg(feature = "server-stdlib")]
    ("Rpc", builtin_source!("builtins/Rpc.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Server", builtin_source!("builtins/Server.fl")),
    // after `Server`: a secret is read only on a server
    #[cfg(feature = "server-stdlib")]
    ("Config", builtin_source!("builtins/Config.fl")),
    // end of Effect: RPC
    // Effect: observability (unstable in Effect 4.0.5)
    #[cfg(feature = "server-stdlib")]
    ("Otlp", builtin_source!("builtins/Otlp.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Prometheus", builtin_source!("builtins/Prometheus.fl")),
    // end of Effect: observability
    // Effect: devtools (unstable in Effect 4.0.5)
    #[cfg(feature = "server-stdlib")]
    ("DevTools", builtin_source!("builtins/DevTools.fl")),
    // end of Effect: devtools
    // Effect: CLI
    #[cfg(feature = "server-stdlib")]
    ("Cli.Prompt", builtin_source!("builtins/Cli/Prompt.fl")),
    #[cfg(feature = "server-stdlib")]
    ("Cli", builtin_source!("builtins/Cli.fl")),
    // end of Effect: CLI
];
