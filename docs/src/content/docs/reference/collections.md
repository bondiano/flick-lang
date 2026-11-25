---
title: "Collection type classes"
---

Flick shares collection operations across two layers while keeping every
qualified module API (`List.map`, `Chunk.get`, `HashMap.get`, `HashSet.member`,
`Trie.get`, `Bytes.get`, `Seq.take`, `NonEmpty.head`) intact:

- **Prelude classes** ([classes](../classes/#classes)) cover compatible
  mapping (`Functor`), left folding (`Foldable`), sequential effectful
  traversal (`Traversable`), applicative/monadic sequencing (`Applicative`,
  `Monad`, `Alternative`), closed `for` iteration (`Iterable`), and associative
  combination (`Semigroup`, `Monoid`).
- **Core module `Collections`** defines five small capability classes shared
  by at least two collections under the same laws: `ToSeq`, `Sized`, `Indexed`,
  `Keyed`, and `Member`.

Every capability class uses functional dependencies on the concrete collection
type `c : Type`, so polymorphic collections (`List a`, `Chunk a`,
`HashMap w k v`), single-keyed maps (`Trie v`), and monomorphic packed
collections (`Bytes`, element type `Int`) all participate uniformly. Generic
consumers ask only for the capabilities they need.

## Class hierarchy and superclasses

```text
Prelude (over f : Type -> Type and values a : Type)
  Functor f
    ├── Traversable f
    └── Applicative f
          ├── Monad f
          └── Alternative f
  Foldable f
  Iterable f           (closed: List, Seq — powers native `for`)
  Semigroup a => Monoid a

Collections (core module, open capability classes over c : Type)
  ToSeq c a | c -> a                  restartable lazy `Seq` traversal (includes `Seq`)
  Sized c                             finite non-negative count (excludes `Seq`)
    ├── Indexed c a | c -> a          zero-based access in 0 .. size - 1
    ├── Keyed c k v | c -> k v        keyed lookup under the collection's own witness
    └── Member c a | c -> a           finite membership under the collection's own equivalence
```

### Why the boundaries are drawn here

- **`ToSeq` versus closed `Iterable`.** `Iterable` is closed (`FLICK-C012`)
  because `for` lowers directly to native iteration over `List` and `Seq`
  without calling `iter`. `ToSeq` is open and converts any collection to a
  restartable, capture-preserving `Seq a`, which itself satisfies `Iterable`,
  so `for x <- Collections.to_seq coll do ...` works for every `ToSeq`
  collection.
- **`Sized` excludes `Seq`.** A `Seq` can be infinite (`Seq.from`,
  `Seq.iterate`, `Seq.repeat`, `Seq.cycle`). Keeping `Sized` (and its
  subclasses `Indexed`, `Keyed`, and `Member`) off `Seq` rejects operations
  that require finiteness — such as `Collections.size`, `Collections.to_list`,
  or `Collections.member` — at compile time.
- **`Functor` on maps only where keys and witness are preserved.**
  `HashMap w k` and `Trie` implement `Functor`, `Foldable`, and `Traversable`
  over their values while keeping keys, witness identity `w`, and traversal
  order unchanged. `HashSet w` (and planned `OrderedSet`) intentionally omits
  `Functor` and `Traversable`: mapping set elements can change equality and
  collapse entries, and requires an explicit target witness (`HashSet.map`).
- **`Semigroup` versus `Monoid`.** Non-empty collections (`NonEmpty`,
  `NonEmptyChunk`) implement `Semigroup` without `Monoid`. Witness-carrying
  hash collections (`HashMap w k v`, `HashSet w a`) implement `Semigroup` for
  any witness `w` (combining values of the same witness), and `Monoid` only
  for `EqHash.Canonical` (where `Hash` provides the canonical empty witness).

## Signatures, defaults, and laws

### `Collections.ToSeq c a | c -> a`

```flick
# import Collections
#
check_to_seq : Collections.ToSeq c a => c^d -> (Seq a)^d
let check_to_seq xs = Collections.to_seq xs
```

| Member | Signature | Default |
| - | - | - |
| `to_seq` | `forall d. c^d -> (Seq a)^d` | abstract |
| `first` | `forall d. c^d -> Option a` | `Seq.first (to_seq xs)` |

Superclasses: none.

Laws:

- `first xs == Seq.first (to_seq xs)`.
- **Restartability:** iterating `to_seq xs` more than once yields the same
  elements in the same order without mutating `xs`.
- **Prefix laziness:** `Seq.take n (to_seq xs)` pulls at most `n` elements and
  terminates on infinite `Seq`.
- **Capture preservation:** hidden captures `d` of `xs` remain tracked on the
  returned `(Seq a)^d`, so a scoped sequence cannot escape its `scope`.
- **Order agreement with `Foldable`:** when `f` has both `Foldable f` and
  `ToSeq (f a) a`, `fold step init xs == Seq.fold step init (to_seq xs)`; for
  value-mapped maps (`HashMap w k`, `Trie`, planned `OrderedMap k`),
  `fold step init m == Seq.fold (fun acc (_, v) -> step acc v) init (to_seq m)`.

### `Collections.Sized c`

| Member | Signature | Default |
| - | - | - |
| `size` | `c -> Int` | abstract |
| `is_empty` | `c -> Bool` | `size xs == 0` |

Superclasses: none.

Laws:

- `size xs >= 0`.
- `is_empty xs == (size xs == 0)`.
- When `ToSeq c a` holds: `to_seq xs` terminates with exactly `size xs` items.
- When `Monoid c` holds: `size empty == 0` and `is_empty empty == true`.

### `Sized c => Collections.Indexed c a | c -> a`

| Member | Signature | Default |
| - | - | - |
| `at` | `Int -> c -> Option a` | abstract |
| `last` | `c -> Option a` | `at (size xs - 1) xs` |

Superclass: `Sized c` (implies `size` and `is_empty`, excludes `Seq`).

Laws:

- **Bounds:** `Option.is_some (at i xs) == (i >= 0 && i < size xs)`.
- **Out-of-bounds:** `at (-1) xs == None` and `at (size xs) xs == None`.
- **Last:** `last xs == at (size xs - 1) xs`.
- **Order agreement with `ToSeq`:** for every `(i, x)` in
  `Seq.enumerate (to_seq xs)`, `at i xs == Some x`, and `at 0 xs == first xs`.

### `Sized c => Collections.Keyed c k v | c -> k v`

| Member | Signature | Default |
| - | - | - |
| `lookup` | `k -> c -> Option v` | abstract |
| `has_key` | `k -> c -> Bool` | `Option.is_some (lookup key m)` |

Superclass: `Sized c`.

Laws:

- `has_key key m == Option.is_some (lookup key m)`.
- **Empty map:** if `is_empty m`, then `lookup key m == None` and
  `has_key key m == false` for every `key`.
- **Witness consistency with `ToSeq`:** when `ToSeq c (k, v)` holds, every
  `(k, v)` yielded by `to_seq m` satisfies `lookup k m == Some v`, and `to_seq m`
  yields `size m` unique keys under `m`'s key equivalence.
- **Coherence with `Functor`:** on maps whose `Functor` preserves keys and
  witness, `lookup k (map f m) == Option.map f (lookup k m)` and
  `size (map f m) == size m`.

### `Sized c => Collections.Member c a | c -> a`

| Member | Signature | Default |
| - | - | - |
| `member` | `a -> c -> Bool` | default signature `(ToSeq c a, Eq a) =>`: `Seq.any (fun y -> eq value y) (to_seq xs)` |

Superclass: `Sized c` (excludes potentially infinite `Seq`).

Instances with their own equality or ordering witness (`HashSet w a`, planned
`OrderedSet a`) override `member` directly and need no canonical `Eq a`.
Finite sequence types with `ToSeq` and `Eq a` can either override with a
native search or inherit the short-circuiting default.

Laws:

- **Empty collection:** if `is_empty xs`, then `member value xs == false`.
- **Agreement with `ToSeq`:** when `ToSeq c a` holds, `member value xs` is
  `true` iff `to_seq xs` yields an element equivalent to `value` under `xs`'s
  equivalence.

### Prelude algebraic laws

- **`Functor`:** `map identity v == v` and
  `map (fun x -> g (h x)) v == map g (map h v)` (stated as `law` members in
  `Prelude`).
- **`Foldable`:** `fold_map f xs == fold (fun acc x -> combine acc (f x)) empty xs`,
  folding in traversal order.
- **`Traversable`:** `traverse (fun x -> x) xs` preserves `xs` with no effects;
  effects run sequentially in traversal order and short-circuit on the first
  typed failure (`Fail e`).
- **`Semigroup` / `Monoid`:** `combine (combine x y) z == combine x (combine y z)`,
  `combine empty x == x`, and `combine x empty == x` (stated as `law` members
  in `Prelude`).

## Class-to-instance matrix

Shipped collections are implemented in `core` and `std`. Planned collections
(`OrderedMap` and `OrderedSet`, tracked in issue `#735`) are included to show
how they fit the same hierarchy.

| Collection | Element / Entry (`ToSeq`) | `Functor` | `Foldable` | `Traversable` | `Semigroup` | `Monoid` | `Iterable` | `ToSeq` | `Sized` | `Indexed` | `Keyed` | `Member` |
| - | - | - | - | - | - | - | - | - | - | - | - | - |
| `List a` | `a` | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes | Yes | No | `Eq a =>` |
| `Seq a` | `a` | Yes | Yes (finite) | No | Yes | Yes | Yes | Yes | **No** | No | No | No |
| `NonEmpty a` | `a` | Yes | Yes | Yes | Yes | No | No | Yes | Yes | Yes | No | `Eq a =>` |
| `Chunk a` | `a` | Yes | Yes | Yes | Yes | Yes | No | Yes | Yes | Yes | No | `Eq a =>` |
| `NonEmptyChunk a` | `a` | Yes | Yes | Yes | Yes | No | No | Yes | Yes | Yes | No | `Eq a =>` |
| `HashMap w k v` | `(k, v)` | `HashMap w k` | `HashMap w k` | `HashMap w k` | Yes | `Hash k =>` (`Canonical`) | No | Yes | Yes | No | `(k, v)` | No |
| `HashSet w a` | `a` | **No** | `HashSet w` | No | Yes | `Hash a =>` (`Canonical`) | No | Yes | Yes | No | No | Yes (witness) |
| `Trie v` | `(String, v)` | Yes | Yes | Yes | Yes | Yes | No | Yes | Yes | No | `(String, v)` | No |
| `Bytes` | `Int` (`0..255`) | No | No | No | Yes | Yes | No | Yes | Yes | Yes | No | Yes (`Int`) |
| `PriorityQueue p a` | `(p, a)` | No | No | No | `Ord p =>` | `Ord p =>` | No | `Ord p =>` | Yes | No | No | No |
| *`OrderedMap k v` (#735)* | `(k, v)` | `OrderedMap k` | `OrderedMap k` | `OrderedMap k` | Yes | `Ord k =>` | No | Yes | Yes | No | `(k, v)` | No |
| *`OrderedSet a` (#735)* | `a` | **No** | Yes | No | Yes | `Ord a =>` | No | Yes | Yes | No | No | Yes (`Ord`) |

`List`, `Seq`, `NonEmpty`, and `Chunk` also implement `Applicative` and
`Monad` (and `List`, `Seq`, and `Chunk` implement `Alternative`).

## Traversal order, complexity, and effects

| Collection | Traversal order | `to_seq` setup + `k`-item prefix | `size` | `at` / `lookup` / `member` | `combine` | `map` / `fold` / `traverse` |
| - | - | - | - | - | - | - |
| `List a` | index `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` | `O(n + m)` | `O(n)` |
| `Seq a` | producer order (lazy) | `O(1) + O(k)` | — | — | `O(1)` lazy | `map` `O(1)` lazy, `fold` `O(n)` |
| `NonEmpty a` | index `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` | `O(n + m)` | `O(n)` |
| `Chunk a` / `NonEmptyChunk a` | index `0 .. n-1` | `O(1) + O(k log n)` | `O(1)` | `at` `O(log n)`, `member` `O(n)` | `O(1)` / `O(log n)` | `O(n)` |
| `HashMap w k v` | 5-bit HAMT hash order (low to high), insertion order on collisions | `O(1) + O(k)` | `O(1)` | `lookup`/`has_key` `O(1)` avg | `O(m)` right-biased | `O(n)` |
| `HashSet w a` | 5-bit HAMT hash order (low to high), insertion order on collisions | `O(1) + O(k)` | `O(1)` | `member` `O(1)` avg | `O(m)` | `fold` `O(n)` |
| `Trie v` | ascending UTF-16 key order | `O(1) + O(k * L)` | `O(1)` | `lookup` `O(L)` | `O(m * L)` right-biased | `O(n)` / `O(n * L)` |
| `Bytes` | byte index `0 .. n-1` | `O(1) + O(k)` | `O(1)` | `at` `O(1)`, `member` `O(n)` (`O(1)` outside `0..255`) | `O(n + m)` | — |
| `PriorityQueue p a` | ascending `Ord p`, insertion order on equal priorities | `O(1) + O(k log n)` | `O(1)` | — | `O(m + log n)` left-first | — |
| *`OrderedMap k v` (#735)* | ascending `Ord k` | `O(log n) + O(k)` | `O(1)` | `lookup` `O(log n)` | `O(m log(n + m))` right-biased | `O(n)` / `O(n log n)` |
| *`OrderedSet a` (#735)* | ascending `Ord a` | `O(log n) + O(k)` | `O(1)` | `member` `O(log n)` | `O(m log(n + m))` | `fold` `O(n)` |

`PriorityQueue` gives `peek` in `O(1)`, and `push` and `pop` in `O(log n)` in
the worst case, also on old versions. `combine q1 q2` renumbers the `m` pairs
of `q2` after those of `q1`, so on equal priorities the pairs of `q1` leave
first. Two queues are equal by `Eq` when they give the same pairs in the same
order.

### Effects, short-circuiting, and captures

- Pure operations (`to_seq`, `first`, `size`, `is_empty`, `at`, `last`,
  `lookup`, `has_key`, `member`, `map`, `fold`, `combine`, `empty`) are pure
  and allocate no fibers.
- Effectful traversals (`Traversable.traverse`, `Collections.each_effect`,
  `Collections.fold_effect`) run sequentially in traversal order, preserve
  both service and error rows `{r; e}`, and short-circuit immediately on the
  first typed failure without visiting later elements.
- Search helpers (`Collections.find`, `Collections.any`, `Collections.all`,
  `Collections.all_in`, and default `Member.member`) short-circuit at the
  first deciding element and work on lazy prefixes.
- Scoped callbacks (`(a ->{r; e} b)^c`, `(b -> a -> b)^c`, `(a -> Bool)^f`)
  and region-holding values (`a^v`, `c^d`) stay bound to their `scope`
  ([qualifiers](../regions/#qualifiers)).

## Generic consumers and examples

`Collections` provides generic consumers constrained only by the minimal
capabilities they need:

- `ToSeq`: `take`, `find`, `any`, `all`, `each_effect`, `fold_effect`
- `(Sized, ToSeq)`: `to_list`, `count`
- `Keyed`: `lookup_or`, `lookup_all` (with `ToSeq` over keys)
- `(ToSeq, Member)`: `filter_members`, `all_in`
- `Indexed`: `endpoints`, `nth_from_end`

```flick
import Collections exposing (Keyed(..), Member(..), Sized(..), ToSeq(..))

// Works on any Keyed collection (HashMap with any witness, Trie, future OrderedMap).
present_values : Keyed c String Int => List String -> c -> List (String, Int)
let present_values keys m = Seq.to_list (Collections.lookup_all keys m)

// Works on any ToSeq source (including infinite Seq) and any Member filter.
first_three_allowed : (ToSeq c a, Member s a) => s -> c -> List a
let first_three_allowed allowed source =
  Seq.to_list (Seq.take 3 (Collections.filter_members allowed source))

// Works on any ToSeq via `for` because `to_seq` yields a `Seq`.
sum_prefix : ToSeq c Int => Int -> c -> Int
let sum_prefix limit xs =
  let mut total = 0
  let mut seen = 0
  for n <- to_seq xs do {
    if seen >= limit then break
    total = total + n
    seen = seen + 1
  }
  total
```

A custom finite collection inherits `first`, `is_empty`, `last`, and
short-circuiting `member` from defaults:

```flick
import Collections exposing (Indexed(..), Member(..), Sized(..), ToSeq(..))

type Bag a = Bag (List a)

instance ToSeq (Bag a) a =
  let to_seq (Bag xs) = to_seq xs

instance Sized (Bag a) =
  let size (Bag xs) = size xs

instance Indexed (Bag a) a =
  let at index (Bag xs) = at index xs

instance Eq a => Member (Bag a) a

sample : (Bool, Option Int, Option Int, Bool)
let sample =
  let b = Bag [4, 5, 6]
  (is_empty b, first b, last b, member 5 b)
```

Potentially infinite `Seq` refuses `Sized`, and `HashSet` refuses `Functor`:

```flick fail C001
let n = Collections.size (Seq.from 0)
```

```flick fail C001
let bad = map (fun n -> n + 1) (HashSet.from_list EqHash.canonical [1, 2])
```
