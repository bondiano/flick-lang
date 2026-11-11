# A repository factory and a logging adapter

This example uses static higher-order modules with an instrumented in-memory
database driver. It does not connect to an external database.

`Make(Database)` exports repository operations and a `Layer`. `WithLogging`
accepts that factory and returns an anonymous factory; `Instances` applies the
returned factory and aliases its result. Applying either factory constructs
deferred layer code. The connection is acquired only when the layer is built.
Acquisition and finalizer registration run together under an interruption mask;
the scope then owns the connection until its layer's body ends.

`Domain.Repo` is a deliberately shared nominal service API. Both repository
contracts require that exact declaration. A different service with an identical
`get` operation cannot satisfy the contract. `Logged.get` explicitly requires
both this `Repo` and `Log.Log`; forwarding `Base.layer` retains the original
service identity and resource ownership.

From this directory:

```sh
flick check --warnings-as-errors
flick build
node dist/flick-main.js
```

`expected.stdout` is the lifecycle contract checked on JS and Go in debug and
release profiles. The initial counters prove that module/factory construction
does not acquire resources. A successful body, a failing body, a failure after
acquisition and an interrupted body each release exactly one connection before
the result reaches the caller. Failed acquisition releases none. Two overlapping
layer runs reach two live connections and close both distinct IDs exactly once.
The logging alias uses the same built service, without another acquisition.

The integration test also checks that a caller which provides only the
repository layer is rejected with `FLICK-E002` because it still needs `Log`.
