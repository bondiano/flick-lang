---
title: "GADT row indices"
---

A constructor result can fix or relate indices of kind `ServiceRow`,
`ErrorRow` or `CaptureRow`. A `match` branch checks its body under those
equations. The function still needs a complete signature.

```flick
service Db =
  get : Int ->{} Int

type Cmd (r : ServiceRow) a =
  | Value a : Cmd none a
  | Read Int : Cmd {Db} Int

run : Cmd r a ->{r} a
let run command =
  match command with
  | Value value -> value
  | Read key -> Db.get key
```

The equation in the `Read` branch permits `Db`. It preserves the runtime
stage: even a call whose requirements become empty remains a runtime call.
Refinement cannot make the same implementation a pure function.

```flick fail E001
# service Db =
#   get : Int ->{} Int
# type Cmd (r : ServiceRow) =
#   | Read Int : Cmd {Db}
pure : Cmd r -> Int
let pure command =
  match command with
  | Read key -> Db.get key
```

An equation can have several residuals. In this example `s` may be empty
or contain `Db`; the branch proves that every allowed `s` fits `{Db}`.
It does not choose an empty residual.

```flick
# service Db =
#   get : Int ->{} Int
type Packed (r : ServiceRow) =
  | Packed (Eff s none Int) : Packed {Db | s}

unpack : Packed {Db} ->{Db} Int
let unpack packed =
  match packed with
  | Packed work -> await work
```

Exhaustiveness keeps these equations while checking the other parts of a
tuple. Here the first part proves that `r` contains `Db`, so `Empty` cannot
occur in the second part:

```flick
# service Db =
#   get : Int ->{} Int
type Includes (r : ServiceRow) (s : ServiceRow) =
  | Includes : Includes r {Db | r}

type RowCmd (r : ServiceRow) =
  | Empty : RowCmd none
  | WithDb Int : RowCmd {Db}

read_value : (Includes r r, RowCmd r) -> Int
let read_value pair =
  match pair with
  | (Includes, WithDb value) -> value
```

Hidden residuals of separate constructor occurrences remain independent.
Only their shared outer indices relate the parts of the value.

The same reasoning applies to error rows. Local handlers remove the
requirements they cover before the surrounding signature is checked.
Equations stay inside the branch, including checks deferred until the
declaration ends. Service and error arguments remain invariant: knowing
that a row contains `Store String` never permits `Store Int`.

Capture indices can relate the permitted captures of values:

```flick
type Share (c : CaptureRow) (d : CaptureRow) =
  | Share : Share c c

share : Share c d -> (Unit -> Int)^c -> (Unit -> Int)^d
let share proof callback =
  match proof with
  | Share -> callback
```

This does not identify distinct regions. Parameters of kind `Region` and
record-row parameters must still be passed as variables, and patterns do
not learn a new region identity. Refinement also preserves the existing
rules for handlers, foreign calls and mutable references.

These rules are shared by JS and Go. See [type equality](../type-equality/)
and [GADTs](../gadts/) for the other constraints on a refining branch.
