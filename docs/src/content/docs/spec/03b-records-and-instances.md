---
title: "Records and instances"
source_hash: fe30a22ecb0983e0
---

## Record structurality

Records are **exact closed structural records with field-row polymorphism**. `type alias User = { ... }` is a transparent alias of a shape; nominality comes from a nominal declaration (an ADT, an `unboxed type`, a `refined type`). ADTs are nominal. There is no implicit width subtyping: extra fields are accepted only through an explicitly open row tail or are removed by an explicit projection. Width subtyping would keep hidden fields at runtime and complicate equality and exact schemas.

A field row has kind `FieldRow`, equivalent to `Row Type`; it is not a row of services or errors. Labels are unique; their order is not part of the type. The module `Record` provides the built-in `HasField`, `Lacks`, `Project`, `Remove`, `UnionDisjoint`, `Map` over a constructor of kind `Type -> Type`, and `Labels`, the canonical enumeration of labels ([row primitives](../../reference/classes/#row-primitives)). User instances for these primitives are forbidden (`FLICK-C012`). `UnionDisjoint` rejects a repeated label; the union of service/error rows, by contrast, normalizes a repetition of one nominal element.

```flick
name :
  forall fields. { name : String | fields } -> String
let name value =
  value.name

project_name :
  forall fields. { name : String | fields } -> { name : String }
let project_name value =
  { name = value.name }
```

`name` allows extra fields through `fields`. `project_name` creates an object with only the field `name`; this is not a cast that hides keys. The update `{ record with field = value }` replaces an existing field with a value of the same type. `Record.extend` requires `Lacks`; `Record.merge_disjoint` requires disjoint rows. Changing a field's type is done by explicit remove/extend or by a new record. There is no hidden overwrite on spread. The values of the fields are evaluated in source order.

### Records, instances and FFI

Two transparent aliases of one shape are one type and do not get different global `Eq` or `Hash`. For a closed record, the canonical structural instance applies when each field has instances. An open row requires constraints on the whole tail. Compiler-owned structural instances cannot be overridden for an individual alias.

For different domain semantics, a nominal wrapper is introduced. An alternative comparison without a new type is an explicit `EqHash` witness, which a `HashSet` or `HashMap` keeps as part of its type ([hash collections](../../reference/stdlib/#hash-collections)).

A record is a plain JS object with exactly its fields (on the Go target, a `*flickrt.Record`). Flick data crosses a foreign binding as a copy, so JS cannot change a Flick value it was given ([FFI](../../reference/ffi/#js-values)). Untrusted input becomes a record only by decoding through a schema or codec, which refuses keys it does not name unless the codec goes through `ignore_extra_fields` ([schemas](../../reference/schema/)).

## Classes and dependencies between types

A class may have several parameters of any kind and superclasses. `Show a` selects an implementation for one type; `Functor f` has a single parameter of kind `Type -> Type`. A relation between independent types needs several parameters:

```flick
# type ConversionError = ConversionError String
#
class Convert input output =
  convert : input -> Result ConversionError output
```

Both `Convert String Email` and `Convert String UserId` are useful here, so this class has no functional dependency. A serialization format is an explicit codec value (`Codec rd re i a`), not a global instance.

### Functional dependencies

```flick
class Collection c a | c -> a =
  empty : c
  insert : forall captures. a^captures -> (c -> c)^captures
```

The qualifier on insert keeps the value a in the partially applied function; the concrete collection type also keeps the captures of its elements.

The dependency `c -> a` reads: “a concrete collection type determines the unique element type”. For example, the heads `Collection UserSet User` and `Collection NameSet String` are possible.

```flick
# class Collection c a | c -> a =
#   empty : c
#   insert : forall captures. a^captures -> (c -> c)^captures
# type alias User = { name : String }
# type UserSet = UserSet (List User)
# instance Collection UserSet User =
#   let empty = UserSet []
#   let insert user set = set
#
empty_users : UserSet
let empty_users =
  empty
```

The expected `UserSet` also determines `User`. Without the dependency the method would have the type `Collection c a => c`, where `a` is not fixed by the result; a separate basis for choosing the dictionary unambiguously would be required.

The pair `Collection UserSet User` and `Collection UserSet String` violates the FD (`FLICK-C004`). The reverse direction is not promised: one `User` can be stored in different collections. After expanding aliases and normalizing built-in row expressions, coverage starts with the variables of the determining head arguments and closes over dependencies in the context and its instantiated superclasses. A dependency contributes its outputs only when all its inputs are covered; every determined head variable must belong to this least fixed point (`FLICK-C005`). Each head dependency is checked independently, so unrelated constraints and unseeded cycles do not help.

Context-assisted coverage permits capability lifting, not context-based selection. Instances are selected by their complete heads before their contexts are solved. Incompatible FD outputs are rejected, as are overlapping heads. For otherwise non-overlapping heads whose determining positions unify, the determined positions must already be equal under that determining substitution alone; contexts cannot justify additional output bindings. Context variables must still occur in the head, each constraint must be structurally smaller, and variable occurrences may not increase. These restrictions do not prove unbounded termination of FD improvement: every resolution and improvement retry consumes the configured step/depth budget, and exhaustion is `FLICK-C009`, never success.

## Coherence and ownership

Coherence means that one constraint gets one implementation. For collections, changing `Eq`, `Hash` or `Ord` between insertion and lookup would break their behavior.

Instance heads, after expanding aliases, must not unify (`FLICK-C003`); their context is not used to resolve overlap. All context variables must be present in the head. Recursive class constraints must shrink structurally without increasing the number of occurrences of a variable (`FLICK-C007`); the superclass graph is acyclic (`FLICK-C008`). This is a conservative termination condition, with no analogue of `UndecidableInstances`. Exceeding the solver budget produces a diagnostic and never means successful resolution. `Eq a => Eq (List a)` is allowed.

### An external type

Let `Hash` be defined by the standard library and `ExternalId` by an SDK. An adapter package that owns neither the class nor the original type cannot declare `Hash ExternalId`: orphans are forbidden.

```flick
# import Sdk exposing (ExternalId)
#
unboxed type LocalExternalId =
  | LocalExternalId ExternalId
```

Now the adapter owns a nominal type and can give it canonical instances. `unboxed` leaves the `ExternalId` itself at run time: the wrapper allocates nothing and goes into the SDK as is, while identity and instances stay with the nominal type. The transparent alias `type alias LocalExternalId = ExternalId` creates no new identity. Other paths are an instance at the owner of the class/type, or an explicitly passed dictionary.

Ownership is determined by the **package** (`FLICK-C006`): an instance can be declared by the package of the class or by the package of the external nominal constructor of the **first parameter of the head**. A transparent alias and a structural record do not create an owner. For MPTC, ownership of only a later parameter is not enough: the package of `LocalEmail` does not get the right to declare a foreign `Convert String LocalEmail`. For that, an explicit codec/dictionary, a wrapper of the first parameter or an instance at the owner of the class is needed. A binding does not change the identity of a foreign type.

A package is a project or one of the libraries it compiles from their sources (a path, git or url dependency, or an npm package with `source-dir`); the core modules and `std` are one package, the built-in one. An instance is visible wherever its class and its head type are: a library's instances come with it into every dependent, and two libraries that each own one side cannot both declare the instance, since only the owner of the class or of the first type may. So every program sees at most one instance for each head, whichever libraries it combines.

The set of instances a module sees is determined by its dependency graph, not by the order or form of `import`; conflicts are diagnosed when the whole accessible graph is checked.
