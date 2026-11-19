---
title: "Proofs"
---

A `proof` states a property of the module's definitions and proves it. The
compiler checks every proof with a small kernel and then erases it: a proof
has no value, runs nowhere and changes nothing in the program. Proofs are
opt-in, and a module without them is compiled as before.

## Proof declarations

```flick
proof rebuilt (v : Option a)
where equal (match v with | None -> None | Some x -> Some x) v =
  match v with
  | None -> refl
  | Some y -> refl

proof same where same_type a a = refl
```

A proof has a name, parameters, a goal after `where` and a proof after `=`.
A parameter is a typed pattern, and its type variables are rigid: the proof
holds for every type, as a signature's variables do. The goal's sides are
ordinary expressions of one type; the proof is an expression built from the
rules below. `proof` is a keyword only at the start of a declaration, and
`equal`, `holds`, `same_type` and the rules are names everywhere else.

## Goals

| Goal | States |
| - | - |
| `equal l r` | `l` and `r`, of one type, are the same value |
| `holds p` | the `Bool` `p` is `true`, as `equal p true` |
| `same_type A B` | the two types are the same type |

A parameter `(h : goal)` is a premise: the proof may use `h` as a proof of
`goal`, and a lemma that has premises needs proofs of them.

```flick
is_some : Option a -> Bool
let is_some v =
  match v with
  | Some _ -> true
  | None -> false

proof some_is_some (x : a) where holds (is_some (Some x)) = refl

proof lifted (x : a) (y : a) (h : equal x y)
where equal (Some x) (Some y) = cong (fun z -> Some z) h
```

## Rules

| Rule | Proves |
| - | - |
| `refl` | `l = r` when both sides evaluate to the same normal form |
| `symm p` | `r = l` from `p : l = r` |
| `trans m p q` | `l = r` from `p : l = m` and `q : m = r` |
| `cong (fun x -> e) p` | `e[a/x] = e[b/x]` from `p : a = b` |
| `funext (fun x -> p)` | `f = g` from `p : f x = g x` for every `x` |
| `match v with …` | the goal in each case of a parameter `v`, one arm per constructor |
| a premise `h` | its goal |
| `let h = p` then a proof | the proof, with `h` a proof of what `p` proves |
| a lemma `name args` | an earlier proof's goal at `args`, its premises proved |

```flick
proof back (x : a) (y : a) (h : equal x y) where equal y x = symm h

proof eta (f : a -> b) where equal (fun x -> f x) f = funext (fun y -> refl)

proof through (x : a) (y : a) (z : a) (p : equal x y) (q : equal y z)
where equal x z = trans y p q
```

A `match` in a proof splits a parameter: each arm proves the goal with the
parameter replaced by the arm's pattern, and a missing arm is `FLICK-T009`,
as in any `match`.

## Definitions and lemmas

A proof sees the bodies of the definitions it names, of the module's own,
of the modules it imports and of the core instances, and unfolds them as it
evaluates. A method is the one of the instance its constraint resolves to.

```flick
twice : (a -> a) -> a -> a
let twice f x = f (f x)

proof unfolded (f : a -> a) (x : a) where equal (twice f x) (f (f x)) = refl

proof twice_identity (x : a)
where equal (twice identity x) x = trans (identity (identity x)) (unfolded identity x) refl

proof right_identity (v : Option a)
where equal (flat_map pure v) v =
  match v with
  | None -> refl
  | Some x -> refl
```

A proof checked before is a lemma of the later ones, and a proof module's
proofs are lemmas of the modules that import it, as `M.name`. A lemma proves
its goal about exactly the definitions it names: `unfolded identity x`
proves `twice identity x = identity (identity x)`, not `twice identity x = x`,
so `trans` adds the step that unfolds `identity`.

## The domain

A proof is about the values of `PureTotalV1`: pure, total functions over
`Bool`, data types that do not contain themselves, tuples and functions. A
statement that names something else is outside the domain, `FLICK-T082`,
and the proof is not checked: recursion, recursive types such as `List`,
foreign code, effects, primitive operations such as `+` on `Int`, records,
and a method or definition that takes dictionaries.

