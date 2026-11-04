---
title: "Schemas and codecs"
---

A schema is a runtime witness of the values of a type; a codec adds a wire
form and turns it into values and back (see [Schema and standard data types](../../spec/07-schema/)). Both are values of the program
built from declarations, a DSL and library combinators. Decoding fails with
`SchemaError`, a list of issues `{ message; path }` that names paths and
never holds the input.

## Refined types

```flick
refined type Port = Float where fun n -> n > 0 && n < 65536
  deriving Eq, Schema

refined type Email = String where Checks.email
  deriving Eq, Hash, Schema
```

A refined type is nominal: its values are those of the base type that pass
the predicate, a pure `Bool` function. It has no constructor; its
companions are `T.parse : U -> Result SchemaError T` and
`T.value : T -> U`, and with `deriving Schema` also `T.json`, its JSON
codec. The value is represented as the base value.

`T.parse` of a constant is checked when the program compiles if the
compiler knows the check — a `Checks` function (`Checks.email`,
`Checks.non_empty`, `Checks.positive`) or a `fun` whose body folds to a
literal with the constant in place of its parameter. A constant that fails
it is `FLICK-D006`.

## Deriving

`deriving Schema` defines the witness `schema T`. The plugin of `std` writes
it in Flick, with the combinators of `Schema` below, which `flick expand`
shows:

| Declaration | Witness |
| - | - |
| `type Shape = …`, an alias of a record or tuple, a `type error` | `Schema Shape` |
| `type Tree a = …` | `Schema a -> Schema (Tree a)` |
| `type alias PairOf f = { count : f Float }` | `(forall a. Schema a -> Schema (f a)) -> Schema (PairOf f)` |
| a phantom parameter, which the representation does not use | a witness all the same |

```flick
tree_schema : Schema a -> Schema (Tree a)
let tree_schema item_schema =
  schema Tree item_schema
```

Every field needs a witness: a primitive, `List`, `Option`, `Result`,
`Redacted`, a tuple or a closed record of them, a type parameter, or a type
that derives `Schema` itself. An alias of the module is written out; a type
of another module, an alias among them, names its own witness, so it
derives `Schema` too. A reference to a type of the same module is lazy, so
recursive and mutually recursive types work. A function, a `Ref`, a scoped
resource or a runtime description (`Eff`, `Layer`, a codec) has no witness:
`std/D001` names the path of the field, as `Session.handler`. `schema T` of
a type that does not derive `Schema` is `FLICK-D001`.

The combinators the derive writes are values of `Schema` a program may use:

| Combinator | Witness |
| - | - |
| `Schema.int`, `Schema.number`, `Schema.string`, … | a primitive |
| `Schema.unsafe_record [Schema.field "x" s, …]` | a record, its fields in label order |
| `Schema.unsafe_tuple [Schema.item s, …]` | a tuple |
| `Schema.unsafe_adt "Shape" [Schema.unsafe_case Circle [Schema.item s], …]` | a variant type, its constructors given as values |
| `Schema.lazy "T" (fun _ -> s)` | a reference named `T` in JSON Schema, made on first use |
| `Schema.recursive "T" (fun self -> s)` | a recursive type, `self` the reference to it |
| `Schema.unsafe_unboxed s`, `Schema.unsafe_refined "T" known s T.parse` | an `unboxed type`, a refined one |

Nothing checks that an `unsafe_` combinator's parts are those of its type:
the derive writes them under the witness's signature.

## The codec DSL

```flick
profile_json : PureCodec Json Profile
let profile_json =
  codec (schema Profile) {
    email = field "email_address" Email.json;
    nickname = field "nickname" (option_as_null String.json);
  }
```

A `codec` block over the schema of a record gives every field exactly once:
a missing field is `FLICK-D002`, one given twice `FLICK-D003`, one the record
does not have `FLICK-D004`, and two fields with the same wire name
`FLICK-D005`. A block over a schema that is not of a record is `FLICK-D007`.
Keys the block does not name are refused unless the codec goes through
`ignore_extra_fields`.

| Field | Encode `None` | Decode |
| - | - | - |
| `field "x" codec` | by the codec | the key is required |
| `field "x" (option_as_null codec)` | `null` | `null` is `None`; a missing key is an error |
| `optional_field "x" codec` | the key is left out | a missing key is `None`; a present value goes through the codec |
| `field "x" (option_as_nullish codec)` | `null` | a missing key and `null` are `None` |
| `field "x" (Codec.tagged_option codec)` | `{"_tag": "None"}` | `{"_tag": "None"}` or `{"_tag": "Some", "value": …}` |