```flick fail T082
proof add_zero (x : Int) where equal (x + 0) x = refl
```

```flick fail T082
last : List a -> Option a
let rec last xs =
  match xs with
  | [] -> None
  | [x] -> Some x
  | [_, ...rest] -> last rest

proof last_of_one (x : a) where equal (last [x]) (Some x) = refl
```

Total means that a proof says nothing of a function that fails or loops: a
law proved of `Option` holds for every total function, and the program is
free to call a function that is not.

## Failures

| Code | The proof |
| - | - |
| `FLICK-T080` | is false: the two sides are different values |
| `FLICK-T081` | does not close its goal; the message shows what is left |
| `FLICK-T082` | names something outside the domain |
| `FLICK-T083` | is no valid certificate, as a bundle without a lemma for a law |
| `FLICK-T084` | needs more steps than the kernel's budget allows |

```flick fail T080
proof never (x : a) where equal (Some x) None = refl
```

```flick fail T081
proof constant (f : Bool -> Bool) where equal (f true) (f false) = refl
```

A proof is checked only in a module that has no other errors, and its
diagnostics are those of the module it is in.

## Laws and bundles

A class states its laws with `law` members: a law is a statement that every
instance should satisfy, typed as a default method is, with no proof. The
compiler erases it.

```flick
class Wrap (f : Type -> Type) =
  wrap : a -> f a
  unwrap : f a -> a
  law round (x : a) where equal (unwrap (wrap x : f a)) x
```

`Functor`, `Applicative` and `Monad` state their laws in the core. A proof
whose goal is `lawful C T` is a bundle: it proves the laws of `C`, and of
each of its superclasses, of the instance `C T` resolves to. Its body is a
record that names, for each law, a lemma proved before; the laws of a
superclass are a record in that superclass's field, as in a dictionary.

```flick
type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box_composition (g : b -> c) (h : a -> b) (v : Box a)
where equal (map (fun x -> g (h x)) v) (map g (map h v)) =
  match v with
  | Box x -> refl

proof box where lawful Functor Box = {
  identity = box_identity
  composition = box_composition
}
```

Each law is read at the bundle's dictionary and types, and its lemma must
state exactly that law: the same parameters, of the same types, and the
law's sides with that instance's methods. A lemma of another function, type
or dictionary proves nothing of the instance, and a field missing for a law
is `FLICK-T083`.

```flick fail T083
type Box a = Box a

instance Functor Box =
  let map f value =
    match value with
    | Box x -> Box (f x)

proof box_identity (v : Box a)
where equal (map identity v) v =
  match v with
  | Box x -> refl

proof box where lawful Functor Box = { identity = box_identity }
```

A broken instance gets no bundle, because its laws have no lemmas: a `map`
that drops every value fails `identity` itself.

```flick fail T080
type Bag a = Empty | Full a

instance Functor Bag =
  let map _ _ = Empty

proof bag_identity (v : Bag a)
where equal (map identity v) v =
  match v with
  | Empty -> refl
  | Full x -> refl
```

`lawful C T from d` names a dictionary value instead of an instance, and is
outside the domain: a bundle is about an instance.

## Proof modules and `flick prove`

Proofs may stand beside the definitions in any module, or in proof modules
under `project.proof-dir` (by default `proofs/`). Proof modules import the
project's modules and each other, and no source module imports one.
`flick check`, `build`, `test` and `pack` never read them, so a library's
proofs cost its users nothing; `flick prove` checks them, and the editor
checks one that is open ([cli](../cli/)).

A module with proofs is checked again on every run, so a proof follows an
edit of a body it names even when the signature stays. The core proves the
twelve laws of its `Option` instances in `packages/core/proofs/Laws/Option.fl`
and certifies them with the bundle `lawful Monad Option`.

## What a proof promises

A checked proof promises that its goal holds of the definitions the compiler
read, for every value of the domain. The kernel, its reading of the
compiled definitions, and the compiler and runtimes that run the program
are trusted; [the proof contract](https://github.com/bondiano/flick-lang/blob/main/docs/formal/proofs.md)
gives the rules and the boundary.