`field … |> on_missing (fun () -> v)` gives a missing key the default `v`,
calling the factory once per decode; `null` or a wrong value is still an
error. `on_missing_effect` takes a runtime factory, whose services decoding
then needs. A block is a `PureCodec Json R` when no field needs services to
decode, and a `Codec rd none Json R` otherwise: an effectful codec is never
a `PureCodec`, and using one as such is a type error.

## Codecs

| Function | |
| - | - |
| `Codec.decode`, `Codec.encode` | runtime, failing with `SchemaError` |
| `Codec.decode_result`, `Codec.encode_result` | pure, for a `PureCodec` |
| `Codec.json schema` | the canonical JSON form: records as objects, variants as `{"_tag": "Name", "_0": …}`, `Option` as the value or `null`, `Unit` as `null`, `BigInt` as a string |
| `String.json`, `Float.json_finite`, `Float.json`, `BigInt.json`, `Codec.bool`, `Codec.list` | primitives; `Float.json_finite` refuses `NaN` and the infinities both ways, `Float.json` writes them as strings |
| `Codec.transform`, `Codec.transform_result`, `Codec.transform_effect` | pure, fallible and runtime transformations |
| `Codec.transform_as schema decode encode codec` | `transform` whose values are those of `schema`: a union or the errors of an `HttpApi` endpoint tell them from others by their constructors, which a plain `transform` cannot |
| `Codec.check predicate message` | a check of the whole decoded value |
| `Redacted.json_reveal codec`, `Redacted.json_masked` | the explicit policies of a `Redacted` value: its content, or `"<redacted>"` one way |

A `Redacted` value has no JSON form without one of those policies:
`Codec.json` of a schema with a `Redacted` field fails to encode and decode
it rather than reveal it. Decoding reads only own data properties of its
input; a value met again on the path being decoded is a cycle and an error,
while a value shared by two paths is fine. `Json.parse` turns JSON text into
an `Unknown` to decode.

## Exporters and adapters

`Codec.json_schema root codec` writes the JSON Schema 2020-12 of the wire
form exactly, or returns an `ExportError` naming the path of the node it
cannot state: `Profile.session -> JSON Schema: scoped resource has no wire
codec`. A check without an exact JSON Schema form, a check of a value
after a transformation, a fallible or runtime transformation or decoding
fallback, a `Redacted` value without a policy, an opaque witness
(`Schema.opaque`) and `Unknown` are refused; nothing is written as `{}` or
`true`. The checks of refined types it knows are `Checks.email`,
`Checks.non_empty` and `Checks.positive`; an Effect check of the `Check`
module is written with the keywords Effect gives it when they are exact
(`Check.min_items 1` is `minItems`, `Check.lowercased` is refused because
its pattern is looser than the check). A recursive type becomes a `$ref`
into `$defs`; `Codec.annotate` adds a `title` and `description`.

`Codec.standard codec` is a Standard Schema V1 object that validates
synchronously. `Codec.standard_async codec` is made at run time: its
`validate` returns a promise run with the services and under the scope of
where it was made, and once that scope has closed it reports an issue.

`Schema.arbitrary schema` generates values for property tests; a refined
type needs `Schema.with_generator`.

## Effect checks and codecs

`Check.apply check codec` restricts what a codec decodes and encodes with
one of Effect Schema's checks: lengths and code points, patterns, string
forms (`Check.uuid`, `Check.base64`), number, `BigInt`, `BigDecimal` and
`Ord` bounds, list items and uniqueness, set sizes, and the keys of a
`Codec.entries` dictionary. `Codec` adds Effect's ready transformations
(`Codec.trim`, `Codec.number_from_string`, `Codec.base64_string`,
`Codec.json_string`, `Codec.split`), literals, dictionaries, decoding and
encoding fallbacks, runtime checks and XML encoding; `Duration`,
`BigDecimal`, `Chunk`, `HashSet` and `HashMap` have JSON codecs, and
`Arbitrary`, `JsonSchema` and `Differ` wrap Effect's generators, dialect
conversions and JSON patches.

## Running decoders

Schemas and codecs are Effect Schema values: a `Schema a` is an Effect
Schema of the program's values of `a`, and a codec an Effect Schema codec
from its wire form, so JS code can use them with Effect directly, and
`--emit-dts` projects them as `Schema.Codec<A, I, RD, RE>`. Effect's
interpreter is the baseline. `flick build --schema-aot` (or
`schema-aot = true`) loads the app's modules once on Node, hands the codecs
they make while loading to Effect's `SchemaAOTCompiler` and writes the
static module `schemas.aot.js`; the runner collects the same codecs and
installs it before `main` starts. A codec made later, or a node the
compiler does not handle, keeps the interpreter, and a module that no
longer matches the program is skipped with a warning. Both give the same
outcome on every input; a test compares them on random inputs.
